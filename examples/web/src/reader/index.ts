// src/reader/index.ts
import {
  type AttrCondition, type ColumnInfo, ColumnType, FcbError, FcbReader,
  type Feature, type HeaderView,
} from '@cityjson/flatcitybuf'
import { type CrsStatus, resolveCrs } from '../crs/index'

export interface QueryableColumn {
  name: string
  type: ColumnType
  typeName: string
}

export interface HeaderModel {
  version: string
  featuresCount: number
  crs: CrsStatus
  extent?: [number, number, number, number, number, number]
  columns: ColumnInfo[]
  queryable: QueryableColumn[]
}

export function columnTypeName(type: ColumnType): string {
  return ColumnType[type] ?? `Unknown(${type})`
}

const QUERYABLE_TYPES = new Set<string>([
  'Bool', 'Byte', 'UByte', 'Short', 'UShort', 'Int', 'UInt',
  'Long', 'ULong', 'Float', 'Double', 'DateTime', 'String',
])

/** Only indexed, non-JSON/Binary columns can be queried (static-btree). Map
 *  the header's attribute indices to their columns and keep the supported
 *  types. */
export function headerModel(header: HeaderView): HeaderModel {
  const info = header.info
  const byIndex = new Map(info.columns.map((c) => [c.index, c]))
  const queryable: QueryableColumn[] = []
  for (const ai of info.attributeIndices) {
    const col = byIndex.get(ai.columnIndex)
    if (col === undefined) continue
    const typeName = columnTypeName(col.type)
    if (!QUERYABLE_TYPES.has(typeName)) continue
    queryable.push({ name: col.name, type: col.type, typeName })
  }
  return {
    version: info.version,
    featuresCount: info.featuresCount,
    crs: resolveCrs(info.referenceSystem),
    extent: info.geographicalExtent,
    columns: info.columns,
    queryable,
  }
}

/** Inclusive per-type bounds for the fixed-width integer ColumnTypes. Values
 *  outside these wrap silently if written through unchecked, since the
 *  underlying encode is a fixed-width truncation, not a checked cast. */
const INT_BOUNDS: Record<string, [number, number]> = {
  Byte: [-128, 127],
  UByte: [0, 255],
  Short: [-32768, 32767],
  UShort: [0, 65535],
  Int: [-2147483648, 2147483647],
  UInt: [0, 4294967295],
}
const BIGINT_BOUNDS: Record<string, [bigint, bigint]> = {
  Long: [-(2n ** 63n), 2n ** 63n - 1n],
  ULong: [0n, 2n ** 64n - 1n],
}

/** Coerces raw text into the type `select`'s `where` expects. Ported from the
 *  old demo; Json/Binary (and any non-queryable type) are rejected. */
export function coerceAttrValue(column: ColumnInfo, raw: string): unknown {
  const typeName = columnTypeName(column.type)
  switch (typeName) {
    case 'Bool':
      if (raw === 'true') return true
      if (raw === 'false') return false
      throw new Error(`"${raw}" is not "true" or "false"`)
    case 'Byte': case 'UByte': case 'Short': case 'UShort':
    case 'Int': case 'UInt': {
      const n = Number(raw)
      if (!Number.isInteger(n)) throw new Error(`"${raw}" is not an integer`)
      const [min, max] = INT_BOUNDS[typeName]
      if (n < min || n > max) {
        throw new Error(`"${raw}" out of range for ${typeName}`)
      }
      return n
    }
    case 'Long': case 'ULong': {
      const n = BigInt(raw)
      const [min, max] = BIGINT_BOUNDS[typeName]
      if (n < min || n > max) {
        throw new Error(`"${raw}" out of range for ${typeName}`)
      }
      return n
    }
    case 'Float': case 'Double': {
      const n = Number(raw)
      if (Number.isNaN(n)) throw new Error(`"${raw}" is not a number`)
      return n
    }
    case 'DateTime': {
      const d = new Date(raw)
      if (Number.isNaN(d.getTime())) throw new Error(`"${raw}" is not a valid date`)
      return d
    }
    case 'String':
      return raw
    default:
      throw new Error(
        `column "${column.name}" (${columnTypeName(column.type)}) is not queryable`,
      )
  }
}

export interface QuerySpec {
  bboxSource?: [number, number, number, number]
  where?: AttrCondition[]
  limit: number
  offset: number
}

/** Runs one page of a query and drains its cursor. `total` is the cursor's
 *  match count (every match, not just this page). */
export async function runQuery(
  reader: FcbReader, spec: QuerySpec,
): Promise<{ features: Feature[]; total: number | undefined }> {
  const cursor = await reader.select({
    spatial: spec.bboxSource
      ? { kind: 'bbox', value: spec.bboxSource }
      : undefined,
    where: spec.where && spec.where.length > 0 ? spec.where : undefined,
    limit: spec.limit,
    offset: spec.offset,
  })
  const features: Feature[] = []
  for await (const f of cursor) features.push(f)
  return { features, total: cursor.featuresCount ?? features.length }
}

export async function openFromUrl(url: string): Promise<FcbReader> {
  return FcbReader.fromUrl(url)
}
export async function openFromBlob(blob: Blob): Promise<FcbReader> {
  return FcbReader.fromBlob(blob)
}

export function describeError(err: unknown): string {
  if (err instanceof FcbError) return `${err.code}: ${err.message}`
  if (err instanceof Error) return err.message
  return String(err)
}
