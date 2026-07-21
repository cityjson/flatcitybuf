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

  it('decodes Byte as u8, matching the WRITER (Rust reader disagrees)', () => {
    // Deliberate divergence #1: the writer stores Byte as u8, the Rust
    // reader decodes i8, so stored values > 127 come back negative there.
    const schema = [col(0, 'b', ColumnType.Byte)]
    expect(decodeAttributes(concat(rec(0, [200])), schema)).toEqual({ b: 200 })
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
