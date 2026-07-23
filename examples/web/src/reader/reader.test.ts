// src/reader/reader.test.ts
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { FcbReader } from '@cityjson/flatcitybuf'
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
})

describe('coerceAttrValue', () => {
  it('rejects a non-integer for an Int column', () => {
    const col = { index: 0, name: 'n', type: 4 /* Int */, nullable: true } as never
    expect(() => coerceAttrValue(col, '1.5')).toThrow()
  })
})

describe('runQuery pagination', () => {
  it('pages a bbox query without exceeding the limit', async () => {
    const reader = await open()
    const ext = reader.header.info.geographicalExtent!
    const bboxSource: [number, number, number, number] =
      [ext[0], ext[1], ext[3], ext[4]]
    const page1 = await runQuery(reader, { bboxSource, limit: 5, offset: 0 })
    expect(page1.features.length).toBeLessThanOrEqual(5)
    expect(page1.total).toBeGreaterThan(0)
    if ((page1.total ?? 0) > 5) {
      const page2 = await runQuery(reader, { bboxSource, limit: 5, offset: 5 })
      expect(page2.features[0]?.id).not.toBe(page1.features[0]?.id)
    }
  })
})
