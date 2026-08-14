# TypeScript — `@cityjson/flatcitybuf`

A pure TypeScript **reader** for FlatCityBuf, the cloud-optimized binary
encoding of CityJSON. It opens a `.fcb` file from a URL over HTTP range
requests, from a `Blob`/`File` in the browser, from a local path in Node, or
from an in-memory `Uint8Array`, and answers spatial and attribute queries by
reading only the index nodes and the features that match — never the whole
file. It is a from-scratch port with no FFI and no WebAssembly: it parses the
bytes directly, has one runtime dependency (`flatbuffers`), and runs the same
code in Node and the browser.

## Status

**Reader only, conformant on the shared corpus, with a narrower hardening
surface than the Rust and C++ implementations.** Concretely:

| | |
|---|---|
| Read | Yes — header, features, geometry, appearance, attributes, spatial and attribute queries |
| Write | **No.** Only Rust (`src/rust/fcb_core`) and C++ (`src/cpp`) produce `.fcb` files |
| Conformance | **14 / 14** corpus cases pass, compared whole-line against the Rust reader's own output ([`src/ts/test/conformance.test.ts:29-70`](../src/ts/test/conformance.test.ts)) |
| Node suite | 16 files, 244 tests passing, 1 skipped (the opt-in live-3DBAG test) |
| Browser suite | 3 tests in real headless Chromium ([`src/ts/test/browser/`](../src/ts/test/browser/)) |
| npm | Published as `@cityjson/flatcitybuf`, currently `0.3.0` |

What is **not** in place, and is the honest reason to treat this reader as less
settled than Rust or C++:

- **No FlatBuffers verifier exists in JavaScript.** Framing *is* bounds-checked
  — the header size prefix, each feature's size prefix, and every slice into
  the buffer are validated against the real byte length, so a truncated or
  misframed file raises an `FcbError` rather than reading out of bounds. But
  the FlatBuffers tables themselves are read without the vtable/offset
  verification that the generated verifier gives Rust and C++. A well-framed
  but internally inconsistent file may throw or return garbage for the affected
  feature. **Treat input `.fcb` files as trusted**; validate the source, not
  the reader.
- **No linter is configured.** There is no ESLint or Prettier setup, so
  `just type` (`tsc --noEmit`, both configs) is the only static gate; `just
  lint` and `just fix` are deliberate no-ops that exist so every language
  exposes the same five verbs ([`src/ts/justfile:51-65`](../src/ts/justfile)).
- **Four divergence notes against the Rust reader** (the first now resolved)
  in attribute-query
  behaviour, each documented with its rationale in the module docstring of
  [`src/ts/src/static-btree/query.ts:7-33`](../src/ts/src/static-btree/query.ts):
  1. *Resolved — no longer a divergence.* `Byte` columns are treated as
     **unsigned** `u8`, and Rust now agrees on every path (its feature-value
     decode was fixed upstream, finding #2a; its index reader always read `u8`).
  2. `Json` and `Binary` columns are **rejected** with
     `ErrorCode.UnsupportedColumnType` rather than answered with near-meaningless
     100-byte-prefix collisions.
  3. `f32`/`f64` range queries use `+Infinity` as their maximum, so `Ge`, `Gt`
     and `Ne` on a float column silently exclude NaN-keyed features (lossy, for
     parity with Rust).
  4. `DateTime` range queries use epoch zero as their minimum, so `Le`, `Lt` and
     `Ne` on a datetime column are blind to pre-1970 timestamps (also lossy, also
     for parity).

## Requirements

- **ESM only.** There is no CommonJS build — `import`, never `require`.
- **Node ≥ 22.12** for the Node entry point (`engines` in
  [`src/ts/package.json`](../src/ts/package.json)). The browser entry point
  needs only `fetch` and `Blob`.

## Install

From npm — the normal path:

```sh
npm install @cityjson/flatcitybuf
```

From source, for working on the reader itself. The package manager is **npm**
(`package-lock.json` is the committed lockfile; the justfile installs with
`npm ci`):

```sh
cd src/ts
just build          # npm ci, then vite build + tsc --emitDeclarationOnly -> dist/
```

## Entry points

Two subpaths, and the split is load-bearing: nothing reachable from the package
root imports `node:*`, so the root stays usable unchanged in a browser.

| Import specifier | Module | Use |
|---|---|---|
| `@cityjson/flatcitybuf` | `dist/index.js` | Everything except local-file reads — works in Node **and** the browser |
| `@cityjson/flatcitybuf/node` | `dist/io/node.js` | `fromFile(path)`, the local-file reader |

## Usage

Every symbol below is verified against the source; each is cited.

```ts
import {
  FcbReader,
  toCityJSONFeature,
  toCityJSONMetadata,
} from '@cityjson/flatcitybuf'

// Open over HTTP range requests. `fromUrl` validates the server's Range
// support strictly: a server that ignores `Range` and answers 200, or whose
// CORS config hides `Content-Range`, is rejected rather than mis-read.
const reader = await FcbReader.fromUrl('https://example.com/city.fcb')

// The CityJSON "metadata" object, from the file header.
console.log(toCityJSONMetadata(reader.header))

// Spatial AND attribute query, paged. `select` returns a FeatureCursor: an
// async-iterable whose `featuresCount` is the TOTAL number of matches,
// unaffected by limit/offset.
const hits = await reader.select({
  spatial: { kind: 'bbox', value: [minX, minY, maxX, maxY] },
  where: [{ field: 'b3_h_dak_50p', operator: 'Gt', value: 20 }],
  limit: 50,
})

console.log(hits.featuresCount)
for await (const feature of hits) {
  console.log(feature.id, toCityJSONFeature(feature, reader.header))
}
```

Source for each symbol used, all paths relative to the repository root:

| Symbol | Defined at |
|---|---|
| `FcbReader` | `src/ts/src/reader.ts:294` |
| `FcbReader.fromBytes(bytes)` | `src/ts/src/reader.ts:334` |
| `FcbReader.fromBlob(blob)` | `src/ts/src/reader.ts:342` |
| `FcbReader.fromUrl(url, opts?)` | `src/ts/src/reader.ts:366` |
| `reader.header` | `src/ts/src/reader.ts:381` |
| `reader.selectAll(opts?)` | `src/ts/src/reader.ts:394` |
| `reader.select(opts?)` | `src/ts/src/reader.ts:445` |
| `reader.cityjson(opts?)` | `src/ts/src/reader.ts:557` |
| `FeatureCursor` / `Operator` / `AttrCondition` / `SelectOptions` | `src/ts/src/reader.ts:25` / `:43` / `:48` / `:72` |
| `FeatureCursor.featuresCount` | `src/ts/src/reader.ts:32` (`number \| undefined`; `undefined` when the header declares an unknown count) |
| `Feature`, `Feature.id` | `src/ts/src/feature/index.ts:151`, `:153` |
| `toCityJSONMetadata(header, opts?)` | `src/ts/src/cityjson/index.ts:630` |
| `toCityJSONFeature(feature, header, opts?)` | `src/ts/src/cityjson/index.ts:687` |
| `fromFile(path)` (`./node` subpath) | `src/ts/src/io/node.ts:129` |
| `ErrorCode`, `FcbError` | `src/ts/src/errors.ts` |

The full public surface — and, per-block, the reasoning for what is
deliberately *not* exported — is the export list in
[`src/ts/src/index.ts`](../src/ts/src/index.ts). Generated TypeDoc for both
entry points comes from `just docs` (output is gitignored and never published
to npm).

### Query notes

- Spatial queries take one of three shapes: `{ kind: 'bbox', value: [minX,
  minY, maxX, maxY] }`, `{ kind: 'point', value: [x, y] }`, or `{ kind:
  'nearest', value: [x, y] }`. `nearest` returns at most one feature and cannot
  be combined with `where` — that throws `UnsupportedQueryCombination`.
- `operator` is one of `Eq | Ne | Gt | Ge | Lt | Le`; multiple `where`
  conditions are AND-intersected.
- A `String` attribute index stores keys truncated to 50 bytes, so it answers
  with **candidates**. `FcbReader.select` post-filters them against each
  feature's fully decoded attributes before counting and paging, so what you
  iterate are exact matches. A caller using `searchAttributes` directly must
  run `postFilterCandidates` themselves.
- Every query accepts an `AbortSignal` via `signal`, threaded into the
  in-flight reads rather than merely held on the facade.
- Querying a file with no spatial index throws `NoIndex`. Traversal always uses
  the header's own `index_node_size`, so files written with a non-default node
  size read correctly.
- `Long`/`Int64` attribute values can exceed `Number.MAX_SAFE_INTEGER`. Pass an
  `Int64Policy` to pick the emission: a lossy JS number (the default, which
  keeps output JSON-serializable), an exact decimal string, or a throw on any
  unsafe value. No policy ever leaks a `bigint` into the emitted object.

## Tooling and testing

`just` is the task runner and `src/ts/justfile` exposes the same five verbs as
every other language directory:

```sh
cd src/ts
just check          # lint + type + test + test-browser + build, read-only
just test           # npx vitest run — the Node suite
just type           # tsc --noEmit, for both tsconfig.json and tsconfig.test.json
just lint           # no-op: no ESLint/Prettier configured
just build          # vite build + tsc --emitDeclarationOnly -> dist/
just fix            # no-op: no formatter configured
```

Extras beyond the five (`just --list` shows them all):

| Recipe | What it does |
|---|---|
| `just test-browser` | The **actual shipping target**: the browser suite in real headless Chromium via Vitest browser mode. One-time setup: `npx playwright install chromium` |
| `just test-remote` | Opt-in live HTTP test against the published ~68 GB 3DBAG file; skipped by default because `FCB_REMOTE_HTTP_URL` is unset |
| `just docs` | TypeDoc API reference for both entry points into `src/ts/docs/` (gitignored; not a gate) |
| `just gen-fbs` | Regenerate the committed FlatBuffers bindings under `src/ts/src/generated/` |
| `just clean` | Remove `dist/`, `docs/`, `node_modules/` |

Two things about the browser suite are worth knowing. It lives in a **separate
Vitest config** (`vitest.browser.config.ts`) on purpose, so the default `npx
vitest run` never launches a browser and a developer with no browser installed
still gets the full green Node suite. And its `globalSetup` serves the
conformance corpus **cross-origin**, which is the only way to exercise the CORS
failure path — a Node `fetch` cannot reach it, because Node does not enforce
CORS.

One caveat when running the Node suite: `test/http.test.ts` spawns the shared
`range_server.py`, so `python3` must be on `PATH`, and the server has a 10 s
startup budget — do not run this concurrently with a heavy build.

### Conformance corpus

`conformance/` at the repository root is the shared oracle: hand-authored
inputs, the `.fcb` binaries, and `.expected.jsonl` files holding **the Rust
reader's own output** for each case. The `.fcb` files are tracked, so a clean
checkout runs the suite with no Rust toolchain. `test/conformance.test.ts`
walks all 14 cases that have a matching `.expected.jsonl`, emits metadata plus
every feature through `toCityJSONMetadata`/`toCityJSONFeature`, and compares
**whole lines** — comparing selected keys is exactly what once hid a missing
per-feature `appearance` object through an entire port.

Run it alone with:

```sh
cd src/ts && npx vitest run test/conformance.test.ts
```

When the TypeScript reader and the Rust reader disagree, Rust is right by
definition and the disagreement is a TypeScript defect until proven otherwise.
Defects found across implementations are recorded in
[`upstream-findings.md`](upstream-findings.md).

## Migrating from the WebAssembly binding

This package **replaces** the previous wasm-bindgen binding, which exposed
`HttpFcbReader`, `AsyncFeatureIter`, `WasmSpatialQuery` and `WasmAttrQuery`.
The native reader collapses the several `select_*` methods into one
`select(options)` and replaces the query wrapper classes with plain objects.
Every capability is preserved except two OBJ/merge helpers, called out at the
bottom of the table.

| Old wasm API | Native TypeScript API |
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
| `cjToObj(...)` | **Dropped.** OBJ export is out of scope for a reader; convert from CityJSON with a separate tool |
| `cjseqToCj(...)` | **Dropped.** Merging a CityJSONSeq back into a single CityJSON is CityJSON tooling, not reader work |

Beyond the shape change, the native reader fixes several defects the wasm
binding shipped with — attribute queries against non-`Double` columns, string
query values over 50 bytes, non-default R-tree node sizes over HTTP, and a
range client that accepted a `200` full-body response as if it were the
requested range. Each is recorded in
[`upstream-findings.md`](upstream-findings.md).

## Relationship to `examples/web`

The browser viewer in `examples/web` depends on this package as
`"@cityjson/flatcitybuf": "file:../../src/ts"` and resolves it from
`src/ts/dist`, so `cd src/ts && just build` must run before the web example is
touched. The root justfile's fan-out order (`src/rust src/cpp src/py src/ts
examples/web`) exists precisely to guarantee that.

## See also

- [Format specification](specification.md) — the format from schema level down
  to byte offsets, constants and formulas. Read this before changing any
  decoder; do not re-derive the format.
- [Testing guide](TESTING.md) — the full manual verification procedure, local
  and remote, for every implementation.
- [`src/ts/README.md`](../src/ts/README.md) — the npm registry page for the
  package.
- Sibling guides: [Rust](rust.md) · [C++](cpp.md) · [Python](py.md)

## License

MIT.
