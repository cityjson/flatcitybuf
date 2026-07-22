// src/crs/crs.test.ts
import { describe, expect, it } from 'vitest'
import { bboxToSource, forward, inverse, resolveCrs } from './index'

describe('resolveCrs', () => {
  it('parses the short EPSG form and marks 7415 supported', () => {
    const s = resolveCrs('EPSG:7415')
    expect(s.code).toBe(7415)
    expect(s.supported).toBe(true)
  })
  it('marks an unknown code unsupported without throwing', () => {
    const s = resolveCrs('EPSG:9999')
    expect(s.code).toBe(9999)
    expect(s.supported).toBe(false)
  })
  it('handles an absent reference system', () => {
    const s = resolveCrs(undefined)
    expect(s.code).toBeNull()
    expect(s.supported).toBe(false)
  })
})

describe('forward/inverse round-trip near Delft', () => {
  // RD New coordinates near Delft city centre.
  const rd: [number, number] = [85530, 447355]
  it('forward lands in the Netherlands lng/lat box', () => {
    const [lng, lat] = forward(7415, rd)
    expect(lng).toBeGreaterThan(4.2)
    expect(lng).toBeLessThan(4.5)
    expect(lat).toBeGreaterThan(51.9)
    expect(lat).toBeLessThan(52.1)
  })
  it('inverse(forward(x)) ~= x within 1 cm', () => {
    const back = inverse(7415, forward(7415, rd))
    expect(Math.abs(back[0] - rd[0])).toBeLessThan(0.01)
    expect(Math.abs(back[1] - rd[1])).toBeLessThan(0.01)
  })
})

describe('bboxToSource', () => {
  it('returns a source envelope ordered min<max', () => {
    const c = forward(7415, [85000, 447000])
    const d = forward(7415, [86000, 448000])
    const [minX, minY, maxX, maxY] = bboxToSource(
      7415, Math.min(c[0], d[0]), Math.min(c[1], d[1]),
      Math.max(c[0], d[0]), Math.max(c[1], d[1]),
    )
    expect(minX).toBeLessThan(maxX)
    expect(minY).toBeLessThan(maxY)
    // Envelope must contain the RD corners it was built from.
    expect(minX).toBeLessThanOrEqual(85000 + 1)
    expect(maxX).toBeGreaterThanOrEqual(86000 - 1)
  })
})
