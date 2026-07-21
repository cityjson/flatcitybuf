/** Attribute blob decoding -- ports `fcb::decode_attributes`
 *  (src/cpp/src/attribute.cpp), itself a port of
 *  `fcb_core::reader::deserializer::decode_attributes` (Rust).
 *
 *  The wire form is a flat sequence of records with NO framing of their own:
 *
 *      u16 column_index | value | u16 column_index | value | ...
 *
 *  A value's width comes ENTIRELY from the column's declared type. That is
 *  why every failure here is fatal to the rest of the blob: an unknown column
 *  index has no known width, so the reader cannot skip it and resume -- it
 *  would resume mid-value and decode plausible garbage. During the C++ port
 *  this exact failure surfaced as column index 28777, which is ASCII "ip"
 *  read out of the middle of a string value. Throw, never guess. */
import { ErrorCode, FcbError } from '../errors.js'
import { ColumnType } from '../generated/column-type.js'
import type { ColumnInfo } from '../header/index.js'
import {
  readF32, readF64, readI32, readI64, readU16, readU32, readU64,
} from '../le.js'

export type JsonValue = null | boolean | number | string | JsonValue[] | { [k: string]: JsonValue }
export type AttrValue = number | bigint | string | boolean | Uint8Array | JsonValue | null

const utf8 = new TextDecoder('utf-8')

/** Bounds check before every read. `at + n` cannot overflow for any blob a
 *  FlatBuffers vector can hold, but the subtraction form is used anyway so
 *  the check stays correct for any `n`. */
function need(blob: Uint8Array, at: number, n: number, what: string): void {
  if (at > blob.length || blob.length - at < n) {
    throw new FcbError(
      ErrorCode.InvalidAttributeValue,
      `truncated attribute blob reading ${what}`,
    )
  }
}

/** Length-prefixed payload shared by String, DateTime, Json and Binary:
 *  a u32 LE byte length followed by that many bytes. Returns the payload and
 *  the position just past it. */
function readLengthPrefixed(
  blob: Uint8Array, dv: DataView, at: number, what: string,
): { body: Uint8Array; next: number } {
  need(blob, at, 4, `${what} length`)
  const len = readU32(dv, at)
  const bodyAt = at + 4
  need(blob, bodyAt, len, `${what} body`)
  return { body: blob.subarray(bodyAt, bodyAt + len), next: bodyAt + len }
}

/** Decodes one attribute blob against the schema that governs it.
 *
 *  The schema is the CALLER'S choice and is load-bearing: `CityObject.columns`
 *  overrides `Header.columns` whenever the object declares it, which is the
 *  normal case rather than the exception. See feature/index.ts, which resolves
 *  that per object before calling in here.
 *
 *  Type policy, where this port makes a deliberate choice:
 *   * Long/ULong always decode to `bigint`, never to `number`. A
 *     data-dependent type would make sorting and serialization change
 *     behaviour the first day a value above 2**53 appears.
 *   * Byte decodes as an UNSIGNED u8. The writer stores u8; the Rust reader
 *     decodes i8 (deserializer.rs:405), so values above 127 come back
 *     negative there. This port matches the writer.
 *   * Byte/UByte/Binary are decoded at all, unlike the C++ reader, which
 *     rejects them outright (attribute.cpp:145-156). The writer emits them.
 *   * DateTime is a length-prefixed STRING on the wire (ISO-8601), not a
 *     packed instant, and stays a string here -- a JS `Date` cannot hold the
 *     nanoseconds the B+tree key form carries, so converting would lose data.
 *
 *  Duplicate column indices in the schema resolve to the FIRST entry, matching
 *  attribute.cpp's `unordered_map::emplace`. Duplicate KEYS in the blob
 *  resolve to the last record written, which is what an object assignment
 *  does naturally. */
export function decodeAttributes(
  blob: Uint8Array,
  schema: readonly ColumnInfo[],
): Record<string, AttrValue> {
  const out: Record<string, AttrValue> = {}
  if (blob.length === 0) return out

  const byIndex = new Map<number, ColumnInfo>()
  for (const c of schema) {
    if (!byIndex.has(c.index)) byIndex.set(c.index, c)
  }

  const dv = new DataView(blob.buffer, blob.byteOffset, blob.byteLength)
  let at = 0
  while (at < blob.length) {
    need(blob, at, 2, 'column index')
    const columnIndex = readU16(dv, at)
    at += 2

    const col = byIndex.get(columnIndex)
    if (col === undefined) {
      throw new FcbError(
        ErrorCode.InvalidAttributeValue,
        `attribute references unknown column index ${columnIndex}`,
      )
    }

    let value: AttrValue
    switch (col.type) {
      case ColumnType.Bool:
        need(blob, at, 1, 'Bool')
        value = blob[at]! !== 0
        at += 1
        break
      case ColumnType.Byte:
      case ColumnType.UByte:
        need(blob, at, 1, 'Byte')
        value = blob[at]!
        at += 1
        break
      case ColumnType.Short:
        need(blob, at, 2, 'Short')
        // No readI16 in le.ts and no reason to add one for a single call
        // site: sign-extend the u16 explicitly rather than reach for a raw
        // DataView getter, which only le.ts may do.
        value = (readU16(dv, at) << 16) >> 16
        at += 2
        break
      case ColumnType.UShort:
        need(blob, at, 2, 'UShort')
        value = readU16(dv, at)
        at += 2
        break
      case ColumnType.Int:
        need(blob, at, 4, 'Int')
        value = readI32(dv, at)
        at += 4
        break
      case ColumnType.UInt:
        need(blob, at, 4, 'UInt')
        value = readU32(dv, at)
        at += 4
        break
      case ColumnType.Long:
        need(blob, at, 8, 'Long')
        value = readI64(dv, at)
        at += 8
        break
      case ColumnType.ULong:
        need(blob, at, 8, 'ULong')
        value = readU64(dv, at)
        at += 8
        break
      case ColumnType.Float:
        need(blob, at, 4, 'Float')
        value = readF32(dv, at)
        at += 4
        break
      case ColumnType.Double:
        need(blob, at, 8, 'Double')
        value = readF64(dv, at)
        at += 8
        break
      case ColumnType.String:
      case ColumnType.DateTime: {
        const { body, next } = readLengthPrefixed(blob, dv, at, 'string')
        value = utf8.decode(body)
        at = next
        break
      }
      case ColumnType.Json: {
        const { body, next } = readLengthPrefixed(blob, dv, at, 'json')
        const text = utf8.decode(body)
        try {
          value = JSON.parse(text) as JsonValue
        } catch {
          // The Rust reader unwraps this parse (deserializer.rs:396), i.e.
          // panics; a Json column whose payload is not JSON is a corrupt
          // file, so fail loudly rather than smuggle the raw text through
          // under a type callers will not expect.
          throw new FcbError(
            ErrorCode.InvalidAttributeValue,
            `column '${col.name}' is Json but its value does not parse`,
          )
        }
        at = next
        break
      }
      case ColumnType.Binary: {
        const { body, next } = readLengthPrefixed(blob, dv, at, 'binary')
        // Copied, not aliased: the caller's blob is a view into the feature
        // buffer, and a Uint8Array handed out from here must not keep the
        // whole feature alive or expose bytes past its own value.
        value = body.slice()
        at = next
        break
      }
      default:
        // An unknown tag has no known width, so nothing after it is
        // readable. The Rust reader stops and returns what it has
        // (deserializer.rs:433); this port throws, because silently
        // truncating an attribute map is indistinguishable from an object
        // that genuinely had fewer attributes.
        throw new FcbError(
          ErrorCode.UnsupportedColumnType,
          `column '${col.name}' has unknown column type ${col.type}`,
        )
    }

    out[col.name] = value
  }
  return out
}
