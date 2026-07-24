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
