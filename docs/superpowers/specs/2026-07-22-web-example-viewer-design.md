# FlatCityBuf Web Example Viewer — Design

**Date:** 2026-07-22
**Status:** Approved (pending codex spec review + user review)
**Supersedes:** `cityjson/flatcitybuf-web-prototype` (to be archived)

## 1. Goal & placement

Replace the vanilla, text-only demo currently at `examples/web` with a
**React + deck.gl + MapLibre** viewer that renders the 3D buildings a query
returns, on a real geographic basemap. The example demonstrates the pure
TypeScript reader `@cityjson/flatcitybuf` end to end — opening a `.fcb` over
HTTP range requests or from a local file, reading the header, running spatial
and attribute queries, and visualizing the results.

The old prototype repo (`cityjson/flatcitybuf-web-prototype`) used the now-removed
WASM binding and is superseded by this example. It will be archived with
`gh repo archive cityjson/flatcitybuf-web-prototype` during implementation,
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
| 3D rendering | deck.gl (`@deck.gl/react` `DeckGL`, `@deck.gl/layers` `SimpleMeshLayer`) |
| Basemap | `react-map-gl/maplibre` + `maplibre-gl` |
| Reprojection | `proj4` |
| State | Jotai (atomic, matches prototype) |
| Styling | Tailwind CSS |
| Reader | `@cityjson/flatcitybuf` via `file:../../src/ts` |

Rationale for deck.gl `SimpleMeshLayer` over alternatives: CityJSON geometry is
Solids/MultiSurfaces of arbitrary 3D polygons (real LoD2 roof shapes). deck.gl
`PolygonLayer`/`SolidPolygonLayer` tessellate a 2D footprint and cannot express
slanted roofs; MapLibre `fill-extrusion` renders flat-roofed prisms —
misrepresenting the data. `SimpleMeshLayer` renders an arbitrary triangle mesh,
which is the correct model. Cesium was rejected for an `examples/` folder:
multi-MB bundle, static-asset copying, ion-token questions — noise around the
~30 lines of reader usage the example exists to teach.

## 3. Geometry & reprojection (the crux)

The reader emits, per feature, a `CityJSONFeature` with **integer** `vertices`
(`[number, number, number][]`) plus a document `transform` (`scale` +
`translate`) obtained from `toCityJSONMetadata(...)`. Real projected coordinates
are `v * scale + translate`, in the file's CRS (EPSG:7415 for the Dutch 3DBAG
data). `header.info.referenceSystem` is an **OGC Name Type Specification URL**
(e.g. `.../EPSG/0/7415`), *not* the `EPSG:7415` short form.

The reprojection strategy keeps proj4 to a **single call**, using deck.gl meter
offsets:

1. **Resolve CRS.** Parse the trailing EPSG code out of
   `header.info.referenceSystem`; build the proj4 source definition. If the code
   is missing or unknown, fall back to EPSG:7415 and surface a visible warning
   in the UI (do not silently guess).
2. **Compute origin once.** Take the center of
   `header.info.geographicalExtent` in projected coords and reproject it a
   single time to `[lng, lat]` (EPSG:4326). This is the only proj4 call in the
   hot path.
3. **Vertices → local meter offsets.** For each returned feature,
   `toCityJSONFeature` → apply `transform` → real projected `(X, Y, Z)` →
   subtract the projected origin → `(X - originX, Y - originY, Z)` meters. No
   per-vertex reprojection: at city scale the planar approximation around a
   single origin is well within visual tolerance, and it keeps the teaching
   code small.
4. **Triangulate each surface.** Newell's-method plane fit → project the ring to
   its dominant 2D plane → earcut → indexed triangles. Accumulate into one
   merged mesh (positions `Float32Array`, indices `Uint32Array`).
5. **Render.** `SimpleMeshLayer` with `COORDINATE_SYSTEM.METER_OFFSETS` and
   `coordinateOrigin = [lng, lat, 0]`. The MapLibre basemap is centered on the
   same origin. No globe, no per-vertex proj4, no static-asset copying.

This is itself a teaching point: the example shows that a projected CRS is a
Cartesian meter grid, so a single anchor reprojection suffices.

## 4. Module boundaries

The invariant from the current demo is preserved: **reader usage lives in one
obvious, framework-free place; rendering is quarantined behind a small
interface** so the renderer can be swapped without touching reader code.

- `src/reader/` — **framework-free teaching core.** Open a `.fcb`
  (`FcbReader.fromUrl` / `FcbReader.fromBlob`), map the header to a UI model,
  drive `reader.select({ spatial, where })`, drain the cursor with pagination
  support, and coerce attribute-query values per column type (ported from the
  current demo's `coerceAttrValue`). Depends only on `@cityjson/flatcitybuf`.
  No React imports.
- `src/geometry/` — pure CityJSON→mesh triangulation (Newell + earcut). No
  React, no deck.gl. Unit-testable.
- `src/crs/` — proj4 setup, EPSG parsing from the OGC URL, origin reprojection.
  Pure. Unit-testable.
- `src/hooks/` — `useFcbData` (open, query, paginate over the reader core) and
  `useDrawBbox` (rectangle-draw interaction on the map).
- `src/components/` — `MapView` (DeckGL + MapLibre + layers), `SourcePanel`
  (URL input + local file drop/pick), `HeaderPanel` (version, extent, CRS,
  columns), `QueryPanel` (bbox fields, attribute field/operator/value, result
  limit, "Load Next Batch"), `FeatureInspector` (click a building → its
  attributes).
- `src/store/` — Jotai atoms: `reader`, `header`, `results` (features + meshes),
  `cursor`/pagination state, `draw` state, `selectedFeature`.

## 5. Features (MVP — all confirmed)

1. **Load from URL + local file** — `FcbReader.fromUrl` (HTTP range requests)
   and `FcbReader.fromBlob` (drop/pick), both carried from the current demo.
2. **Draw-bbox spatial query** — draw a rectangle on the map → bbox →
   `reader.select({ spatial: { kind: 'bbox', value } })` → render matches in 3D.
3. **Attribute query** — field/operator/value from the header's column list →
   `reader.select({ where })`, combinable with the bbox.
4. **Pagination / "Load Next Batch"** — result-count limit and batched draining
   of the cursor for large result sets.

Folded-in cheap extras (approved):

5. **Click-to-inspect** — clicking a building shows its CityObject attributes.
6. **Color-by-attribute** — color the mesh by a queried/selected attribute
   column, teaching what `where` filters on.

## 6. Error handling & testing

**Errors.** `FcbError` is surfaced as `code: message` (as the current demo
does). Graceful degradation for: unknown/absent CRS (warn + fall back),
non-`.fcb` input (the reader's magic-byte check → friendly message), empty
result sets ("no features matched"), and URL/CORS failures.

**Testing.**

- Pure modules get vitest unit tests: `geometry` (triangulation of a known
  solid → expected triangle count/winding), `crs` (EPSG parse from OGC URL;
  origin reprojection against a known EPSG:7415 point), `reader` (attribute
  coercion per column type; cursor draining + pagination boundaries).
- The React/deck.gl shell is validated by a successful `vite build`,
  `tsc --noEmit`, and a written manual smoke checklist (open the 3DBAG subset
  URL → header renders → draw bbox → buildings render → attribute query →
  inspect a building). WebGL rendering is not cheaply unit-testable and this is
  an example, not production code.

## 7. Non-goals (YAGNI)

- No editing/writing of `.fcb` (Rust is the only writer).
- No globe/terrain, no multiple basemaps, no CRS picker UI.
- No server component; everything runs in the browser.
- No exhaustive CityJSON geometry-type coverage beyond what the 3DBAG demo data
  exercises (Solids/MultiSurfaces); other types degrade to "not rendered" rather
  than crashing.

## 8. Process

1. Write this spec, commit.
2. **codex gpt-5.6-sol reviews the spec**; fold in fixes.
3. User reviews the spec.
4. Invoke `writing-plans` to produce the implementation plan.
5. During implementation, archive the prototype repo
   (`gh repo archive cityjson/flatcitybuf-web-prototype`) with an explicit
   confirmation immediately before running it.
