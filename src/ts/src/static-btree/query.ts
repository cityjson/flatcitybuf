/** Attribute query planning: column resolution, operand coercion, and the
 *  AND-intersection of several conditions -- ports `FcbReader::select_attr`'s
 *  planning half (src/cpp/src/reader.cpp:326-392). The post-filter half
 *  (reader.cpp:394-436) lives in ../post-filter.ts and is applied by
 *  `FcbReader.select`, deliberately NOT here; see `searchAttributes`.
 *
 *  ---------------------------------------------------------------------
 *  FOUR DELIBERATE DIVERGENCES FROM THE RUST READER, VISIBLE FROM HERE
 *  ---------------------------------------------------------------------
 *  These were documented in `key.ts` when the key layer landed, because the
 *  query API did not exist yet. This is that API: a caller who reads one
 *  docstring before writing a query should read them here.
 *
 *   1. `Byte` columns are treated as UNSIGNED `u8`. The writer stores `Byte`
 *      as `u8` and indexes it as `MemoryIndex<u8>`, but Rust's own reader
 *      decodes that index as `i8`, so for stored values > 127 it returns a
 *      negative number that was never written. This port matches the WRITER.
 *   2. `Json` and `Binary` columns are REJECTED here with
 *      `ErrorCode.UnsupportedColumnType`. Their index is a
 *      `FixedStringKey<100>` over a JSON or binary blob, so a hit means
 *      "the first 100 bytes of some serialisation collide" -- near-meaningless
 *      without a post-filter this port does not have for them. Rejecting is
 *      honest; returning candidates that look like answers is not.
 *   3. `f32`/`f64` range queries use `+Infinity` as their maximum, NOT NaN,
 *      even though `ordered_float` sorts NaN strictly above `+Infinity`.
 *      Consequence: `Ge`, `Gt` and `Ne` on a float column SILENTLY EXCLUDE
 *      NaN-keyed features. Deliberately lossy, for parity with Rust.
 *   4. `DateTime` range queries use epoch zero as their minimum, even though
 *      the wire format is a signed `i64` that round-trips pre-1970 instants
 *      fine. Consequence: `Le`, `Lt` and `Ne` on a datetime column are BLIND
 *      to pre-1970 timestamps. Also deliberately lossy, also for parity.
 *
 *  A fifth behaviour is not a divergence but a property callers of THIS
 *  function must know: for `String` columns the index is built over keys
 *  TRUNCATED to 50 bytes and zero-padded, so what `searchAttributes` returns
 *  is a CANDIDATE SET -- `Eq` over-returns, and `Gt`/`Lt`/`Ne` over-return
 *  deliberately (see stree.ts). `FcbReader.select` narrows it with
 *  `postFilterCandidates` (../post-filter.ts) before counting or paging;
 *  a caller who uses `searchAttributes` directly must do the same. */
import { ColumnType } from '../generated/column-type.js'
import { ErrorCode, FcbError } from '../errors.js'
import type { HeaderView } from '../header/index.js'
import type { RangeReader, ReadOpts } from '../io/range-reader.js'
import type { SearchResultItem } from '../packed-rtree/index.js'
import type { AttrCondition } from '../reader.js'
import { encodeKey, keyKindForColumn } from './key.js'
import type { DateTimeKey, KeyKind } from './key.js'
import { searchStree } from './stree.js'

function invalid(field: string, kind: KeyKind, value: unknown): FcbError {
  return new FcbError(
    ErrorCode.InvalidArgument,
    `condition on "${field}" (key kind ${kind}) cannot use value ${String(value)}`,
  )
}

function isDateTimeKey(v: unknown): v is DateTimeKey {
  return typeof v === 'object' && v !== null
    && typeof (v as DateTimeKey).seconds === 'bigint'
    && typeof (v as DateTimeKey).nanos === 'number'
}

/** Coerces a caller's `unknown` into THIS kind's decoded representation, the
 *  form `compareKeys` compares against keys read off a node.
 *
 *  The 64-bit kinds accept a `number` and widen it to `bigint`, because that
 *  is what a caller naturally writes (`{ field: 'h', operator: 'Gt', value: 5
 *  }`) and the on-disk column type is the writer's inference, not the
 *  caller's choice -- `h` holding 1, 5 and 9 is indexed as `u64`. A number
 *  that is not an integer is rejected rather than truncated: silently
 *  querying `5` for `5.5` returns a plausible, wrong answer.
 *
 *  String operands are encoded to their FIXED-WIDTH, zero-padded byte form
 *  immediately, so every comparison in the traversal is byte-vs-byte. */
function toKeyValue(kind: KeyKind, field: string, value: unknown): unknown {
  switch (kind) {
    case 'bool':
      if (typeof value !== 'boolean') throw invalid(field, kind, value)
      return value
    case 'u8':
    case 'i16':
    case 'u16':
    case 'i32':
    case 'u32':
      if (typeof value !== 'number' || !Number.isInteger(value)) {
        throw invalid(field, kind, value)
      }
      return value
    case 'f32':
    case 'f64':
      if (typeof value !== 'number') throw invalid(field, kind, value)
      return value
    case 'i64':
    case 'u64':
      if (typeof value === 'bigint') return value
      if (typeof value === 'number' && Number.isInteger(value)) return BigInt(value)
      throw invalid(field, kind, value)
    case 'datetime': {
      if (isDateTimeKey(value)) return value
      // A `Date` is millisecond-resolution, so its nanos are always a
      // multiple of 1e6; that is a lossless widening, not a truncation.
      if (value instanceof Date) {
        const ms = value.getTime()
        if (!Number.isFinite(ms)) throw invalid(field, kind, value)
        return {
          seconds: BigInt(Math.floor(ms / 1000)),
          nanos: ((ms % 1000) + 1000) % 1000 * 1_000_000,
        } satisfies DateTimeKey
      }
      throw invalid(field, kind, value)
    }
    case 'str50':
    case 'str100':
      if (value instanceof Uint8Array) return value
      if (typeof value !== 'string') throw invalid(field, kind, value)
      return encodeKey(kind, value)
    default: {
      const exhaustive: never = kind
      throw new FcbError(ErrorCode.InvalidArgument, `unknown key kind ${String(exhaustive)}`)
    }
  }
}

/** Sorted ascending by feature offset, with duplicates collapsed. Both halves
 *  are required, not tidiness: one feature can be reached through several
 *  keys (a payload entry, or two CityObjects with different values), and the
 *  intersection below is a sorted merge. Ascending offset is also the order
 *  the feature section should be read in. */
function normalise(hits: SearchResultItem[]): SearchResultItem[] {
  hits.sort((a, b) => a.offset - b.offset)
  const out: SearchResultItem[] = []
  for (const hit of hits) {
    if (out.length === 0 || out[out.length - 1]!.offset !== hit.offset) out.push(hit)
  }
  return out
}

/** Sorted-merge intersection on feature offset. */
export function intersectHits(
  a: readonly SearchResultItem[],
  b: readonly SearchResultItem[],
): SearchResultItem[] {
  const out: SearchResultItem[] = []
  let i = 0
  let j = 0
  while (i < a.length && j < b.length) {
    const l = a[i]!
    const r = b[j]!
    if (l.offset < r.offset) i++
    else if (l.offset > r.offset) j++
    else {
      out.push(l)
      i++
      j++
    }
  }
  return out
}

/** Runs every condition against its column's B+tree and AND-intersects the
 *  results, returning candidate feature offsets sorted ascending.
 *
 *  Conditions are evaluated SEQUENTIALLY with an early exit once the
 *  accumulator is empty: an impossible first condition costs one traversal,
 *  not one per condition.
 *
 *  Semantics are EXISTENTIAL over CityObjects, and that is why the result is
 *  a set of features rather than of (feature, object) pairs: a feature
 *  matches `Gt(1)` if ANY of its CityObjects carries an `h` greater than 1,
 *  even if another carries exactly 1. This is also why the operator is
 *  evaluated at the leaf instead of being lowered to "range minus exact" the
 *  way the Rust reader does -- see `scanRange` in stree.ts.
 *
 *  THE POST-FILTER PLUGS IN AFTER THIS FUNCTION RETURNS: for `String` columns
 *  (`needsPostFilter(kind)`) the hits are CANDIDATES, because the keys are
 *  truncated to 50 bytes and zero-padded. `postFilterCandidates`
 *  (../post-filter.ts) reads each candidate feature, decodes the real
 *  attribute value and re-applies `operator` to the untruncated value,
 *  dropping what does not survive; `FcbReader.select` runs it before
 *  `featuresCount` and paging. Nothing in this file should start doing that
 *  work: the traversal deliberately over-returns so the verifier has
 *  something to verify, and a bound tightened here is a false negative the
 *  verifier can never recover. */
export async function searchAttributes(
  reader: RangeReader,
  header: HeaderView,
  conditions: readonly AttrCondition[],
  opts?: ReadOpts,
): Promise<SearchResultItem[]> {
  if (conditions.length === 0) {
    throw new FcbError(ErrorCode.QueryExecutionError, 'empty attribute query')
  }

  let acc: SearchResultItem[] | undefined

  for (const cond of conditions) {
    const column = header.info.columns.find((c) => c.name === cond.field)
    if (column === undefined) {
      throw new FcbError(
        ErrorCode.AttributeIndexNotFound,
        `no such column: ${cond.field}`,
      )
    }

    // Divergence #2, checked BEFORE the index lookup: a Json or Binary
    // column is unsupported whether or not the writer happened to index it,
    // and "unsupported column type" is the honest answer either way.
    if (column.type === ColumnType.Json || column.type === ColumnType.Binary) {
      throw new FcbError(
        ErrorCode.UnsupportedColumnType,
        `column "${cond.field}" is Json or Binary; its index is a truncated blob and is not queryable`,
      )
    }

    const info = header.info.attributeIndices.find((a) => a.columnIndex === column.index)
    if (info === undefined) {
      throw new FcbError(
        ErrorCode.AttributeIndexNotFound,
        `column is not indexed: ${cond.field}`,
      )
    }

    const kind = keyKindForColumn(column.type)
    const value = toKeyValue(kind, cond.field, cond.value)
    const hits = normalise(
      await searchStree(reader, info, kind, cond.operator, value, opts),
    )

    acc = acc === undefined ? hits : intersectHits(acc, hits)
    if (acc.length === 0) break
  }

  return acc ?? []
}
