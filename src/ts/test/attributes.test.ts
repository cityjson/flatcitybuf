import { describe, expect, it } from 'vitest'
import { ColumnType } from '../src/generated/column-type.js'
import { decodeAttributes } from '../src/feature/attribute.js'

const col = (index: number, name: string, type: ColumnType) =>
  ({ index, name, type, nullable: true })

/** Attribute records are `u16 column_index` then the value, back to back. */
const rec = (index: number, body: number[]) => {
  const out = new Uint8Array(2 + body.length)
  new DataView(out.buffer).setUint16(0, index, true)
  out.set(body, 2)
  return out
}
const concat = (...parts: Uint8Array[]) => {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
  let o = 0
  for (const p of parts) { out.set(p, o); o += p.length }
  return out
}

describe('decodeAttributes', () => {
  it('decodes an int and a bool against their schema', () => {
    const schema = [col(0, 'n', ColumnType.Int), col(1, 'b', ColumnType.Bool)]
    const blob = concat(rec(0, [0x2a, 0, 0, 0]), rec(1, [1]))
    expect(decodeAttributes(blob, schema)).toEqual({ n: 42, b: true })
  })

  it('decodes Long as bigint ALWAYS, never as a number', () => {
    // Data-dependent types make sorting and serialization behave differently
    // the day one large value appears. See the design doc, 64-bit policy.
    const schema = [col(0, 'big', ColumnType.Long)]
    const body = new Uint8Array(8)
    new DataView(body.buffer).setBigInt64(0, 3n, true)
    expect(decodeAttributes(concat(rec(0, Array.from(body))), schema))
      .toEqual({ big: 3n })
  })

  it('decodes Byte as u8, unsigned, like every other implementation', () => {
    // The writer stores Byte as u8 and all four readers decode it as u8, so a
    // stored 200 is 200 and not -56. The Rust reader used to decode i8 here;
    // that divergence is closed (deserializer.rs), and this pins the agreed
    // answer rather than a port-local choice.
    const schema = [col(0, 'b', ColumnType.Byte)]
    expect(decodeAttributes(concat(rec(0, [200])), schema)).toEqual({ b: 200 })
  })

  it('decodes Byte, UByte and Binary together, mirroring the Rust test', () => {
    // The same fixture as `test_decode_attributes_byte_ubyte_binary`
    // (src/rust/fcb_core/src/reader/deserializer.rs), which asserts
    // {"b": 200, "ub": 200, "bin": [1, 255]}. Binary is a u32 LE byte length
    // followed by that many bytes.
    const schema = [
      col(0, 'b', ColumnType.Byte),
      col(1, 'ub', ColumnType.UByte),
      col(2, 'bin', ColumnType.Binary),
    ]
    const blob = concat(
      rec(0, [200]), rec(1, [200]), rec(2, [2, 0, 0, 0, 1, 255]),
    )
    const decoded = decodeAttributes(blob, schema)
    expect(decoded).toEqual({ b: 200, ub: 200, bin: new Uint8Array([1, 255]) })
    // The RAW decode API hands out bytes, not a number array: the conversion
    // to Rust's `[1, 255]` JSON shape belongs to the CityJSON boundary, and
    // conformance.test.ts pins it there. Keep the two apart deliberately.
    expect(decoded['bin']).toBeInstanceOf(Uint8Array)
  })

  it('returns an empty object for an empty blob', () => {
    expect(decodeAttributes(new Uint8Array(0), [])).toEqual({})
  })

  it('throws on a column index that is not in the schema', () => {
    // Cannot be skipped: the record is not self-delimiting, so the rest of
    // the blob is unreadable once alignment is lost.
    expect(() => decodeAttributes(concat(rec(99, [1])), [col(0, 'n', ColumnType.Bool)]))
      .toThrow()
  })

  it('throws on a truncated value rather than reading past the blob', () => {
    const schema = [col(0, 'n', ColumnType.Int)]
    expect(() => decodeAttributes(concat(rec(0, [1, 2])), schema)).toThrow()
  })
})
