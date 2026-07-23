# FlatCityBuf web viewer

A browser viewer for [FlatCityBuf](../../README.md) built on the native
TypeScript reader `@cityjson/flatcitybuf` — no WASM, no server component. It
opens a `.fcb` over HTTP range requests or from a local file, runs bounding-box
and attribute queries, and renders the returned 3D buildings on a MapLibre
basemap with deck.gl.

> Supersedes the archived `cityjson/flatcitybuf-web-prototype`, which used the
> old WASM binding.

## Prerequisite

`@cityjson/flatcitybuf` is a `file:../../src/ts` dependency, resolved from its
`dist/` output. Build that package once before installing here:

```bash
cd ../../src/ts && npm install && npm run build
```

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

## Troubleshooting

**"sent a 206 response without an accessible Content-Range header"** — the
reader learns a file's size from the `Content-Range` header of a range
response. Browsers hide response headers on cross-origin requests unless the
server explicitly exposes them, so a `.fcb` hosted on another origin must send:

```
Access-Control-Expose-Headers: Content-Range, Accept-Ranges
```

Without `Content-Range` exposed, the fetch reader cannot determine the file
size and refuses to guess. Local files (drop/pick) are same-origin and never
hit this. For a Google Cloud Storage bucket, set a CORS config that exposes
the header:

```bash
echo '[{"maxAgeSeconds":3600,"method":["GET","HEAD","OPTIONS"],"origin":["*"],"responseHeader":["Content-Type","Content-Range","Accept-Ranges"]}]' > cors.json
gsutil cors set cors.json gs://your-bucket
```

After changing CORS, hard-reload (or clear the site cache) — a browser may have
cached the earlier failed response.

## Tests

```bash
npm test         # pure-module unit tests (crs, geometry, reader)
```
