import { describe, expect, it } from 'vitest'
import { ColumnType } from '../src/generated/column-type.js'
import {
  compareKeys, decodeKey, encodeKey, keyKindForColumn, keyMax, keyMin, keySize,
  needsPostFilter,
} from '../src/static-btree/key.js'

const roundTrip = (kind: Parameters<typeof encodeKey>[0], v: unknown) => {
  const b = encodeKey(kind, v)
  return decodeKey(kind, new DataView(b.buffer, b.byteOffset, b.byteLength), 0)
}

describe('sizes', () => {
  it('gives DateTime twelve bytes: i64 seconds then u32 nanos', () => {
    expect(keySize('datetime')).toBe(12)
    expect(keySize('str50')).toBe(50)
    expect(keySize('str100')).toBe(100)
    expect(keySize('f64')).toBe(8)
  })
})

describe('round trips', () => {
  it('round-trips every numeric kind', () => {
    expect(roundTrip('i32', -5)).toBe(-5)
    expect(roundTrip('u32', 4294967295)).toBe(4294967295)
    expect(roundTrip('i64', -5n)).toBe(-5n)
    expect(roundTrip('u64', 18446744073709551615n)).toBe(18446744073709551615n)
    expect(roundTrip('f64', -1.5)).toBe(-1.5)
    expect(roundTrip('bool', true)).toBe(true)
  })

  it('stores floats as PLAIN IEEE-754 bits, with no total-order transform', () => {
    const b = encodeKey('f64', 1.5)
    const dv = new DataView(b.buffer, b.byteOffset, b.byteLength)
    expect(dv.getFloat64(0, true)).toBe(1.5)
  })
})

describe('float ordering is ordered_float, not JavaScript', () => {
  it('sorts NaN greatest and equal to itself', () => {
    const nan = Number.NaN
    expect(compareKeys('f64', nan, nan)).toBe(0)          // JS says NaN !== NaN
    expect(compareKeys('f64', nan, Number.POSITIVE_INFINITY)).toBeGreaterThan(0)
  })

  it('treats -0.0 and +0.0 as equal, unlike Object.is', () => {
    expect(compareKeys('f64', -0, 0)).toBe(0)             // Object.is says false
  })
})

describe('string keys', () => {
  it('truncates at the BYTE level, SPLITTING a UTF-8 sequence', () => {
    // 'é' is 2 bytes and 50 is even, so 'é'.repeat(40) truncates on a clean
    // boundary and demonstrates nothing. Use a 3-byte character: 16 of them
    // is 48 bytes, so the 17th is cut after its FIRST byte.
    const s = '☃'.repeat(20)                               // 60 bytes, 3 each
    const k = encodeKey('str50', s)
    expect(k).toHaveLength(50)
    expect(Array.from(k.subarray(48, 50)))
      .toEqual([0xe2, 0x98])                               // half a snowman
    // And it must still be usable: decoding is display-only and lossy.
    expect(() => new TextDecoder().decode(k)).not.toThrow()
  })

  it('decodes fixed-width keys as BYTES, never as a JS string', () => {
    // A truncated key can end mid-sequence; TextDecoder would replace those
    // bytes with U+FFFD and re-encoding would produce different bytes, so
    // the tree order would no longer be reproducible.
    const s = '☃'.repeat(20)
    const k = encodeKey('str50', s)
    const back = decodeKey('str50', new DataView(k.buffer, k.byteOffset, 50), 0)
    expect(back).toBeInstanceOf(Uint8Array)
    expect(Array.from(back as Uint8Array)).toEqual(Array.from(k))
  })

  it('zero-pads, so "a" and "a\\0" have the SAME key -- hence post-filtering', () => {
    expect(Array.from(encodeKey('str50', 'a')))
      .toEqual(Array.from(encodeKey('str50', 'a\0')))
  })

  it('compares as UTF-8 bytes, which disagrees with JS string order', () => {
    // JS: "｡" < "\u{10000}" is false (UTF-16 surrogates sort below U+FF61).
    // UTF-8 byte order says the opposite, and that is what the tree used.
    expect('｡' < '\u{10000}').toBe(false)
    expect(compareKeys('str50', '｡', '\u{10000}')).toBeLessThan(0)
  })
})

describe('column type mapping', () => {
  it('maps every ColumnType exactly as the WRITER emits it', () => {
    // Format Reference, "Column type -> key type". Enum values from
    // src/fbs/header.fbs:9-26 -- Byte=0 ... String=11, Json=12, DateTime=13,
    // Binary=14. Getting these off by one silently indexes the wrong column.
    expect(keyKindForColumn(ColumnType.Bool)).toBe('bool')
    expect(keyKindForColumn(ColumnType.Byte)).toBe('u8')     // u8, not i8
    expect(keyKindForColumn(ColumnType.UByte)).toBe('u8')
    expect(keyKindForColumn(ColumnType.Short)).toBe('i16')
    expect(keyKindForColumn(ColumnType.UShort)).toBe('u16')
    expect(keyKindForColumn(ColumnType.Int)).toBe('i32')
    expect(keyKindForColumn(ColumnType.UInt)).toBe('u32')
    expect(keyKindForColumn(ColumnType.Long)).toBe('i64')
    expect(keyKindForColumn(ColumnType.ULong)).toBe('u64')
    expect(keyKindForColumn(ColumnType.Float)).toBe('f32')
    expect(keyKindForColumn(ColumnType.Double)).toBe('f64')
    expect(keyKindForColumn(ColumnType.String)).toBe('str50')
    expect(keyKindForColumn(ColumnType.DateTime)).toBe('datetime')
    expect(keyKindForColumn(ColumnType.Json)).toBe('str100')
    expect(keyKindForColumn(ColumnType.Binary)).toBe('str100')
  })

  it('flags exactly the fixed-width string kinds for post-filtering', () => {
    expect(needsPostFilter('str50')).toBe(true)
    expect(needsPostFilter('str100')).toBe(true)
    expect(needsPostFilter('i32')).toBe(false)
    expect(needsPostFilter('f64')).toBe(false)
    expect(needsPostFilter('datetime')).toBe(false)
  })
})

describe('DateTime keys', () => {
  it('round-trips seconds and nanos independently', () => {
    const v = { seconds: 1700000000n, nanos: 123456789 }
    expect(roundTrip('datetime', v)).toEqual(v)
  })

  it('orders by seconds first, then by nanos', () => {
    const a = { seconds: 5n, nanos: 0 }
    const b = { seconds: 5n, nanos: 1 }
    const c = { seconds: 6n, nanos: 0 }
    expect(compareKeys('datetime', a, b)).toBeLessThan(0)
    expect(compareKeys('datetime', b, c)).toBeLessThan(0)
    expect(compareKeys('datetime', a, a)).toBe(0)
  })

  it('round-trips a NEGATIVE (pre-1970) timestamp, even though ranges hide it', () => {
    // The wire format is a signed i64; only the min_value sentinel is epoch 0.
    const v = { seconds: -86400n, nanos: 0 }
    expect(roundTrip('datetime', v)).toEqual(v)
  })
})

describe('narrow integer kinds', () => {
  it('round-trips the extremes of every width', () => {
    expect(roundTrip('u8', 255)).toBe(255)
    expect(roundTrip('i16', -32768)).toBe(-32768)
    expect(roundTrip('u16', 65535)).toBe(65535)
    expect(roundTrip('i32', -2147483648)).toBe(-2147483648)
    expect(roundTrip('f32', 0.5)).toBe(0.5)              // exact in binary32
  })

  it('orders u8 as UNSIGNED, which is the writer semantics for Byte', () => {
    // Deliberate divergence #1: Rust's reader decodes Byte as i8, so it
    // orders 200 below 100. The writer stores u8; we match the writer.
    expect(compareKeys('u8', 200, 100)).toBeGreaterThan(0)
  })
})

describe('sentinels reproduce the deliberate divergences', () => {
  it('uses +inf as the float maximum, so NaN keys are invisible to ranges', () => {
    expect(keyMax('f64')).toBe(Number.POSITIVE_INFINITY)
  })

  it('uses epoch 0 as the DateTime minimum, hiding pre-1970 timestamps', () => {
    expect(keyMin('datetime')).toEqual({ seconds: 0n, nanos: 0 })
  })

  it('uses the full i64/u64 range for 64-bit keys, as bigint', () => {
    expect(keyMax('u64')).toBe(18446744073709551615n)
    expect(keyMin('i64')).toBe(-(2n ** 63n))
  })
})
