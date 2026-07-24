/** Packed R-tree traversal -- ports `fcb::rtree_search_bbox`
 *  (src/cpp/src/packed_rtree.cpp), itself a port of
 *  `PackedRTree::stream_search` / `http_stream_search`
 *  (src/rust/fcb_core/src/packed_rtree/mod.rs:520-560, :930-1010). */
import { ErrorCode, FcbError } from '../errors.js'
import type { RangeReader, ReadOpts } from '../io/range-reader.js'
import { NODE_ITEM_SIZE } from '../layout.js'
import { decodeNodeItem, intersects } from './node-item.js'
import type { BBox } from './node-item.js'

/** What a caller asks the index for. `nearest` is declared here so the union
 *  is complete and `SelectOptions` does not change shape when Task 16 lands;
 *  until then it is rejected explicitly rather than silently treated as a
 *  point. */
export type SpatialQuery =
  | { kind: 'bbox'; value: [number, number, number, number] }
  | { kind: 'point'; value: [number, number] }
  | { kind: 'nearest'; value: [number, number] }

/** One index hit. `offset` is a byte offset RELATIVE to `featureBegin`
 *  (writer/mod.rs:207-215), matching `Feature.byteOffset`; `index` is the
 *  feature's ordinal, i.e. its position within the leaf level. */
export interface SearchResultItem {
  offset: number
  index: number
}

/** Half-open `[start, end)` node-index range for one tree level. */
interface LevelBound {
  start: number
  end: number
}

/** Mirrors `generate_level_bounds` (packed_rtree/mod.rs:342-375).
 *
 *  THE ORDERING IS THE EASY THING TO GET WRONG: `levelBounds[0]` is the LEAF
 *  level and is LAST in storage order, while `levelBounds[length - 1]` is the
 *  root and occupies node 0. The array is indexed bottom-up (level 0 = leaves)
 *  but the byte ranges it holds run top-down. Inverting this produces a tree
 *  that traverses without erroring and returns wrong answers, which is why
 *  every consumer below phrases leaf-ness as `level === 0` and never as
 *  "the last entry".
 *
 *  For 3 items at node size 16 this is `[{1,4}, {0,1}]`; for 12 items at node
 *  size 8 it is `[{3,15}, {1,3}, {0,1}]`. */
export function generateLevelBounds(numItems: number, nodeSize: number): LevelBound[] {
  if (!Number.isInteger(nodeSize) || nodeSize < 2) {
    throw new FcbError(ErrorCode.IllegalHeaderSize, `invalid index_node_size: ${nodeSize}`)
  }
  if (!Number.isInteger(numItems) || numItems <= 0) {
    throw new FcbError(ErrorCode.NoIndex, `cannot traverse an rtree over ${numItems} items`)
  }

  // Node counts per level, bottom-up: leaves first.
  const levelNumNodes: number[] = []
  let n = numItems
  let numNodes = n
  levelNumNodes.push(n)
  for (;;) {
    n = Math.ceil(n / nodeSize)
    numNodes += n
    levelNumNodes.push(n)
    if (n === 1) break
  }

  // Offsets accumulate from the END of the node array backwards, which is
  // what puts the leaf level (entry 0) last in storage.
  const bounds: LevelBound[] = []
  let acc = numNodes
  for (const size of levelNumNodes) {
    acc -= size
    bounds.push({ start: acc, end: acc + size })
  }
  return bounds
}

/** Total node count, leaves included (packed_rtree/mod.rs:879-898). */
export function rtreeNumNodes(numItems: number, nodeSize: number): number {
  const bounds = generateLevelBounds(numItems, nodeSize)
  return bounds[0]!.end
}

function finite(...vs: number[]): boolean {
  return vs.every((v) => Number.isFinite(v))
}

/** Validates a query and lowers it to the single rectangle the traversal
 *  compares against. Runs BEFORE any I/O, so a bad argument never costs a
 *  request.
 *
 *  A point becomes the degenerate box `[x, y, x, y]`: `intersects` on that
 *  box is exactly `contains_point`'s predicate (`x >= min_x && x <= max_x
 *  && ...`), so the two query kinds share one descent instead of duplicating
 *  it -- which is what the Rust reader's second, near-identical
 *  `Query::PointIntersects` arm does (packed_rtree/mod.rs:531-560). */
export function queryToBBox(query: SpatialQuery): BBox {
  if (query.kind === 'nearest') {
    // A nearest query does not lower to a rectangle -- it is a ranked walk,
    // not an intersection -- so it never travels this path: `select()` routes
    // it to `searchNearest` and `nearest.ts`'s `validateNearestPoint` guards
    // its argument. This branch is a defensive invariant: `searchRtree` (which
    // is the only caller of `queryToBBox`) does not implement nearest.
    throw new FcbError(
      ErrorCode.QueryExecutionError,
      'nearest is not a bbox query; searchNearest handles it',
    )
  }
  if (query.kind === 'point') {
    const [x, y] = query.value
    if (!finite(x, y)) {
      throw new FcbError(ErrorCode.InvalidArgument, `invalid point: [${x}, ${y}]`)
    }
    return { minX: x, minY: y, maxX: x, maxY: y }
  }
  const [minX, minY, maxX, maxY] = query.value
  if (!finite(minX, minY, maxX, maxY)) {
    throw new FcbError(
      ErrorCode.InvalidArgument,
      `invalid bbox: [${minX}, ${minY}, ${maxX}, ${maxY}] is not finite`,
    )
  }
  if (minX > maxX || minY > maxY) {
    throw new FcbError(
      ErrorCode.InvalidArgument,
      `invalid bbox: [${minX}, ${minY}, ${maxX}, ${maxY}] is inverted`,
    )
  }
  return { minX, minY, maxX, maxY }
}

/** One queued unit of work: a contiguous run of nodes on one level.
 *
 *  `end` is where EVALUATION stops; `fetchEnd` is where the READ stops, and
 *  is one node further on a leaf level -- see the +1 rule in `searchRtree`. */
interface NodeRange {
  level: number
  start: number
  end: number
  fetchEnd: number
}

/** Searches the packed R-tree and returns the matching features' offsets,
 *  sorted ascending so the caller reads the feature section forwards.
 *
 *  `rtreeBegin` is the absolute byte offset of node 0 (`layout.rtreeBegin`),
 *  `numItems` the feature count the tree was built over, and `nodeSize` the
 *  header's `index_node_size` -- WHICH MUST BE PASSED, never assumed to be
 *  the default 16. Both the wasm binding and `fcb_core`'s HTTP reader hardcode
 *  16 and silently mis-traverse a file written with any other node size.
 *
 *  Breadth-first over node RANGES, not individual nodes, so the reads run in
 *  roughly ascending file order and one read covers a whole sibling group.
 *  The `await` sits at the top of the loop body: the queue is drained in the
 *  order it was filled, and no two node reads are in flight at once, which
 *  keeps the request log deterministic and reviewable.
 *
 *  `opts.signal` is threaded into EVERY `read`. A signal that only lived on
 *  the facade would cancel nothing: the traversal is where the in-flight
 *  fetches are. */
export async function searchRtree(
  reader: RangeReader,
  rtreeBegin: number,
  numItems: number,
  nodeSize: number,
  query: SpatialQuery,
  opts?: ReadOpts,
): Promise<SearchResultItem[]> {
  const bounds = queryToBBox(query)
  const levelBounds = generateLevelBounds(numItems, nodeSize)
  // levelBounds[0] is the LEAF level; its start is the node index the first
  // feature sits at, so `pos - leafStart` is a feature ordinal.
  const leafStart = levelBounds[0]!.start

  const results: SearchResultItem[] = []
  const queue: NodeRange[] = [{
    level: levelBounds.length - 1,
    start: 0,
    end: 1,
    fetchEnd: 1,
  }]

  while (queue.length > 0) {
    const range = queue.shift()!
    const block = await reader.read(
      rtreeBegin + range.start * NODE_ITEM_SIZE,
      (range.fetchEnd - range.start) * NODE_ITEM_SIZE,
      opts,
    )

    const isLeaf = range.level === 0
    const childLevel = range.level - 1
    for (let pos = range.start; pos < range.end; pos++) {
      const item = decodeNodeItem(block, pos - range.start)
      if (!intersects(item, bounds)) continue

      if (isLeaf) {
        // Leaf offset: a byte offset relative to featureBegin.
        results.push({ offset: item.offset, index: pos - leafStart })
        continue
      }

      // Internal offset: the index of this node's FIRST CHILD.
      const child = levelBounds[childLevel]!
      if (item.offset < child.start || item.offset >= child.end) {
        // Child indices come off disk and are hostile. Prove the range lies
        // inside the level we believe we are descending to before trusting it,
        // rather than reading arbitrary bytes as node items.
        throw new FcbError(
          ErrorCode.NoIndex,
          `rtree child index ${item.offset} outside level ${childLevel} [${child.start}, ${child.end})`,
        )
      }
      const end = Math.min(item.offset + nodeSize, child.end)
      // THE +1 LEAF FETCH RULE (packed_rtree/mod.rs:979-987): when the
      // children are leaves, extend the READ by one extra node, clamped to
      // levelBounds[0].end, so the next leaf's offset -- and therefore this
      // feature's byte length -- is available from the same request.
      //
      // The extra node is fetched but NOT evaluated. It belongs to the next
      // parent's sibling group, and its own parent's box necessarily contains
      // it, so any query it matched would also reach it through that parent:
      // evaluating it here yields a DUPLICATE result. The Rust HTTP reader
      // iterates the whole fetched block including the extra node
      // (mod.rs:958-966) and does duplicate; this port does not.
      const fetchEnd = childLevel === 0 ? Math.min(end + 1, child.end) : end
      queue.push({ level: childLevel, start: item.offset, end, fetchEnd })
    }
  }

  results.sort((a, b) => a.offset - b.offset)
  return results
}
