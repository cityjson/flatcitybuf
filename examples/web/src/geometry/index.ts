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

/** One triangulated triangle: three world-vertex indices plus this
 *  triangle's own unit-length geometric normal (not a shared surface
 *  normal -- see spec §3.2 on non-planar surfaces). */
interface Triangle {
  tri: [number, number, number]
  normal: [number, number, number]
}

/** Relative tolerance for degeneracy checks below, applied against the
 *  square of a ring's characteristic length so it scales with world
 *  coordinate magnitude (RD coordinates are ~1e5-1e6) instead of testing
 *  an absolute residual that floating-point cancellation can blow past. */
const DEGEN_TOL = 1e-7

/** Picks one geometry from a CityObject's list. With `lod` set, returns the
 *  geometry whose LoD matches exactly (string-compared, so numeric and
 *  "2.2"-style labels agree with the discovered set) or `undefined` if the
 *  object has no geometry at that LoD — so an exclusive LoD selection shows
 *  *only* that LoD and objects lacking it are skipped, not silently rendered at
 *  a different LoD. With `lod` undefined (the pre-selection default), returns
 *  the highest LoD; unlabelled LoDs sort last. */
export function pickGeometry<G extends { lod?: string }>(
  geoms: G[], lod?: string,
): G | undefined {
  if (geoms.length === 0) return undefined
  if (lod !== undefined) return geoms.find((g) => String(g.lod) === lod)
  return geoms.reduce((best, g) =>
    (Number(g.lod ?? -1) > Number(best.lod ?? -1) ? g : best), geoms[0])
}

/** The highest LoD label present across a feature's objects, or undefined when
 *  no geometry is LoD-labelled. The default view renders exclusively at this
 *  LoD so it shows a single level (e.g. just LoD 2.2) rather than each object's
 *  own highest mixed together (a Building's LoD 0 roofprint *under* a
 *  BuildingPart's LoD 2.2 solid). */
export function featureMaxLod(
  objects: { geometry?: { lod?: string }[] }[],
): string | undefined {
  let best: string | undefined
  let bestNum = -Infinity
  for (const co of objects) {
    for (const g of co.geometry ?? []) {
      if (g.lod === undefined || g.lod === null) continue
      const n = Number(g.lod)
      if (Number.isFinite(n) && n > bestNum) { bestNum = n; best = String(g.lod) }
    }
  }
  return best
}

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

/** Removes consecutive duplicate vertex indices from a ring, including the
 *  wrap-around duplicate (ring[last] === ring[0]). Spec §3.2: a ring with
 *  accidental repeated indices should still triangulate, not crash or skew
 *  the degeneracy test. */
function sanitizeRing(ring: Ring): number[] {
  const out: number[] = []
  for (const idx of ring) {
    if (out.length === 0 || out[out.length - 1] !== idx) out.push(idx)
  }
  while (out.length > 1 && out[out.length - 1] === out[0]) out.pop()
  return out
}

/** Newell's method: area-weighted normal of a 3D polygon ring. Robust to
 *  non-planarity; near-zero for a degenerate (collinear/empty) ring.
 *  Vertices are translated by the ring's first vertex before accumulating
 *  so the summed terms stay near building scale instead of RD/world scale
 *  (~1e5-1e6), which keeps floating-point cancellation from swamping a
 *  genuinely-zero result. */
function newellNormal(ring: number[], world: number[][]): [number, number, number] {
  if (ring.length === 0) return [0, 0, 0]
  const o = world[ring[0]]
  let nx = 0, ny = 0, nz = 0
  for (let i = 0; i < ring.length; i++) {
    const pa = world[ring[i]]
    const pb = world[ring[(i + 1) % ring.length]]
    const ax = pa[0] - o[0], ay = pa[1] - o[1], az = pa[2] - o[2]
    const bx = pb[0] - o[0], by = pb[1] - o[1], bz = pb[2] - o[2]
    nx += (ay - by) * (az + bz)
    ny += (az - bz) * (ax + bx)
    nz += (ax - bx) * (ay + by)
  }
  return [nx, ny, nz]
}

/** A ring's characteristic length (longest edge), used to scale the
 *  degeneracy tolerance to the surface's own size rather than an absolute
 *  constant. */
function ringCharLength(ring: number[], world: number[][]): number {
  let maxLen = 0
  for (let i = 0; i < ring.length; i++) {
    const a = world[ring[i]]
    const b = world[ring[(i + 1) % ring.length]]
    const d = Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2])
    if (d > maxLen) maxLen = d
  }
  return maxLen
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
 *  into `world`, each carrying its OWN geometric normal (spec §3.2: a
 *  surface is not guaranteed planar, so a single shared Newell normal is
 *  wrong for its individual triangles). Returns [] for a degenerate surface
 *  (too few distinct vertices, near-zero normal relative to its own scale,
 *  or earcut failure). Winding -- and each triangle's normal -- is oriented
 *  to agree with the surface's overall Newell normal. */
function triangulateSurface(surface: Surface, world: number[][]): Triangle[] {
  const exteriorRaw = surface[0]
  if (exteriorRaw === undefined) return []
  const exterior = sanitizeRing(exteriorRaw)
  if (new Set(exterior).size < 3) return []

  const charLength = ringCharLength(exterior, world)
  const raw = newellNormal(exterior, world)
  const rawLen = Math.hypot(raw[0], raw[1], raw[2])
  // Degenerate (collinear/near-collinear exterior ring) when the Newell
  // normal is small relative to the ring's own size, not an absolute
  // constant -- see DEGEN_TOL doc comment.
  if (charLength <= 0 || rawLen <= DEGEN_TOL * charLength * charLength) return []
  const n: [number, number, number] = [raw[0] / rawLen, raw[1] / rawLen, raw[2] / rawLen]
  const [u, v] = planeBasis(n)

  const flat: number[] = []
  const holeIndices: number[] = []
  const idxMap: number[] = [] // flat vertex i -> world index
  for (const wi of exterior) {
    const p = world[wi]
    flat.push(p[0] * u[0] + p[1] * u[1] + p[2] * u[2])
    flat.push(p[0] * v[0] + p[1] * v[1] + p[2] * v[2])
    idxMap.push(wi)
  }
  for (let r = 1; r < surface.length; r++) {
    const hole = sanitizeRing(surface[r])
    // Skip (don't crash on) a hole ring that's degenerate after sanitizing.
    if (new Set(hole).size < 3) continue
    holeIndices.push(flat.length / 2)
    for (const wi of hole) {
      const p = world[wi]
      flat.push(p[0] * u[0] + p[1] * u[1] + p[2] * u[2])
      flat.push(p[0] * v[0] + p[1] * v[1] + p[2] * v[2])
      idxMap.push(wi)
    }
  }
  const tris = earcut(flat, holeIndices.length ? holeIndices : undefined, 2)
  if (tris.length === 0) return []

  // Zero-area threshold, scaled to the surface's own size.
  const areaTol = DEGEN_TOL * charLength * charLength
  const out: Triangle[] = []
  for (let i = 0; i < tris.length; i += 3) {
    const a = idxMap[tris[i]], b = idxMap[tris[i + 1]], c = idxMap[tris[i + 2]]
    const pa = world[a], pb = world[b], pc = world[c]
    const e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]]
    const e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]]
    const cx = e1[1] * e2[2] - e1[2] * e2[1]
    const cy = e1[2] * e2[0] - e1[0] * e2[2]
    const cz = e1[0] * e2[1] - e1[1] * e2[0]
    const crossLen = Math.hypot(cx, cy, cz)
    // Skip an output triangle that earcut produced but which has ~zero area
    // (e.g. three near-collinear points along a sanitized ring).
    if (crossLen / 2 <= areaTol) continue
    // Orient the triangle -- and its own normal -- so it agrees with the
    // Newell normal (earcut works in the projected 2D frame and may flip
    // handedness).
    const dot = cx * n[0] + cy * n[1] + cz * n[2]
    if (dot < 0) {
      out.push({
        tri: [a, c, b],
        normal: [-cx / crossLen, -cy / crossLen, -cz / crossLen],
      })
    } else {
      out.push({
        tri: [a, b, c],
        normal: [cx / crossLen, cy / crossLen, cz / crossLen],
      })
    }
  }
  return out
}

/** Builds one local-metre mesh for a feature, anchored at its centroid.
 *  Vertices become `(X - cx, Y - cy, Z)` metres; the centroid is reprojected
 *  once to `[lng, lat]`. Flat per-face normals (vertices are split per
 *  triangle -- no smoothing across hard edges, and each triangle gets its
 *  own geometric normal rather than a shared per-surface one). `lod` selects
 *  which LoD to triangulate per object (see pickGeometry); undefined = highest.
 *  Returns null if no triangles survive. */
export function buildFeatureMesh(
  feature: CityJSONFeature,
  transform: Transform,
  reproject: (xy: [number, number]) => [number, number],
  lod?: string,
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
  const objects = Object.values(feature.CityObjects)
  // Render one LoD exclusively: the requested one, or — for the default — this
  // feature's highest, so objects lacking that LoD are skipped rather than
  // drawn at some other level (see featureMaxLod).
  const effectiveLod = lod ?? featureMaxLod(objects)
  for (const co of objects) {
    const chosen = pickGeometry(co.geometry ?? [], effectiveLod)
    if (chosen === undefined) continue
    for (const surface of collectSurfaces(chosen.boundaries)) {
      for (const { tri: [a, b, c], normal } of triangulateSurface(surface, world)) {
        for (const wi of [a, b, c]) {
          const p = world[wi]
          const base = positions.length / 3
          positions.push(p[0] - cx, p[1] - cy, p[2])
          normals.push(normal[0], normal[1], normal[2])
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
