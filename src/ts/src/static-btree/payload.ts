/** Leaf-offset tagging and the payload section -- ports the `kPayloadTag` /
 *  `emit_offset` half of `src/cpp/src/stree.cpp` (:126-161, :331-347), itself
 *  a port of `static_btree/stree.rs:15-17`.
 *
 *  A leaf entry's 8-byte offset means one of two things, discriminated by its
 *  MOST SIGNIFICANT BIT:
 *   * clear -- the offset of the single feature holding this key, relative to
 *     the features section;
 *   * set   -- a reference into the PAYLOAD SECTION, where a `u32` count is
 *     followed by that many feature offsets. This is how one key shared by
 *     several features is stored.
 *
 *  THE TAG MUST BE TESTED AND STRIPPED IN BIGINT, BEFORE ANY `Number()`.
 *  `1 << 63` in JS is `-2147483648` (the shift operand is coerced to i32 and
 *  the count taken mod 32), so the tag cannot be written with a shift at all.
 *  And `Number(taggedOffset)` is not merely "big": at 2^63 the double spacing
 *  is 2048, so the low bits are ROUNDED, not preserved -- `Number(TAG | 1n)`
 *  is exactly `Number(TAG)`, while `Number(TAG | 12345n)` is neither
 *  `Number(TAG)` nor `Number(TAG) + 12345`. Converting first and masking
 *  afterwards silently corrupts every payload reference. */
import { ErrorCode, FcbError } from '../errors.js'
import type { RangeReader, ReadOpts } from '../io/range-reader.js'
import { readU32, readU64, toSafeNumber } from '../le.js'
import type { SearchResultItem } from '../packed-rtree/index.js'

/** The MSB of a leaf offset. A bigint literal, never `1 << 63`. */
export const PAYLOAD_TAG = 0x8000000000000000n

/** The low 63 bits: everything the tag is not. */
export const PAYLOAD_MASK = 0x7fffffffffffffffn

export function isTagged(offset: bigint): boolean {
  return (offset & PAYLOAD_TAG) !== 0n
}

/** The payload-section-relative offset carried by a tagged leaf entry. Still
 *  a bigint on the way out: the caller converts once it has been bounds
 *  checked against the section length. */
export function stripTag(offset: bigint): bigint {
  return offset & PAYLOAD_MASK
}

/** `u32` count, then that many `u64` feature offsets, all little-endian.
 *  Exported for its own sake because it is the one part of the payload format
 *  that can be checked without a file (stree.cpp:331-347). */
export function decodePayloadEntry(bytes: Uint8Array): number[] {
  if (bytes.length < 4) {
    throw new FcbError(ErrorCode.AttributeIndexNotFound, 'short payload entry')
  }
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const count = readU32(dv, 0)
  if (bytes.length < 4 + count * 8) {
    throw new FcbError(ErrorCode.AttributeIndexNotFound, 'truncated payload entry')
  }
  const out: number[] = []
  for (let i = 0; i < count; i++) {
    out.push(toSafeNumber(readU64(dv, 4 + i * 8), 'payload feature offset'))
  }
  return out
}

/** Turns one leaf entry's offset into the feature offsets it stands for,
 *  appending them to `out` with `index` as their key ordinal.
 *
 *  Reads the payload entry's 4-byte header first and only then its body, so a
 *  hostile `count` cannot make this allocate or request an arbitrary span:
 *  both the header and the full entry are bounds checked against the declared
 *  payload size before the body read is issued (stree.cpp:128-161). */
export async function emitOffset(
  offset: bigint,
  index: number,
  reader: RangeReader,
  payloadBegin: number,
  payloadSize: number,
  out: SearchResultItem[],
  opts?: ReadOpts,
): Promise<void> {
  if (!isTagged(offset)) {
    out.push({ offset: toSafeNumber(offset, 'feature offset'), index })
    return
  }

  const rel = toSafeNumber(stripTag(offset), 'payload offset')
  if (rel + 4 > payloadSize) {
    throw new FcbError(ErrorCode.AttributeIndexNotFound, 'payload reference out of range')
  }

  const head = await reader.read(payloadBegin + rel, 4, opts)
  const count = readU32(new DataView(head.buffer, head.byteOffset, head.byteLength), 0)
  const want = 4 + count * 8
  if (rel + want > payloadSize) {
    throw new FcbError(
      ErrorCode.AttributeIndexNotFound,
      'payload entry overruns its section',
    )
  }

  const body = await reader.read(payloadBegin + rel, want, opts)
  for (const featureOffset of decodePayloadEntry(body)) {
    out.push({ offset: featureOffset, index })
  }
}
