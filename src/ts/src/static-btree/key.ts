/** Attribute B+tree key encoding, comparison and sentinels -- ports
 *  `static_btree/key.rs` (Rust origin) and `src/cpp/src/key.cpp` (conformant
 *  C++ port). See Format Reference -> "Attribute B+tree" in
 *  `docs/superpowers/plans/2026-07-19-native-cpp-core.md`.
 *
 *  Every key kind is a FIXED-WIDTH, little-endian encoding so that entries in
 *  an on-disk node can be strided over without a length prefix. There is no
 *  sign-flip / total-order bit transform for floats: the bytes are the plain
 *  IEEE-754 bit pattern, and `ordered_float` ordering (NaN greatest, NaN ==
 *  NaN, -0.0 == +0.0) is applied only by `compareKeys`, after decode.
 *
 *  Deliberate divergences from the Rust reader, reproduced here on purpose
 *  (documented in the Format Reference under "Known divergences"; also
 *  followed by the C++ and Python ports):
 *
 *   1. `Byte` columns decode as UNSIGNED `u8`. The writer stores `Byte` as
 *      `u8` and builds its index as `MemoryIndex<u8>`, but Rust's OWN reader
 *      decodes that index as `i8` -- so for stored values > 127 Rust's reader
 *      returns a negative number that was never written. This port matches
 *      the WRITER, not Rust's reader bug.
 *   2. `Json`/`Binary` columns are classified here (`str100`) so the query
 *      layer (Task 14) can REJECT index queries against them, matching
 *      Rust's `UnsupportedColumnType`: they are `FixedStringKey<100>` over a
 *      JSON/binary blob, so index hits are near-meaningless without
 *      post-verification, and rejecting is honest.
 *   3. `f32`/`f64` `keyMax` is `+Infinity`, NOT NaN, even though `ordered_float`
 *      sorts NaN strictly greater than `+Infinity`. Consequence: range
 *      queries lowered against this sentinel (`Ge`, `Ne`, ...) SILENTLY
 *      EXCLUDE NaN-keyed features. This is deliberately lossy, reproduced so
 *      results match Rust -- not an oversight.
 *   4. `datetime` `keyMin` is epoch `{ seconds: 0n, nanos: 0 }`, even though
 *      the wire format is a signed `i64` and negative (pre-1970) timestamps
 *      round-trip fine through `encodeKey`/`decodeKey`. Consequence: `Le`/`Ne`
 *      range queries lowered against this sentinel are BLIND to pre-1970
 *      timestamps. Also deliberately lossy, reproduced for parity with Rust.
 */
import { ColumnType } from '../generated/column-type.js'
import { ErrorCode, FcbError } from '../errors.js'
import {
  readF32, readF64, readI16, readI32, readI64, readU16, readU32, readU64, readU8,
  writeF32, writeF64, writeI16, writeI32, writeI64, writeU16, writeU32, writeU64, writeU8,
} from '../le.js'

export type KeyKind =
  | 'u8' | 'i16' | 'u16' | 'i32' | 'u32' | 'i64' | 'u64'
  | 'f32' | 'f64' | 'bool' | 'datetime' | 'str50' | 'str100'

/** The on-disk `DateTime` key shape: `i64 LE` UNIX seconds, then `u32 LE`
 *  subsecond nanos -- 12 bytes total, never a JS `Date` (which cannot hold
 *  nanosecond precision and cannot represent the sentinel year 9999 used by
 *  `keyMax`). */
export interface DateTimeKey {
  seconds: bigint
  nanos: number
}

const utf8Encoder = new TextEncoder()

const KEY_SIZES: Record<KeyKind, number> = {
  u8: 1,
  i16: 2,
  u16: 2,
  i32: 4,
  u32: 4,
  i64: 8,
  u64: 8,
  f32: 4,
  f64: 8,
  bool: 1,
  datetime: 12,
  str50: 50,
  str100: 100,
}

export function keySize(kind: KeyKind): number {
  return KEY_SIZES[kind]
}

/** Column type -> key kind, exactly as the WRITER emits it
 *  (`writer/attr_index.rs:240`, `:272`, `:288`). Uses the `ColumnType` enum,
 *  never a numeric literal: `src/fbs/header.fbs:9-26` numbers `Byte=0 ...
 *  String=11, Json=12, DateTime=13, Binary=14`, and an off-by-one here
 *  silently classifies the wrong column. `str20`/`i8` are defined upstream
 *  but never produced by the writer, so neither appears in `KeyKind`. */
export function keyKindForColumn(t: ColumnType): KeyKind {
  switch (t) {
    case ColumnType.Bool: return 'bool'
    case ColumnType.Byte: return 'u8' // divergence #1: writer stores u8, not i8
    case ColumnType.UByte: return 'u8'
    case ColumnType.Short: return 'i16'
    case ColumnType.UShort: return 'u16'
    case ColumnType.Int: return 'i32'
    case ColumnType.UInt: return 'u32'
    case ColumnType.Long: return 'i64'
    case ColumnType.ULong: return 'u64'
    case ColumnType.Float: return 'f32'
    case ColumnType.Double: return 'f64'
    case ColumnType.String: return 'str50'
    case ColumnType.DateTime: return 'datetime'
    case ColumnType.Json: return 'str100' // divergence #2: query layer rejects (Task 14)
    case ColumnType.Binary: return 'str100' // divergence #2
    default: {
      const exhaustive: never = t
      throw new FcbError(ErrorCode.UnsupportedColumnType, `unknown column type ${String(exhaustive)}`)
    }
  }
}

/** True only for the fixed-width string kinds. Zero-padding means distinct
 *  values can collide on-disk (`'a'` and `'a\0'` produce identical keys), so
 *  every tree hit for these kinds needs a post-filter against the decoded
 *  attribute value (Task 15) before it is trusted. */
export function needsPostFilter(kind: KeyKind): boolean {
  return kind === 'str50' || kind === 'str100'
}

/** Encodes one key value into a freshly-allocated, fixed-width buffer.
 *
 *  String kinds truncate the UTF-8 encoding of `value` at the BYTE level and
 *  zero-pad the remainder -- this can split a multi-byte UTF-8 sequence in
 *  half, which is why `decodeKey` never turns the bytes back into a string
 *  (see its docstring). */
export function encodeKey(kind: KeyKind, value: unknown): Uint8Array {
  const size = keySize(kind)
  const buf = new Uint8Array(size)
  const dv = new DataView(buf.buffer)
  switch (kind) {
    case 'bool':
      writeU8(dv, 0, (value as boolean) ? 1 : 0)
      break
    case 'u8':
      writeU8(dv, 0, value as number)
      break
    case 'i16':
      writeI16(dv, 0, value as number)
      break
    case 'u16':
      writeU16(dv, 0, value as number)
      break
    case 'i32':
      writeI32(dv, 0, value as number)
      break
    case 'u32':
      writeU32(dv, 0, value as number)
      break
    case 'i64':
      writeI64(dv, 0, value as bigint)
      break
    case 'u64':
      writeU64(dv, 0, value as bigint)
      break
    case 'f32':
      writeF32(dv, 0, value as number)
      break
    case 'f64':
      writeF64(dv, 0, value as number)
      break
    case 'datetime': {
      const dt = value as DateTimeKey
      writeI64(dv, 0, dt.seconds)
      writeU32(dv, 8, dt.nanos)
      break
    }
    case 'str50':
    case 'str100': {
      const encoded = utf8Encoder.encode(value as string)
      buf.set(encoded.subarray(0, Math.min(encoded.length, size)))
      break
    }
    default: {
      const exhaustive: never = kind
      throw new FcbError(ErrorCode.InvalidArgument, `unknown key kind ${String(exhaustive)}`)
    }
  }
  return buf
}

/** Fixed-width string kinds decode to the raw padded Uint8Array, NOT to a
 *  string: a truncated key can end mid-UTF-8, and decoding then re-encoding
 *  would change the bytes the tree is ordered by. Treat the result as
 *  display-only if you ever need text out of it -- `new TextDecoder().decode`
 *  is lossy (replaces the broken tail with U+FFFD) but does not throw. */
export function decodeKey(
  kind: KeyKind, dv: DataView, offset: number,
): number | bigint | boolean | Uint8Array | DateTimeKey {
  switch (kind) {
    case 'bool':
      return readU8(dv, offset) !== 0
    case 'u8':
      return readU8(dv, offset)
    case 'i16':
      return readI16(dv, offset)
    case 'u16':
      return readU16(dv, offset)
    case 'i32':
      return readI32(dv, offset)
    case 'u32':
      return readU32(dv, offset)
    case 'i64':
      return readI64(dv, offset)
    case 'u64':
      return readU64(dv, offset)
    case 'f32':
      return readF32(dv, offset)
    case 'f64':
      return readF64(dv, offset)
    case 'datetime':
      return { seconds: readI64(dv, offset), nanos: readU32(dv, offset + 8) }
    case 'str50':
    case 'str100': {
      const size = keySize(kind)
      return new Uint8Array(dv.buffer, dv.byteOffset + offset, size).slice()
    }
    default: {
      const exhaustive: never = kind
      throw new FcbError(ErrorCode.InvalidArgument, `unknown key kind ${String(exhaustive)}`)
    }
  }
}

/** Normalizes a string-kind operand for comparison: `compareKeys` may be
 *  called with either a raw JS string (a query bound, not yet encoded) or a
 *  Uint8Array (a key already decoded off a tree node), so both must produce
 *  the same fixed-width byte sequence before comparing. */
function toStringKeyBytes(kind: KeyKind, v: unknown): Uint8Array {
  return v instanceof Uint8Array ? v : encodeKey(kind, v)
}

/** Compares two decoded (or not-yet-encoded) key values under this kind's
 *  ordering.
 *
 *  Floats special-case NaN FIRST, then fall back to plain `<`/`>`: neither
 *  `===` nor `Object.is` gives `ordered_float` semantics (NaN sorts greatest
 *  and equals itself; `-0.0` equals `+0.0`). Never "simplify" this to a bare
 *  `a - b` or `a < b` -- that silently reintroduces `NaN !== NaN` and loses
 *  the deliberate NaN-greatest rule the on-disk tree was built with.
 *
 *  String kinds compare the UTF-8-encoded bytes UNSIGNED and lexicographic,
 *  never the JS strings themselves: JS `<` is UTF-16 code-unit order, which
 *  disagrees with byte order for non-BMP text (e.g. `'｡' < '\u{10000}'` is
 *  `false` in JS, but the byte comparison the tree was built with says the
 *  opposite). "Simplifying" this to `a < b` on the raw strings would silently
 *  break every query involving a supplementary-plane character. */
export function compareKeys(kind: KeyKind, a: unknown, b: unknown): number {
  switch (kind) {
    case 'bool': {
      const av = a as boolean
      const bv = b as boolean
      if (av === bv) return 0
      return av ? 1 : -1
    }
    case 'u8':
    case 'i16':
    case 'u16':
    case 'i32':
    case 'u32': {
      const av = a as number
      const bv = b as number
      return av < bv ? -1 : av > bv ? 1 : 0
    }
    case 'i64':
    case 'u64': {
      const av = a as bigint
      const bv = b as bigint
      return av < bv ? -1 : av > bv ? 1 : 0
    }
    case 'f32':
    case 'f64': {
      const av = a as number
      const bv = b as number
      const aNaN = Number.isNaN(av)
      const bNaN = Number.isNaN(bv)
      if (aNaN && bNaN) return 0
      if (aNaN) return 1
      if (bNaN) return -1
      // Plain `<`/`>` already treat -0.0 and +0.0 as equal (neither is
      // strictly less than the other), matching ordered_float here for free.
      return av < bv ? -1 : av > bv ? 1 : 0
    }
    case 'datetime': {
      const av = a as DateTimeKey
      const bv = b as DateTimeKey
      if (av.seconds !== bv.seconds) return av.seconds < bv.seconds ? -1 : 1
      return av.nanos < bv.nanos ? -1 : av.nanos > bv.nanos ? 1 : 0
    }
    case 'str50':
    case 'str100': {
      const ab = toStringKeyBytes(kind, a)
      const bb = toStringKeyBytes(kind, b)
      const len = Math.min(ab.length, bb.length)
      for (let i = 0; i < len; i++) {
        const diff = (ab[i] as number) - (bb[i] as number)
        if (diff !== 0) return diff < 0 ? -1 : 1
      }
      return ab.length - bb.length
    }
    default: {
      const exhaustive: never = kind
      throw new FcbError(ErrorCode.InvalidArgument, `unknown key kind ${String(exhaustive)}`)
    }
  }
}

/** Sentinel minimum for range-query lowering (`find_range(MIN, key)`, etc).
 *  `datetime`'s minimum is EPOCH ZERO, not `-Infinity` seconds, even though
 *  the wire format is a signed i64 that round-trips negative seconds fine
 *  through `encodeKey`/`decodeKey` -- deliberate divergence #4: pre-1970
 *  timestamps are invisible to range queries built from this sentinel. */
export function keyMin(kind: KeyKind): unknown {
  switch (kind) {
    case 'bool': return false
    case 'u8': return 0
    case 'i16': return -32768
    case 'u16': return 0
    case 'i32': return -2147483648
    case 'u32': return 0
    case 'i64': return -(2n ** 63n)
    case 'u64': return 0n
    case 'f32':
    case 'f64': return Number.NEGATIVE_INFINITY
    case 'datetime': return { seconds: 0n, nanos: 0 } as DateTimeKey
    case 'str50':
    case 'str100': return new Uint8Array(keySize(kind)) // all-zero
    default: {
      const exhaustive: never = kind
      throw new FcbError(ErrorCode.InvalidArgument, `unknown key kind ${String(exhaustive)}`)
    }
  }
}

/** Sentinel maximum for range-query lowering. `f32`/`f64`'s maximum is
 *  `+Infinity`, NOT NaN, even though `ordered_float` sorts NaN strictly
 *  above `+Infinity` -- deliberate divergence #3: NaN-keyed features are
 *  invisible to range queries built from this sentinel. */
export function keyMax(kind: KeyKind): unknown {
  switch (kind) {
    case 'bool': return true
    case 'u8': return 255
    case 'i16': return 32767
    case 'u16': return 65535
    case 'i32': return 2147483647
    case 'u32': return 4294967295
    case 'i64': return 2n ** 63n - 1n
    case 'u64': return 2n ** 64n - 1n
    case 'f32':
    case 'f64': return Number.POSITIVE_INFINITY
    case 'datetime':
      // Year 9999, matching `static_btree/key.rs`'s `DateTime<Utc>::max_value()`.
      return { seconds: 253402300799n, nanos: 999999999 } as DateTimeKey
    case 'str50':
    case 'str100': return new Uint8Array(keySize(kind)).fill(0xff)
    default: {
      const exhaustive: never = kind
      throw new FcbError(ErrorCode.InvalidArgument, `unknown key kind ${String(exhaustive)}`)
    }
  }
}
