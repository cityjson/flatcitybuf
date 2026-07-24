# Web viewer — export & download in multiple formats

**Date:** 2026-07-24
**Component:** `examples/web` (browser viewer for `@cityjson/flatcitybuf`)
**Status:** design approved, pending spec review

## Goal

Let the user pick an output format and download the currently displayed data.
Format conversion runs in the browser, reusing the prebuilt WASM binding
(`fcb_wasm`) that the old prototype used, rather than re-implementing conversion
in TypeScript.

## Scope

- **Formats:** CityJSON (single merged file), CityJSONSeq (`.city.jsonl`), OBJ.
- **Data exported:** the *current query result* — exactly the page currently
  rendered, described by `activeQueryAtom`. Not the whole dataset (the default
  3DBAG file is 10.7 M features).
- Out of scope: glTF/GLB (the WASM binding only offers OBJ), exporting the whole
  dataset, filtering OBJ by LoD.

## Background facts that shape the design

1. **The app already produces CityJSON.** The worker reads FCB with the native
   TS reader and calls `feature.toCityJSON(header)` per feature and
   `toCityJSONMetadata(header)` for the metadata object. Export reuses this — no
   new decoding.

2. **The WASM binding is a gitignored build artifact.** `src/rust/wasm/pkg/`
   (4.1 MB `.wasm`) is not tracked and its source is not in the current Rust
   workspace. So we **vendor** the three needed files into `examples/web` rather
   than depend on the pkg or rebuild it.

3. **The WASM's genuine added value is OBJ.** Its two conversion exports:
   - `cjToObj(city_json_js)` — accepts either a single CityJSON object *or* an
     array `[CityJSON metadata, ...CityJSONFeature]`; returns an OBJ string.
   - `cjseqToCj(base_cj, features)` — merges a metadata CityJSON + array of
     CityJSONFeatures into one CityJSON object.
   CityJSONSeq needs no WASM (it is just the metadata line + one line per
   feature). CityJSON-merged uses `cjseqToCj`; OBJ uses `cjToObj`.

4. **Paging is a replace-pager.** `loadNext` calls `setRendered(out)` (replaces,
   not appends), so at any moment `rendered` is exactly the page described by
   `activeQueryAtom` `{bboxSource, where, limit, offset}`. Re-running the worker
   query with that spec reproduces the on-screen set deterministically (R-tree /
   B+tree query order is stable). No feature-ID bookkeeping is needed.

5. **The vendored WASM works in a Vite worker.** `fcb_wasm.js` is a web-target
   wasm-bindgen module; its default `__wbg_init` uses
   `new URL('fcb_wasm_bg.wasm', import.meta.url)`, which Vite statically analyses
   and emits as a bundled asset. `cjToObj` / `cjseqToCj` are plain named exports.

## Architecture

### Files

- **New:** `examples/web/src/wasm/fcb_wasm.js`, `fcb_wasm.d.ts`,
  `fcb_wasm_bg.wasm` — vendored copies of the prebuilt pkg (the two `.d.ts`
  glue files as needed for `tsc`).
- **New:** `examples/web/src/export/index.ts` — pure-TS export helpers
  (CityJSONSeq assembly, filename derivation, format metadata) that are unit
  testable without WASM, plus a lazy WASM initializer wrapper.
- **New:** `examples/web/src/components/ExportPanel.tsx` — the UI.
- **Edit:** `examples/web/src/worker/protocol.ts` — add `ExportRequest` /
  `ExportResponse`.
- **Edit:** `examples/web/src/worker/fcb.worker.ts` — add `handleExport`.
- **Edit:** `examples/web/src/hooks/useFcbData.ts` — add an `exportAs(format)`
  action that sends the export request and triggers the browser download on the
  result.
- **Edit:** `examples/web/src/store/index.ts` — add `exportFormatAtom` and an
  `exportingAtom` (in-flight flag), plus a `sourceNameAtom` (basename of the
  open URL/file) for the download filename.
- **Edit:** `examples/web/src/components/App.tsx` — mount `ExportPanel`.
- **Edit:** `examples/web/README.md` — document export.

### Worker protocol

```ts
export interface ExportRequest {
  type: 'export'
  id: number
  bboxSource?: [number, number, number, number]
  where?: AttrCondition[]
  limit: number
  offset: number
  format: 'cityjson' | 'cityjsonseq' | 'obj'
}
export interface ExportResponse {
  type: 'export-result'
  id: number
  data: string            // the file contents
  mime: string            // e.g. 'application/json'
  ext: string             // '.city.json' | '.city.jsonl' | '.obj'
}
```

Errors reuse the existing `ErrorResponse`.

### Worker `handleExport`

1. Guard: reader/model present, else `ErrorResponse`.
2. Use a dedicated `exportController` (separate from the render-query
   `controller`) so export never cancels the live render query; a new export
   aborts a prior export.
3. `const { features } = await runQuery(reader, {bboxSource, where, limit,
   offset, signal})`.
4. `const metadata = toCityJSONMetadata(reader.header)`.
   `const feats = features.map(f => f.toCityJSON(reader.header))`.
5. Assemble by `format`:
   - `cityjsonseq`: `[JSON.stringify(metadata), ...feats.map(f =>
     JSON.stringify(f))].join('\n')`, mime `application/x-ndjson`, ext
     `.city.jsonl`. (no WASM)
   - `cityjson`: `JSON.stringify(cjseqToCj(metadata, feats))`, mime
     `application/json`, ext `.city.json`. (WASM)
   - `obj`: `cjToObj([metadata, ...feats])`, mime `text/plain`, ext `.obj`.
     (WASM)
6. WASM is lazy-initialized once, on the first WASM-backed export, via the
   `export/index.ts` initializer.
7. Post `ExportResponse` with the string. (No transfer needed; strings are
   copied. Optionally encode to a transferred `Uint8Array` if payloads get
   large — deferred unless measured slow.)

### Main-thread download

The worker has no DOM, so `useFcbData.exportAs`:
1. Reads the active query spec from the store; if none/`rendered.length === 0`,
   no-op (button is disabled anyway).
2. Sends `ExportRequest`; sets `exportingAtom` true.
3. On `ExportResponse`: build `new Blob([data], {type: mime})`,
   `URL.createObjectURL`, a temporary `<a download={filename}>`, click, then
   `URL.revokeObjectURL`. `filename = (sourceName ?? 'flatcitybuf-export') +
   ext`.
4. On `ErrorResponse`: surface via status. Always clear `exportingAtom`.

### UI — `ExportPanel`

- Themed section titled "Export" (reuses `SectionHeading` / `PrimaryButton`).
- Exclusive format segmented control: CityJSON · CityJSONSeq · OBJ, bound to
  `exportFormatAtom`.
- `PrimaryButton` labelled `Download {count} features` where `count =
  rendered.length`. Disabled when `rendered.length === 0` or `exporting`.
- While `exporting`, button shows "preparing…".
- Small helper note that OBJ includes all LoDs present in the data.

## Error handling

- No file open / empty result → button disabled; `exportAs` guards anyway.
- WASM init failure or conversion throw → `ErrorResponse` → panel status shows
  the message; `exportingAtom` cleared.
- A second export click while one is in flight → the worker's `exportController`
  aborts the prior export; the latest wins.

## Testing

- **vitest (pure TS, no WASM):**
  - CityJSONSeq assembly: metadata line first, one line per feature, newline
    joined, each line valid JSON.
  - Filename derivation: URL basename, file basename, fallback, correct
    extensions per format.
- **In-browser (Playwright):** load the default dataset, settle to a page, for
  each format click Download and assert a file is produced, non-empty, and
  well-formed:
  - CityJSON parses and `type === 'CityJSON'`.
  - CityJSONSeq first line parses with `type === 'CityJSON'`, later lines
    `type === 'CityJSONFeature'`.
  - OBJ contains `v ` and `f ` lines.
- `cd examples/web && just check` (type + vitest + build) green. (Root
  `just check` has a pre-existing unrelated rustfmt drift in
  `src/rust/fcb_core/tests/http.rs`; scope verification to `examples/web`.)

## Known behavior (accepted)

- OBJ export includes every LoD present in the exported CityJSON (3DBAG →
  0/1.2/1.3/2.2), because `cjToObj` triangulates all geometries. Documented in
  the panel and README; not filtered.
- Vendoring commits a 4.1 MB `.wasm` into the demo. Accepted for a
  self-contained demo; a future minimal converter-only wasm crate could shrink
  it, out of scope here.
