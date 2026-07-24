// src/render/mergeFeatures.ts
// Merges the per-feature meshes returned by the reader into ONE indexed mesh
// for a single deck.gl layer. This removes the 255-pickable-layer cap and the
// per-feature draw-call cost of one SimpleMeshLayer per building.
//
// Each feature's mesh is in local metres relative to its own reprojected
// centroid. Those are converted to absolute lng/lat here with a per-feature
// linearisation (metres -> degrees at the feature's latitude): the feature is
// small (~tens of metres), so the flat approximation is sub-decimetre, and the
// accurate per-feature centroid absorbs the global position. z stays in metres.
import type { RenderedFeature } from '../store/index'

export interface MergedMesh {
  positions: { size: 3; value: Float32Array } // [lng, lat, altitude(m)]
  normals: { size: 3; value: Float32Array }
  colors: { size: 4; value: Float32Array } // rgba 0..1
  pickIndex: { size: 1; value: Float32Array } // feature index, for picking
  indices: { size: 1; value: Uint32Array }
  vertexCount: number
}

const M_PER_DEG_LAT = 111320

/** Per-feature colour (0..1 rgba): a steel blue, or a ramp over a numeric
 *  attribute when `colorBy` is set. */
export function featureColor(
  f: RenderedFeature, colorBy: string | undefined,
): [number, number, number, number] {
  if (colorBy !== undefined) {
    const v = f.attributes[colorBy]
    if (typeof v === 'number') {
      const t = Math.max(0, Math.min(1, (v % 100) / 100))
      return [(50 + 200 * t) / 255, (120 * (1 - t) + 60) / 255, 180 / 255, 1]
    }
  }
  return [70 / 255, 130 / 255, 180 / 255, 1]
}

export function mergeFeatures(
  features: RenderedFeature[], colorBy: string | undefined,
): MergedMesh {
  let vtot = 0
  let itot = 0
  for (const f of features) {
    vtot += f.mesh.positions.length / 3
    itot += f.mesh.indices.length
  }
  const positions = new Float32Array(vtot * 3)
  const normals = new Float32Array(vtot * 3)
  const colors = new Float32Array(vtot * 4)
  const pickIndex = new Float32Array(vtot)
  const indices = new Uint32Array(itot)

  let vo = 0
  let io = 0
  for (let fi = 0; fi < features.length; fi++) {
    const f = features[fi]
    const clng = f.centroidLngLat[0]
    const clat = f.centroidLngLat[1]
    const mPerDegLng = M_PER_DEG_LAT * Math.cos((clat * Math.PI) / 180)
    const p = f.mesh.positions
    const n = f.mesh.normals
    const nv = p.length / 3
    const [cr, cg, cb, ca] = featureColor(f, colorBy)
    for (let i = 0; i < nv; i++) {
      const b = (vo + i) * 3
      positions[b] = clng + p[i * 3] / mPerDegLng
      positions[b + 1] = clat + p[i * 3 + 1] / M_PER_DEG_LAT
      positions[b + 2] = p[i * 3 + 2]
      normals[b] = n[i * 3]
      normals[b + 1] = n[i * 3 + 1]
      normals[b + 2] = n[i * 3 + 2]
      const c = (vo + i) * 4
      colors[c] = cr
      colors[c + 1] = cg
      colors[c + 2] = cb
      colors[c + 3] = ca
      pickIndex[vo + i] = fi
    }
    const idx = f.mesh.indices
    for (let i = 0; i < idx.length; i++) indices[io + i] = idx[i] + vo
    vo += nv
    io += idx.length
  }

  return {
    positions: { size: 3, value: positions },
    normals: { size: 3, value: normals },
    colors: { size: 4, value: colors },
    pickIndex: { size: 1, value: pickIndex },
    indices: { size: 1, value: indices },
    vertexCount: vtot,
  }
}
