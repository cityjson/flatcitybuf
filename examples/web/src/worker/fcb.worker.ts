// src/worker/fcb.worker.ts
/// <reference lib="webworker" />
// Runs the FlatCityBuf reader off the main thread: opens the file (its HTTP
// range reads happen here), runs queries, and triangulates the results. Only
// the finished meshes cross back to the main thread — the ~40 ms/query of
// triangulation never blocks rendering.
import { type FcbReader as FcbReaderT, FcbReader, toCityJSONMetadata } from '@cityjson/flatcitybuf'
import { forward } from '../crs/index'
import { assembleCityJSONSeq, FORMATS } from '../export/index'
import { convertMergedCityJSON, convertObj } from '../export/wasm'
import { buildFeatureMesh, pickGeometry } from '../geometry/index'
import { type HeaderModel, headerModel, runQuery } from '../reader/index'
import type { FeatureInfo } from '../store/index'
import type { WorkerFeature, WorkerRequest, WorkerResponse } from './protocol'

const ctx = self as unknown as Worker
function post(msg: WorkerResponse, transfer: Transferable[] = []): void {
  ctx.postMessage(msg, transfer)
}

let reader: FcbReaderT | undefined
let model: HeaderModel | undefined
let controller: AbortController | null = null
let exportController: AbortController | null = null

/** Triangulates the query's features into transfer-ready meshes at the given
 *  LoD, and reports the union of LoD labels seen (for the selector). */
function buildFeatures(
  r: FcbReaderT, m: HeaderModel,
  features: Awaited<ReturnType<typeof runQuery>>['features'],
  lod: string | undefined,
): { features: WorkerFeature[]; lods: string[] } {
  if (!m.crs.supported || m.crs.code === null) return { features: [], lods: [] }
  const code = m.crs.code
  const transform = toCityJSONMetadata(r.header).transform
  const out: WorkerFeature[] = []
  const lodSet = new Set<string>()
  for (const f of features) {
    const cj = f.toCityJSON(r.header)
    // Record every LoD the file offers for this feature, regardless of which
    // one we render, so the selector can list all of them.
    for (const co of Object.values(cj.CityObjects)) {
      for (const g of co.geometry ?? []) {
        if (g.lod !== undefined && g.lod !== null) lodSet.add(String(g.lod))
      }
    }
    const fm = buildFeatureMesh(cj, transform, (xy) => forward(code, xy), lod)
    if (fm === null) continue
    const objects = Object.values(cj.CityObjects)
    // Attributes/type come from the richest object (in 3DBAG the Building
    // parent), which carries the semantics.
    const primary = objects.reduce<typeof objects[number] | undefined>(
      (best, obj) => {
        const bestCount = Object.keys(best?.attributes ?? {}).length
        const count = Object.keys(obj.attributes ?? {}).length
        return count > bestCount ? obj : best
      },
      objects[0],
    )
    // Geometry info comes from what was actually rendered at this LoD, across
    // ALL objects (the Building parent holds only LoD 0; the geometry the user
    // sees is the BuildingPart's solid) — so the inspector's LoD matches the
    // mesh on screen rather than the parent's.
    const allGeoms = objects.flatMap((o) => o.geometry ?? [])
    const g = pickGeometry(allGeoms, lod)
    const info: FeatureInfo = {
      objectType: primary?.type,
      geometryType: g?.type,
      lod: g?.lod,
      vertexCount: cj.vertices.length,
      triangleCount: fm.triangleCount,
    }
    out.push({
      id: f.id,
      centroidLngLat: fm.centroidLngLat,
      positions: fm.mesh.positions,
      normals: fm.mesh.normals,
      indices: fm.mesh.indices,
      info,
      attributes: primary?.attributes ?? {},
    })
  }
  return { features: out, lods: [...lodSet] }
}

async function handleQuery(msg: Extract<WorkerRequest, { type: 'query' }>): Promise<void> {
  if (reader === undefined || model === undefined) {
    post({ type: 'error', id: msg.id, message: 'no file open', aborted: false })
    return
  }
  // A newer query aborts this one's range reads.
  controller?.abort()
  const myController = new AbortController()
  controller = myController
  const signal = myController.signal
  try {
    const { features, total } = await runQuery(reader, {
      bboxSource: msg.bboxSource, where: msg.where, limit: msg.limit, offset: msg.offset, signal,
    })
    if (signal.aborted) { post({ type: 'error', id: msg.id, message: 'aborted', aborted: true }); return }
    const { features: built, lods } = buildFeatures(reader, model, features, msg.lod)
    const transfer: Transferable[] = []
    for (const f of built) transfer.push(f.positions.buffer, f.normals.buffer, f.indices.buffer)
    post({ type: 'result', id: msg.id, total, features: built, lods }, transfer)
  } catch (e) {
    post({
      type: 'error', id: msg.id,
      message: e instanceof Error ? e.message : String(e),
      aborted: signal.aborted,
    })
  }
}

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
    if (signal.aborted) { post({ type: 'error', id: msg.id, message: 'aborted', aborted: true }); return }
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

ctx.onmessage = async (ev: MessageEvent<WorkerRequest>) => {
  const msg = ev.data
  if (msg.type === 'open') {
    try {
      reader = msg.url !== undefined
        ? await FcbReader.fromUrl(msg.url)
        : await FcbReader.fromBytes(new Uint8Array(msg.buffer as ArrayBuffer))
      model = headerModel(reader.header)
      post({ type: 'opened', id: msg.id, header: model })
    } catch (e) {
      post({ type: 'error', id: msg.id, message: e instanceof Error ? e.message : String(e), aborted: false })
    }
    return
  }
  if (msg.type === 'export') { await handleExport(msg); return }
  await handleQuery(msg)
}
