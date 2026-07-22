# FlatCityBuf Web Example Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the text-only demo at `examples/web` with a React + deck.gl + MapLibre viewer that opens a `.fcb` over HTTP range requests or a local file, runs bbox + attribute queries via the native `@cityjson/flatcitybuf` reader, and renders the returned 3D buildings on a basemap.

**Architecture:** Three framework-free, unit-tested core modules — `crs/` (proj4 allowlist + forward/inverse reprojection), `geometry/` (CityJSON→triangle-mesh), `reader/` (open + query + paginate + attribute coercion) — sit under a React shell (Jotai state, hooks, components). Each queried feature becomes its own deck.gl `SimpleMeshLayer` anchored at the feature's reprojected centroid, so meter offsets stay locally valid and per-feature picking/colour work.

**Tech Stack:** React 18, TypeScript, Vite, deck.gl (`@deck.gl/react`, `@deck.gl/core`, `@deck.gl/mesh-layers`), `react-map-gl/maplibre`, `maplibre-gl`, `proj4`, `earcut`, Jotai, Tailwind CSS, Vitest.

## Global Constraints

- Node `>=22.12.0` (repo engines floor); dev machine is Node 24.
- The reader is consumed ONLY through its public export surface `@cityjson/flatcitybuf` (wired `file:../../src/ts`). No deep imports into `src/ts/src/...`.
- `header.info.referenceSystem` is the SHORT `EPSG:<code>` form, never an OGC URL.
- proj4 does NOT bundle EPSG:7415/28992 — it must be registered from the allowlist. Unknown CRS is refused for georeferencing, never silently defaulted.
- deck.gl `SimpleMeshLayer` is imported from `@deck.gl/mesh-layers`.
- Attribute-query fields are restricted to indexed, non-JSON/Binary columns (`header.info.attributeIndices`).
- `u32::MAX` / vertex `transform`: world coord = `v * scale + translate`, scale/translate from `toCityJSONMetadata(header).transform`.
- TypeScript `strict: true`. All new pure modules have Vitest tests; the React shell is verified by `tsc --noEmit` + `vite build` + a manual smoke checklist.
- Demo data: `examples/data/delft.fcb` (EPSG:7415) for tests and manual smoke.

---

## File Structure

```
examples/web/
  package.json            # rewritten: React + deck.gl + maplibre + proj4 + earcut + jotai + tailwind + vitest
  tsconfig.json           # rewritten: React JSX, include src + tests
  vite.config.ts          # React plugin; base './'
  vitest.config.ts        # node env for pure-module tests
  tailwind.config.js
  postcss.config.js
  index.html              # #root mount
  src/
    main.tsx              # React entry
    App.tsx               # layout + panels + MapView
    index.css            # tailwind directives
    crs/
      index.ts            # CRS_DEFS allowlist, resolveCrs, forward, inverse, bboxToSource
      crs.test.ts
    geometry/
      index.ts            # collectSurfaces, triangulateSurface, buildFeatureMesh
      geometry.test.ts
    reader/
      index.ts            # openFromUrl/openFromBlob, headerModel, coerceAttrValue, runQuery
      reader.test.ts
    store/
      index.ts            # Jotai atoms
    hooks/
      useFcbData.ts
      useDrawBbox.ts
    components/
      MapView.tsx
      SourcePanel.tsx
      HeaderPanel.tsx
      QueryPanel.tsx
      FeatureInspector.tsx
  README.md
```

The old `examples/web/main.ts` is deleted (its logic moves into `src/reader/` and `src/geometry/`).

---

## Task 1: Scaffold the React + Vite + Tailwind project

**Files:**
- Modify: `examples/web/package.json`
- Modify: `examples/web/tsconfig.json`
- Modify: `examples/web/vite.config.ts`
- Create: `examples/web/vitest.config.ts`, `examples/web/tailwind.config.js`, `examples/web/postcss.config.js`
- Modify: `examples/web/index.html`
- Create: `examples/web/src/main.tsx`, `examples/web/src/App.tsx`, `examples/web/src/index.css`
- Delete: `examples/web/main.ts`

**Interfaces:**
- Produces: a buildable empty React app; no exported symbols yet.

- [ ] **Step 1: Rewrite `package.json`**

```json
{
  "name": "flatcitybuf-web-demo",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "description": "Browser viewer for the native TypeScript FlatCityBuf reader (@cityjson/flatcitybuf)",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "typecheck": "tsc --noEmit",
    "test": "vitest run"
  },
  "dependencies": {
    "@cityjson/flatcitybuf": "file:../../src/ts",
    "@deck.gl/core": "^9.0.0",
    "@deck.gl/mesh-layers": "^9.0.0",
    "@deck.gl/react": "^9.0.0",
    "earcut": "^3.0.0",
    "jotai": "^2.10.0",
    "maplibre-gl": "^4.7.0",
    "proj4": "^2.15.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-map-gl": "^7.1.7"
  },
  "devDependencies": {
    "@types/earcut": "^3.0.0",
    "@types/proj4": "^2.5.5",
    "@types/react": "^18.3.0",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.0",
    "autoprefixer": "^10.4.0",
    "postcss": "^8.4.0",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.6.0",
    "vite": "^8.1.5",
    "vitest": "^3.0.0"
  }
}
```

- [ ] **Step 2: Rewrite `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "jsx": "react-jsx",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "resolveJsonModule": true,
    "types": ["node"]
  },
  "include": ["src", "vite.config.ts", "vitest.config.ts"]
}
```

- [ ] **Step 3: Rewrite `vite.config.ts`**

```ts
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// A plain app build. index.html is the entry point; base './' lets the built
// dist/index.html open from any path. The only runtime dependency worth noting
// is @cityjson/flatcitybuf, wired via file:../../src/ts so npm install picks up
// its dist/ build.
export default defineConfig({
  base: './',
  plugins: [react()],
})
```

- [ ] **Step 4: Create `vitest.config.ts`**

```ts
import { defineConfig } from 'vitest/config'

// Pure-module tests only (crs, geometry, reader). They run in Node: reader.test
// opens examples/data/delft.fcb from disk via FcbReader.fromBytes. No DOM.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
})
```

- [ ] **Step 5: Create `tailwind.config.js` and `postcss.config.js`**

```js
// tailwind.config.js
/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: { extend: {} },
  plugins: [],
}
```

```js
// postcss.config.js
export default {
  plugins: { tailwindcss: {}, autoprefixer: {} },
}
```

- [ ] **Step 6: Rewrite `index.html`**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>FlatCityBuf viewer</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 7: Create `src/index.css`, `src/main.tsx`, `src/App.tsx`, delete `main.ts`**

```css
/* src/index.css */
@tailwind base;
@tailwind components;
@tailwind utilities;

html, body, #root { height: 100%; margin: 0; }
```

```tsx
// src/main.tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import './index.css'
import 'maplibre-gl/dist/maplibre-gl.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
```

```tsx
// src/App.tsx
export function App() {
  return <div className="h-full grid place-items-center">FlatCityBuf viewer</div>
}
```

```bash
rm examples/web/main.ts
```

- [ ] **Step 8: Install and verify build**

Run: `cd examples/web && npm install && npm run typecheck && npm run build`
Expected: install succeeds, typecheck clean, `dist/` produced.

- [ ] **Step 9: Commit**

```bash
git add -A examples/web
git commit -m "feat(examples): scaffold React+deck.gl viewer, remove text demo"
```

---

## Task 2: CRS module — allowlist, resolve, forward/inverse reprojection

**Files:**
- Create: `examples/web/src/crs/index.ts`
- Test: `examples/web/src/crs/crs.test.ts`

**Interfaces:**
- Produces:
  - `interface CrsStatus { code: number | null; supported: boolean; label: string }`
  - `resolveCrs(referenceSystem: string | undefined): CrsStatus`
  - `forward(code: number, xy: [number, number]): [number, number]` — source → `[lng, lat]`
  - `inverse(code: number, lngLat: [number, number]): [number, number]` — `[lng, lat]` → source
  - `bboxToSource(code: number, west: number, south: number, east: number, north: number): [number, number, number, number]` — densified lng/lat rect → source-CRS `[minX, minY, maxX, maxY]` envelope

- [ ] **Step 1: Write the failing test**

```ts
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd examples/web && npx vitest run src/crs`
Expected: FAIL — `./index` has no exports.

- [ ] **Step 3: Write the implementation**

```ts
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
  const m = /(\d+)\s*$/.exec(referenceSystem)
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

/** A lng/lat rectangle is not a rectangle in a transverse-Mercator source CRS,
 *  so inverse-project the boundary densified with edge midpoints and take the
 *  source-CRS envelope. Returns `[minX, minY, maxX, maxY]`. */
export function bboxToSource(
  code: number, west: number, south: number, east: number, north: number,
): [number, number, number, number] {
  const midX = (west + east) / 2
  const midY = (south + north) / 2
  const samples: [number, number][] = [
    [west, south], [east, south], [east, north], [west, north],
    [midX, south], [east, midY], [midX, north], [west, midY],
  ]
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  for (const s of samples) {
    const [x, y] = inverse(code, s)
    minX = Math.min(minX, x); minY = Math.min(minY, y)
    maxX = Math.max(maxX, x); maxY = Math.max(maxY, y)
  }
  return [minX, minY, maxX, maxY]
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd examples/web && npx vitest run src/crs`
Expected: PASS (all cases).

- [ ] **Step 5: Commit**

```bash
git add examples/web/src/crs
git commit -m "feat(examples): CRS allowlist + forward/inverse reprojection"
```

---

## Task 3: Geometry module — surface walk, triangulation, per-feature mesh

**Files:**
- Create: `examples/web/src/geometry/index.ts`
- Test: `examples/web/src/geometry/geometry.test.ts`

**Interfaces:**
- Consumes: `CityJSONFeature`, `Transform` from `@cityjson/flatcitybuf`.
- Produces:
  - `interface Mesh { positions: Float32Array; normals: Float32Array; indices: Uint32Array }`
  - `interface FeatureMesh { centroidLngLat: [number, number]; mesh: Mesh; triangleCount: number }`
  - `collectSurfaces(boundaries: unknown): number[][][]` — flattens any MultiSurface/Solid/MultiSolid nesting to a list of surfaces, each `[exteriorRing, ...holes]`, each ring a list of vertex indices.
  - `buildFeatureMesh(feature: CityJSONFeature, transform: Transform, reproject: (xy: [number, number]) => [number, number]): FeatureMesh | null` — returns `null` when the feature yields no valid triangles.

- [ ] **Step 1: Write the failing test**

```ts
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd examples/web && npx vitest run src/geometry`
Expected: FAIL — no exports.

- [ ] **Step 3: Write the implementation**

```ts
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
function newellNormal(ring: number[][], world: number[][]): [number, number, number] {
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd examples/web && npx vitest run src/geometry`
Expected: PASS (8 triangles; degenerate → null).

- [ ] **Step 5: Commit**

```bash
git add examples/web/src/geometry
git commit -m "feat(examples): CityJSON surface triangulation + per-feature mesh"
```

---

## Task 4: Reader module — open, header model, coercion, query + paginate

**Files:**
- Create: `examples/web/src/reader/index.ts`
- Test: `examples/web/src/reader/reader.test.ts`

**Interfaces:**
- Consumes: `FcbReader`, `ColumnInfo`, `ColumnType`, `HeaderView`, `Feature`, `AttrCondition`, `Operator`, `FcbError` from `@cityjson/flatcitybuf`.
- Produces:
  - `interface QueryableColumn { name: string; type: ColumnType; typeName: string }`
  - `interface HeaderModel { version: string; featuresCount: number; crs: CrsStatus; extent?: [number,number,number,number,number,number]; columns: ColumnInfo[]; queryable: QueryableColumn[] }`
  - `headerModel(header: HeaderView): HeaderModel`
  - `coerceAttrValue(column: ColumnInfo, raw: string): unknown` (ported from the old demo, plus rejects non-queryable types)
  - `interface QuerySpec { bboxSource?: [number,number,number,number]; where?: AttrCondition[]; limit: number; offset: number }`
  - `runQuery(reader: FcbReader, spec: QuerySpec): Promise<{ features: Feature[]; total: number | undefined }>`
  - `openFromUrl(url: string): Promise<FcbReader>` / `openFromBlob(blob: Blob): Promise<FcbReader>`
  - `describeError(err: unknown): string`

- [ ] **Step 1: Write the failing test**

```ts
// src/reader/reader.test.ts
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { FcbReader } from '@cityjson/flatcitybuf'
import { describe, expect, it } from 'vitest'
import { coerceAttrValue, headerModel, runQuery } from './index'

const fcbPath = fileURLToPath(
  new URL('../../../data/delft.fcb', import.meta.url),
)
async function open(): Promise<FcbReader> {
  return FcbReader.fromBytes(new Uint8Array(readFileSync(fcbPath)))
}

describe('headerModel', () => {
  it('reports CRS, extent and queryable columns for delft.fcb', async () => {
    const m = headerModel((await open()).header)
    expect(m.crs.code).toBe(7415)
    expect(m.crs.supported).toBe(true)
    expect(m.extent).toBeDefined()
    // Every queryable column must be a header-declared column.
    const names = new Set(m.columns.map((c) => c.name))
    for (const q of m.queryable) expect(names.has(q.name)).toBe(true)
  })
})

describe('coerceAttrValue', () => {
  it('rejects a non-integer for an Int column', () => {
    const col = { index: 0, name: 'n', type: 4 /* Int */, nullable: true } as never
    expect(() => coerceAttrValue(col, '1.5')).toThrow()
  })
})

describe('runQuery pagination', () => {
  it('pages a bbox query without exceeding the limit', async () => {
    const reader = await open()
    const ext = reader.header.info.geographicalExtent!
    const bboxSource: [number, number, number, number] =
      [ext[0], ext[1], ext[3], ext[4]]
    const page1 = await runQuery(reader, { bboxSource, limit: 5, offset: 0 })
    expect(page1.features.length).toBeLessThanOrEqual(5)
    expect(page1.total).toBeGreaterThan(0)
    if ((page1.total ?? 0) > 5) {
      const page2 = await runQuery(reader, { bboxSource, limit: 5, offset: 5 })
      expect(page2.features[0]?.id).not.toBe(page1.features[0]?.id)
    }
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd examples/web && npx vitest run src/reader`
Expected: FAIL — no exports.

- [ ] **Step 3: Write the implementation**

```ts
// src/reader/index.ts
import {
  type AttrCondition, type ColumnInfo, ColumnType, FcbError, FcbReader,
  type Feature, type HeaderView,
} from '@cityjson/flatcitybuf'
import { type CrsStatus, resolveCrs } from '../crs/index'

export interface QueryableColumn {
  name: string
  type: ColumnType
  typeName: string
}

export interface HeaderModel {
  version: string
  featuresCount: number
  crs: CrsStatus
  extent?: [number, number, number, number, number, number]
  columns: ColumnInfo[]
  queryable: QueryableColumn[]
}

export function columnTypeName(type: ColumnType): string {
  return ColumnType[type] ?? `Unknown(${type})`
}

const QUERYABLE_TYPES = new Set<string>([
  'Bool', 'Byte', 'UByte', 'Short', 'UShort', 'Int', 'UInt',
  'Long', 'ULong', 'Float', 'Double', 'DateTime', 'String',
])

/** Only indexed, non-JSON/Binary columns can be queried (static-btree). Map
 *  the header's attribute indices to their columns and keep the supported
 *  types. */
export function headerModel(header: HeaderView): HeaderModel {
  const info = header.info
  const byIndex = new Map(info.columns.map((c) => [c.index, c]))
  const queryable: QueryableColumn[] = []
  for (const ai of info.attributeIndices) {
    const col = byIndex.get(ai.columnIndex)
    if (col === undefined) continue
    const typeName = columnTypeName(col.type)
    if (!QUERYABLE_TYPES.has(typeName)) continue
    queryable.push({ name: col.name, type: col.type, typeName })
  }
  return {
    version: info.version,
    featuresCount: info.featuresCount,
    crs: resolveCrs(info.referenceSystem),
    extent: info.geographicalExtent,
    columns: info.columns,
    queryable,
  }
}

/** Coerces raw text into the type `select`'s `where` expects. Ported from the
 *  old demo; Json/Binary (and any non-queryable type) are rejected. */
export function coerceAttrValue(column: ColumnInfo, raw: string): unknown {
  switch (columnTypeName(column.type)) {
    case 'Bool':
      if (raw === 'true') return true
      if (raw === 'false') return false
      throw new Error(`"${raw}" is not "true" or "false"`)
    case 'Byte': case 'UByte': case 'Short': case 'UShort':
    case 'Int': case 'UInt': {
      const n = Number(raw)
      if (!Number.isInteger(n)) throw new Error(`"${raw}" is not an integer`)
      return n
    }
    case 'Long': case 'ULong':
      return BigInt(raw)
    case 'Float': case 'Double': {
      const n = Number(raw)
      if (Number.isNaN(n)) throw new Error(`"${raw}" is not a number`)
      return n
    }
    case 'DateTime': {
      const d = new Date(raw)
      if (Number.isNaN(d.getTime())) throw new Error(`"${raw}" is not a valid date`)
      return d
    }
    case 'String':
      return raw
    default:
      throw new Error(
        `column "${column.name}" (${columnTypeName(column.type)}) is not queryable`,
      )
  }
}

export interface QuerySpec {
  bboxSource?: [number, number, number, number]
  where?: AttrCondition[]
  limit: number
  offset: number
}

/** Runs one page of a query and drains its cursor. `total` is the cursor's
 *  match count (every match, not just this page). */
export async function runQuery(
  reader: FcbReader, spec: QuerySpec,
): Promise<{ features: Feature[]; total: number | undefined }> {
  const cursor = await reader.select({
    spatial: spec.bboxSource
      ? { kind: 'bbox', value: spec.bboxSource }
      : undefined,
    where: spec.where && spec.where.length > 0 ? spec.where : undefined,
    limit: spec.limit,
    offset: spec.offset,
  })
  const features: Feature[] = []
  for await (const f of cursor) features.push(f)
  return { features, total: cursor.featuresCount ?? features.length }
}

export async function openFromUrl(url: string): Promise<FcbReader> {
  return FcbReader.fromUrl(url)
}
export async function openFromBlob(blob: Blob): Promise<FcbReader> {
  return FcbReader.fromBlob(blob)
}

export function describeError(err: unknown): string {
  if (err instanceof FcbError) return `${err.code}: ${err.message}`
  if (err instanceof Error) return err.message
  return String(err)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd examples/web && npx vitest run src/reader`
Expected: PASS.

- [ ] **Step 5: Run the full pure-module suite + typecheck**

Run: `cd examples/web && npm run test && npm run typecheck`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add examples/web/src/reader
git commit -m "feat(examples): reader core — header model, coercion, paged query"
```

---

## Task 5: Jotai store + hooks (useFcbData, useDrawBbox)

**Files:**
- Create: `examples/web/src/store/index.ts`
- Create: `examples/web/src/hooks/useFcbData.ts`, `examples/web/src/hooks/useDrawBbox.ts`

**Interfaces:**
- Consumes: `runQuery`, `headerModel`, `openFromUrl`, `openFromBlob`, `describeError`, `coerceAttrValue`, `HeaderModel` (Task 4); `buildFeatureMesh`, `FeatureMesh` (Task 3); `forward`, `bboxToSource` (Task 2); `FcbReader`, `Feature`, `AttrCondition`, `toCityJSONMetadata` (`@cityjson/flatcitybuf`).
- Produces:
  - store atoms: `readerAtom`, `headerAtom`, `metaAtom`, `renderedAtom` (`RenderedFeature[]`), `totalAtom`, `statusAtom`, `drawAtom`, `selectedAtom`, `colorByAtom`.
  - `interface RenderedFeature { id: string; centroidLngLat: [number, number]; mesh: Mesh; attributes: Record<string, unknown> }`
  - `useFcbData(): { openUrl, openFile, query, loadNext, status, header, rendered, total }`
  - `useDrawBbox(): { drawing, start, points, onMapClick, onMapHover, clear }`

- [ ] **Step 1: Write the store**

```ts
// src/store/index.ts
import type { AttrCondition, FcbReader } from '@cityjson/flatcitybuf'
import { atom } from 'jotai'
import type { Mesh } from '../geometry/index'
import type { HeaderModel } from '../reader/index'

export interface RenderedFeature {
  id: string
  centroidLngLat: [number, number]
  mesh: Mesh
  attributes: Record<string, unknown>
}

export const readerAtom = atom<FcbReader | undefined>(undefined)
export const headerAtom = atom<HeaderModel | undefined>(undefined)
export const renderedAtom = atom<RenderedFeature[]>([])
export const totalAtom = atom<number | undefined>(undefined)
export const statusAtom = atom<string>('open a .fcb file to begin')
export const selectedAtom = atom<RenderedFeature | undefined>(undefined)
export const colorByAtom = atom<string | undefined>(undefined)

/** The active query, remembered so "Load Next Batch" can advance the offset. */
export interface ActiveQuery {
  bboxSource?: [number, number, number, number]
  where?: AttrCondition[]
  limit: number
  offset: number
}
export const activeQueryAtom = atom<ActiveQuery | undefined>(undefined)

/** Draw state: the two lng/lat corners the user is placing. */
export interface DrawState {
  active: boolean
  a?: [number, number]
  b?: [number, number]
}
export const drawAtom = atom<DrawState>({ active: false })
```

- [ ] **Step 2: Write `useFcbData`**

```ts
// src/hooks/useFcbData.ts
import { type Feature, toCityJSONMetadata } from '@cityjson/flatcitybuf'
import { useAtom } from 'jotai'
import { useCallback } from 'react'
import { forward } from '../crs/index'
import { buildFeatureMesh } from '../geometry/index'
import {
  describeError, headerModel, openFromBlob, openFromUrl, runQuery,
} from '../reader/index'
import {
  activeQueryAtom, headerAtom, readerAtom, renderedAtom, selectedAtom,
  statusAtom, totalAtom,
} from '../store/index'

export function useFcbData() {
  const [reader, setReader] = useAtom(readerAtom)
  const [header, setHeader] = useAtom(headerAtom)
  const [rendered, setRendered] = useAtom(renderedAtom)
  const [total, setTotal] = useAtom(totalAtom)
  const [status, setStatus] = useAtom(statusAtom)
  const [active, setActive] = useAtom(activeQueryAtom)
  const [, setSelected] = useAtom(selectedAtom)

  const onOpened = useCallback(async (r: Awaited<ReturnType<typeof openFromUrl>>) => {
    setReader(r)
    setHeader(headerModel(r.header))
    setRendered([]); setTotal(undefined); setSelected(undefined)
    setActive(undefined)
    setStatus('file opened')
  }, [setReader, setHeader, setRendered, setTotal, setSelected, setActive, setStatus])

  const openUrl = useCallback(async (url: string) => {
    setStatus(`opening ${url} ...`)
    try { await onOpened(await openFromUrl(url)) }
    catch (e) { setStatus(`failed to open URL: ${describeError(e)}`) }
  }, [onOpened, setStatus])

  const openFile = useCallback(async (file: File) => {
    setStatus(`opening ${file.name} ...`)
    try { await onOpened(await openFromBlob(file)) }
    catch (e) { setStatus(`failed to open file: ${describeError(e)}`) }
  }, [onOpened, setStatus])

  const render = useCallback((features: Feature[]) => {
    if (reader === undefined || header === undefined) return
    if (!header.crs.supported || header.crs.code === null) {
      setStatus('CRS not supported — cannot georeference; not rendering')
      return
    }
    const code = header.crs.code
    const transform = toCityJSONMetadata(reader.header).transform
    const out = []
    let skipped = 0
    for (const f of features) {
      const cj = f.toCityJSON(reader.header)
      const fm = buildFeatureMesh(cj, transform, (xy) => forward(code, xy))
      if (fm === null) { skipped++; continue }
      const primary = Object.values(cj.CityObjects)[0]
      out.push({
        id: f.id, centroidLngLat: fm.centroidLngLat, mesh: fm.mesh,
        attributes: primary?.attributes ?? {},
      })
    }
    setRendered(out)
    setStatus(`${out.length} rendered${skipped ? `, ${skipped} skipped` : ''}`)
  }, [reader, header, setRendered, setStatus])

  const query = useCallback(async (
    spec: { bboxSource?: [number, number, number, number]
            where?: import('@cityjson/flatcitybuf').AttrCondition[]; limit: number },
  ) => {
    if (reader === undefined) return
    const q = { ...spec, offset: 0 }
    setActive(q); setSelected(undefined)
    setStatus('querying...')
    try {
      const { features, total: t } = await runQuery(reader, q)
      setTotal(t); render(features)
    } catch (e) { setStatus(`query failed: ${describeError(e)}`) }
  }, [reader, render, setActive, setSelected, setStatus, setTotal])

  const loadNext = useCallback(async () => {
    if (reader === undefined || active === undefined) return
    const q = { ...active, offset: active.offset + active.limit }
    setActive(q)
    setStatus('loading next batch...')
    try {
      const { features } = await runQuery(reader, q)
      render(features) // replaces the rendered set with the next page
    } catch (e) { setStatus(`load failed: ${describeError(e)}`) }
  }, [reader, active, render, setActive, setStatus])

  return { openUrl, openFile, query, loadNext, status, header, rendered, total,
           hasMore: total !== undefined && active !== undefined
             && active.offset + active.limit < total }
}
```

- [ ] **Step 3: Write `useDrawBbox`**

```ts
// src/hooks/useDrawBbox.ts
import { useAtom } from 'jotai'
import { useCallback } from 'react'
import { drawAtom } from '../store/index'

/** Two-click rectangle draw. `onMapClick` receives a lng/lat coordinate from
 *  deck.gl's pick info; first click sets corner A, second sets B and finishes.
 *  `onMapHover` rubber-bands B while placing. */
export function useDrawBbox() {
  const [draw, setDraw] = useAtom(drawAtom)

  const start = useCallback(() => setDraw({ active: true }), [setDraw])
  const clear = useCallback(() => setDraw({ active: false }), [setDraw])

  const onMapClick = useCallback((coord: [number, number]) => {
    setDraw((d) => {
      if (!d.active) return d
      if (d.a === undefined) return { ...d, a: coord, b: coord }
      return { active: false, a: d.a, b: coord }
    })
  }, [setDraw])

  const onMapHover = useCallback((coord: [number, number]) => {
    setDraw((d) => (d.active && d.a !== undefined ? { ...d, b: coord } : d))
  }, [setDraw])

  const bbox = draw.a && draw.b
    ? [Math.min(draw.a[0], draw.b[0]), Math.min(draw.a[1], draw.b[1]),
       Math.max(draw.a[0], draw.b[0]), Math.max(draw.a[1], draw.b[1])] as
       [number, number, number, number]
    : undefined

  return { draw, start, clear, onMapClick, onMapHover, bbox }
}
```

- [ ] **Step 4: Typecheck**

Run: `cd examples/web && npm run typecheck`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add examples/web/src/store examples/web/src/hooks
git commit -m "feat(examples): Jotai store + useFcbData/useDrawBbox hooks"
```

---

## Task 6: Components — MapView, SourcePanel, HeaderPanel, QueryPanel, FeatureInspector

**Files:**
- Create: `examples/web/src/components/MapView.tsx`, `SourcePanel.tsx`, `HeaderPanel.tsx`, `QueryPanel.tsx`, `FeatureInspector.tsx`

**Interfaces:**
- Consumes: hooks + store from Task 5; `SimpleMeshLayer` from `@deck.gl/mesh-layers`; `COORDINATE_SYSTEM` from `@deck.gl/core`; `DeckGL` from `@deck.gl/react`; `Map` from `react-map-gl/maplibre`.
- Produces: the five components, default-exported as named exports, assembled by `App.tsx` in Task 7.

- [ ] **Step 1: Write `MapView.tsx`**

```tsx
// src/components/MapView.tsx
import { COORDINATE_SYSTEM } from '@deck.gl/core'
import { SimpleMeshLayer } from '@deck.gl/mesh-layers'
import { DeckGL } from '@deck.gl/react'
import { useAtom, useAtomValue } from 'jotai'
import { useMemo } from 'react'
import { Map } from 'react-map-gl/maplibre'
import { useDrawBbox } from '../hooks/useDrawBbox'
import { colorByAtom, renderedAtom, selectedAtom } from '../store/index'
import type { RenderedFeature } from '../store/index'

const BASEMAP = 'https://basemaps.cartocdn.com/gl/positron-gl-style/style.json'
const INITIAL_VIEW = { longitude: 4.36, latitude: 52.0, zoom: 13, pitch: 45, bearing: 0 }

/** Colours a feature by a numeric attribute, if `colorBy` is set and numeric;
 *  otherwise a steel blue. */
function featureColor(f: RenderedFeature, colorBy: string | undefined): [number, number, number] {
  if (colorBy !== undefined) {
    const v = f.attributes[colorBy]
    if (typeof v === 'number') {
      const t = Math.max(0, Math.min(1, (v % 100) / 100))
      return [Math.round(50 + 200 * t), Math.round(120 * (1 - t) + 60), 180]
    }
  }
  return [70, 130, 180]
}

export function MapView() {
  const rendered = useAtomValue(renderedAtom)
  const colorBy = useAtomValue(colorByAtom)
  const [selected, setSelected] = useAtom(selectedAtom)
  const { draw, onMapClick, onMapHover, bbox } = useDrawBbox()

  const layers = useMemo(() => {
    const meshLayers = rendered.map((f) => new SimpleMeshLayer<RenderedFeature>({
      id: `feat-${f.id}`,
      data: [f],
      mesh: {
        attributes: {
          positions: { value: f.mesh.positions, size: 3 },
          normals: { value: f.mesh.normals, size: 3 },
        },
        indices: { value: f.mesh.indices, size: 1 },
      },
      coordinateSystem: COORDINATE_SYSTEM.METER_OFFSETS,
      coordinateOrigin: [f.centroidLngLat[0], f.centroidLngLat[1], 0],
      getPosition: () => [0, 0, 0],
      getColor: () => {
        const c = featureColor(f, colorBy)
        return f.id === selected?.id ? [255, 160, 0] : c
      },
      pickable: true,
      updateTriggers: { getColor: [colorBy, selected?.id] },
    }))
    return meshLayers
  }, [rendered, colorBy, selected])

  return (
    <DeckGL
      initialViewState={INITIAL_VIEW}
      controller
      layers={layers}
      getCursor={() => (draw.active ? 'crosshair' : 'grab')}
      onClick={(info) => {
        if (draw.active && info.coordinate) {
          onMapClick([info.coordinate[0], info.coordinate[1]])
        } else if (info.layer && (info.object as RenderedFeature)) {
          setSelected(info.object as RenderedFeature)
        }
      }}
      onHover={(info) => {
        if (draw.active && info.coordinate) {
          onMapHover([info.coordinate[0], info.coordinate[1]])
        }
      }}
    >
      <Map mapStyle={BASEMAP} />
      {bbox && (
        <div className="pointer-events-none absolute top-2 left-2 rounded bg-black/60 px-2 py-1 text-xs text-white">
          bbox: {bbox.map((n) => n.toFixed(4)).join(', ')}
        </div>
      )}
    </DeckGL>
  )
}
```

- [ ] **Step 2: Write `SourcePanel.tsx`**

```tsx
// src/components/SourcePanel.tsx
import { useState } from 'react'
import { useFcbData } from '../hooks/useFcbData'

const DEFAULT_URL =
  'https://storage.googleapis.com/flatcitybuf/3dbag_subset_all_index.fcb'

export function SourcePanel() {
  const { openUrl, openFile, status } = useFcbData()
  const [url, setUrl] = useState(DEFAULT_URL)
  return (
    <section className="space-y-2">
      <h2 className="text-sm font-semibold">1. Open a file</h2>
      <div className="flex gap-2">
        <input
          className="flex-1 rounded border px-2 py-1 text-sm"
          value={url} onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void openUrl(url) }}
        />
        <button className="rounded border px-3 py-1 text-sm"
          onClick={() => void openUrl(url)}>Load URL</button>
      </div>
      <label className="block rounded border border-dashed px-3 py-4 text-center text-sm cursor-pointer">
        Choose a local .fcb file
        <input type="file" accept=".fcb" className="hidden"
          onChange={(e) => { const f = e.target.files?.[0]; if (f) void openFile(f) }} />
      </label>
      <p className="text-xs opacity-70">{status}</p>
    </section>
  )
}
```

- [ ] **Step 3: Write `HeaderPanel.tsx`**

```tsx
// src/components/HeaderPanel.tsx
import { useFcbData } from '../hooks/useFcbData'
import { columnTypeName } from '../reader/index'

export function HeaderPanel() {
  const { header } = useFcbData()
  if (header === undefined) return null
  return (
    <section className="space-y-1 text-xs">
      <h2 className="text-sm font-semibold">2. Header</h2>
      <div>version: {header.version}</div>
      <div>features: {header.featuresCount || 'unknown'}</div>
      <div className={header.crs.supported ? '' : 'text-red-600'}>
        CRS: {header.crs.label}{header.crs.supported ? '' : ' (unsupported — not georeferenced)'}
      </div>
      <details>
        <summary>columns ({header.columns.length})</summary>
        <ul className="mt-1 space-y-0.5">
          {header.columns.map((c) => (
            <li key={c.name}>{c.name} — {columnTypeName(c.type)}</li>
          ))}
        </ul>
      </details>
    </section>
  )
}
```

- [ ] **Step 4: Write `QueryPanel.tsx`**

```tsx
// src/components/QueryPanel.tsx
import type { AttrCondition, Operator } from '@cityjson/flatcitybuf'
import { useAtom } from 'jotai'
import { useState } from 'react'
import { bboxToSource } from '../crs/index'
import { useDrawBbox } from '../hooks/useDrawBbox'
import { useFcbData } from '../hooks/useFcbData'
import { coerceAttrValue } from '../reader/index'
import { colorByAtom } from '../store/index'

const OPERATORS: Operator[] = ['Eq', 'Ne', 'Gt', 'Ge', 'Lt', 'Le']

export function QueryPanel() {
  const { header, query, loadNext, total, rendered, hasMore } = useFcbData()
  const { draw, start, clear, bbox } = useDrawBbox()
  const [colorBy, setColorBy] = useAtom(colorByAtom)
  const [field, setField] = useState('')
  const [op, setOp] = useState<Operator>('Eq')
  const [value, setValue] = useState('')
  const [limit, setLimit] = useState(200)
  const [err, setErr] = useState('')

  if (header === undefined) return null
  const cols = header.columns
  const queryable = header.queryable

  const run = () => {
    setErr('')
    let bboxSource: [number, number, number, number] | undefined
    if (bbox && header.crs.code !== null && header.crs.supported) {
      bboxSource = bboxToSource(header.crs.code, bbox[0], bbox[1], bbox[2], bbox[3])
    }
    let where: AttrCondition[] | undefined
    if (field !== '') {
      const col = cols.find((c) => c.name === field)
      if (col === undefined) { setErr('unknown field'); return }
      try {
        where = [{ field, operator: op, value: coerceAttrValue(col, value) }]
      } catch (e) { setErr(String(e instanceof Error ? e.message : e)); return }
    }
    if (bboxSource === undefined && where === undefined) {
      setErr('draw a bbox or set an attribute condition'); return
    }
    void query({ bboxSource, where, limit })
  }

  return (
    <section className="space-y-2 text-sm">
      <h2 className="text-sm font-semibold">3. Query</h2>
      <div className="flex items-center gap-2">
        <button className="rounded border px-2 py-1"
          onClick={() => (draw.active ? clear() : start())}>
          {draw.active ? 'cancel draw' : 'draw bbox'}
        </button>
        <span className="text-xs opacity-70">
          {bbox ? 'bbox set' : 'no bbox'}
        </span>
      </div>
      <div className="grid grid-cols-3 gap-1">
        <select className="rounded border px-1 py-1 text-xs"
          value={field} onChange={(e) => setField(e.target.value)}>
          <option value="">(no attribute)</option>
          {queryable.map((c) => (
            <option key={c.name} value={c.name}>{c.name} ({c.typeName})</option>
          ))}
        </select>
        <select className="rounded border px-1 py-1 text-xs"
          value={op} onChange={(e) => setOp(e.target.value as Operator)}>
          {OPERATORS.map((o) => <option key={o} value={o}>{o}</option>)}
        </select>
        <input className="rounded border px-1 py-1 text-xs" value={value}
          onChange={(e) => setValue(e.target.value)} placeholder="value" />
      </div>
      <div className="flex items-center gap-2 text-xs">
        <label>limit</label>
        <input type="number" className="w-20 rounded border px-1 py-1"
          value={limit} onChange={(e) => setLimit(Number(e.target.value))} />
        <button className="rounded border px-2 py-1" onClick={run}>Run</button>
        {hasMore && (
          <button className="rounded border px-2 py-1"
            onClick={() => void loadNext()}>Load next batch</button>
        )}
      </div>
      <div className="flex items-center gap-2 text-xs">
        <label>colour by</label>
        <select className="rounded border px-1 py-1"
          value={colorBy ?? ''} onChange={(e) => setColorBy(e.target.value || undefined)}>
          <option value="">(uniform)</option>
          {cols.map((c) => <option key={c.name} value={c.name}>{c.name}</option>)}
        </select>
      </div>
      {err && <p className="text-xs text-red-600">{err}</p>}
      <p className="text-xs opacity-70">
        showing {rendered.length}{total !== undefined ? ` of ${total}` : ''}
      </p>
    </section>
  )
}
```

- [ ] **Step 5: Write `FeatureInspector.tsx`**

```tsx
// src/components/FeatureInspector.tsx
import { useAtomValue } from 'jotai'
import { selectedAtom } from '../store/index'

export function FeatureInspector() {
  const selected = useAtomValue(selectedAtom)
  if (selected === undefined) return null
  return (
    <section className="space-y-1 text-xs">
      <h2 className="text-sm font-semibold">4. Selected: {selected.id}</h2>
      <table className="w-full">
        <tbody>
          {Object.entries(selected.attributes).map(([k, v]) => (
            <tr key={k}><td className="pr-2 opacity-70">{k}</td>
              <td>{String(v)}</td></tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
```

- [ ] **Step 6: Typecheck**

Run: `cd examples/web && npm run typecheck`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add examples/web/src/components
git commit -m "feat(examples): MapView + source/header/query/inspector panels"
```

---

## Task 7: Assemble App, README, build, manual smoke

**Files:**
- Modify: `examples/web/src/App.tsx`
- Create: `examples/web/README.md`

**Interfaces:**
- Consumes: all five components (Task 6).

- [ ] **Step 1: Write `App.tsx`**

```tsx
// src/App.tsx
import { FeatureInspector } from './components/FeatureInspector'
import { HeaderPanel } from './components/HeaderPanel'
import { MapView } from './components/MapView'
import { QueryPanel } from './components/QueryPanel'
import { SourcePanel } from './components/SourcePanel'

export function App() {
  return (
    <div className="flex h-full">
      <aside className="w-80 shrink-0 space-y-4 overflow-y-auto border-r p-4">
        <h1 className="text-base font-bold">FlatCityBuf viewer</h1>
        <p className="text-xs opacity-70">
          Native TypeScript reader (@cityjson/flatcitybuf) — no WASM, no server.
        </p>
        <SourcePanel />
        <HeaderPanel />
        <QueryPanel />
        <FeatureInspector />
      </aside>
      <main className="relative flex-1">
        <MapView />
      </main>
    </div>
  )
}
```

- [ ] **Step 2: Write `README.md`**

````markdown
# FlatCityBuf web viewer

A browser viewer for [FlatCityBuf](../../README.md) built on the native
TypeScript reader `@cityjson/flatcitybuf` — no WASM, no server component. It
opens a `.fcb` over HTTP range requests or from a local file, runs bounding-box
and attribute queries, and renders the returned 3D buildings on a MapLibre
basemap with deck.gl.

> Supersedes the archived `cityjson/flatcitybuf-web-prototype`, which used the
> old WASM binding.

## Run

```bash
cd examples/web
npm install      # picks up ../../src/ts via a file: dependency
npm run dev
```

Open the printed URL, load the default 3DBAG subset URL (or a local `.fcb`),
draw a bbox, and run a query.

## How it works

- `src/reader/` — opens the file and drives `reader.select(...)` (framework-free).
- `src/geometry/` — triangulates CityJSON surfaces into meshes.
- `src/crs/` — reprojects EPSG:7415 ↔ WGS84 (proj4 allowlist).
- Each returned feature becomes one deck.gl `SimpleMeshLayer` anchored at its
  reprojected centroid.

## Tests

```bash
npm test         # pure-module unit tests (crs, geometry, reader)
```
````

- [ ] **Step 3: Build**

Run: `cd examples/web && npm run typecheck && npm run test && npm run build`
Expected: all green; `dist/` produced.

- [ ] **Step 4: Manual smoke checklist (record results in the commit message)**

Run: `cd examples/web && npm run dev`, open the URL, and confirm:
1. Default 3DBAG URL loads; HeaderPanel shows `CRS: EPSG:7415` (supported).
2. "draw bbox" → two clicks on the map set a bbox (overlay shows coordinates).
3. "Run" renders buildings **aligned to the streets** on the basemap.
4. Attribute field dropdown lists only indexed columns; a valid condition filters.
5. "Load next batch" appears when total > limit and advances the page.
6. Clicking a building highlights it and fills the inspector.
7. "colour by" a numeric column recolours the buildings.

- [ ] **Step 5: Commit**

```bash
git add examples/web/src/App.tsx examples/web/README.md
git commit -m "feat(examples): assemble viewer app + README; manual smoke passed"
```

---

## Task 8: Archive the prototype repo

**Files:** none (external action).

- [ ] **Step 1: Confirm with the user immediately before running**

Ask: "Ready to archive `cityjson/flatcitybuf-web-prototype`? This is hard to reverse." Proceed only on explicit yes.

- [ ] **Step 2: Verify gh auth + admin, then archive**

```bash
gh auth status
gh repo view cityjson/flatcitybuf-web-prototype --json viewerPermission
gh repo archive cityjson/flatcitybuf-web-prototype --yes
```
Expected: `viewerPermission` is `ADMIN`; archive succeeds. If not ADMIN, stop and report — the user archives it via the GitHub UI.

- [ ] **Step 3: (Optional) push a deprecation note to the old README** — only if the user wants it; otherwise skip.

---

## Self-Review

**Spec coverage** (spec §→task):
- §1 placement / replace text demo → Task 1 (delete `main.ts`, rewrite scaffold).
- §2 stack → Task 1 deps; `@deck.gl/mesh-layers` import → Task 6.
- §3.1 per-feature anchoring → Task 3 `buildFeatureMesh` + Task 6 `SimpleMeshLayer` per feature with `METER_OFFSETS` + per-feature `coordinateOrigin`.
- §3.2 triangulation (holes, Newell winding, degenerate skip) → Task 3.
- §3.3 CRS allowlist + inverse-projected densified bbox → Task 2 (`CRS_DEFS`, `bboxToSource`) + Task 4/5 wiring.
- §3.4 Z metres-up contract → Task 3 (Z passed through) + README.
- §4 module boundaries → Tasks 2–6 file structure.
- §5 features 1–6 → Task 5 (`openUrl/openFile/query/loadNext`, colour/inspect atoms) + Task 6 panels; indexed-column restriction → Task 4 `headerModel.queryable` + Task 6 QueryPanel.
- §6 errors + testing → `describeError` (Task 4), unsupported-CRS guard (Task 5 `render`), unit tests (Tasks 2–4), manual smoke (Task 7).
- §7 non-goals → not implemented (correct).
- §8 process / archive → Task 8.

**Placeholder scan:** no TBD/TODO; every code step shows complete code; the only "optional" step (Task 8 Step 3) is gated on explicit user opt-in, not a placeholder.

**Type consistency:** `HeaderModel`, `QueryableColumn`, `RenderedFeature`, `Mesh`, `FeatureMesh`, `QuerySpec`, `ActiveQuery`, `DrawState` are each defined once and consumed with matching field names across tasks. `buildFeatureMesh(feature, transform, reproject)` signature matches its call in `useFcbData`. `runQuery(reader, spec)` matches its calls. `forward`/`inverse`/`bboxToSource` signatures match QueryPanel/useFcbData usage. `coerceAttrValue(column, raw)` and `columnTypeName(type)` are exported from `reader/index.ts` and imported where used.

**Note for the implementer:** exact dependency minor versions may need adjustment at install time (deck.gl 9.x, react-map-gl 7.x pairs with maplibre 4.x). If `npm install` resolves a conflict, pin the compatible pair and record it in the Task 1 commit. deck.gl `SimpleMeshLayer` `mesh` accepts the `{attributes:{positions,normals},indices}` shape shown; if a deck.gl 9.x point release changes it, consult the mesh-layers docs (do not switch layers).
```
