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
  it('envelope contains a densely sampled boundary (50 points per edge)', () => {
    // Independently generated from the implementation: walks all four edges
    // of the same lng/lat rectangle at a resolution not tied to
    // bboxToSource's own subdivision count, so this fails for a
    // corners-only (or corners+midpoints-only) envelope that omits area
    // near a curved edge.
    //
    // The rectangle straddles the RD New central meridian (lon_0 ~5.3877)
    // at a latitude ~1.7 degrees south of the projection origin
    // (lat_0 ~52.156) — exactly where the south edge bows hardest away from
    // the chord between its corners, since the true extremum sits near the
    // meridian crossing, well inside the edge. A box near the origin (e.g.
    // central Delft) does *not* expose this: at that scale the bow is far
    // below floating-point-relevant magnitudes, so a corners-only bug would
    // go undetected there. Here a corners-only envelope is wrong by ~167m.
    const west = 4.8, south = 50.4, east = 6.0, north = 50.5
    const perEdge = 50
    const samples: [number, number][] = []
    const addEdge = (x0: number, y0: number, x1: number, y1: number) => {
      for (let i = 0; i <= perEdge; i++) {
        const t = i / perEdge
        samples.push([x0 + (x1 - x0) * t, y0 + (y1 - y0) * t])
      }
    }
    addEdge(west, south, east, south)
    addEdge(east, south, east, north)
    addEdge(east, north, west, north)
    addEdge(west, north, west, south)

    const [minX, minY, maxX, maxY] = bboxToSource(7415, west, south, east, north)
    // A "tiny" epsilon here is relative to the ~600 km span of RD New
    // coordinates, not to floating-point precision: bboxToSource and this
    // test sample the same smooth curve on independent, non-nested grids,
    // so two genuinely-converged envelopes can still differ by sub-cm noise
    // right at the flat extremum near the central meridian. 1 cm comfortably
    // absorbs that noise while still rejecting the corners-only bug (~167 m,
    // four orders of magnitude larger) by a wide margin.
    const eps = 1e-2
    for (const s of samples) {
      const [x, y] = inverse(7415, s)
      expect(x).toBeGreaterThanOrEqual(minX - eps)
      expect(x).toBeLessThanOrEqual(maxX + eps)
      expect(y).toBeGreaterThanOrEqual(minY - eps)
      expect(y).toBeLessThanOrEqual(maxY + eps)
    }
  })
})
