/** Header parsing: file preamble validation, layout computation, and
 *  FileInfo construction -- ports `fcb::read_header`
 *  (src/cpp/src/header.cpp), itself a port of `fcb_core::header::read_header`
 *  (Rust). This is the first module in this port that reads a real `.fcb`
 *  file end to end: Task 8 (feature scan), Task 12 (R-tree) and Task 14
 *  (B+tree) all locate their sections from the `FileLayout` returned here. */
import * as flatbuffers from 'flatbuffers'
import { ErrorCode, FcbError } from '../errors.js'
import { Header } from '../generated/header.js'
import type { RangeReader } from '../io/range-reader.js'
import {
  MAGIC_SIZE, MAX_HEADER_SIZE, checkMagicBytes, computeLayout, validateLayoutAgainstSize,
} from '../layout.js'
import type { FileLayout } from '../layout.js'
import { readU32, toSafeNumber } from '../le.js'
import { collectAttrIndices } from './attribute-index.js'
import { buildFileInfo } from './file-info.js'
import type { FileInfo } from './file-info.js'

export type { ColumnInfo, FileInfo } from './file-info.js'
export type { AttrIndexInfo } from './attribute-index.js'

/** The 4-byte LE u32 size prefix itself -- layout.hpp's kHeaderSizeSize. */
const HEADER_SIZE_SIZE = 4

export interface HeaderView {
  info: FileInfo
  /** The generated FlatBuffers accessor, for callers that need a field this
   *  port's FileInfo does not surface (e.g. the point-of-contact fields). */
  raw: Header
  layout: FileLayout
}

/** Reads and validates the file preamble and header, in this order:
 *  1. Read the first 8 bytes and validate the magic.
 *  2. Read the 4-byte size prefix and validate it against MAX_HEADER_SIZE
 *     and the file's total size.
 *  3. Read `[8, 12 + headerSize)` -- the prefix INCLUDED -- and hand that
 *     slice to `Header.getSizePrefixedRootAsHeader`.
 *  4. Build `FileInfo` off the parsed header.
 *  5. Compute the layout with `computeLayout` and validate it with
 *     `validateLayoutAgainstSize`.
 *  6. Fill in each attribute index's absolute `begin` offset. */
export async function readHeader(reader: RangeReader): Promise<HeaderView> {
  const magic = await reader.read(0, MAGIC_SIZE)
  if (!checkMagicBytes(magic)) {
    throw new FcbError(ErrorCode.MissingMagicBytes, 'not a FlatCityBuf file')
  }

  const sizePrefix = await reader.read(MAGIC_SIZE, HEADER_SIZE_SIZE)
  const headerSize = readU32(
    new DataView(sizePrefix.buffer, sizePrefix.byteOffset, sizePrefix.byteLength),
    0,
  )
  if (headerSize > MAX_HEADER_SIZE) {
    throw new FcbError(ErrorCode.IllegalHeaderSize, `illegal header size: ${headerSize}`)
  }
  const want = HEADER_SIZE_SIZE + headerSize
  if (MAGIC_SIZE + want > reader.size()) {
    throw new FcbError(ErrorCode.IllegalHeaderSize, 'truncated before end of header')
  }

  // The buffer handed to FlatBuffers MUST include the 4-byte size prefix:
  // `getSizePrefixedRootAsHeader` skips over it internally when locating the
  // root table, per Task 3's finding pinned in test/generated.test.ts.
  // `reader.read` returns a `subarray` of the reader's own internal buffer,
  // not a copy (io/range-reader.ts's BufferedRangeReader documents this
  // contract). `hdr`/`bb` below outlive this function -- they are returned
  // in `HeaderView.raw` and back every lazily-read FlatBuffers field
  // (strings, columns, ...) that a caller may touch long after this read.
  // Today that's safe only because BufferedRangeReader.read happens to
  // reassign its buffer on a miss rather than mutate it in place -- an
  // implementation detail, not part of the contract. A reader that reuses a
  // fixed buffer (e.g. Task 11's HTTP reader) would silently corrupt every
  // HeaderView already handed out. Copy the bytes so `bb`'s backing store is
  // ours alone.
  const raw = (await reader.read(MAGIC_SIZE, want)).slice()
  const bb = new flatbuffers.ByteBuffer(raw)
  const hdr = Header.getSizePrefixedRootAsHeader(bb)

  const { indices, totalSize: attrIndexSize } = collectAttrIndices(hdr)
  const featuresCount = toSafeNumber(hdr.featuresCount(), 'featuresCount')

  const layout = computeLayout({
    headerSize,
    featuresCount,
    indexNodeSize: hdr.indexNodeSize(),
    attrIndexSize,
  })
  validateLayoutAgainstSize(layout, reader.size())

  // `begin` is cumulative: attrIndexBegin + the sum of every preceding
  // entry's length, walked in ascending column-index order (collectAttrIndices
  // already sorted `indices` that way).
  let cursor = layout.attrIndexBegin
  for (const ai of indices) {
    ai.begin = cursor
    cursor += ai.length
  }

  const info = buildFileInfo(hdr, featuresCount, indices)

  return { info, raw: hdr, layout }
}
