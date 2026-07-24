# Web Viewer Multi-Format Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a format selector + Download button to the `examples/web` viewer that exports the current query result as CityJSON (merged), CityJSONSeq, or OBJ, converting in-browser by reusing the prototype's `fcb_wasm` binding.

**Architecture:** The worker (which already holds the reader and produces CityJSON per feature) re-runs the active query, assembles the chosen format — CityJSONSeq in pure TS, merged CityJSON via WASM `cjseqToCj`, OBJ via WASM `cjToObj` — and returns a string. The main thread turns that string into a Blob download (the worker has no DOM). The 4.1 MB `.wasm` is vendored into `examples/web/src/wasm/` and lazy-loaded on the first WASM-backed export.

**Tech Stack:** React 18 + TypeScript + Vite 8 + Jotai + Tailwind 3; Web Worker; vendored wasm-bindgen (`web` target) module; vitest for pure-TS units; Playwright for in-browser acceptance.

## Global Constraints

- Little-endian / byte conventions are irrelevant here (no new decoding — reuse `feature.toCityJSON(header)` / `toCityJSONMetadata(header)`).
- The merged-CityJSON and OBJ conversions MUST go through the WASM binding (`cjseqToCj`, `cjToObj`), not a hand-written TS reimplementation — reimplementing the vertex-offset merge risks diverging from the Rust oracle.
- Export scope is the **current query result only** — the page described by `activeQueryAtom` `{bboxSource, where, limit, offset}`. Never the whole dataset.
- Per-task gate for pure-TS code: `cd examples/web && npx tsc --noEmit && npm test`. For worker/UI/wasm code that vitest cannot exercise (no DOM/worker/wasm in the node test env): `cd examples/web && npx tsc --noEmit && npm run build`. Final acceptance is in-browser (Task 6).
- Do NOT run root `just check`: it has a pre-existing, unrelated rustfmt drift in `src/rust/fcb_core/tests/http.rs`. Scope all verification to `examples/web`.
- Commit to the current branch (`develop`) at the end of each task.
- Every git commit message ends with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

## File Structure

- `examples/web/src/export/index.ts` (new) — pure-TS: `ExportFormat` type, `FORMATS` registry (ext/mime/label), `assembleCityJSONSeq`, `deriveFilename`. Node/vitest-testable.
- `examples/web/src/export/export.test.ts` (new) — vitest units for the above.
- `examples/web/src/wasm/fcb_wasm.js`, `fcb_wasm.d.ts`, `fcb_wasm_bg.wasm`, `fcb_wasm_bg.wasm.d.ts` (new, vendored) — the prebuilt binding.
- `examples/web/src/export/wasm.ts` (new) — lazy WASM initializer + `convertMergedCityJSON`, `convertObj`.
- `examples/web/src/worker/protocol.ts` (modify) — `ExportRequest` / `ExportResponse`.
- `examples/web/src/worker/fcb.worker.ts` (modify) — `handleExport`.
- `examples/web/src/store/index.ts` (modify) — `exportFormatAtom`, `exportingAtom`, `sourceNameAtom`.
- `examples/web/src/hooks/useFcbData.ts` (modify) — `exportAs` action + download trigger; set `sourceNameAtom`.
- `examples/web/src/components/ExportPanel.tsx` (new) — the UI.
- `examples/web/src/App.tsx` (modify) — mount `ExportPanel`; fix the "no WASM" tagline.
- `examples/web/README.md` (modify) — document export.

---

### Task 1: Pure-TS export helpers

**Files:**
- Create: `examples/web/src/export/index.ts`
- Test: `examples/web/src/export/export.test.ts`

**Interfaces:**
- Produces:
  - `type ExportFormat = 'cityjson' | 'cityjsonseq' | 'obj'`
  - `interface FormatSpec { ext: string; mime: string; label: string }`
  - `const FORMATS: Record<ExportFormat, FormatSpec>`
  - `function assembleCityJSONSeq(metadata: unknown, feats: unknown[]): string`
  - `function deriveFilename(source: string | undefined, format: ExportFormat): string`

- [ ] **Step 1: Write the failing test**

Create `examples/web/src/export/export.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import {
  assembleCityJSONSeq, deriveFilename, FORMATS,
} from './index'

describe('FORMATS registry', () => {
  it('covers all three formats with distinct extensions', () => {
    expect(FORMATS.cityjson.ext).toBe('.city.json')
    expect(FORMATS.cityjsonseq.ext).toBe('.city.jsonl')
    expect(FORMATS.obj.ext).toBe('.obj')
    expect(FORMATS.cityjson.mime).toBe('application/json')
    expect(FORMATS.cityjsonseq.mime).toBe('application/x-ndjson')
    expect(FORMATS.obj.mime).toBe('text/plain')
  })
})

describe('assembleCityJSONSeq', () => {
  it('emits the metadata line first, then one line per feature', () => {
    const meta = { type: 'CityJSON', version: '2.0' }
    const feats = [{ type: 'CityJSONFeature', id: 'a' }, { type: 'CityJSONFeature', id: 'b' }]
    const out = assembleCityJSONSeq(meta, feats)
    const lines = out.split('\n')
    expect(lines).toHaveLength(3)
    expect(JSON.parse(lines[0]).type).toBe('CityJSON')
    expect(JSON.parse(lines[1]).id).toBe('a')
    expect(JSON.parse(lines[2]).id).toBe('b')
  })

  it('produces a single metadata line when there are no features', () => {
    const out = assembleCityJSONSeq({ type: 'CityJSON' }, [])
    expect(out.split('\n')).toHaveLength(1)
  })
})

describe('deriveFilename', () => {
  it('strips .fcb and the URL path, then appends the format extension', () => {
    const url = 'https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb'
    expect(deriveFilename(url, 'cityjson')).toBe('3dbag_all_index.city.json')
    expect(deriveFilename(url, 'cityjsonseq')).toBe('3dbag_all_index.city.jsonl')
    expect(deriveFilename(url, 'obj')).toBe('3dbag_all_index.obj')
  })

  it('handles a bare local filename', () => {
    expect(deriveFilename('delft.fcb', 'obj')).toBe('delft.obj')
  })

  it('drops query strings and falls back when there is no source', () => {
    expect(deriveFilename('http://x/y/a.fcb?token=1', 'cityjson')).toBe('a.city.json')
    expect(deriveFilename(undefined, 'obj')).toBe('flatcitybuf-export.obj')
    expect(deriveFilename('', 'obj')).toBe('flatcitybuf-export.obj')
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd examples/web && npx vitest run src/export/export.test.ts`
Expected: FAIL — cannot resolve `./index` (module not found).

- [ ] **Step 3: Write minimal implementation**

Create `examples/web/src/export/index.ts`:

```ts
// src/export/index.ts
// Pure-TS export helpers: the format registry, CityJSONSeq assembly, and
// download-filename derivation. No DOM and no WASM here, so this is unit
// testable in the node vitest env; the WASM-backed conversions live in
// ./wasm.ts.

export type ExportFormat = 'cityjson' | 'cityjsonseq' | 'obj'

export interface FormatSpec {
  ext: string
  mime: string
  label: string
}

export const FORMATS: Record<ExportFormat, FormatSpec> = {
  cityjson: { ext: '.city.json', mime: 'application/json', label: 'CityJSON' },
  cityjsonseq: { ext: '.city.jsonl', mime: 'application/x-ndjson', label: 'CityJSONSeq' },
  obj: { ext: '.obj', mime: 'text/plain', label: 'OBJ' },
}

/** One JSON object per line: the metadata line first, then each feature. This
 *  is CityJSONSeq (`.city.jsonl`) and needs no WASM. */
export function assembleCityJSONSeq(metadata: unknown, feats: unknown[]): string {
  return [metadata, ...feats].map((o) => JSON.stringify(o)).join('\n')
}

function basename(source: string): string {
  const noFragment = source.split(/[?#]/)[0]
  return noFragment.split('/').pop() ?? ''
}

/** Build the download filename from the open source (URL or local file name):
 *  take the basename, strip a trailing `.fcb`, and append the format's
 *  extension. Falls back to `flatcitybuf-export` when there is no source. */
export function deriveFilename(source: string | undefined, format: ExportFormat): string {
  const stem = source ? basename(source).replace(/\.fcb$/i, '').trim() : ''
  const base = stem === '' ? 'flatcitybuf-export' : stem
  return base + FORMATS[format].ext
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd examples/web && npx vitest run src/export/export.test.ts`
Expected: PASS (all cases green).

- [ ] **Step 5: Commit**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git add examples/web/src/export/index.ts examples/web/src/export/export.test.ts
git commit -m "feat(examples): pure-TS export helpers (formats, CityJSONSeq, filename)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Vendor the WASM binding + lazy initializer

**Files:**
- Create (vendored copies): `examples/web/src/wasm/fcb_wasm.js`, `examples/web/src/wasm/fcb_wasm.d.ts`, `examples/web/src/wasm/fcb_wasm_bg.wasm`, `examples/web/src/wasm/fcb_wasm_bg.wasm.d.ts`
- Create: `examples/web/src/export/wasm.ts`

**Interfaces:**
- Consumes (from the vendored binding): `default init(): Promise<...>`, `cjseqToCj(base_cj: any, features: any): any`, `cjToObj(city_json_js: any): string`.
- Produces:
  - `function convertMergedCityJSON(metadata: unknown, feats: unknown[]): Promise<string>`
  - `function convertObj(metadata: unknown, feats: unknown[]): Promise<string>`

- [ ] **Step 1: Vendor the prebuilt binding**

Run:

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
mkdir -p examples/web/src/wasm
cp src/rust/wasm/pkg/fcb_wasm.js \
   src/rust/wasm/pkg/fcb_wasm.d.ts \
   src/rust/wasm/pkg/fcb_wasm_bg.wasm \
   src/rust/wasm/pkg/fcb_wasm_bg.wasm.d.ts \
   examples/web/src/wasm/
ls -lh examples/web/src/wasm/
```

Expected: four files present; `fcb_wasm_bg.wasm` ≈ 4.1 MB.

- [ ] **Step 2: Confirm the vendored copy is trackable (not gitignored)**

Run: `cd /Users/hbbaba/tudelft/cityjson/flatcitybuf && git check-ignore -v examples/web/src/wasm/fcb_wasm_bg.wasm; echo "exit=$?"`
Expected: no output and `exit=1` (i.e. NOT ignored). If it prints a rule, stop and reconsider — do not force-add.

- [ ] **Step 3: Write the lazy initializer**

Create `examples/web/src/export/wasm.ts`:

```ts
// src/export/wasm.ts
// Lazily initialise the vendored fcb_wasm binding and expose just the two
// conversion functions the exporter needs. The 4 MB .wasm is fetched only on
// first use (first CityJSON-merged or OBJ export), never at module load. This
// module is worker-only (it runs where the reader produces CityJSON); it is
// not exercised by vitest.
import init, { cjToObj, cjseqToCj } from '../wasm/fcb_wasm.js'

let ready: Promise<unknown> | undefined
function ensureWasm(): Promise<unknown> {
  if (ready === undefined) ready = init()
  return ready
}

/** Merge a metadata CityJSON + array of CityJSONFeatures into one CityJSON
 *  object (the Rust oracle's merge), serialized. */
export async function convertMergedCityJSON(
  metadata: unknown, feats: unknown[],
): Promise<string> {
  await ensureWasm()
  return JSON.stringify(cjseqToCj(metadata, feats))
}

/** Triangulate the CityJSONSeq (metadata first, then features) to Wavefront
 *  OBJ. Includes every LoD present in the data. */
export async function convertObj(
  metadata: unknown, feats: unknown[],
): Promise<string> {
  await ensureWasm()
  return cjToObj([metadata, ...feats])
}
```

- [ ] **Step 4: Type-check and build**

Run: `cd examples/web && npx tsc --noEmit && npm run build`
Expected: tsc clean; `vite build` succeeds and reports an emitted `fcb_wasm_bg-*.wasm` asset in the bundle output.

- [ ] **Step 5: Commit**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git add examples/web/src/wasm/ examples/web/src/export/wasm.ts
git commit -m "feat(examples): vendor fcb_wasm binding + lazy conversion initializer

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Worker protocol + `handleExport`

**Files:**
- Modify: `examples/web/src/worker/protocol.ts`
- Modify: `examples/web/src/worker/fcb.worker.ts`

**Interfaces:**
- Consumes: `assembleCityJSONSeq`, `FORMATS`, `ExportFormat` (Task 1); `convertMergedCityJSON`, `convertObj` (Task 2); `runQuery`, `toCityJSONMetadata`, `f.toCityJSON(header)` (existing).
- Produces: `ExportRequest`, `ExportResponse` on the `WorkerRequest` / `WorkerResponse` unions.

- [ ] **Step 1: Add the protocol messages**

In `examples/web/src/worker/protocol.ts`:

Add the import near the top (after the existing type imports):

```ts
import type { ExportFormat } from '../export/index'
```

Add `ExportRequest` after `QueryRequest` and include it in `WorkerRequest`:

```ts
export interface ExportRequest {
  type: 'export'
  id: number
  bboxSource?: [number, number, number, number]
  where?: AttrCondition[]
  limit: number
  offset: number
  format: ExportFormat
}
export type WorkerRequest = OpenRequest | QueryRequest | ExportRequest
```

Add `ExportResponse` after `ResultResponse` and include it in `WorkerResponse`:

```ts
export interface ExportResponse {
  type: 'export-result'
  id: number
  data: string
  mime: string
  ext: string
}
export type WorkerResponse = OpenedResponse | ResultResponse | ExportResponse | ErrorResponse
```

- [ ] **Step 2: Implement `handleExport` in the worker**

In `examples/web/src/worker/fcb.worker.ts`, extend the imports:

```ts
import { assembleCityJSONSeq, FORMATS } from '../export/index'
import { convertMergedCityJSON, convertObj } from '../export/wasm'
```

Add a dedicated abort controller beside the existing `controller`:

```ts
let exportController: AbortController | null = null
```

Add the handler (place it after `handleQuery`):

```ts
async function handleExport(msg: Extract<WorkerRequest, { type: 'export' }>): Promise<void> {
  if (reader === undefined || model === undefined) {
    post({ type: 'error', id: msg.id, message: 'no file open', aborted: false })
    return
  }
  // A newer export aborts this one, but never the live render query.
  exportController?.abort()
  const my = new AbortController()
  exportController = my
  const signal = my.signal
  try {
    const { features } = await runQuery(reader, {
      bboxSource: msg.bboxSource, where: msg.where,
      limit: msg.limit, offset: msg.offset, signal,
    })
    if (signal.aborted) { post({ type: 'error', id: msg.id, message: 'aborted', aborted: true }); return }
    const metadata = toCityJSONMetadata(reader.header)
    const feats = features.map((f) => f.toCityJSON(reader!.header))
    let data: string
    if (msg.format === 'cityjsonseq') data = assembleCityJSONSeq(metadata, feats)
    else if (msg.format === 'cityjson') data = await convertMergedCityJSON(metadata, feats)
    else data = await convertObj(metadata, feats)
    const spec = FORMATS[msg.format]
    post({ type: 'export-result', id: msg.id, data, mime: spec.mime, ext: spec.ext })
  } catch (e) {
    post({
      type: 'error', id: msg.id,
      message: e instanceof Error ? e.message : String(e),
      aborted: signal.aborted,
    })
  }
}
```

Route export messages in `ctx.onmessage`. The current tail is:

```ts
  await handleQuery(msg)
}
```

Replace that tail with:

```ts
  if (msg.type === 'export') { await handleExport(msg); return }
  await handleQuery(msg)
}
```

- [ ] **Step 3: Type-check and build**

Run: `cd examples/web && npx tsc --noEmit && npm run build`
Expected: both succeed. (The worker is verified functionally in Task 6.)

- [ ] **Step 4: Commit**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git add examples/web/src/worker/protocol.ts examples/web/src/worker/fcb.worker.ts
git commit -m "feat(examples): worker export handler for CityJSON/Seq/OBJ

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Store atoms + `exportAs` action

**Files:**
- Modify: `examples/web/src/store/index.ts`
- Modify: `examples/web/src/hooks/useFcbData.ts`

**Interfaces:**
- Consumes: `FORMATS`, `deriveFilename`, `ExportFormat` (Task 1); `ExportResponse` via `WorkerResponse` (Task 3); existing `activeQueryAtom`, `renderedAtom`, `statusAtom`.
- Produces: `exportFormatAtom`, `exportingAtom`, `sourceNameAtom`; `useFcbData().exportAs(format?: ExportFormat): Promise<void>` and `.exporting: boolean`.

- [ ] **Step 1: Add the store atoms**

In `examples/web/src/store/index.ts`, add the import for the type at the top with the other type imports:

```ts
import type { ExportFormat } from '../export/index'
```

Add the atoms (place them near `lodAtom` / `availableLodsAtom`):

```ts
/** The chosen export format for the download button. */
export const exportFormatAtom = atom<ExportFormat>('cityjson')

/** True while an export is being prepared in the worker. Disables the button
 *  and swaps its label to "preparing…". */
export const exportingAtom = atom<boolean>(false)

/** The open source (URL or local file name), kept for the download filename. */
export const sourceNameAtom = atom<string | undefined>(undefined)
```

- [ ] **Step 2: Wire `sourceNameAtom` and add `exportAs` in the hook**

In `examples/web/src/hooks/useFcbData.ts`:

Extend the store imports to include the new atoms and `renderedAtom`/`activeQueryAtom` are already imported. Add to the existing import block from `'../store/index'`:

```ts
  exportFormatAtom, exportingAtom, sourceNameAtom,
```

Add the export helpers import near the top:

```ts
import { deriveFilename, type ExportFormat, FORMATS } from '../export/index'
```

Inside `useFcbData`, add the atom hooks (next to the other `useAtom` calls):

```ts
  const [, setExporting] = useAtom(exportingAtom)
  const [, setSourceName] = useAtom(sourceNameAtom)
```

Set the source name in both openers. In `openUrl`, right after `const seq = ++requestSeq`:

```ts
    setSourceName(url)
```

In `openFile`, right after its `const seq = ++requestSeq`:

```ts
    setSourceName(file.name)
```

Add `setSourceName` to both openers' dependency arrays (append `, setSourceName`).

Add a DOM download helper above `useFcbData` (module scope, next to `zoomForSpan`):

```ts
function triggerDownload(data: string, mime: string, filename: string): void {
  const url = URL.createObjectURL(new Blob([data], { type: mime }))
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  URL.revokeObjectURL(url)
}
```

Add the `exportAs` action inside `useFcbData` (place it after `applyLod`):

```ts
  // Export the current query result. Re-runs the active query in the worker,
  // converts to the chosen format there, and downloads the returned string.
  const exportAs = useCallback(async (format?: ExportFormat) => {
    const fmt = format ?? store.get(exportFormatAtom)
    const a = store.get(activeQueryAtom)
    const count = store.get(renderedAtom).length
    if (a === undefined || count === 0) return
    setExporting(true)
    setStatus(`preparing ${FORMATS[fmt].label} …`)
    const r = await callWorker({
      type: 'export', id: ++msgId,
      bboxSource: a.bboxSource, where: a.where, limit: a.limit, offset: a.offset, format: fmt,
    })
    setExporting(false)
    if (r.type === 'error') { setStatus(`export failed: ${r.message}`); return }
    if (r.type !== 'export-result') return
    const filename = deriveFilename(store.get(sourceNameAtom), fmt)
    triggerDownload(r.data, r.mime, filename)
    setStatus(`downloaded ${filename} (${count} feature${count === 1 ? '' : 's'})`)
  }, [store, setExporting, setStatus])
```

Add `exportAs` and the exporting flag to the returned object. Change the final `return { ... }` to also include:

```ts
           exportAs,
           exporting: store.get(exportingAtom),
```

(Insert these into the existing returned object literal, e.g. right after `applyLod,`.)

- [ ] **Step 3: Type-check and build**

Run: `cd examples/web && npx tsc --noEmit && npm run build`
Expected: both succeed.

- [ ] **Step 4: Commit**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git add examples/web/src/store/index.ts examples/web/src/hooks/useFcbData.ts
git commit -m "feat(examples): export atoms + exportAs action with browser download

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: ExportPanel UI + mount

**Files:**
- Create: `examples/web/src/components/ExportPanel.tsx`
- Modify: `examples/web/src/App.tsx`

**Interfaces:**
- Consumes: `useFcbData().{rendered, exportAs, exporting, header}`, `exportFormatAtom`, `FORMATS`, `ExportFormat`; `SectionHeading`, `PrimaryButton`.

- [ ] **Step 1: Write the panel**

Create `examples/web/src/components/ExportPanel.tsx`:

```tsx
// src/components/ExportPanel.tsx
import { useAtom } from 'jotai'
import { type ExportFormat, FORMATS } from '../export/index'
import { useFcbData } from '../hooks/useFcbData'
import { exportFormatAtom } from '../store/index'
import { PrimaryButton, SectionHeading } from './ui'

const ORDER: ExportFormat[] = ['cityjson', 'cityjsonseq', 'obj']

export function ExportPanel() {
  const { header, rendered, exportAs, exporting } = useFcbData()
  const [format, setFormat] = useAtom(exportFormatAtom)
  if (header === undefined) return null
  const disabled = rendered.length === 0 || exporting
  return (
    <section className="space-y-2 text-sm">
      <SectionHeading>4. Export</SectionHeading>
      <div className="flex flex-wrap gap-1">
        {ORDER.map((f) => {
          const on = format === f
          return (
            <button key={f} type="button" onClick={() => setFormat(f)}
              aria-pressed={on}
              className={
                'rounded border px-2 py-1 text-xs transition-colors '
                + (on
                  ? 'border-cj-gold bg-cj-gold-soft font-semibold text-cj-charcoal'
                  : 'border-cj-charcoal/20 text-cj-charcoal-soft hover:border-cj-gold')
              }>
              {FORMATS[f].label}
            </button>
          )
        })}
      </div>
      <PrimaryButton disabled={disabled} onClick={() => void exportAs(format)}>
        {exporting ? 'preparing…' : `Download ${rendered.length} feature${rendered.length === 1 ? '' : 's'}`}
      </PrimaryButton>
      <p className="text-xs text-cj-charcoal-soft">
        Exports the current result. OBJ includes every LoD present in the data.
      </p>
    </section>
  )
}
```

- [ ] **Step 2: Mount it and fix the tagline**

In `examples/web/src/App.tsx`:

Add the import:

```ts
import { ExportPanel } from './components/ExportPanel'
```

Mount it after `<QueryPanel />`:

```tsx
        <QueryPanel />
        <ExportPanel />
```

Update the tagline `<p>` (reading is still WASM-free; only export uses WASM):

```tsx
          <p className="text-xs text-cj-charcoal-soft">
            Native TypeScript reader (@cityjson/flatcitybuf) — no server; format export runs in-browser via WASM.
          </p>
```

- [ ] **Step 3: Type-check and build**

Run: `cd examples/web && npx tsc --noEmit && npm test && npm run build`
Expected: tsc clean; vitest green (Task 1 units still pass); build succeeds.

- [ ] **Step 4: Commit**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git add examples/web/src/components/ExportPanel.tsx examples/web/src/App.tsx
git commit -m "feat(examples): ExportPanel UI (format selector + download)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: In-browser acceptance + docs

**Files:**
- Modify: `examples/web/README.md`

- [ ] **Step 1: Start the dev server (if not already running)**

Run: `cd examples/web && npm run dev` (serves at http://localhost:5173/). Leave it running.

- [ ] **Step 2: Verify each format downloads and is well-formed (Playwright MCP)**

Drive a real browser (synthetic events do not exercise the worker/download path — use the Playwright MCP tools):
1. Navigate to `http://localhost:5173/`; wait for the status to settle to `… rendered …` (the default dataset auto-loads).
2. In the Export panel, for each format:
   - Click the format button, then click **Download**.
   - Capture the downloaded file (Playwright exposes the download; save it to the scratchpad dir).
3. Assert on the saved files:
   - **CityJSON:** `JSON.parse(file).type === 'CityJSON'` and `Object.keys(CityObjects).length > 0`.
   - **CityJSONSeq:** first line parses with `type === 'CityJSON'`; a later line parses with `type === 'CityJSONFeature'`.
   - **OBJ:** contains at least one line starting with `v ` and one starting with `f `.

Expected: all three assertions hold.

If `cjseqToCj` or `cjToObj` throws (surfaced as `export failed: …` in the panel status), STOP and report the exact message to the user before any further change. The most likely cause is a CityJSON `version` the vendored 0.7.5 binding does not accept — that is a decision point (rebuild the binding vs. adjust), not something to trial-and-error.

- [ ] **Step 3: Update the README**

In `examples/web/README.md`, add an "Export" subsection documenting: the three formats, that export covers the current query result (not the whole dataset), that conversion runs in-browser via the vendored `fcb_wasm` binding (lazy-loaded), and that OBJ includes all LoDs present. Keep it consistent with the existing README's tone and headings.

- [ ] **Step 4: Full scoped check**

Run: `cd examples/web && just check`
Expected: type + vitest + build all green. (Do NOT run root `just check` — pre-existing unrelated rustfmt drift.)

- [ ] **Step 5: Commit**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git add examples/web/README.md
git commit -m "docs(examples): document multi-format export in the web viewer

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- Formats CityJSON/CityJSONSeq/OBJ → Task 1 (registry, Seq), Task 2 (merged, OBJ). ✓
- Current-query-result scope via `activeQueryAtom` → Task 4 `exportAs`. ✓
- WASM reuse (`cjseqToCj`/`cjToObj`), vendored, lazy → Task 2. ✓
- Worker conversion + main-thread download → Task 3 + Task 4. ✓
- UI (format selector + Download, disabled/preparing states, OBJ-all-LoDs note) → Task 5. ✓
- Own AbortController, error surfacing → Task 3 `handleExport`, Task 4 error path. ✓
- Filename from source basename, fallback → Task 1 `deriveFilename`, Task 4 wiring. ✓
- Testing: pure-TS vitest (Task 1), in-browser acceptance (Task 6). ✓
- README + tagline fix → Task 5, Task 6. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; commands have expected output. ✓

**Type consistency:** `ExportFormat` defined in Task 1, imported by Tasks 3/4/5. `FORMATS[fmt].{mime,ext,label}` used consistently. `ExportResponse` fields `{data,mime,ext}` produced in Task 3, consumed in Task 4. `exportAs(format?)` / `exporting` produced in Task 4, consumed in Task 5. `convertMergedCityJSON`/`convertObj` produced in Task 2, consumed in Task 3. ✓
