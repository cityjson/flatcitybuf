// src/geometry/geometry.test.ts
import type { CityJSONFeature, Transform } from '@cityjson/flatcitybuf'
import { describe, expect, it } from 'vitest'
import { buildFeatureMesh, collectSurfaces } from './index'

const IDENTITY: Transform = { scale: [1, 1, 1], translate: [0, 0, 0] }
const noop = (xy: [number, number]): [number, number] => xy

describe('collectSurfaces', () => {
  it('finds one surface in a MultiSurface', () => {
    // MultiSurface boundaries: [ surface[ ring[idx,idx,idx] ] ]
    const b = [[[0, 1, 2]]]
    expect(collectSurfaces(b)).toEqual([[[0, 1, 2]]])
  })
  it('finds every surface of a Solid shell', () => {
    // Solid: [ shell[ surface[ ring ], surface[ ring ] ] ]
    const b = [[[[0, 1, 2]], [[2, 1, 3]]]]
    expect(collectSurfaces(b)).toEqual([[[0, 1, 2]], [[2, 1, 3]]])
  })
  it('finds every surface across a MultiSolid', () => {
    // MultiSolid: [ solid[ shell[ surface[ring], surface[ring] ] ],
    //               solid[ shell[ surface[ring] ] ] ]
    const b = [
      [[[[0, 1, 2]], [[2, 1, 3]]]],
      [[[[4, 5, 6]]]],
    ]
    expect(collectSurfaces(b)).toEqual([[[0, 1, 2]], [[2, 1, 3]], [[4, 5, 6]]])
  })
})

describe('buildFeatureMesh', () => {
  it('triangulates a square-with-square-hole into 8 triangles', () => {
    // Outer 10x10 square (ccw), inner 4..6 hole. All at z=0 in the XY plane.
    const verts: [number, number, number][] = [
      [0, 0, 0], [10, 0, 0], [10, 10, 0], [0, 10, 0], // outer 0..3
      [4, 4, 0], [6, 4, 0], [6, 6, 0], [4, 6, 0],     // hole 4..7
    ]
    const feature: CityJSONFeature = {
      type: 'CityJSONFeature',
      id: 'x',
      vertices: verts,
      CityObjects: {
        x: { type: 'Building', geometry: [{ type: 'MultiSurface', lod: '2',
          boundaries: [[[0, 1, 2, 3], [4, 5, 6, 7]]] }] },
      },
    }
    const fm = buildFeatureMesh(feature, IDENTITY, noop)
    expect(fm).not.toBeNull()
    // earcut of a quad-with-quad-hole yields 8 triangles.
    expect(fm!.triangleCount).toBe(8)
    expect(fm!.mesh.indices.length).toBe(24)
    // centroid of the outer square is (5,5); noop reproject keeps it.
    expect(fm!.centroidLngLat[0]).toBeCloseTo(5, 5)
    expect(fm!.centroidLngLat[1]).toBeCloseTo(5, 5)
    // Prove the hole is actually cut out (not just a count coincidence): no
    // output triangle's centroid falls inside the hole rectangle. Positions
    // are local (feature-centroid-relative); the outer square's centroid is
    // (5,5), so the hole rectangle [4,6]x[4,6] is local [-1,1]x[-1,1].
    const pos = fm!.mesh.positions
    const idx = fm!.mesh.indices
    for (let t = 0; t < idx.length; t += 3) {
      const a = idx[t] * 3, b = idx[t + 1] * 3, c = idx[t + 2] * 3
      const cx = (pos[a] + pos[b] + pos[c]) / 3
      const cy = (pos[a + 1] + pos[b + 1] + pos[c + 1]) / 3
      const insideHole = cx > -1 && cx < 1 && cy > -1 && cy < 1
      expect(insideHole).toBe(false)
    }
  })
  it('returns null for a degenerate collinear surface', () => {
    const feature: CityJSONFeature = {
      type: 'CityJSONFeature', id: 'y',
      vertices: [[0, 0, 0], [1, 0, 0], [2, 0, 0]],
      CityObjects: { y: { type: 'Building', geometry: [{ type: 'MultiSurface',
        lod: '2', boundaries: [[[0, 1, 2]]] }] } },
    }
    expect(buildFeatureMesh(feature, IDENTITY, noop)).toBeNull()
  })

  it('assigns every triangle of a tilted (non-axis-aligned) planar quad the same, correct normal', () => {
    // A planar parallelogram: p2 = p0 + (p1-p0) + (p3-p0), tilted off every
    // world axis so per-triangle normals can't accidentally agree just
    // because the surface happens to be axis-aligned.
    const verts: [number, number, number][] = [
      [0, 0, 0], [10, 0, 0], [10, 10, 10], [0, 10, 10],
    ]
    const feature: CityJSONFeature = {
      type: 'CityJSONFeature', id: 'tilt',
      vertices: verts,
      CityObjects: { tilt: { type: 'Building', geometry: [{ type: 'MultiSurface',
        lod: '2', boundaries: [[[0, 1, 2, 3]]] }] } },
    }
    const fm = buildFeatureMesh(feature, IDENTITY, noop)
    expect(fm).not.toBeNull()
    expect(fm!.triangleCount).toBe(2)
    // True plane normal via cross(e1, e2), e1 = p1-p0 = (10,0,0),
    // e2 = p3-p0 = (0,10,10): cross = (0*10-0*10, 0*0-10*10, 10*10-0*0)
    //   = (0, -100, 100) -> unit (0, -1/sqrt2, 1/sqrt2).
    const trueNormal = [0, -1 / Math.sqrt(2), 1 / Math.sqrt(2)]
    const normals = fm!.mesh.normals
    for (let t = 0; t < fm!.triangleCount; t++) {
      const n = [normals[t * 9], normals[t * 9 + 1], normals[t * 9 + 2]]
      const dot = n[0] * trueNormal[0] + n[1] * trueNormal[1] + n[2] * trueNormal[2]
      expect(Math.abs(dot)).toBeCloseTo(1, 5)
    }
  })

  it('gives a genuinely non-planar quad DIFFERENT normals per triangle (proves per-face, not shared)', () => {
    // Lift one corner off the plane defined by the other three.
    const verts: [number, number, number][] = [
      [0, 0, 0], [10, 0, 0], [10, 10, 5], [0, 10, 0],
    ]
    const feature: CityJSONFeature = {
      type: 'CityJSONFeature', id: 'warp',
      vertices: verts,
      CityObjects: { warp: { type: 'Building', geometry: [{ type: 'MultiSurface',
        lod: '2', boundaries: [[[0, 1, 2, 3]]] }] } },
    }
    const fm = buildFeatureMesh(feature, IDENTITY, noop)
    expect(fm).not.toBeNull()
    expect(fm!.triangleCount).toBe(2)
    const normals = fm!.mesh.normals
    const n0 = [normals[0], normals[1], normals[2]]
    const n1 = [normals[9], normals[10], normals[11]]
    const dot = n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2]
    // Same normal would give dot ~= 1; a non-planar quad's two triangles
    // must disagree.
    expect(dot).toBeLessThan(0.999)
  })

  it('transform maps Z to an absolute world value and keeps XY centroid-relative', () => {
    const scale: [number, number, number] = [2, 3, 5]
    const translate: [number, number, number] = [100, 200, 7]
    const transform: Transform = { scale, translate }
    // Flat unit square at local z=1 -> world Z = 1*5+7 = 12 for every vertex.
    const verts: [number, number, number][] = [
      [0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1],
    ]
    const feature: CityJSONFeature = {
      type: 'CityJSONFeature', id: 'xf',
      vertices: verts,
      CityObjects: { xf: { type: 'Building', geometry: [{ type: 'MultiSurface',
        lod: '2', boundaries: [[[0, 1, 2, 3]]] }] } },
    }
    const fm = buildFeatureMesh(feature, transform, noop)
    expect(fm).not.toBeNull()
    const pos = fm!.mesh.positions
    // Z is untouched by the centroid shift: every vertex's world Z is
    // v.z*scale.z + translate.z, independent of XY.
    for (let i = 0; i < pos.length; i += 3) {
      expect(pos[i + 2]).toBeCloseTo(12, 4)
    }
    // XY is centroid-relative: world XY of vertex 0 is (0*2+100, 0*3+200) =
    // (100, 200); the bbox midpoint is (101, 201.5), so its local XY is
    // (-1, -1.5).
    let found = false
    for (let i = 0; i < pos.length; i += 3) {
      if (Math.abs(pos[i] - -1) < 1e-3 && Math.abs(pos[i + 1] - -1.5) < 1e-3) {
        found = true
      }
    }
    expect(found).toBe(true)
  })

  it('uses the highest-LoD geometry when a CityObject carries more than one', () => {
    const verts: [number, number, number][] = [
      [0, 0, 0], [1, 0, 0], [1, 1, 0],                   // lod '1': small triangle, 0..2
      [0, 0, 0], [10, 0, 0], [10, 10, 0], [0, 10, 0],    // lod '2': larger quad, 3..6
    ]
    const feature: CityJSONFeature = {
      type: 'CityJSONFeature', id: 'lod',
      vertices: verts,
      CityObjects: { lod: { type: 'Building', geometry: [
        { type: 'MultiSurface', lod: '1', boundaries: [[[0, 1, 2]]] },
        { type: 'MultiSurface', lod: '2', boundaries: [[[3, 4, 5, 6]]] },
      ] } },
    }
    const fm = buildFeatureMesh(feature, IDENTITY, noop)
    expect(fm).not.toBeNull()
    // lod '1' triangle -> 1 triangle; lod '2' quad -> 2 triangles. Only the
    // higher LoD's geometry should be meshed.
    expect(fm!.triangleCount).toBe(2)
  })

  it('detects a collinear ring at RD-scale coordinates (scale-robust degeneracy)', () => {
    // Seven points, exactly collinear (visited out of order along the line,
    // as messy real data might be), at RD-magnitude coordinates (~1e5).
    // Under the old algorithm (no first-vertex translation, absolute 1e-9
    // threshold) the accumulated Newell sum for this ring is ~1.1e-8 --
    // above the absolute threshold, so it was wrongly treated as
    // non-degenerate. The scale-robust check (translate by ring[0], judge
    // against a tolerance relative to the ring's own size) correctly flags
    // it as degenerate.
    const verts: [number, number, number][] = [
      [94873.63118708295, 450154.44969368033, 0],
      [94898.61493438439, 450155.6406261837, 0],
      [94879.89352896735, 450154.74820880726, 0],
      [94881.01627222392, 450154.8017280581, 0],
      [94871.55462308605, 450154.3507074262, 0],
      [94885.93581857505, 450155.03623441857, 0],
      [94899.90146382924, 450155.70195284195, 0],
    ]
    const feature: CityJSONFeature = {
      type: 'CityJSONFeature', id: 'rd',
      vertices: verts,
      CityObjects: { rd: { type: 'Building', geometry: [{ type: 'MultiSurface',
        lod: '2', boundaries: [[[0, 1, 2, 3, 4, 5, 6]]] }] } },
    }
    expect(buildFeatureMesh(feature, IDENTITY, noop)).toBeNull()
  })

  it('sanitizes a ring with a repeated consecutive index and still triangulates it', () => {
    // Outer ring [0, 0, 1, 2, 3]: index 0 repeated consecutively. Sanitizing
    // must collapse it to [0, 1, 2, 3] (a normal square) and triangulate.
    const verts: [number, number, number][] = [
      [0, 0, 0], [10, 0, 0], [10, 10, 0], [0, 10, 0],
    ]
    const feature: CityJSONFeature = {
      type: 'CityJSONFeature', id: 'dup',
      vertices: verts,
      CityObjects: { dup: { type: 'Building', geometry: [{ type: 'MultiSurface',
        lod: '2', boundaries: [[[0, 0, 1, 2, 3]]] }] } },
    }
    const fm = buildFeatureMesh(feature, IDENTITY, noop)
    expect(fm).not.toBeNull()
    expect(fm!.triangleCount).toBe(2)
    expect(fm!.mesh.indices.length).toBe(6)
  })
})
