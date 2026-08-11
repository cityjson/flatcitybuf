# FlatCityBuf web viewer

A browser viewer for [FlatCityBuf](../../README.md) built on the native
TypeScript reader `@cityjson/flatcitybuf` — no server component, and reading
is pure TypeScript with no WASM. It opens a `.fcb` over HTTP range requests or
from a local file, runs bounding-box and attribute queries, and renders the
returned 3D buildings on a MapLibre basemap with deck.gl. Query results can
also be exported to CityJSON, CityJSONSeq, or OBJ (see [Export](#export)).

**Live at [flatcitybuf-prototype.hideba.me](https://flatcitybuf-prototype.hideba.me)**
— built and deployed to Cloudflare Workers on every push to `main` (see
[`.github/workflows/deploy-web-demo.yml`](../../.github/workflows/deploy-web-demo.yml)).

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

Open the printed URL and load a `.fcb` — the default is the full 3DBAG
(country-scale, ~10.7M features), or paste another URL / pick a local file. The
camera frames to the data and **Follow camera** mode (the default) loads the
visible area, re-querying as you pan and zoom; zoom in past the "get closer"
hint to fetch. Switch the **Level of Detail** (1.2 / 1.3 / 2.2, plus LoD 0
roofprints) to re-render at that LoD, refine with a drawn bbox or an attribute
query, or `colour by` an attribute.

## Export

Downloads the **current query result** — exactly the features currently
rendered (the page described by the active query, up to the render limit),
not the whole dataset. For the default full-3DBAG file that's the difference
between what's on screen and 10.7M features.

Pick a format with the selector, then **Download**:

- **CityJSON** — every rendered feature merged into a single `.city.json`.
- **CityJSONSeq** — `.city.jsonl`: one metadata line, then one line per
  feature.
- **OBJ** — a triangulated Wavefront `.obj` mesh. Includes every LoD present
  in the data (for 3DBAG that's LoD 0, 1.2, 1.3, and 2.2 all in the same
  file), because the converter triangulates all geometries, not just the
  currently-rendered LoD.

Conversion runs entirely in the browser. CityJSONSeq is assembled in pure
TypeScript; the merged CityJSON and OBJ conversions reuse the prebuilt
`fcb_wasm` WebAssembly binding (vendored under `src/wasm/`), lazy-loaded on
first use — the ~4 MB `.wasm` is only fetched the first time you export
CityJSON or OBJ. The download filename derives from the open file/URL's
basename, e.g. `3dbag_all_index.city.json`.

## How it works

- `src/worker/` — a Web Worker owns the reader; it runs `reader.select(...)`,
  triangulates the results off the main thread, and transfers the meshes back.
- `src/reader/` — opens the file and drives the query (framework-free).
- `src/geometry/` — triangulates CityJSON surfaces into meshes; `pickGeometry`
  selects which LoD to build per object.
- `src/crs/` — reprojects EPSG:7415 ↔ WGS84 (proj4 allowlist).
- `src/render/` — merges every feature into ONE deck.gl layer (a single indexed
  mesh with per-vertex colour and feature id for picking), so there is no
  per-feature layer and no 255-pickable-layer cap.

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
hit this. On a Cloudflare R2 bucket, add `Content-Range` and `Accept-Ranges`
to `ExposeHeaders` in the bucket's CORS policy. For a Google Cloud Storage
bucket, set a CORS config that exposes the header:

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
