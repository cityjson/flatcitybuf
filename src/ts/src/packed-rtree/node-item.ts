/** The 40-byte R-tree node struct and its predicates -- ports
 *  `fcb::NodeItem` (src/cpp/src/packed_rtree.cpp), itself a port of
 *  `fcb_core::packed_rtree::NodeItem` (src/rust/fcb_core/src/packed_rtree/
 *  mod.rs:23-150). */
import { readF64, readU64, toSafeNumber } from '../le.js'
import { NODE_ITEM_SIZE } from '../layout.js'

export { NODE_ITEM_SIZE }

/** One R-tree node, as stored: `{ f64 min_x, f64 min_y, f64 max_x, f64 max_y,
 *  u64 offset }`, all little-endian, 40 bytes, no padding
 *  (packed_rtree/mod.rs:23-33, :56-77).
 *
 *  `offset` HAS TWO MEANINGS and which one applies is decided by the level
 *  the node was read from, never by the node itself:
 *   * on an INTERNAL node it is the index of the node's FIRST CHILD within
 *     the flat node array (packed_rtree/mod.rs:385, :531);
 *   * on a LEAF node it is a byte offset RELATIVE TO `featureBegin`
 *     (writer/mod.rs:207-215).
 *  Reading a leaf's offset as a child index (or the reverse) traverses
 *  without erroring and returns wrong answers, so `search.ts` derives
 *  leaf-ness from the trusted level and not from the value. */
export interface NodeItem {
  minX: number
  minY: number
  maxX: number
  maxY: number
  offset: number
}

/** A query rectangle. Same four numbers as a NodeItem's box, named apart so
 *  a query is never accidentally used where a stored node is meant. */
export interface BBox {
  minX: number
  minY: number
  maxX: number
  maxY: number
}

/** Decodes the node at `slot` within a `block` that was fetched starting at
 *  some node boundary (`slot` is relative to that boundary, not an absolute
 *  node index). Goes through `le.ts` -- a raw DataView getter defaults to
 *  BIG-endian, and a byteswapped f64 bbox is still a finite f64, so the
 *  mistake would not surface as an error. */
export function decodeNodeItem(block: Uint8Array, slot: number): NodeItem {
  const at = slot * NODE_ITEM_SIZE
  const dv = new DataView(block.buffer, block.byteOffset, block.byteLength)
  return {
    minX: readF64(dv, at + 0),
    minY: readF64(dv, at + 8),
    maxX: readF64(dv, at + 16),
    maxY: readF64(dv, at + 24),
    offset: toSafeNumber(readU64(dv, at + 32), 'rtree node offset'),
  }
}

/** Transcribed operator-for-operator from `NodeItem::intersects`
 *  (packed_rtree/mod.rs:122-137). The comparisons are STRICT, so boxes that
 *  merely touch along an edge or at a corner DO intersect. An inclusive /
 *  exclusive slip here silently changes every query result. */
export function intersects(n: NodeItem, q: BBox): boolean {
  if (q.maxX < n.minX) return false
  if (q.maxY < n.minY) return false
  if (q.minX > n.maxX) return false
  if (q.minY > n.maxY) return false
  return true
}

/** `NodeItem::contains_point` (packed_rtree/mod.rs:139-141):
 *  `x >= min_x && x <= max_x && y >= min_y && y <= max_y`.
 *
 *  Provided for reference and pinned by `intersects` on the degenerate box
 *  `[x, y, x, y]`, which is algebraically the same predicate -- which is why
 *  `search.ts` lowers a point query to a degenerate bbox rather than carrying
 *  a second traversal. */
export function containsPoint(n: NodeItem, x: number, y: number): boolean {
  return x >= n.minX && x <= n.maxX && y >= n.minY && y <= n.maxY
}

/** Squared Euclidean distance from `(x, y)` to this node's bbox CENTROID
 *  (`NodeItem::centroid_distance_squared`, packed_rtree/mod.rs:143-150).
 *
 *  This is the FINAL score a nearest-centroid search assigns to a LEAF: the
 *  answer is the feature whose stored bbox centre is closest. It is NOT a
 *  lower bound on anything -- a centroid can be far from the query even when
 *  the box's nearest edge is close -- which is exactly why internal nodes are
 *  ordered by `minDistanceSquared` and only leaves use this. Squared, so no
 *  `sqrt` is ever taken; comparisons of squared distances order identically. */
export function centroidDistanceSquared(n: NodeItem, x: number, y: number): number {
  const cx = (n.minX + n.maxX) / 2
  const cy = (n.minY + n.maxY) / 2
  const dx = x - cx
  const dy = y - cy
  return dx * dx + dy * dy
}

/** Squared Euclidean distance from `(x, y)` to the NEAREST point of this
 *  node's bbox, or 0 when the point is inside it
 *  (`NodeItem::min_distance_squared`, packed_rtree/mod.rs:152-167).
 *
 *  This is the admissible LOWER BOUND that makes best-first search exact for
 *  the nearest-centroid problem: a child's box is contained in its parent's,
 *  and a leaf's centroid lies inside its own box, so a parent's min-distance
 *  can never exceed the centroid distance of any leaf beneath it. Ordering and
 *  pruning internal nodes by this value therefore never discards the true
 *  nearest centroid. Squared throughout -- no `sqrt`. */
export function minDistanceSquared(n: NodeItem, x: number, y: number): number {
  if (containsPoint(n, x, y)) return 0
  // Closest point on the box to (x, y): clamp each coordinate to the box.
  const closestX = Math.min(Math.max(x, n.minX), n.maxX)
  const closestY = Math.min(Math.max(y, n.minY), n.maxY)
  const dx = x - closestX
  const dy = y - closestY
  return dx * dx + dy * dy
}
