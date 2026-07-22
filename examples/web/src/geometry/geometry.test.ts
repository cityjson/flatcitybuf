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
})
