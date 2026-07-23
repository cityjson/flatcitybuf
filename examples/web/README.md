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
