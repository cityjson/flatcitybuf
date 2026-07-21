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

/** Decodes the node at `index` within a block whose first byte is node
 *  `blockStart`. Goes through `le.ts` -- a raw DataView getter defaults to
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
