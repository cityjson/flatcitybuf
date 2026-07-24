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
