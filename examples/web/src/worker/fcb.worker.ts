// src/worker/fcb.worker.ts
/// <reference lib="webworker" />
// Runs the FlatCityBuf reader off the main thread: opens the file (its HTTP
// range reads happen here), runs queries, and triangulates the results. Only
// the finished meshes cross back to the main thread — the ~40 ms/query of
// triangulation never blocks rendering.
import { type FcbReader as FcbReaderT, FcbReader, toCityJSONMetadata } from '@cityjson/flatcitybuf'
import { forward } from '../crs/index'
import { buildFeatureMesh } from '../geometry/index'
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

function highestLodGeometry(
  obj: { geometry?: { type: string; lod?: string }[] } | undefined,
): { type: string; lod?: string } | undefined {
  const geoms = obj?.geometry ?? []
  if (geoms.length === 0) return undefined
  return geoms.reduce((best, g) =>
    (Number(g.lod ?? -1) > Number(best.lod ?? -1) ? g : best), geoms[0])
}

/** Triangulates the query's features into transfer-ready meshes. */
function buildFeatures(
  r: FcbReaderT, m: HeaderModel, features: Awaited<ReturnType<typeof runQuery>>['features'],
): WorkerFeature[] {
  if (!m.crs.supported || m.crs.code === null) return []
  const code = m.crs.code
  const transform = toCityJSONMetadata(r.header).transform
  const out: WorkerFeature[] = []
  for (const f of features) {
    const cj = f.toCityJSON(r.header)
    const fm = buildFeatureMesh(cj, transform, (xy) => forward(code, xy))
    if (fm === null) continue
    const objects = Object.values(cj.CityObjects)
    const primary = objects.reduce<typeof objects[number] | undefined>(
      (best, obj) => {
        const bestCount = Object.keys(best?.attributes ?? {}).length
        const count = Object.keys(obj.attributes ?? {}).length
        return count > bestCount ? obj : best
      },
      objects[0],
    )
    const g = highestLodGeometry(primary)
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
  return out
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
    const built = buildFeatures(reader, model, features)
    const transfer: Transferable[] = []
    for (const f of built) transfer.push(f.positions.buffer, f.normals.buffer, f.indices.buffer)
    post({ type: 'result', id: msg.id, total, features: built }, transfer)
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
  await handleQuery(msg)
}
