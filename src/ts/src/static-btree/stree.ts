/** Static B+tree traversal -- ports `src/cpp/src/stree.cpp`, itself a port of
 *  `static_btree/stree.rs`. The C++ file is the conformant reference and
 *  every non-obvious rule below cites the line it came from.
 *
 *  FOUR WAYS THIS TREE IS NOT THE PACKED R-TREE, all of them real:
 *   1. A node holds `branchingFactor - 1` ENTRIES, because each entry is a
 *      separator key and a node with fan-out f needs only f-1 of them. The
 *      fan-out itself never appears in a search.
 *   2. The level-bounds loop stops at `n < branchingFactor` -- NOT at
 *      `n === 1`. It stops as soon as a level fits in one node's worth of
 *      separators, so the top level may legitimately hold several entries.
 *   3. Payload offsets are relative to the payload section, which begins
 *      immediately after the node array inside the same index blob.
 *   4. There are no leaf sibling pointers. A range scan walks the contiguous
 *      leaf array by index. */
import { ErrorCode, FcbError } from '../errors.js'
import type { RangeReader, ReadOpts } from '../io/range-reader.js'
import type { AttrIndexInfo } from '../header/index.js'
import { toSafeNumber } from '../le.js'
import type { SearchResultItem } from '../packed-rtree/index.js'
import type { Operator } from '../reader.js'
import { readEntries, entrySize } from './entry.js'
import type { Entry } from './entry.js'
import { compareKeys, keyMax, keyMin, needsPostFilter } from './key.js'
import type { KeyKind } from './key.js'
import { emitOffset } from './payload.js'

/** Half-open `[start, end)` node-index range for one tree level, indexed
 *  bottom-up: `levels[0]` is the LEAF level (and is LAST in storage), the
 *  final entry is the root and starts at node 0. Same inversion as the
 *  R-tree's `generateLevelBounds`, and just as easy to get backwards, which
 *  is why every consumer says `level === 0` for leaf-ness. */
interface LevelBound {
  start: number
  end: number
}

/** Mirrors `generate_level_bounds` (stree.cpp:28-57).
 *
 *  The `n < branchingFactor` break is the asymmetry against the R-tree's
 *  `n === 1`. Do not "fix" it: it is what the writer built the file with. */
export function generateStreeLevelBounds(
  numItems: number,
  branchingFactor: number,
): LevelBound[] {
  if (!Number.isInteger(branchingFactor) || branchingFactor < 2) {
    throw new FcbError(
      ErrorCode.AttributeIndexNotFound,
      `invalid branching factor ${branchingFactor}`,
    )
  }
  if (!Number.isInteger(numItems) || numItems <= 0) {
    throw new FcbError(ErrorCode.AttributeIndexNotFound, 'empty attribute index')
  }

  const levelNumNodes: number[] = []
  let n = numItems
  let numNodes = n
  levelNumNodes.push(n)
  for (;;) {
    n = Math.ceil(n / branchingFactor)
    numNodes += n
    levelNumNodes.push(n)
    if (n < branchingFactor) break
  }

  const bounds: LevelBound[] = []
  let acc = numNodes
  for (const size of levelNumNodes) {
    acc -= size
    bounds.push({ start: acc, end: acc + size })
  }
  return bounds
}

/** Total node count, the tree's own nodes only -- the payload section that
 *  follows them in the same blob is not counted (stree.cpp:315-329). */
export function streeNumNodes(numItems: number, branchingFactor: number): number {
  if (!Number.isInteger(branchingFactor) || branchingFactor < 2) {
    throw new FcbError(
      ErrorCode.AttributeIndexNotFound,
      `invalid branching factor ${branchingFactor}`,
    )
  }
  if (numItems === 0) return 0
  return generateStreeLevelBounds(numItems, branchingFactor)[0]!.end
}

/** Rust's `binary_search_by` result: whether an exact match was found, plus
 *  either its index or the insertion point (stree.cpp:104-124). Both halves
 *  matter -- the descent rules below branch on all three cases. */
interface BinarySearch {
  found: boolean
  index: number
}

function binarySearch(kind: KeyKind, items: readonly Entry[], key: unknown): BinarySearch {
  let lo = 0
  let hi = items.length
  while (lo < hi) {
    const mid = lo + ((hi - lo) >> 1)
    const c = compareKeys(kind, items[mid]!.key, key)
    if (c === 0) return { found: true, index: mid }
    if (c < 0) lo = mid + 1
    else hi = mid
  }
  return { found: false, index: lo }
}

/** Everything one query needs about one column's index blob. */
interface Tree {
  reader: RangeReader
  indexBegin: number
  payloadBegin: number
  payloadSize: number
  kind: KeyKind
  /** `branchingFactor - 1`: the number of ENTRIES a node holds. */
  nodeSize: number
  levels: LevelBound[]
  opts?: ReadOpts
}

const leafStart = (t: Tree) => t.levels[0]!.start
const leafEnd = (t: Tree) => t.levels[0]!.end

/** Reads one node: `nodeSize` entries from `nodeIndex`, clamped to the end of
 *  its own level so a node at the tail of a level never reads into the next
 *  one (stree.cpp:176-180). */
function nodeAt(t: Tree, nodeIndex: number, level: number): Promise<Entry[]> {
  const end = Math.min(nodeIndex + t.nodeSize, t.levels[level]!.end)
  return readEntries(t.reader, t.indexBegin, t.kind, nodeIndex, end, t.opts)
}

/** An INTERNAL entry's offset, as a node index. Never tagged -- the payload
 *  tag is a leaf-level meaning -- so converting here is safe, and going
 *  through `toSafeNumber` keeps a corrupt 2^53+ value from silently becoming
 *  an unusable float. */
function childIndex(entry: Entry): number {
  return toSafeNumber(entry.offset, 'stree child index')
}

/** A child node index read off disk is hostile input; prove it lands inside
 *  the level we believe we are descending to before reading bytes there. */
function checkedChild(t: Tree, child: number, childLevel: number): number {
  const bound = t.levels[childLevel]!
  if (child < bound.start || child >= bound.end) {
    throw new FcbError(
      ErrorCode.AttributeIndexNotFound,
      `stree child index ${child} outside level ${childLevel} [${bound.start}, ${bound.end})`,
    )
  }
  return child
}

/** Mirrors `find_exact` (stree.cpp:184-234). Breadth-first, because the
 *  descent rules can in principle enqueue more than one node. */
async function findExact(t: Tree, key: unknown): Promise<SearchResultItem[]> {
  const out: SearchResultItem[] = []
  const queue: Array<[number, number]> = [[0, t.levels.length - 1]]

  while (queue.length > 0) {
    const [nodeIndex, level] = queue.shift()!
    const items = await nodeAt(t, nodeIndex, level)
    if (items.length === 0) continue

    const hit = binarySearch(t.kind, items, key)

    if (level !== 0) {
      // Internal descent. On an exact hit the search key belongs to the RIGHT
      // of that separator, hence the + nodeSize; findPartition deliberately
      // omits it, which is what makes it return the LEFTMOST position.
      let child: number
      if (hit.found) {
        child = childIndex(items[hit.index]!) + t.nodeSize
      } else if (hit.index === 0) {
        child = childIndex(items[0]!)
      } else if (hit.index >= items.length) {
        child = childIndex(items[items.length - 1]!) + t.nodeSize
      } else {
        child = childIndex(items[hit.index]!)
      }

      // TRAP: a separator entry with no right sibling carries the type's
      // MAXIMUM as a sentinel, and that sentinel's offset ALREADY points at
      // the last child group -- adding nodeSize walks off the end of the
      // level. Any query whose key equals the type maximum triggers it, and
      // `Eq(true)` on a bool column is enough (a bool index with one unique
      // key has `true` as its only separator). Clamping back to the entry's
      // own offset is a no-op for ordinary keys (stree.cpp:213-222).
      const childLevel = level - 1
      if (child >= t.levels[childLevel]!.end) {
        const at = hit.index < items.length ? hit.index : items.length - 1
        child = childIndex(items[at]!)
      }
      queue.push([checkedChild(t, child, childLevel), childLevel])
      continue
    }

    if (hit.found) {
      await emitOffset(
        items[hit.index]!.offset,
        nodeIndex + hit.index - leafStart(t),
        t.reader,
        t.payloadBegin,
        t.payloadSize,
        out,
        t.opts,
      )
    }
  }
  return out
}

/** Mirrors `find_partition` (stree.cpp:240-258): the same descent as
 *  `findExact` EXCEPT that an exact hit descends to `offset` with NO
 *  + nodeSize. That single difference is what makes this return the leftmost
 *  leaf position the key could occupy rather than skipping past equal keys --
 *  and it is also why the caller must widen its upper scan bound (below).
 *
 *  Deliberately NOT bounds-checked the way `findExact`'s descent is: the
 *  `+ nodeSize` branch may legitimately land one past the end of a level when
 *  the key is greater than every separator, and `nodeAt` already clamps such
 *  a node to an empty read, leaving the position unchanged. `scanRange` then
 *  clamps the result into the leaf level itself. Rejecting here would turn a
 *  routine above-everything query into an error. */
async function findPartition(t: Tree, key: unknown): Promise<number> {
  let nodeIndex = 0
  for (let level = t.levels.length - 1; level >= 1; level--) {
    const items = await nodeAt(t, nodeIndex, level)
    if (items.length === 0) continue

    const hit = binarySearch(t.kind, items, key)
    if (hit.found) {
      nodeIndex = childIndex(items[hit.index]!)
    } else if (hit.index === 0) {
      nodeIndex = childIndex(items[0]!)
    } else if (hit.index >= items.length) {
      nodeIndex = childIndex(items[items.length - 1]!) + t.nodeSize
    } else {
      nodeIndex = childIndex(items[hit.index]!)
    }
  }
  return nodeIndex
}

/** Leaf scan with independently strict-or-inclusive bounds
 *  (stree.cpp:270-311).
 *
 *  THIS REPLACES THE RUST READER'S "range minus exact" LOWERING for Gt/Lt/Ne
 *  and must not be "fixed" back to it. That lowering subtracts FEATURE
 *  OFFSETS, but one feature can appear under several keys when its
 *  CityObjects carry different values of the indexed attribute: a feature
 *  holding both k and k' > k is returned by the range scan (via k') AND by
 *  find_exact(k) (via k), so the subtraction deletes a genuine match. Testing
 *  the bound's strictness at the leaf cannot make that mistake, and costs one
 *  traversal instead of two (docs/upstream-findings.md:130-145, "NOT FIXED
 *  upstream"). */
async function scanRange(
  t: Tree,
  lower: unknown,
  lowerStrict: boolean,
  upper: unknown,
  upperStrict: boolean,
): Promise<SearchResultItem[]> {
  const lu = compareKeys(t.kind, lower, upper)
  if (lu > 0) return []
  if (lu === 0 && (lowerStrict || upperStrict)) return []

  const lowerIdx = await findPartition(t, lower)
  const upperIdx = await findPartition(t, upper)

  const start = Math.max(lowerIdx, leafStart(t))

  // WIDENED BY ONE EXTRA NODE versus the reference's `upperIdx + nodeSize`.
  // findPartition descends LEFT on an exact hit, so when `upper` is itself a
  // separator key its matching leaf entry sits at exactly
  // `upperIdx + nodeSize` -- one PAST an un-widened, exclusive scan end, and
  // is silently dropped. Widening is safe because the per-key filter below
  // rejects out-of-range keys, and costs at most one extra node read
  // (stree.cpp:282-292, docs/upstream-findings.md:101).
  const end = Math.min(upperIdx + 2 * t.nodeSize, leafEnd(t))

  const out: SearchResultItem[] = []
  let cur = start
  while (cur < end) {
    const nodeEnd = Math.min(cur + t.nodeSize, end)
    const items = await readEntries(t.reader, t.indexBegin, t.kind, cur, nodeEnd, t.opts)
    for (let i = 0; i < items.length; i++) {
      const cl = compareKeys(t.kind, items[i]!.key, lower)
      const cu = compareKeys(t.kind, items[i]!.key, upper)
      if (lowerStrict ? cl > 0 : cl >= 0) {
        if (upperStrict ? cu < 0 : cu <= 0) {
          await emitOffset(
            items[i]!.offset,
            cur + i - leafStart(t),
            t.reader,
            t.payloadBegin,
            t.payloadSize,
            out,
            t.opts,
          )
        }
      }
    }
    cur = nodeEnd
  }
  return out
}

/** Runs one condition against one column's index blob and returns candidate
 *  feature offsets, relative to the features section, in traversal order (the
 *  caller sorts and de-duplicates).
 *
 *  `value` must already be in this kind's decoded representation -- a number
 *  for `f64`, a `bigint` for `u64`, a `Uint8Array` for the string kinds, and
 *  so on. `query.ts`'s `toKeyValue` is what performs that coercion from a
 *  caller's `unknown`; calling this directly with a raw JS `5` on a `u64`
 *  column would compare a number against a bigint and match nothing.
 *
 *  `opts` is threaded into EVERY read, node and payload alike: a signal that
 *  only lived on the facade would cancel nothing, because the traversal is
 *  where the in-flight requests are. */
export async function searchStree(
  reader: RangeReader,
  info: AttrIndexInfo,
  kind: KeyKind,
  op: Operator,
  value: unknown,
  opts?: ReadOpts,
): Promise<SearchResultItem[]> {
  const numNodes = streeNumNodes(info.numUniqueItems, info.branchingFactor)
  const treeBytes = numNodes * entrySize(kind)
  if (treeBytes > info.length) {
    throw new FcbError(
      ErrorCode.AttributeIndexNotFound,
      'attribute index node region exceeds its declared length',
    )
  }

  const t: Tree = {
    reader,
    indexBegin: info.begin,
    payloadBegin: info.begin + treeBytes,
    payloadSize: info.length - treeBytes,
    kind,
    nodeSize: info.branchingFactor - 1,
    levels: generateStreeLevelBounds(info.numUniqueItems, info.branchingFactor),
    ...(opts === undefined ? {} : { opts }),
  }

  // STRICTNESS IS INVERTED FOR STRING KINDS, AND THAT IS NOT A BUG.
  // Fixed-width string keys are TRUNCATED (50 or 100 bytes), so ordering
  // after the truncation point is invisible to the index: two values sharing
  // a 50-byte prefix compare EQUAL here but may order either way in full.
  // Every string comparison is therefore widened to keep the equal-prefix
  // band alive, and Task 15's post-filter applies the real operator to the
  // untruncated attribute value. Using strict bounds here instead would
  // discard candidates BEFORE they could be verified -- a false negative no
  // post-filter can recover. `Ne` in particular must be a FULL scan:
  // excluding the prefix matches would drop features whose value merely
  // shares a prefix with the query (stree.cpp:371-400).
  const isString = needsPostFilter(kind)

  switch (op) {
    case 'Eq':
      // For string kinds these are CANDIDATES, not answers: equal-prefix
      // collisions land here too, and Task 15's post-filter narrows them.
      return findExact(t, value)
    case 'Ge':
      return scanRange(t, value, false, keyMax(kind), false)
    case 'Le':
      return scanRange(t, keyMin(kind), false, value, false)
    case 'Gt':
      return scanRange(t, value, !isString, keyMax(kind), false)
    case 'Lt':
      return scanRange(t, keyMin(kind), false, value, !isString)
    case 'Ne': {
      if (isString) return scanRange(t, keyMin(kind), false, keyMax(kind), false)
      // TWO HALF-OPEN SCANS, not a full scan minus the equal set: subtraction
      // on feature offsets is wrong when one feature carries several values
      // of the attribute (see scanRange's docstring).
      const lo = await scanRange(t, keyMin(kind), false, value, true)
      const hi = await scanRange(t, value, true, keyMax(kind), false)
      return [...lo, ...hi]
    }
    default: {
      const exhaustive: never = op
      throw new FcbError(
        ErrorCode.QueryExecutionError,
        `unknown operator ${String(exhaustive)}`,
      )
    }
  }
}
