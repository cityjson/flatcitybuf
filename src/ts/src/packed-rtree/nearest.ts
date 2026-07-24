/** Nearest-centroid spatial search over the packed R-tree.
 *
 *  The one algorithm in this port with NO C++ or Python form to copy: it is
 *  transcribed directly from the three Rust forms of `Query::PointNearest` --
 *  in-memory (packed_rtree/mod.rs:571-668), stream (:771-873), and HTTP
 *  (:1140-1256). All three run the identical best-first walk; they differ only
 *  in whether a node range is already in memory or must be read. This file
 *  keeps that structure: one shared `nearestCore`, parameterised by how it
 *  fetches a node range, driven by two node-supplying strategies.
 *
 *  TWO DISTANCE METRICS, MIXED DELIBERATELY -- and the mix must NOT be
 *  "fixed":
 *   * internal nodes are ORDERED and PRUNED by `minDistanceSquared` (squared
 *     distance to the nearest point of the bbox, 0 if the point is inside);
 *   * a leaf's FINAL score is `centroidDistanceSquared` (squared distance to
 *     the bbox centre).
 *  Because a child's box is contained in its parent's and a leaf's centroid
 *  lies inside its own box, the internal min-distance is an admissible lower
 *  bound for the leaf centroid metric, so the search is EXACT for the
 *  nearest-CENTROID problem. It is not nearest-feature-geometry: do not
 *  "correct" it to use centroids for internal nodes or bbox-distance for
 *  leaves. Both metrics are squared -- there is no `sqrt` anywhere, and
 *  comparing squared distances orders identically to comparing distances.
 *
 *  Returns AT MOST ONE item. Tie order is unspecified upstream (a JS binary
 *  heap breaks ties differently from Rust's `BinaryHeap`, both equally valid),
 *  so a leaf replaces the best only on a STRICT improvement -- on an exact tie
 *  the first-reached leaf wins, and callers assert distance, not identity. */
import { ErrorCode, FcbError } from '../errors.js'
import type { RangeReader, ReadOpts } from '../io/range-reader.js'
import { NODE_ITEM_SIZE } from '../layout.js'
import { centroidDistanceSquared, decodeNodeItem, minDistanceSquared } from './node-item.js'
import type { NodeItem } from './node-item.js'
import { generateLevelBounds } from './search.js'
import type { SearchResultItem } from './search.js'

/** The 256 KB spatial "combine" threshold (packed_rtree combines index reads
 *  below this into one). At or below it, the whole R-tree is fetched in ONE
 *  read and the best-first walk runs against that buffer with zero further
 *  index I/O; above it, the walk streams a read per popped node range. Every
 *  corpus file's index is far below this, so tests must be able to lower it to
 *  exercise the streaming path -- see `wholeIndexThreshold`. */
export const WHOLE_INDEX_THRESHOLD = 262144

/** One queued unit of work: the child group starting at node `nodeIndex` on
 *  `level`, keyed for the heap by the PARENT'S min-distance (`distance`).
 *  Keying by the parent's lower bound is what makes the pop order a valid
 *  best-first order. */
interface HeapItem {
  distance: number
  nodeIndex: number
  level: number
}

/** A binary min-heap keyed by `distance`. Small and local on purpose: the
 *  only runtime dependency is `flatbuffers`, so no heap library is pulled in.
 *  Not a stable heap -- ties come out in an unspecified order, which matches
 *  the "tie order is unspecified" contract above. */
class MinHeap {
  private readonly items: HeapItem[] = []

  get size(): number {
    return this.items.length
  }

  push(item: HeapItem): void {
    const items = this.items
    items.push(item)
    let i = items.length - 1
    while (i > 0) {
      const parent = (i - 1) >> 1
      if (items[parent]!.distance <= items[i]!.distance) break
      const tmp = items[parent]!
      items[parent] = items[i]!
      items[i] = tmp
      i = parent
    }
  }

  pop(): HeapItem | undefined {
    const items = this.items
    const n = items.length
    if (n === 0) return undefined
    const top = items[0]!
    const last = items.pop()!
    if (n > 1) {
      items[0] = last
      let i = 0
      for (;;) {
        const left = 2 * i + 1
        const right = 2 * i + 2
        let smallest = i
        if (left < items.length && items[left]!.distance < items[smallest]!.distance) smallest = left
        if (right < items.length && items[right]!.distance < items[smallest]!.distance) smallest = right
        if (smallest === i) break
        const tmp = items[smallest]!
        items[smallest] = items[i]!
        items[i] = tmp
        i = smallest
      }
    }
    return top
  }
}

/** Fetches the decoded node items for the half-open range `[start, start +
 *  count)` of absolute node indices. The ONLY thing that differs between the
 *  fast and streaming paths. */
type NodeReader = (start: number, count: number) => Promise<NodeItem[]>

/** Rejects a non-finite nearest point before any I/O, so a bad argument never
 *  costs a request (mirrors `queryToBBox`'s finiteness guard for bbox/point).
 *  Exported so `select()` can validate up front, exactly where it validates
 *  the other query kinds. */
export function validateNearestPoint(point: readonly [number, number]): void {
  const [x, y] = point
  if (!Number.isFinite(x) || !Number.isFinite(y)) {
    throw new FcbError(ErrorCode.InvalidArgument, `invalid point: [${x}, ${y}]`)
  }
}

/** The shared best-first walk. Transcribed operator-for-operator from the
 *  Rust forms cited in the file header; the `NodeReader` abstracts away the
 *  only difference between them. */
async function nearestCore(
  numItems: number,
  nodeSize: number,
  x: number,
  y: number,
  readNodes: NodeReader,
): Promise<SearchResultItem[]> {
  const levelBounds = generateLevelBounds(numItems, nodeSize)
  // levelBounds[0] is the LEAF level; `pos - leafStart` is a feature ordinal.
  const leafStart = levelBounds[0]!.start

  const heap = new MinHeap()
  // Seed with the root (node 0, top level) at distance 0.
  heap.push({ distance: 0, nodeIndex: 0, level: levelBounds.length - 1 })

  let best: { distance: number; item: SearchResultItem } | undefined

  for (;;) {
    const next = heap.pop()
    if (next === undefined) break
    // Termination: the heap is ordered by an admissible lower bound, so once
    // the smallest remaining lower bound STRICTLY exceeds the best centroid
    // distance found, nothing left can beat it. Strict, not `>=`, so a node
    // whose bound ties the best is still expanded (it might hold a closer
    // centroid).
    if (best !== undefined && next.distance > best.distance) break

    const isLeaf = next.level === 0
    const childLevel = next.level - 1
    const levelEnd = levelBounds[next.level]!.end
    const end = Math.min(next.nodeIndex + nodeSize, levelEnd)
    const count = end - next.nodeIndex
    const nodes = await readNodes(next.nodeIndex, count)

    for (let i = 0; i < count; i++) {
      const pos = next.nodeIndex + i
      const item = nodes[i]!
      const dist = minDistanceSquared(item, x, y)
      // Prune BEFORE the leaf/internal split, on the lower bound: a subtree
      // (or leaf) whose nearest-point distance already reaches the best can
      // hold nothing strictly closer. `>=`, so an equal bound is pruned.
      if (best !== undefined && dist >= best.distance) continue

      if (isLeaf) {
        // Leaf: score by CENTROID distance and keep it only on a STRICT
        // improvement, so on an exact tie the first-reached leaf is retained.
        const centroidDist = centroidDistanceSquared(item, x, y)
        if (best === undefined || centroidDist < best.distance) {
          best = { distance: centroidDist, item: { offset: item.offset, index: pos - leafStart } }
        }
      } else {
        // Internal: `offset` is the index of this node's first child. Prove
        // the child range lies within the level we descend to before trusting
        // an index that came off disk (same guard as `searchRtree`).
        const child = levelBounds[childLevel]!
        if (item.offset < child.start || item.offset >= child.end) {
          throw new FcbError(
            ErrorCode.NoIndex,
            `rtree child index ${item.offset} outside level ${childLevel} [${child.start}, ${child.end})`,
          )
        }
        // Push the child group keyed by the PARENT'S lower bound `dist`.
        heap.push({ distance: dist, nodeIndex: item.offset, level: childLevel })
      }
    }
  }

  return best === undefined ? [] : [best.item]
}

/** Nearest-centroid search. Returns at most one `SearchResultItem`.
 *
 *  `rtreeBegin` is the absolute byte offset of node 0; `rtreeSize` the index's
 *  byte length; `numItems` the feature count the tree was built over; and
 *  `nodeSize` the header's `index_node_size` -- which MUST be passed, never
 *  assumed to be 16.
 *
 *  Two paths, one algorithm:
 *   * FAST PATH (`rtreeSize <= wholeIndexThreshold`): read the whole index in
 *     ONE request, then walk it in memory. `delft.fcb`'s 47 KB index is one
 *     request; every corpus file is smaller still.
 *   * STREAMING PATH (above the threshold): issue one read per popped node
 *     range. Wave batching of concurrent reads is deliberately DEFERRED.
 *  The threshold is overridable so tests can force the streaming path (which
 *  would otherwise never run, since every file is below the default) and prove
 *  the two paths agree.
 *
 *  `opts.signal` is threaded into EVERY `read` on both paths, so an abort
 *  reaches the actual in-flight fetch rather than being inert on the facade. */
export async function searchNearest(
  reader: RangeReader,
  rtreeBegin: number,
  rtreeSize: number,
  numItems: number,
  nodeSize: number,
  x: number,
  y: number,
  wholeIndexThreshold: number = WHOLE_INDEX_THRESHOLD,
  opts?: ReadOpts,
): Promise<SearchResultItem[]> {
  validateNearestPoint([x, y])

  if (rtreeSize <= wholeIndexThreshold) {
    // Fetch the entire index once; every node range is then a slice of it.
    const whole = await reader.read(rtreeBegin, rtreeSize, opts)
    const readFromBuffer: NodeReader = (start, count) => {
      const out: NodeItem[] = []
      // The buffer starts at node 0, so an absolute index IS its slot.
      for (let i = 0; i < count; i++) out.push(decodeNodeItem(whole, start + i))
      return Promise.resolve(out)
    }
    return nearestCore(numItems, nodeSize, x, y, readFromBuffer)
  }

  const readFromReader: NodeReader = async (start, count) => {
    const block = await reader.read(rtreeBegin + start * NODE_ITEM_SIZE, count * NODE_ITEM_SIZE, opts)
    const out: NodeItem[] = []
    for (let i = 0; i < count; i++) out.push(decodeNodeItem(block, i))
    return out
  }
  return nearestCore(numItems, nodeSize, x, y, readFromReader)
}
