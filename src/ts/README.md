# @cityjson/flatcitybuf

A pure TypeScript reader for [FlatCityBuf](https://github.com/cityjson/flatcitybuf), a cloud-optimized binary encoding of [CityJSON](https://www.cityjson.org/). It reads `.fcb` files from a URL over HTTP range requests, from a `Blob`/`File` in the browser, or from a local file in Node, and answers spatial and attribute queries by reading only the index and the features that match — without downloading the whole file.

This package replaces the previous WebAssembly binding. It has **one runtime dependency** ([`flatbuffers`](https://www.npmjs.com/package/flatbuffers)), ships no `.wasm`, and runs the same code in Node and the browser. See [Migrating from the wasm binding](#migrating-from-the-wasm-binding) below.

## Requirements

- **ESM only.** The package has no CommonJS build; import it with `import`, not `require`.
- **Node ≥ 22.12** for the Node entry point. The browser entry point runs in any modern browser with `fetch` and `Blob`.

## Install

```sh
npm install @cityjson/flatcitybuf
```

## Quick start

### From a URL (browser or Node) — HTTP range reads

```ts
import { FcbReader } from '@cityjson/flatcitybuf'

const reader = await FcbReader.fromUrl('https://example.com/city.fcb')

// Stream the whole file as CityJSONSeq: the metadata line first, then one
// CityJSONFeature per feature, in stored order.
for await (const line of reader.cityjson()) {
  console.log(JSON.stringify(line))
}
```

`fromUrl` wraps the source in a buffered reader and validates the server's Range support strictly: a server that ignores `Range` and returns `200`, or whose CORS config hides the `Content-Range` header, is rejected rather than silently mis-read.

### From a Blob or File (browser) — e.g. a drag-and-drop upload

```ts
import { FcbReader } from '@cityjson/flatcitybuf'

const file: File = /* from an <input type="file"> or a drop event */
const reader = await FcbReader.fromBlob(file)

for await (const feature of await reader.selectAll()) {
  console.log(feature.id)
}
```

### From a local file (Node) — the `./node` subpath

The Node file reader lives behind a separate subpath so the package root never imports `node:*` and stays usable in the browser:

```ts
import { fromFile } from '@cityjson/flatcitybuf/node'

await using reader = await fromFile('./city.fcb')
for await (const feature of await reader.selectAll()) {
  console.log(feature.id)
}
// `await using` closes the file handle on scope exit; otherwise call
// `await reader.close()` yourself.
```

You can also read from an in-memory buffer with `FcbReader.fromBytes(uint8Array)`.

## Query API

`reader.select(options)` returns a `FeatureCursor` — an async-iterable of features whose `featuresCount` reports the **total** number of matches (unaffected by `limit`/`offset`; `undefined` only when the file declares an unknown count).

```ts
// Spatial: bounding box.
const inBox = await reader.select({
  spatial: { kind: 'bbox', value: [minX, minY, maxX, maxY] },
})

// Spatial: point intersection.
const atPoint = await reader.select({
  spatial: { kind: 'point', value: [x, y] },
})

// Spatial: nearest feature to a point (returns at most one).
const nearest = await reader.select({
  spatial: { kind: 'nearest', value: [x, y] },
})

// Attribute query. `operator` is one of Eq | Ne | Gt | Ge | Lt | Le.
// Multiple conditions are AND-intersected.
const tall = await reader.select({
  where: [{ field: 'b3_h_dak_50p', operator: 'Gt', value: 20 }],
})

// Spatial AND attribute, with paging over the result.
const page = await reader.select({
  spatial: { kind: 'bbox', value: [minX, minY, maxX, maxY] },
  where: [{ field: 'b3_h_dak_50p', operator: 'Ge', value: 10 }],
  limit: 50,
  offset: 0,
})

console.log(page.featuresCount) // total matches, not the page size
for await (const feature of page) {
  /* ... */
}
```

Notes:

- A `String` attribute index stores keys truncated to 50 bytes, so it answers with **candidates**; the reader post-filters them against each feature's full, decoded attributes before counting and paging, so the results you iterate are exact matches.
- `nearest` cannot be combined with a `where` filter (it throws `UnsupportedQueryCombination`).
- Every query accepts an `AbortSignal` via `signal`, which is threaded into the actual in-flight reads, not merely held on the facade.
- Querying a file that has no spatial index throws `NoIndex`; the reader always uses the header's own `index_node_size`, so files written with a non-default node size traverse correctly.

## Converting to CityJSON

```ts
import { toCityJSONMetadata, toCityJSONFeature } from '@cityjson/flatcitybuf'

const metadata = toCityJSONMetadata(reader.header) // the CityJSON "metadata" object
for await (const feature of await reader.selectAll()) {
  const cjFeature = toCityJSONFeature(feature, reader.header)
}
```

`Long`/`Int64` attribute values can exceed `Number.MAX_SAFE_INTEGER`. Pass an `Int64Policy` to choose how they are emitted: a lossy JS number (default, keeps the output JSON-serializable), an exact decimal string, or a throw on any unsafe value. No policy ever leaks a `bigint` into the emitted object.

## Trust model

**Input `.fcb` files are trusted.** The reader bounds-checks its framing — the header size prefix, each feature's size prefix, and every slice into the buffer are validated against the actual byte length, so a truncated or misframed file throws an `FcbError` rather than reading out of bounds. But **there is no FlatBuffers verifier in JavaScript**: the FlatBuffers tables themselves are read without the full vtable/offset verification that the Rust and C++ readers get from the generated verifier. A malformed or hostile file that is well-framed but internally inconsistent may therefore throw, or return garbage values, for the affected feature. Do not point this reader at untrusted `.fcb` data and treat its output as safe; validate the source, not the reader.

## Migrating from the wasm binding

The previous package exposed a wasm-bindgen surface (`HttpFcbReader`, `AsyncFeatureIter`, `WasmSpatialQuery`, `WasmAttrQuery`). The native reader collapses the several `select_*` methods into one `select(options)` and replaces the query wrapper classes with plain objects. Every capability is preserved except two OBJ/merge helpers, called out below.

| Old wasm API | New TypeScript API |
|---|---|
| `new HttpFcbReader(url)` | `await FcbReader.fromUrl(url)` |
| *(none — browser only)* | `await FcbReader.fromBlob(blob)`, `FcbReader.fromBytes(bytes)`, `fromFile(path)` (`./node`) |
| `reader.meta()` | `reader.header` (a `HeaderView` with `info` and `layout`) |
| `reader.cityjson()` | `toCityJSONMetadata(reader.header)`, or `reader.cityjson()` to stream metadata **and** every feature |
| `reader.select_all()` | `reader.selectAll()` |
| `reader.select_spatial(q)` | `reader.select({ spatial })` |
| `reader.select_spatial_paged(q, limit, offset)` | `reader.select({ spatial, limit, offset })` |
| `reader.select_attr_query(q)` | `reader.select({ where })` |
| `reader.select_attr_query_paged(q, limit, offset)` | `reader.select({ where, limit, offset })` |
| `new WasmSpatialQuery({ type: 'bbox', minX, minY, maxX, maxY })` | `{ kind: 'bbox', value: [minX, minY, maxX, maxY] }` |
| `new WasmSpatialQuery({ type: 'pointIntersects', x, y })` | `{ kind: 'point', value: [x, y] }` |
| `new WasmSpatialQuery({ type: 'pointNearest', x, y })` | `{ kind: 'nearest', value: [x, y] }` |
| `new WasmAttrQuery([[field, op, value]])` | `where: [{ field, operator, value }]` |
| `for (…) { const f = await iter.next(); … }` | `for await (const feature of cursor) { … }` |
| `iter.features_count()` | `cursor.featuresCount` |
| `iter.header()` | `reader.header` / `toCityJSONMetadata(reader.header)` |
| `iter.cur_cj_feature()` | `toCityJSONFeature(feature, reader.header)` |
| `cjToObj(...)` | **Dropped.** OBJ export is out of scope for the reader; convert from CityJSON with a separate tool. |
| `cjseqToCj(...)` | **Dropped.** Merging a CityJSONSeq back into a single CityJSON is a CityJSON-tooling concern, not a reader concern. |

Beyond the shape change, the native reader fixes several defects that the wasm binding shipped with — attribute queries against non-`Double` columns, string query values over 50 bytes, non-default R-tree node sizes over HTTP, and a range client that accepted a `200` full-body response as if it were the requested range. These are documented in the repository's `docs/upstream-findings.md`.

## License

MIT
