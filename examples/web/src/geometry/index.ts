// src/geometry/index.ts
import type { CityJSONFeature, Transform } from '@cityjson/flatcitybuf'
import earcut from 'earcut'

export interface Mesh {
  positions: Float32Array
  normals: Float32Array
  indices: Uint32Array
}

export interface FeatureMesh {
  centroidLngLat: [number, number]
  mesh: Mesh
  triangleCount: number
}

type Ring = number[]
type Surface = Ring[]

/** Flattens MultiSurface/Solid/MultiSolid nesting to a flat list of surfaces.
 *  A ring is an array whose first element is a number (vertex index); a surface
 *  is an array whose first element is a ring. */
export function collectSurfaces(boundaries: unknown): Surface[] {
  const surfaces: Surface[] = []
  const isRing = (x: unknown): x is Ring =>
    Array.isArray(x) && typeof x[0] === 'number'
  const isSurface = (x: unknown): x is Surface =>
    Array.isArray(x) && isRing((x as unknown[])[0])
  const walk = (x: unknown): void => {
    if (isSurface(x)) surfaces.push(x)
    else if (Array.isArray(x)) x.forEach(walk)
  }
  walk(boundaries)
  return surfaces
}

/** Newell's method: area-weighted normal of a 3D polygon ring. Robust to
 *  non-planarity; zero-length for a degenerate (collinear/empty) ring. */
function newellNormal(ring: number[], world: number[][]): [number, number, number] {
  let nx = 0, ny = 0, nz = 0
  for (let i = 0; i < ring.length; i++) {
    const a = world[ring[i]]
    const b = world[ring[(i + 1) % ring.length]]
    nx += (a[1] - b[1]) * (a[2] + b[2])
    ny += (a[2] - b[2]) * (a[0] + b[0])
    nz += (a[0] - b[0]) * (a[1] + b[1])
  }
  return [nx, ny, nz]
}

/** Two in-plane basis vectors for a plane with the given normal. */
function planeBasis(n: [number, number, number]): [number[], number[]] {
  const ax = Math.abs(n[0]), ay = Math.abs(n[1]), az = Math.abs(n[2])
  // Pick the world axis least aligned with n to seed a stable tangent.
  const seed = ax <= ay && ax <= az ? [1, 0, 0] : ay <= az ? [0, 1, 0] : [0, 0, 1]
  let ux = seed[1] * n[2] - seed[2] * n[1]
  let uy = seed[2] * n[0] - seed[0] * n[2]
  let uz = seed[0] * n[1] - seed[1] * n[0]
  const ul = Math.hypot(ux, uy, uz) || 1
  ux /= ul; uy /= ul; uz /= ul
  const vx = n[1] * uz - n[2] * uy
  const vy = n[2] * ux - n[0] * uz
  const vz = n[0] * uy - n[1] * ux
  return [[ux, uy, uz], [vx, vy, vz]]
}

/** Triangulates one surface (exterior ring + holes) into triangles indexed
 *  into `world`. Returns [] for a degenerate surface (near-zero normal, too
 *  few vertices, earcut failure). Winding is oriented to the Newell normal. */
function triangulateSurface(surface: Surface, world: number[][]): number[][] {
  const exterior = surface[0]
  if (exterior === undefined || exterior.length < 3) return []
  const raw = newellNormal(exterior, world)
  const len = Math.hypot(raw[0], raw[1], raw[2])
  if (len < 1e-9) return []
  const n: [number, number, number] = [raw[0] / len, raw[1] / len, raw[2] / len]
  const [u, v] = planeBasis(n)

  const flat: number[] = []
  const holeIndices: number[] = []
  const idxMap: number[] = [] // flat vertex i -> world index
  for (let r = 0; r < surface.length; r++) {
    if (r > 0) holeIndices.push(flat.length / 2)
    for (const wi of surface[r]) {
      const p = world[wi]
      flat.push(p[0] * u[0] + p[1] * u[1] + p[2] * u[2])
      flat.push(p[0] * v[0] + p[1] * v[1] + p[2] * v[2])
      idxMap.push(wi)
    }
  }
  const tris = earcut(flat, holeIndices.length ? holeIndices : undefined, 2)
  if (tris.length === 0) return []

  const out: number[][] = []
  for (let i = 0; i < tris.length; i += 3) {
    const a = idxMap[tris[i]], b = idxMap[tris[i + 1]], c = idxMap[tris[i + 2]]
    // Orient the triangle so its geometric normal agrees with the Newell
    // normal (earcut works in the projected 2D frame and may flip handedness).
    const pa = world[a], pb = world[b], pc = world[c]
    const e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]]
    const e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]]
    const tn = [
      e1[1] * e2[2] - e1[2] * e2[1],
      e1[2] * e2[0] - e1[0] * e2[2],
      e1[0] * e2[1] - e1[1] * e2[0],
    ]
    const dot = tn[0] * n[0] + tn[1] * n[1] + tn[2] * n[2]
    out.push(dot < 0 ? [a, c, b] : [a, b, c])
  }
  return out
}

/** Builds one local-metre mesh for a feature, anchored at its centroid.
 *  Vertices become `(X - cx, Y - cy, Z)` metres; the centroid is reprojected
 *  once to `[lng, lat]`. Flat per-face normals (vertices are split per
 *  triangle — no smoothing across hard edges). Returns null if no triangles
 *  survive. */
export function buildFeatureMesh(
  feature: CityJSONFeature,
  transform: Transform,
  reproject: (xy: [number, number]) => [number, number],
): FeatureMesh | null {
  const [sx, sy, sz] = transform.scale
  const [tx, ty, tz] = transform.translate
  const world = feature.vertices.map((v) => [
    v[0] * sx + tx, v[1] * sy + ty, v[2] * sz + tz,
  ])
  if (world.length === 0) return null

  // Centroid: mean of the axis-aligned bbox corners in XY (stable, cheap).
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  for (const p of world) {
    minX = Math.min(minX, p[0]); minY = Math.min(minY, p[1])
    maxX = Math.max(maxX, p[0]); maxY = Math.max(maxY, p[1])
  }
  const cx = (minX + maxX) / 2, cy = (minY + maxY) / 2

  const positions: number[] = []
  const normals: number[] = []
  const indices: number[] = []
  for (const co of Object.values(feature.CityObjects)) {
    const geoms = co.geometry ?? []
    if (geoms.length === 0) continue
    // Highest available LoD (numeric compare; unlabeled sorts last).
    const chosen = geoms.reduce((best, g) =>
      (Number(g.lod ?? -1) > Number(best.lod ?? -1) ? g : best), geoms[0])
    for (const surface of collectSurfaces(chosen.boundaries)) {
      const raw = newellNormal(surface[0] ?? [], world)
      const nl = Math.hypot(raw[0], raw[1], raw[2]) || 1
      const nrm = [raw[0] / nl, raw[1] / nl, raw[2] / nl]
      for (const [a, b, c] of triangulateSurface(surface, world)) {
        for (const wi of [a, b, c]) {
          const p = world[wi]
          const base = positions.length / 3
          positions.push(p[0] - cx, p[1] - cy, p[2])
          normals.push(nrm[0], nrm[1], nrm[2])
          indices.push(base)
        }
      }
    }
  }
  if (indices.length === 0) return null

  return {
    centroidLngLat: reproject([cx, cy]),
    mesh: {
      positions: new Float32Array(positions),
      normals: new Float32Array(normals),
      indices: new Uint32Array(indices),
    },
    triangleCount: indices.length / 3,
  }
}
