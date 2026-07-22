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
  it('rejects a non-short-form URN string', () => {
    const s = resolveCrs('urn:ogc:def:crs:EPSG::7415')
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

describe('forward against an authoritative control point', () => {
  // Onze-Lieve-Vrouwetoren, Amersfoort: RD [155000, 463000] is the RD New
  // origin itself, with a well-known published WGS84 equivalent.
  it('matches the known WGS84 coordinates within 0.002 degrees', () => {
    const [lng, lat] = forward(7415, [155000, 463000])
    expect(Math.abs(lng - 5.387206)).toBeLessThan(0.002)
    expect(Math.abs(lat - 52.155174)).toBeLessThan(0.002)
  })
})

describe('unsupported CRS refusal', () => {
  it('forward throws for an unregistered code', () => {
    expect(() => forward(9999, [0, 0])).toThrow()
  })
  it('inverse throws for an unregistered code', () => {
    expect(() => inverse(9999, [0, 0])).toThrow()
  })
})

describe('EPSG:28992 coverage', () => {
  it('is marked supported', () => {
    expect(resolveCrs('EPSG:28992').supported).toBe(true)
  })
  it('round-trips through forward/inverse within 1 cm', () => {
    const rd: [number, number] = [85530, 447355]
    const back = inverse(28992, forward(28992, rd))
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
  it('envelope contains all eight densified inverse-projected samples', () => {
    const west = 4.35, south = 51.98, east = 4.4, north = 52.02
    const midLng = (west + east) / 2
    const midLat = (south + north) / 2
    const samples: [number, number][] = [
      [west, south], [east, south], [east, north], [west, north],
      [midLng, south], [east, midLat], [midLng, north], [west, midLat],
    ]
    const [minX, minY, maxX, maxY] = bboxToSource(7415, west, south, east, north)
    for (const s of samples) {
      const [x, y] = inverse(7415, s)
      expect(x).toBeGreaterThanOrEqual(minX - 1e-6)
      expect(x).toBeLessThanOrEqual(maxX + 1e-6)
      expect(y).toBeGreaterThanOrEqual(minY - 1e-6)
      expect(y).toBeLessThanOrEqual(maxY + 1e-6)
    }
  })
})
