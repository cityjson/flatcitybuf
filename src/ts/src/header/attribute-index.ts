/** Attribute B+tree index metadata, decoded off `Header.attribute_index` --
 *  ports `fcb::AttrIndexInfo` and `collect_attr_indices`
 *  (src/cpp/include/fcb/header.hpp, src/cpp/src/header.cpp), themselves a
 *  port of the Rust writer's per-column B+tree blobs. */
import { ErrorCode, FcbError } from '../errors.js'
import type { Header } from '../generated/header.js'

export interface AttrIndexInfo {
  columnIndex: number
  /** Whole blob, INCLUDING its payload section -- not just the tree nodes. */
  length: number
  branchingFactor: number
  /** Unique KEYS, not features: a key shared by many features counts once. */
  numUniqueItems: number
  /** Absolute byte offset in the file. Filled in by header/index.ts once
   *  computeLayout has produced `attrIndexBegin` -- 0 here is a placeholder,
   *  not a claim that the blob starts at the file's first byte. */
  begin: number
}

/** Reads every 16-byte `AttributeIndex` struct off `hdr`, sorted ascending
 *  by column index (the order the writer concatenated the index blobs in --
 *  writer/mod.rs:190-195), and returns them alongside the summed byte size
 *  `computeLayout` needs for `attrIndexSize`. `begin` is left at 0 in every
 *  entry; the caller fills it in once the layout is known.
 *
 *  The struct's wire layout is `0:u16 index, 2:pad, 4:u32 length,
 *  8:u16 branching_factor, 10:pad, 12:u32 num_unique_items` -- field order
 *  in src/fbs/header.fbs forces 2 bytes of padding after each u16, making
 *  it 16 bytes rather than the 12 a naive stride would assume. The
 *  generated `AttributeIndex` accessors (src/generated/attribute-index.ts)
 *  already decode at these exact byte offsets, and `Header.attributeIndex`
 *  already strides by 16 -- so nothing here touches a raw DataView; this
 *  function only reads through those accessors. */
export function collectAttrIndices(hdr: Header): { indices: AttrIndexInfo[]; totalSize: number } {
  const n = hdr.attributeIndexLength()
  const indices: AttrIndexInfo[] = []
  for (let i = 0; i < n; i++) {
    const ai = hdr.attributeIndex(i)
    if (ai === null) continue
    indices.push({
      columnIndex: ai.index(),
      length: ai.length(),
      branchingFactor: ai.branchingFactor(),
      numUniqueItems: ai.numUniqueItems(),
      begin: 0,
    })
  }

  indices.sort((a, b) => a.columnIndex - b.columnIndex)

  // Two indexes claiming the same column makes the cumulative-offset walk
  // ambiguous: there is no way to know which blob comes first. Mirrors
  // header.cpp's duplicate check, ErrorCode included -- an odd code name for
  // a duplicate, kept for parity with the C++ port.
  for (let i = 1; i < indices.length; i++) {
    if (indices[i]!.columnIndex === indices[i - 1]!.columnIndex) {
      throw new FcbError(
        ErrorCode.AttributeIndexNotFound,
        `duplicate attribute index for column ${indices[i]!.columnIndex}`,
      )
    }
  }

  let totalSize = 0
  for (const ai of indices) totalSize += ai.length
  return { indices, totalSize }
}
