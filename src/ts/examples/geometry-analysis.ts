/** Walking the FlatBuffers geometry directly, for analysis.
 *
 *      node examples/geometry-analysis.ts in.fcb [count]
 *
 *  Every other example converts to CityJSON first. This one does not:
 *  it reads the format's OWN representation -- five flat count arrays
 *  plus a flat vertex-index list -- and computes over them. That is the
 *  representation to use for analysis, because nothing has to be nested,
 *  allocated, or turned into JSON to get a number out of it.
 *
 *  The arrays, per `Geometry`:
 *
 *    solids[i]      shell count of solid i
 *    shells[i]      surface count of shell i
 *    surfaces[i]    ring count of surface i
 *    strings[i]     vertex count of ring i
 *    boundaries     the flat vertex-index list
 *    semantics[i]   semantic-object index of surface i (u32::MAX = none)
 *
 *  THE NESTING DEPTH COMES FROM `Geometry.type()`, NEVER FROM THE
 *  ARRAYS. A `Solid` with one shell and a `MultiSolid` with one solid
 *  flatten to byte-identical arrays -- only the type tells them apart.
 *  Inferring depth from which array is populated is upstream finding #8.
 *  This example never needs the depth at all: surface areas sum the same
 *  however the surfaces are grouped, so it walks `surfaces`/`strings`
 *  straight through. Anything that DOES care about grouping (per-shell
 *  volume, say) must switch on the type.
 *
 *  Vertices are quantised integers shared by the whole feature: multiply
 *  by `transform.scale` and add `transform.translate` for real-world
 *  coordinates. For area the translate cancels, but the scale does not.
 *
 *  The flat walk is checked against `toCityJSONFeature`'s nested output
 *  for the same feature, so the example proves itself rather than asking
 *  to be trusted.
 */
import {
  NULL_INDEX,
  SemanticSurfaceType,
  semanticSurfaceTypeName,
  toCityJSONFeature,
  type Feature,
  type HeaderView,
} from '@cityjson/flatcitybuf'
import { fromFile } from '@cityjson/flatcitybuf/node'

const path = process.argv[2]
const limit = process.argv[3] === undefined ? 20 : Number(process.argv[3])
const lod = process.argv[4] ?? '2.2'
if (path === undefined) {
  console.log('usage: node examples/geometry-analysis.ts <file.fcb> [count] [lod]')
  process.exit(2)
}

/** Area of one planar polygon in 3D, by Newell's method: the magnitude of
 *  the summed edge cross-products, halved. Works for any simple polygon
 *  and needs no projection or triangulation. */
function ringArea(xs: number[], ys: number[], zs: number[]): number {
  let nx = 0
  let ny = 0
  let nz = 0
  const n = xs.length
  for (let i = 0; i < n; i++) {
    const j = (i + 1) % n
    nx += ys[i]! * zs[j]! - zs[i]! * ys[j]!
    ny += zs[i]! * xs[j]! - xs[i]! * zs[j]!
    nz += xs[i]! * ys[j]! - ys[i]! * xs[j]!
  }
  return Math.sqrt(nx * nx + ny * ny + nz * nz) / 2
}

/** Area per semantic surface type, walked from the flat arrays alone.
 *
 *  Only geometries at `lod` are counted. A City Object carries ONE
 *  geometry per level of detail -- a 3DBAG BuildingPart has lod 1.2, 1.3
 *  and 2.2, and its parent Building an lod 0 footprint -- so summing
 *  every geometry would count each building three or four times over. */
function areaBySurfaceType(feature: Feature, header: HeaderView, lod: string): Map<string, number> {
  const out = new Map<string, number>()
  const scale = header.info.scale ?? [1, 1, 1]
  const translate = header.info.translate ?? [0, 0, 0]
  // One flat Int32Array of x,y,z triples, shared by every geometry in
  // this feature. Indices in `boundaries` point into it.
  const verts = feature.vertices()

  for (const view of feature.cityObjects()) {
    const raw = view.rawObject()
    for (let g = 0; g < raw.geometryLength(); g++) {
      const geom = raw.geometry(g)
      if (geom === null || geom.lod() !== lod) continue

      const surfaces = geom.surfacesArray()
      const strings = geom.stringsArray()
      const boundaries = geom.boundariesArray()
      const semantics = geom.semanticsArray()
      if (surfaces === null || strings === null || boundaries === null) continue

      let ring = 0 // index into `strings`
      let vertex = 0 // index into `boundaries`

      for (let s = 0; s < surfaces.length; s++) {
        const ringCount = surfaces[s]!
        let area = 0

        for (let r = 0; r < ringCount; r++) {
          const n = strings[ring]!
          const xs: number[] = []
          const ys: number[] = []
          const zs: number[] = []
          for (let k = 0; k < n; k++) {
            const vi = boundaries[vertex + k]!
            xs.push(verts[vi * 3]! * scale[0]! + translate[0]!)
            ys.push(verts[vi * 3 + 1]! * scale[1]! + translate[1]!)
            zs.push(verts[vi * 3 + 2]! * scale[2]! + translate[2]!)
          }
          // Ring 0 is the outer boundary; the rest are holes, which
          // subtract (CityJSON 2.0 section 6).
          area += (r === 0 ? 1 : -1) * ringArea(xs, ys, zs)
          vertex += n
          ring += 1
        }

        // `semantics` is one entry per surface, in surface order, so it
        // indexes directly here -- no regrouping needed for a per-surface
        // question. u32::MAX means "no semantic surface".
        let label = 'unassigned'
        if (semantics !== null && s < semantics.length) {
          const si = semantics[s]!
          if (si !== NULL_INDEX) {
            const so = geom.semanticsObjects(si)
            if (so !== null) {
              label = so.extensionType() ?? semanticSurfaceTypeName(so.type())
            }
          }
        }
        out.set(label, (out.get(label) ?? 0) + area)
      }
    }
  }
  return out
}

/** The same totals via the nested CityJSON, used only to check the walk
 *  above. This is the slow path: it allocates the whole nested structure
 *  and the semantics arrays for every feature. */
function areaBySurfaceTypeViaJSON(feature: Feature, header: HeaderView, lod: string): Map<string, number> {
  const out = new Map<string, number>()
  const cj = toCityJSONFeature(feature, header)
  const scale = header.info.scale ?? [1, 1, 1]
  const translate = header.info.translate ?? [0, 0, 0]
  const at = (i: number): [number, number, number] => {
    const v = cj.vertices[i]!
    return [
      v[0]! * scale[0]! + translate[0]!,
      v[1]! * scale[1]! + translate[1]!,
      v[2]! * scale[2]! + translate[2]!,
    ]
  }

  for (const obj of Object.values(cj.CityObjects)) {
    for (const geom of obj.geometry ?? []) {
      if (geom.lod !== lod) continue
      const surfaces: number[][][] = []
      const collect = (node: unknown): void => {
        if (!Array.isArray(node)) return
        if (Array.isArray(node[0]) && typeof node[0][0] === 'number') {
          surfaces.push(node as number[][])
          return
        }
        for (const child of node) collect(child)
      }
      collect(geom.boundaries)

      const values: (number | null)[] = []
      const flatten = (node: unknown): void => {
        if (Array.isArray(node)) node.forEach(flatten)
        else values.push(node as number | null)
      }
      flatten(geom.semantics?.values ?? [])

      surfaces.forEach((surface, s) => {
        let area = 0
        surface.forEach((ringIdx, r) => {
          const pts = ringIdx.map(at)
          area +=
            (r === 0 ? 1 : -1) *
            ringArea(pts.map((p) => p[0]), pts.map((p) => p[1]), pts.map((p) => p[2]))
        })
        const si = values[s]
        const surf = si === null || si === undefined ? undefined : geom.semantics?.surfaces[si]
        out.set(surf?.type ?? 'unassigned', (out.get(surf?.type ?? 'unassigned') ?? 0) + area)
      })
    }
  }
  return out
}

const reader = await fromFile(path)
try {
  console.log(`analysing the first ${limit} feature(s) of ${path}, lod ${lod}`)
  console.log('walking the flat FlatBuffers arrays -- no CityJSON, no nesting\n')

  const totals = new Map<string, number>()
  let features = 0
  let mismatches = 0
  // 3DBAG publishes its own computed ground area per building, so the
  // walk can be checked against the dataset rather than only against
  // this library's other code path.
  let publishedGroundArea = 0
  let havePublished = false

  const t0 = Date.now()
  for await (const feature of await reader.select({ limit })) {
    const flat = areaBySurfaceType(feature, reader.header, lod)
    for (const [k, v] of flat) totals.set(k, (totals.get(k) ?? 0) + v)

    for (const view of feature.cityObjects()) {
      const a = view.attributes()
      if (typeof a['b3_opp_grond'] === 'number') {
        publishedGroundArea += a['b3_opp_grond']
        havePublished = true
      }
    }

    // Self-check: the nested path must agree to floating-point noise.
    const viaJson = areaBySurfaceTypeViaJSON(feature, reader.header, lod)
    for (const [k, v] of flat) {
      const other = viaJson.get(k) ?? 0
      if (Math.abs(v - other) > 1e-6 * Math.max(1, Math.abs(v))) mismatches += 1
    }
    features += 1
  }
  const ms = Date.now() - t0

  const order = [
    semanticSurfaceTypeName(SemanticSurfaceType.RoofSurface),
    semanticSurfaceTypeName(SemanticSurfaceType.GroundSurface),
    semanticSurfaceTypeName(SemanticSurfaceType.WallSurface),
  ]
  const labels = [...order.filter((k) => totals.has(k)), ...[...totals.keys()].filter((k) => !order.includes(k))]

  console.log(`surface area at lod ${lod} over ${features} feature(s), m^2`)
  let sum = 0
  for (const label of labels) {
    const v = totals.get(label) ?? 0
    sum += v
    console.log(`  ${label.padEnd(16)} ${v.toFixed(2).padStart(12)}`)
  }
  console.log(`  ${'TOTAL'.padEnd(16)} ${sum.toFixed(2).padStart(12)}`)

  console.log()
  console.log(`flat walk vs nested CityJSON: ${mismatches === 0 ? 'AGREE' : `${mismatches} MISMATCH(ES)`}`)

  // The stronger check: against a number this library did not produce.
  if (havePublished) {
    const ground = totals.get(semanticSurfaceTypeName(SemanticSurfaceType.GroundSurface)) ?? 0
    const delta = Math.abs(ground - publishedGroundArea)
    const pct = (100 * delta) / Math.max(1, publishedGroundArea)
    console.log(
      `GroundSurface vs the dataset's own b3_opp_grond: ` +
        `${ground.toFixed(2)} vs ${publishedGroundArea.toFixed(2)} m^2 ` +
        `(${pct.toFixed(3)}% apart)`,
    )
    console.log(
      '  a sanity check against a number this library did not produce. Ordinary\n' +
        '  buildings agree to well under 1% (the 1 mm coordinate grid); a few large\n' +
        '  multi-part ones differ more, because b3_opp_grond came from the source\n' +
        '  geometry by a different pipeline. The READER check is the line above.',
    )
  }
  console.log(`${features} feature(s) in ${ms} ms`)
} finally {
  await reader.close()
}
