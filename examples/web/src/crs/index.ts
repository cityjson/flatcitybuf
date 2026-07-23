// src/crs/index.ts
import proj4 from 'proj4'

/** Explicit allowlist of CRS definitions. proj4 bundles only WGS84 and a
 *  handful of others; EPSG:7415 (Amersfoort / RD New + NAP height) and its
 *  horizontal-only sibling 28992 are NOT bundled, so we register them here.
 *  Both use the same horizontal transform — the demo treats Z as metres-up
 *  and does no vertical-datum transform (see spec §3.4). Unknown codes are
 *  refused for georeferencing rather than guessed. */
const RD_NEW =
  '+proj=sterea +lat_0=52.15616055555555 +lon_0=5.38763888888889 ' +
  '+k=0.9999079 +x_0=155000 +y_0=463000 +ellps=bessel ' +
  '+towgs84=565.417,50.3319,465.552,-0.398957,0.343988,-1.8774,4.0725 ' +
  '+units=m +no_defs'

const CRS_DEFS: Record<number, string> = {
  7415: RD_NEW,
  28992: RD_NEW,
}

for (const [code, def] of Object.entries(CRS_DEFS)) {
  proj4.defs(`EPSG:${code}`, def)
}

export interface CrsStatus {
  code: number | null
  supported: boolean
  label: string
}

/** Parses the numeric code off the short `EPSG:<code>` form
 *  (`header.info.referenceSystem`) and reports whether it is in the
 *  allowlist. Never throws. */
export function resolveCrs(referenceSystem: string | undefined): CrsStatus {
  if (referenceSystem === undefined) {
    return { code: null, supported: false, label: '(none)' }
  }
  const m = /^EPSG:(\d+)$/i.exec(referenceSystem.trim())
  const code = m ? Number(m[1]) : null
  const supported = code !== null && code in CRS_DEFS
  return { code, supported, label: referenceSystem }
}

function defFor(code: number): string {
  const def = CRS_DEFS[code]
  if (def === undefined) {
    throw new Error(`unsupported CRS EPSG:${code}`)
  }
  return def
}

/** Source projected `[x, y]` -> WGS84 `[lng, lat]`. */
export function forward(code: number, xy: [number, number]): [number, number] {
  return proj4(defFor(code), 'EPSG:4326', xy) as [number, number]
}

/** WGS84 `[lng, lat]` -> source projected `[x, y]`. */
export function inverse(code: number, lngLat: [number, number]): [number, number] {
  return proj4('EPSG:4326', defFor(code), lngLat) as [number, number]
}

/** A lng/lat rectangle is not a rectangle in a transverse-Mercator source CRS
 *  — its edges bow — so sampling only the corners (or corners + midpoints)
 *  can miss the extremum and OMIT area near a curved edge from the returned
 *  envelope. This matters most when an edge crosses near the projection's
 *  central meridian/parallel, where the bow is sharpest and the true extremum
 *  sits well inside the edge rather than at a corner. To stay conservative we
 *  densify each of the four edges into `EDGE_SUBDIVISIONS` segments (>=16 per
 *  edge; 64 is used here — a coarser 16-32 measurably undershoots the true
 *  envelope for edges near the central meridian), inverse-project every
 *  sample, and take the min/max. Returns `[minX, minY, maxX, maxY]`. */
const EDGE_SUBDIVISIONS = 64

export function bboxToSource(
  code: number, west: number, south: number, east: number, north: number,
): [number, number, number, number] {
  const samples: [number, number][] = []
  const addEdge = (x0: number, y0: number, x1: number, y1: number) => {
    for (let i = 0; i < EDGE_SUBDIVISIONS; i++) {
      const t = i / EDGE_SUBDIVISIONS
      samples.push([x0 + (x1 - x0) * t, y0 + (y1 - y0) * t])
    }
  }
  addEdge(west, south, east, south) // bottom
  addEdge(east, south, east, north) // right
  addEdge(east, north, west, north) // top
  addEdge(west, north, west, south) // left
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  for (const s of samples) {
    const [x, y] = inverse(code, s)
    minX = Math.min(minX, x); minY = Math.min(minY, y)
    maxX = Math.max(maxX, x); maxY = Math.max(maxY, y)
  }
  return [minX, minY, maxX, maxY]
}
