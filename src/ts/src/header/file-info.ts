/** FileInfo, ColumnInfo, and the metadata-collection logic that reads them
 *  off a parsed `Header` -- ports `fcb::FileInfo`, `fcb::ColumnInfo` and
 *  `fill_columns`/`fill_metadata` (src/cpp/include/fcb/header.hpp,
 *  src/cpp/src/header.cpp). */
import type { Column } from '../generated/column.js'
import type { ColumnType } from '../generated/column-type.js'
import type { Header } from '../generated/header.js'
import type { ReferenceSystem } from '../generated/reference-system.js'
import type { AttrIndexInfo } from './attribute-index.js'

/** One attribute column's schema, copied out of the header. */
export interface ColumnInfo {
  index: number
  name: string
  type: ColumnType
  nullable: boolean
}

/** Everything a caller normally wants from the header, as owned values. */
export interface FileInfo {
  /** 0 means UNKNOWN, not empty. Task 8's feature scan must not treat this
   *  as "the file has zero features" -- see no_count.fcb in the corpus. */
  featuresCount: number
  indexNodeSize: number
  columns: ColumnInfo[]
  /** Schema for SemanticObject.attributes, which is separate from the
   *  feature attribute schema (Header.semantic_columns in header.fbs). */
  semanticColumns: ColumnInfo[]
  geographicalExtent?: [number, number, number, number, number, number]
  /** `transform` is NOT required by the schema (src/fbs/header.fbs). An
   *  absent transform must stay distinguishable from a real zero transform
   *  -- hence this flag rather than defaulting scale/translate to zeros,
   *  which would make a missing transform look like one that collapses
   *  every coordinate to the origin. */
  hasTransform: boolean
  scale?: [number, number, number]
  translate?: [number, number, number]
  referenceSystem?: string
  version: string
  identifier?: string
  title?: string
  attributeIndices: AttrIndexInfo[]
}

function collectColumns(count: number, at: (i: number) => Column | null): ColumnInfo[] {
  const out: ColumnInfo[] = []
  for (let i = 0; i < count; i++) {
    const c = at(i)
    if (c === null) continue
    out.push({
      index: c.index(),
      name: c.name() ?? '',
      type: c.type(),
      nullable: c.nullable(),
    })
  }
  return out
}

/** Mirrors header.cpp's reference-system formatting exactly: default
 *  authority "EPSG" when absent, prefer the numeric `code` over
 *  `code_string` when both are present (code 0 means "not set", since 0 is
 *  not a valid EPSG code), and produce no string at all if neither is set. */
function buildReferenceSystem(rs: ReferenceSystem | null): string | undefined {
  if (rs === null) return undefined
  const authority = rs.authority() ?? 'EPSG'
  const code = rs.code()
  if (code !== 0) return `${authority}:${code}`
  const codeString = rs.codeString()
  if (codeString !== null) return `${authority}:${codeString}`
  return undefined
}

/** Builds `FileInfo` from a parsed `Header`. `featuresCount` is taken as a
 *  parameter rather than re-read here because header/index.ts already needs
 *  it (as a plain number) for computeLayout before this runs. */
export function buildFileInfo(
  hdr: Header,
  featuresCount: number,
  attributeIndices: AttrIndexInfo[],
): FileInfo {
  const columns = collectColumns(hdr.columnsLength(), (i) => hdr.columns(i))
  const semanticColumns = collectColumns(hdr.semanticColumnsLength(), (i) => hdr.semanticColumns(i))

  const transform = hdr.transform()
  const hasTransform = transform !== null
  let scale: [number, number, number] | undefined
  let translate: [number, number, number] | undefined
  if (transform !== null) {
    const s = transform.scale()!
    const t = transform.translate()!
    scale = [s.x(), s.y(), s.z()]
    translate = [t.x(), t.y(), t.z()]
  }

  const extent = hdr.geographicalExtent()
  let geographicalExtent: [number, number, number, number, number, number] | undefined
  if (extent !== null) {
    const min = extent.min()!
    const max = extent.max()!
    geographicalExtent = [min.x(), min.y(), min.z(), max.x(), max.y(), max.z()]
  }

  return {
    featuresCount,
    indexNodeSize: hdr.indexNodeSize(),
    columns,
    semanticColumns,
    geographicalExtent,
    hasTransform,
    scale,
    translate,
    referenceSystem: buildReferenceSystem(hdr.referenceSystem()),
    // Required by the schema (header.fbs marks `version` required, enforced
    // at write time by Header.endHeader's requiredField call), but the
    // generated getter is still typed string|null for a malformed file.
    // header.cpp silently defaults to "" in that case rather than throwing;
    // mirrored here for parity rather than inventing a stricter policy.
    version: hdr.version() ?? '',
    identifier: hdr.identifier() ?? undefined,
    title: hdr.title() ?? undefined,
    attributeIndices,
  }
}
