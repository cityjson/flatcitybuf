// src/reader/reader.test.ts
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import {
  type ColumnInfo, ColumnType, FcbReader, type HeaderView,
} from '@cityjson/flatcitybuf'
import { describe, expect, it } from 'vitest'
import { coerceAttrValue, headerModel, runQuery } from './index'

const fcbPath = fileURLToPath(
  new URL('../../../data/delft.fcb', import.meta.url),
)
async function open(): Promise<FcbReader> {
  return FcbReader.fromBytes(new Uint8Array(readFileSync(fcbPath)))
}

describe('headerModel', () => {
  it('reports CRS, extent and queryable columns for delft.fcb', async () => {
    const m = headerModel((await open()).header)
    expect(m.crs.code).toBe(7415)
    expect(m.crs.supported).toBe(true)
    expect(m.extent).toBeDefined()
    // Every queryable column must be a header-declared column.
    const names = new Set(m.columns.map((c) => c.name))
    for (const q of m.queryable) expect(names.has(q.name)).toBe(true)
  })

  it('excludes Json/Binary and unindexed columns from queryable', () => {
    const columns: ColumnInfo[] = [
      { index: 0, name: 'str_col', type: ColumnType.String, nullable: true },
      { index: 1, name: 'json_col', type: ColumnType.Json, nullable: true },
      {
        index: 2, name: 'bin_col', type: ColumnType.Binary, nullable: true,
      },
      {
        index: 3, name: 'int_col', type: ColumnType.Int, nullable: true,
      },
    ]
    const info = {
      featuresCount: 0,
      indexNodeSize: 16,
      columns,
      semanticColumns: [],
      geographicalExtent: undefined,
      hasTransform: false,
      referenceSystem: 'EPSG:7415',
      version: '2.0',
      attributeIndices: [
        {
          columnIndex: 0, length: 100, branchingFactor: 8,
          numUniqueItems: 10, begin: 0,
        },
        {
          columnIndex: 1, length: 100, branchingFactor: 8,
          numUniqueItems: 10, begin: 0,
        },
        {
          columnIndex: 2, length: 100, branchingFactor: 8,
          numUniqueItems: 10, begin: 0,
        },
        // Note: int_col (index 3) deliberately has NO attribute index entry.
      ],
    }
    const header = { info } as unknown as HeaderView
    const m = headerModel(header)
    expect(m.queryable.map((q) => q.name)).toStrictEqual(['str_col'])
  })
})

describe('coerceAttrValue', () => {
  const col = (type: ColumnType, name = 'c'): ColumnInfo => (
    { index: 0, name, type, nullable: true }
  )

  it('Bool', () => {
    expect(coerceAttrValue(col(ColumnType.Bool), 'true')).toBe(true)
    expect(coerceAttrValue(col(ColumnType.Bool), 'false')).toBe(false)
    expect(() => coerceAttrValue(col(ColumnType.Bool), 'x')).toThrow()
  })

  it.each([
    ColumnType.Byte, ColumnType.UByte, ColumnType.Short, ColumnType.UShort,
    ColumnType.Int, ColumnType.UInt,
  ])('integer type %i parses and rejects non-integers', (type) => {
    expect(coerceAttrValue(col(type), '42')).toBe(42)
    expect(() => coerceAttrValue(col(type), '1.5')).toThrow()
  })

  it.each([ColumnType.Long, ColumnType.ULong])(
    'bigint type %i parses beyond Number.MAX_SAFE_INTEGER',
    (type) => {
      const result = coerceAttrValue(col(type), '9007199254740993')
      expect(result).toBe(9007199254740993n)
      expect(typeof result).toBe('bigint')
    },
  )

  it.each([ColumnType.Float, ColumnType.Double])(
    'float type %i parses and rejects non-numbers',
    (type) => {
      expect(coerceAttrValue(col(type), '1.5')).toBe(1.5)
      expect(() => coerceAttrValue(col(type), 'x')).toThrow()
    },
  )

  it('DateTime', () => {
    const result = coerceAttrValue(col(ColumnType.DateTime), '2020-01-01')
    expect(result).toBeInstanceOf(Date)
    expect((result as Date).getTime()).toBe(new Date('2020-01-01').getTime())
    expect(() => coerceAttrValue(col(ColumnType.DateTime), 'notadate'))
      .toThrow()
  })

  it('String', () => {
    expect(coerceAttrValue(col(ColumnType.String), 'hi')).toBe('hi')
  })

  it.each([ColumnType.Json, ColumnType.Binary])(
    'type %i is not queryable',
    (type) => {
      expect(() => coerceAttrValue(col(type), 'anything'))
        .toThrow(/not queryable/)
    },
  )
})

describe('coerceAttrValue range validation', () => {
  const col = (type: ColumnType, name = 'c'): ColumnInfo => (
    { index: 0, name, type, nullable: true }
  )

  it('Byte accepts -128..127 and rejects one past each bound', () => {
    expect(coerceAttrValue(col(ColumnType.Byte), '-128')).toBe(-128)
    expect(coerceAttrValue(col(ColumnType.Byte), '127')).toBe(127)
    expect(() => coerceAttrValue(col(ColumnType.Byte), '-129'))
      .toThrow(/out of range/)
    expect(() => coerceAttrValue(col(ColumnType.Byte), '128'))
      .toThrow(/out of range/)
  })

  it('UByte accepts 0..255 and rejects a negative and 256', () => {
    expect(coerceAttrValue(col(ColumnType.UByte), '0')).toBe(0)
    expect(coerceAttrValue(col(ColumnType.UByte), '255')).toBe(255)
    expect(() => coerceAttrValue(col(ColumnType.UByte), '-1'))
      .toThrow(/out of range/)
    expect(() => coerceAttrValue(col(ColumnType.UByte), '256'))
      .toThrow(/out of range/)
  })

  it('Short accepts -32768..32767 and rejects one past each bound', () => {
    expect(coerceAttrValue(col(ColumnType.Short), '-32768')).toBe(-32768)
    expect(coerceAttrValue(col(ColumnType.Short), '32767')).toBe(32767)
    expect(() => coerceAttrValue(col(ColumnType.Short), '-32769'))
      .toThrow(/out of range/)
    expect(() => coerceAttrValue(col(ColumnType.Short), '32768'))
      .toThrow(/out of range/)
  })

  it('UShort accepts 0..65535 and rejects a negative and 65536', () => {
    expect(coerceAttrValue(col(ColumnType.UShort), '0')).toBe(0)
    expect(coerceAttrValue(col(ColumnType.UShort), '65535')).toBe(65535)
    expect(() => coerceAttrValue(col(ColumnType.UShort), '-1'))
      .toThrow(/out of range/)
    expect(() => coerceAttrValue(col(ColumnType.UShort), '65536'))
      .toThrow(/out of range/)
  })

  it('Int accepts -2147483648..2147483647 and rejects one past each bound', () => {
    expect(coerceAttrValue(col(ColumnType.Int), '-2147483648'))
      .toBe(-2147483648)
    expect(coerceAttrValue(col(ColumnType.Int), '2147483647'))
      .toBe(2147483647)
    expect(() => coerceAttrValue(col(ColumnType.Int), '-2147483649'))
      .toThrow(/out of range/)
    expect(() => coerceAttrValue(col(ColumnType.Int), '2147483648'))
      .toThrow(/out of range/)
  })

  it('UInt accepts 0..4294967295 and rejects a negative and 4294967296', () => {
    expect(coerceAttrValue(col(ColumnType.UInt), '0')).toBe(0)
    expect(coerceAttrValue(col(ColumnType.UInt), '4294967295'))
      .toBe(4294967295)
    expect(() => coerceAttrValue(col(ColumnType.UInt), '-1'))
      .toThrow(/out of range/)
    expect(() => coerceAttrValue(col(ColumnType.UInt), '4294967296'))
      .toThrow(/out of range/)
  })

  it('Long accepts the signed 64-bit bounds and rejects one past each', () => {
    expect(coerceAttrValue(col(ColumnType.Long), (-(2n ** 63n)).toString()))
      .toBe(-(2n ** 63n))
    expect(coerceAttrValue(col(ColumnType.Long), (2n ** 63n - 1n).toString()))
      .toBe(2n ** 63n - 1n)
    expect(() => coerceAttrValue(
      col(ColumnType.Long), (-(2n ** 63n) - 1n).toString(),
    )).toThrow(/out of range/)
    expect(() => coerceAttrValue(col(ColumnType.Long), (2n ** 63n).toString()))
      .toThrow(/out of range/)
  })

  it('ULong accepts 0..2^64-1 and rejects a negative and 2^64', () => {
    expect(coerceAttrValue(col(ColumnType.ULong), '0')).toBe(0n)
    expect(coerceAttrValue(col(ColumnType.ULong), (2n ** 64n - 1n).toString()))
      .toBe(2n ** 64n - 1n)
    expect(() => coerceAttrValue(col(ColumnType.ULong), '-1'))
      .toThrow(/out of range/)
    expect(() => coerceAttrValue(col(ColumnType.ULong), (2n ** 64n).toString()))
      .toThrow(/out of range/)
  })
})

describe('runQuery pagination', () => {
  it('total is the full match count and pagination advances over a stable order', async () => {
    const reader = await open()
    const ext = reader.header.info.geographicalExtent!
    const bboxSource: [number, number, number, number] =
      [ext[0], ext[1], ext[3], ext[4]]

    // Unpaged: total must equal the number of features actually returned.
    const full = await runQuery(
      reader, { bboxSource, limit: 100000, offset: 0 },
    )
    expect(full.total).toBe(full.features.length)
    // Must exceed the page size below so paging is actually exercised.
    expect(full.total).toBeGreaterThan(5)

    const page1 = await runQuery(reader, { bboxSource, limit: 5, offset: 0 })
    const page2 = await runQuery(reader, { bboxSource, limit: 5, offset: 5 })

    expect(page1.features.length).toBeLessThanOrEqual(5)
    expect(page2.features.length).toBeLessThanOrEqual(5)

    // total is the FULL match count, not the page length, on every page.
    expect(page1.total).toBe(full.total)
    expect(page2.total).toBe(full.total)

    // Results are a stable-ordered prefix of the unpaged result.
    const page1Ids = page1.features.map((f) => f.id)
    const page2Ids = page2.features.map((f) => f.id)
    const fullIds = full.features.map((f) => f.id)
    expect(page1Ids).toHaveLength(5)
    expect(page2Ids).toHaveLength(5)
    expect(page1Ids).toEqual(fullIds.slice(0, 5))
    expect(page2Ids).toEqual(fullIds.slice(5, 10))
  })
})
