# FlatCityBuf Web Example Viewer — Design

**Date:** 2026-07-22
**Status:** Approved (revised after codex gpt-5.6-sol review; pending user re-review)
**Supersedes:** `cityjson/flatcitybuf-web-prototype` (to be archived)

## 1. Goal & placement

Replace the vanilla, text-only demo currently at `examples/web` with a
**React + deck.gl + MapLibre** viewer that renders the 3D buildings a query
returns, on a real geographic basemap. The example demonstrates the pure
TypeScript reader `@cityjson/flatcitybuf` end to end — opening a `.fcb` over
HTTP range requests or from a local file, reading the header, running spatial
and attribute queries, and visualizing the results.

The old prototype repo (`cityjson/flatcitybuf-web-prototype`) used the
now-removed WASM binding and is superseded by this example. It will be archived
with `gh repo archive cityjson/flatcitybuf-web-prototype` during implementation,
with an explicit confirmation immediately before the command runs.

### What is replaced

The current `examples/web` is a ~320-line framework-free Vite app that prints
the header and matching feature IDs as text. Its reader-usage logic — attribute
value coercion (`coerceAttrValue`), cursor draining (`collectIds`), error
formatting (`describeError`) — is good and carries forward into the new
`src/reader/` module rather than being rewritten.

## 2. Stack

| Concern | Choice |
|---|---|
| Framework | React + TypeScript |
| Bundler | Vite |
| 3D rendering | deck.gl (`@deck.gl/react` `DeckGL`, `@deck.gl/mesh-layers` `SimpleMeshLayer`) |
| Basemap | `react-map-gl/maplibre` + `maplibre-gl` |
| Reprojection | `proj4` (with an explicit CRS-definition allowlist) |
| State | Jotai (atomic, matches prototype) |
| Styling | Tailwind CSS |
| Reader | `@cityjson/flatcitybuf` via `file:../../src/ts` |

Rationale for deck.gl `SimpleMeshLayer` over alternatives: CityJSON geometry is
Solids/MultiSurfaces of arbitrary 3D polygons (real LoD2 roof shapes). deck.gl
`PolygonLayer`/`SolidPolygonLayer` tessellate a 2D footprint and cannot express
slanted roofs; MapLibre `fill-extrusion` renders flat-roofed prisms —
misrepresenting the data. `SimpleMeshLayer` (note: it lives in
`@deck.gl/mesh-layers`, not `@deck.gl/layers`) renders an arbitrary triangle
mesh, which is the correct model. Cesium was rejected for an `examples/` folder:
multi-MB bundle, static-asset copying, ion-token questions — noise around the
~30 lines of reader usage the example exists to teach.

## 3. Geometry & reprojection (the crux)

The reader emits, per feature, a `CityJSONFeature` with **integer** `vertices`
(`[number, number, number][]`) plus a document `transform` (`scale` +
`translate`) obtained from `toCityJSONMetadata(...).transform`. Real projected
coordinates are `v * scale + translate`, in the file's CRS (EPSG:7415 for the
Dutch 3DBAG data).

`header.info.referenceSystem` is the **short `EPSG:<code>` form** (built as
`${authority}:${code}` in `header/file-info.ts`), *not* an OGC URL. Parse the
numeric code directly off it.

### 3.1 Per-feature anchoring (resolves the meter-offset rotation problem)

A single global origin with raw projected-XY deltas treated as deck.gl
`METER_OFFSETS` is **wrong** for a geo-referenced basemap: a projected CRS has
meridian convergence and scale distortion, so projected-delta axes are not
local east/north. For EPSG:7415 (Amersfoort/RD New) at Delft the convergence is
≈0.8°, which is ≈14 m of edge displacement over a 1 km city extent — visibly
rotated off the streets.

The fix keeps proj4 cheap **and** correct by anchoring **per feature** rather
than once globally:

1. **Resolve CRS.** Parse the numeric EPSG code off
   `header.info.referenceSystem`. Look it up in an explicit **allowlist** of
   bundled proj4 definitions (§3.3). If the code is absent or not in the
   allowlist, do **not** silently fall back — render un-georeferenced or refuse
   georeferenced rendering, and surface the reason in the UI.
2. **Per feature: compute a centroid** in source (projected) coordinates from
   the feature's transformed vertices, and reproject **that one point** to
   `[lng, lat]` (EPSG:4326). One proj4 call per feature.
3. **Vertices → local meters, relative to the feature centroid:**
   `(X - centroidX, Y - centroidY, Z)`. At building scale (~15 m) the
   ENU-vs-projected-delta error is sub-decimetre — invisible — so the
   meter-offset approximation is valid *locally* even though it is not valid
   city-wide.
4. **Render each feature as its own `SimpleMeshLayer`** (`data` = that feature),
   `getPosition` = the centroid `[lng, lat]`, `mesh` = the feature's triangulated
   local-meter mesh. `SimpleMeshLayer` interprets mesh coordinates as metres in
   an ENU frame anchored at `getPosition` — exactly what step 3 produces.

This makes **feature = layer = pick unit = colour unit**, cleanly enabling
click-to-inspect and colour-by-attribute (§5), which are *not* achievable with
a single merged-mesh `SimpleMeshLayer` (its `mesh` is shared across all data
items, giving one colour and one pick id). The cost is N deck.gl layers; N is
**bounded by the pagination result cap** (§5.4) and that cap is documented and
enforced.

### 3.2 Triangulation

Per surface, per ring:

1. Enumerate surfaces by geometry **type** (`MultiSurface`, `Solid`,
   `MultiSolid` nest boundary arrays to different depths); walk to the ring
   level generically.
2. A surface is `[exteriorRing, hole1, hole2, ...]`. Fit a plane to the
   **exterior** ring by Newell's method; project **every** ring (exterior +
   holes) onto that plane's dominant 2D axes.
3. Triangulate with **earcut**, passing hole start-indices so interior rings
   are cut out correctly.
4. **Winding/normals:** orient every output triangle to agree with the
   exterior-ring Newell normal; emit **flat** per-face normals (split vertices
   at surface boundaries — no shared-vertex smoothing across hard building
   edges). Back faces are not culled (interiors of open solids should stay
   visible). Lighting uses these normals.
5. Accumulate per feature into one local-meter mesh (positions `Float32Array`,
   normals `Float32Array`, indices `Uint32Array`).

**Degenerate handling.** Skip, with a surfaced warning (never crash): rings with
fewer than three distinct vertices, near-zero Newell normal (collinear),
repeated consecutive indices, and earcut outputs that fail a basic sanity check.

### 3.3 CRS allowlist & the query bounding box

- **Allowlist.** proj4 bundles only WGS84 and a few CRSs; EPSG:7415/RD New is
  **not** included. Register an explicit map of `code → proj4 definition`
  (EPSG:7415 at minimum, for the demo data). Unknown codes are refused for
  georeferenced rendering rather than guessed.
- **Draw bbox is in lng/lat; `reader.select` expects source-CRS coordinates**
  (the R-tree is built in the file's CRS). Inverse-project the drawn
  rectangle's boundary — corners **plus edge midpoints** (densified, because a
  transverse-Mercator rectangle is not a rectangle in source space) — and take
  the source-CRS **envelope** as the query bbox.

### 3.4 Z / vertical datum (contract)

Z is passed through as **metres, up**, with basemap altitude 0. Compound
vertical-datum transformations (e.g. NAP) are **out of scope**: EPSG:7415
heights are used as-is. A file whose vertical system is not metres-up is not
supported and says so.

## 4. Module boundaries

Reader usage lives in one obvious, framework-free place; rendering is
quarantined behind a small interface so the renderer can be swapped without
touching reader code.

- `src/reader/` — **framework-free teaching core.** Open a `.fcb`
  (`FcbReader.fromUrl` / `FcbReader.fromBlob`), map the header to a UI model,
  drive `reader.select({ spatial, where })`, drain the cursor with pagination,
  and coerce attribute-query values per column type (ported from the current
  demo's `coerceAttrValue`). Depends only on `@cityjson/flatcitybuf`. No React.
- `src/geometry/` — pure CityJSON→mesh triangulation (§3.2). No React, no
  deck.gl. Unit-testable. Also owns the per-feature centroid + local-meter
  vertex transform (§3.1 steps 2–3), taking a reproject function as a parameter
  so it stays pure.
- `src/crs/` — proj4 setup, the CRS allowlist (§3.3), EPSG-code parsing from the
  short form, forward (centroid) and inverse (bbox) reprojection. Pure.
  Unit-testable.
- `src/hooks/` — `useFcbData` (open, query, paginate) and `useDrawBbox`
  (rectangle-draw → densified inverse-projected source-CRS envelope).
- `src/components/` — `MapView` (DeckGL + MapLibre + the per-feature layers),
  `SourcePanel` (URL + local file drop/pick), `HeaderPanel` (version, extent,
  CRS, columns, plus a CRS-support warning when unresolved), `QueryPanel` (bbox
  fields, attribute field/operator/value **restricted to indexed columns**,
  result limit, "Load Next Batch"), `FeatureInspector` (click a building → its
  CityObject attributes).
- `src/store/` — Jotai atoms: `reader`, `header`, `results` (features + meshes +
  layers), `cursor`/pagination state, `draw` state, `selectedFeature`,
  `crsStatus`.

## 5. Features (MVP — all confirmed)

1. **Load from URL + local file** — `FcbReader.fromUrl` (HTTP range requests)
   and `FcbReader.fromBlob` (drop/pick), carried from the current demo.
2. **Draw-bbox spatial query** — draw a rectangle → densified inverse-project to
   a source-CRS envelope (§3.3) → `reader.select({ spatial })` → render matches.
3. **Attribute query** — field/operator/value, combinable with the bbox. The
   field dropdown is **restricted to `header.info.attributeIndices` columns**,
   excluding JSON/Binary types, because only indexed, supported columns are
   queryable (`static-btree/query.ts`). Non-indexed columns are shown as
   display-only in `HeaderPanel`, not offered as query fields.
4. **Pagination / "Load Next Batch"** — a result-count limit (also the **layer
   cap** from §3.1) plus batched draining of the cursor for large result sets.

Folded-in extras (approved; enabled by the per-feature layer model of §3.1):

5. **Click-to-inspect** — clicking a building shows its CityObject attributes.
6. **Colour-by-attribute** — colour each feature's mesh by a selected column,
   teaching what `where` filters on.

**Geometry/attribute ownership.** A feature may hold multiple CityObjects and
multiple LoDs. Policy: render the **highest available LoD**; the feature `id` is
the pick unit; colour-by-attribute reads the feature's primary CityObject
attributes. This is documented in the reader module.

## 6. Error handling & testing

**Errors.** `FcbError` is surfaced as `code: message` (as the current demo
does). Graceful degradation for: unresolved/unsupported CRS (warn + refuse
georeferencing, per §3.3, never silently mislocate), non-`.fcb` input (the
reader's magic-byte check → friendly message), empty result sets, degenerate
geometry (skip + warn, §3.2), and URL/CORS failures.

**Testing.**

- Pure modules get vitest unit tests:
  - `geometry` — triangulation of a known solid (exterior + hole) → expected
    triangle count and normal orientation; degenerate rings are skipped.
  - `crs` — EPSG-code parse from the short `EPSG:7415` form; forward centroid
    reprojection against a known RD point; densified inverse-projection of a
    rectangle → plausible source-CRS envelope.
  - `reader` — attribute coercion per column type; cursor draining + pagination
    boundaries; indexed-column filtering of the query field list.
- The React/deck.gl shell is validated by a successful `vite build`,
  `tsc --noEmit`, and a written manual smoke checklist (open the 3DBAG subset
  URL → header + CRS status render → draw bbox → buildings render aligned to the
  basemap → attribute query → click-inspect → colour-by-attribute). WebGL
  rendering is not cheaply unit-testable and this is an example, not production.

## 7. Non-goals (YAGNI)

- No editing/writing of `.fcb` (Rust is the only writer).
- No globe/terrain, no multiple basemaps, no CRS picker UI (only the bundled
  allowlist).
- No compound vertical-datum transforms (§3.4).
- No server component; everything runs in the browser.
- No exhaustive CityJSON geometry-type coverage beyond what the 3DBAG demo data
  exercises (Solids/MultiSurfaces); other types degrade to "not rendered" +
  warning rather than crashing.

## 8. Process

1. Write this spec, commit. ✔
2. **codex gpt-5.6-sol reviews the spec**; fold in fixes. ✔ (this revision)
3. User reviews the spec.
4. Invoke `writing-plans` to produce the implementation plan.
5. During implementation, archive the prototype repo
   (`gh repo archive cityjson/flatcitybuf-web-prototype`) with an explicit
   confirmation immediately before running it.

## Appendix — codex review disposition

All 12 findings were verified against the source and incorporated:

| # | Finding | Resolution |
|---|---|---|
| 1 | `SimpleMeshLayer` shares one mesh → no per-feature pick/colour | Per-feature layer model (§3.1) |
| 2 | Projected deltas ≠ ENU meter offsets (convergence/scale) | Per-feature centroid anchoring makes offsets locally valid (§3.1) |
| 3 | "Single origin safe at city extent" unverified | Mooted — anchoring is per building, not city-wide (§3.1) |
| 4 | `referenceSystem` is short `EPSG:` form, not OGC URL | Corrected; parse code off short form (§3) |
| 5 | Silent EPSG:7415 fallback; proj4 lacks the def | CRS allowlist, no silent fallback, explicit registration (§3.3) |
| 6 | Draw bbox is lng/lat; `select` needs source CRS | Densified inverse-projection → source envelope (§3.3) |
| 7 | Interior rings/holes missing | Holes via earcut hole-indices; type-generic ring walk (§3.2) |
| 8 | Winding/normals under-specified | Newell-oriented flat normals, split vertices, cull policy (§3.2) |
| 9 | Degenerate inputs | Skip-with-warning policy (§3.2) |
| 10 | Z / vertical datum assumptions | Metres-up contract; NAP out of scope (§3.4) |
| 11 | Multi-CityObject / multi-LoD ownership ambiguous | Highest-LoD, feature-id pick unit policy (§5) |
| 12 | Import path + non-indexed columns | `@deck.gl/mesh-layers`; query UI restricted to indexed columns (§2, §5.3) |
