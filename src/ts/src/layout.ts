/** File layout arithmetic: magic bytes, header size guard, and section
 *  offsets. Ported directly from `src/cpp/src/layout.cpp` and
 *  `src/cpp/include/fcb/layout.hpp`, which are themselves the conformant
 *  port of the Rust format. See
 *  docs/superpowers/plans/2026-07-19-native-cpp-core.md, "File layout" and
 *  "Packed R-tree", for the formula table and citations into the Rust
 *  source (packed_rtree/mod.rs, reader/mod.rs, http_reader/mod.rs). */
import { ErrorCode, FcbError } from './errors.js'

/** {'f','c','b',0x01,'f','c','b',0x00} -- const_vars.rs:5, layout.hpp:10. */
export const MAGIC_SIZE = 8
/** NodeItem: 4 LE f64 (min_x, min_y, max_x, max_y) + 1 LE u64 offset, no
 *  padding, 40 bytes -- packed_rtree/mod.rs:23-33, :56-77, layout.hpp:15. */
export const NODE_ITEM_SIZE = 40
/** packed_rtree/mod.rs:325, layout.hpp:16. */
export const DEFAULT_NODE_SIZE = 16
/** 512 MiB -- const_vars.rs:8, layout.hpp:13 (kHeaderMaxBufferSize). */
export const MAX_HEADER_SIZE = 536870912
/** 256 MiB -- layout.hpp:20 (kMaxFeatureSize): hard ceiling on a single
 *  feature's byte length, enforced before allocating a buffer for it. */
export const MAX_FEATURE_SIZE = 268435456

/** layout.hpp:11 (kHeaderSizeSize): the 4-byte LE u32 size prefix itself. */
const HEADER_SIZE_SIZE = 4
/** layout.hpp:12 (kHeaderMinBufferSize). */
const HEADER_MIN_BUFFER_SIZE = 8
/** const_vars.rs:2, layout.hpp:14 (kVersion). */
const VERSION = 1

/** All section sizes here comfortably fit in a JS safe integer for any
 *  file that could exist (headers <= 512 MiB, node sizes <= u16, feature
 *  counts that make physical sense). The C++ port uses checked u64
 *  add/mul that THROW on wraparound rather than silently wrapping; a JS
 *  number never wraps, but it silently loses precision once a result
 *  exceeds 2**53. Checking the result against Number.isSafeInteger after
 *  every add/mul gives the same guarantee the C++ code has: throw rather
 *  than invent a layout, instead of the (different, worse) failure mode
 *  a wrapping or precision-losing computation would produce. */
function checkedAdd(a: number, b: number, what: string): number {
  const r = a + b
  if (!Number.isSafeInteger(r)) {
    throw new FcbError(ErrorCode.IllegalHeaderSize, `size arithmetic overflow (${what})`)
  }
  return r
}

function checkedMul(a: number, b: number, what: string): number {
  const r = a * b
  if (!Number.isSafeInteger(r)) {
    throw new FcbError(ErrorCode.IllegalHeaderSize, `size arithmetic overflow (${what})`)
  }
  return r
}

/** ceil(a / b) for the non-negative integers this module deals in. Callers
 *  guarantee b >= 2 before this is reached (rtreeIndexSize rejects a
 *  smaller node size up front), so division by zero cannot occur here. */
function ceilDiv(a: number, b: number): number {
  return Math.floor(a / b) + (a % b !== 0 ? 1 : 0)
}

/** Mirrors fcb::check_magic_bytes (layout.cpp:14-21), itself a mirror of
 *  fcb_core::check_magic_bytes (lib.rs:56-58). Compares only bytes [0,3)
 *  and [4,7); byte 3 must be <= VERSION (a forward-compat rejection, not
 *  an equality check, so older readers on newer-version files still fail
 *  loudly); byte 7 is written as 0 but never validated. */
export function checkMagicBytes(b: Uint8Array): boolean {
  if (b.length < MAGIC_SIZE) return false
  // Length is checked above, so indices 0..7 are all in bounds despite the
  // `number | undefined` element type noUncheckedIndexedAccess gives us.
  if (b[0] !== 0x66 || b[1] !== 0x63 || b[2] !== 0x62) return false // 'f','c','b'
  if (b[4] !== 0x66 || b[5] !== 0x63 || b[6] !== 0x62) return false // 'f','c','b'
  return b[3]! <= VERSION
}

/** Mirrors fcb::rtree_index_size (layout.cpp:23-44), itself a mirror of
 *  PackedRTree::index_size (packed_rtree/mod.rs:879-898).
 *
 *  A node_size < 2 is a corrupt file, not something to clamp: Rust asserts
 *  node_size >= 2 (packed_rtree/mod.rs:879), so 0 or 1 here means "reject
 *  rather than clamp, so we never invent a layout." (layout.cpp:25-29).
 *  The separate meaning "0 means no spatial index" belongs to computeLayout,
 *  which never calls this function with a node size of 0.
 *
 *  The loop DIVIDES FIRST, then tests n === 1 -- so a single item still
 *  produces a leaf node AND a root node (2 nodes, 80 bytes), not 1 node. */
export function rtreeIndexSize(numItems: number, nodeSize: number): number {
  if (nodeSize < 2) {
    throw new FcbError(ErrorCode.IllegalHeaderSize, `invalid index_node_size: ${nodeSize}`)
  }
  if (numItems === 0) {
    // The loop below would never terminate: ceilDiv(0, nodeSize) is 0
    // forever, so n never reaches 1.
    throw new FcbError(ErrorCode.IllegalHeaderSize, 'rtree_index_size requires num_items > 0')
  }
  let n = numItems
  let numNodes = n
  for (;;) {
    n = ceilDiv(n, nodeSize)
    numNodes = checkedAdd(numNodes, n, 'rtree num_nodes')
    if (n === 1) break
  }
  return checkedMul(numNodes, NODE_ITEM_SIZE, 'rtree index size')
}

/** Byte offsets of each section. Nothing in the file records these -- they
 *  must be computed, and an off-by-one silently corrupts everything after.
 *  Mirrors fcb::FileLayout (layout.hpp:32-39). */
export interface FileLayout {
  headerLen: number
  rtreeBegin: number
  rtreeSize: number
  attrIndexBegin: number
  attrIndexSize: number
  featureBegin: number
}

/** Mirrors fcb::compute_layout (layout.cpp:46-68). Sections are packed
 *  back-to-back with no padding or alignment (writer/mod.rs:266-271, and
 *  see the Format Reference's note that the spec's alignment claim is
 *  false); this function only computes offsets, it never reads any bytes. */
export function computeLayout(opts: {
  headerSize: number
  featuresCount: number
  indexNodeSize: number
  attrIndexSize: number
}): FileLayout {
  const { headerSize, featuresCount, indexNodeSize, attrIndexSize } = opts

  if (headerSize < HEADER_MIN_BUFFER_SIZE || headerSize > MAX_HEADER_SIZE) {
    throw new FcbError(ErrorCode.IllegalHeaderSize, `illegal header size: ${headerSize}`)
  }

  const headerLen = MAGIC_SIZE + HEADER_SIZE_SIZE + headerSize
  const rtreeBegin = headerLen
  // index_node_size == 0 means "no spatial index" and is legal; any other
  // value below 2 is corrupt and rtreeIndexSize rejects it.
  const rtreeSize = indexNodeSize === 0 || featuresCount === 0
    ? 0
    : rtreeIndexSize(featuresCount, indexNodeSize)
  const attrIndexBegin = checkedAdd(rtreeBegin, rtreeSize, 'attr_index_begin')
  const featureBegin = checkedAdd(attrIndexBegin, attrIndexSize, 'feature_begin')

  return { headerLen, rtreeBegin, rtreeSize, attrIndexBegin, attrIndexSize, featureBegin }
}

/** Mirrors fcb::validate_layout_against_size (layout.cpp:70-77). Call this
 *  immediately after computeLayout, before issuing any index read. */
export function validateLayoutAgainstSize(layout: FileLayout, totalSize: number): void {
  if (layout.featureBegin > totalSize) {
    throw new FcbError(
      ErrorCode.IllegalHeaderSize,
      `sections extend past end of file: feature_begin=${layout.featureBegin} total_size=${totalSize}`,
    )
  }
}
