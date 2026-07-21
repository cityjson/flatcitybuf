/** One B+tree node entry -- ports `Entry` / `read_entries`
 *  (src/cpp/src/stree.cpp:59-102), themselves a port of
 *  `static_btree/entry.rs:25-52`.
 *
 *  An entry is a fixed-width key followed by a `u64` little-endian offset,
 *  and NOTHING ELSE: no length prefix, no padding. That is what lets the
 *  whole index be one flat array that a search strides over by index, and it
 *  is why the key kind alone determines the stride.
 *
 *  The offset's meaning depends on the level. On an internal level it is a
 *  NODE INDEX -- the first child of the group this separator covers. On the
 *  leaf level it is either a feature offset or, when the MSB is set, a
 *  payload reference (see payload.ts). It stays a `bigint` here for exactly
 *  that reason: converting to Number before the tag has been tested loses the
 *  low bits of a tagged offset. */
import { ErrorCode, FcbError } from '../errors.js'
import type { RangeReader, ReadOpts } from '../io/range-reader.js'
import { readU64 } from '../le.js'
import { decodeKey, keySize } from './key.js'
import type { KeyKind } from './key.js'

export interface Entry {
  key: unknown
  /** Raw, still-tagged on the leaf level. Never `Number()` this blindly. */
  offset: bigint
}

/** Key bytes plus the 8-byte offset. */
export function entrySize(kind: KeyKind): number {
  return keySize(kind) + 8
}

/** Reads entries `[first, last)` of the flat node array. Returns an empty
 *  array for an empty range without issuing a read -- `RangeReader.read`
 *  tolerates a zero length, but not issuing the request at all keeps the
 *  request log of a traversal equal to the number of nodes it really
 *  visited. */
export async function readEntries(
  reader: RangeReader,
  indexBegin: number,
  kind: KeyKind,
  first: number,
  last: number,
  opts?: ReadOpts,
): Promise<Entry[]> {
  if (last <= first) return []

  const size = entrySize(kind)
  const length = (last - first) * size
  const block = await reader.read(indexBegin + first * size, length, opts)
  if (block.length < length) {
    throw new FcbError(ErrorCode.AttributeIndexNotFound, 'truncated attribute index node')
  }

  const dv = new DataView(block.buffer, block.byteOffset, block.byteLength)
  const ksz = keySize(kind)
  const out: Entry[] = []
  for (let i = 0; i < last - first; i++) {
    const base = i * size
    out.push({ key: decodeKey(kind, dv, base), offset: readU64(dv, base + ksz) })
  }
  return out
}
