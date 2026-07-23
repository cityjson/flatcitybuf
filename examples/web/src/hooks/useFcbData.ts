// src/hooks/useFcbData.ts
import type { AttrCondition } from '@cityjson/flatcitybuf'
import { useAtom } from 'jotai'
import { useCallback } from 'react'
import { bboxToSource, forward } from '../crs/index'
import type { HeaderModel } from '../reader/index'
import {
  activeQueryAtom, fetchBboxAtom, headerAtom, limitAtom, loadingAtom, readyAtom,
  type RenderedFeature, renderedAtom, selectedAtom, statusAtom, totalAtom,
  type ViewState, viewStateAtom,
} from '../store/index'
import type { WorkerRequest, WorkerResponse } from '../worker/protocol'

// One worker for the whole app: it owns the reader and does the query +
// triangulation off the main thread. Created lazily on first use.
let worker: Worker | undefined
let msgId = 0
const pending = new Map<number, (r: WorkerResponse) => void>()

function getWorker(): Worker {
  if (worker === undefined) {
    worker = new Worker(new URL('../worker/fcb.worker.ts', import.meta.url), { type: 'module' })
    worker.onmessage = (ev: MessageEvent<WorkerResponse>) => {
      const cb = pending.get(ev.data.id)
      if (cb !== undefined) { pending.delete(ev.data.id); cb(ev.data) }
    }
  }
  return worker
}

function callWorker(msg: WorkerRequest, transfer: Transferable[] = []): Promise<WorkerResponse> {
  return new Promise((resolve) => {
    pending.set(msg.id, resolve)
    getWorker().postMessage(msg, transfer)
  })
}

// Drops stale commits: a newer open/query bumps this, and any response whose
// captured seq no longer matches is ignored (the worker also aborts the
// superseded query's range reads).
let requestSeq = 0

function zoomForSpan(spanLng: number, spanLat: number): number {
  const span = Math.max(spanLng, spanLat, 1e-4)
  return Math.min(18, Math.max(11, Math.log2(360 / span) - 1.5))
}

export function useFcbData() {
  const [header, setHeader] = useAtom(headerAtom)
  const [rendered, setRendered] = useAtom(renderedAtom)
  const [total, setTotal] = useAtom(totalAtom)
  const [status, setStatus] = useAtom(statusAtom)
  const [active, setActive] = useAtom(activeQueryAtom)
  const [limit] = useAtom(limitAtom)
  const [, setReady] = useAtom(readyAtom)
  const [, setLoading] = useAtom(loadingAtom)
  const [, setFetchBbox] = useAtom(fetchBboxAtom)
  const [, setSelected] = useAtom(selectedAtom)
  const [, setViewState] = useAtom(viewStateAtom)

  const frameToFeatures = useCallback((out: RenderedFeature[]) => {
    if (out.length === 0) return
    let minLng = Infinity, minLat = Infinity, maxLng = -Infinity, maxLat = -Infinity
    for (const f of out) {
      const [lng, lat] = f.centroidLngLat
      minLng = Math.min(minLng, lng); maxLng = Math.max(maxLng, lng)
      minLat = Math.min(minLat, lat); maxLat = Math.max(maxLat, lat)
    }
    setViewState((v: ViewState) => ({
      ...v,
      longitude: (minLng + maxLng) / 2,
      latitude: (minLat + maxLat) / 2,
      zoom: zoomForSpan(maxLng - minLng, maxLat - minLat),
    }))
  }, [setViewState])

  // Sends a query to the worker and renders the meshes it returns. `frameCamera`
  // is false for follow-camera queries (re-framing would move the camera, which
  // would retrigger the follow query).
  const runQuery = useCallback(async (
    spec: { bboxSource?: [number, number, number, number]; where?: AttrCondition[] },
    frameCamera: boolean,
  ): Promise<boolean> => {
    const seq = ++requestSeq
    const q = { ...spec, limit, offset: 0 }
    setLoading(true)
    setStatus('querying…')
    const r = await callWorker({ type: 'query', id: ++msgId, ...q })
    // A newer request owns the indicator now — leave it on for that one.
    if (seq !== requestSeq) return false
    setLoading(false)
    if (r.type === 'error') {
      if (!r.aborted) setStatus(`query failed: ${r.message}`)
      return false
    }
    if (r.type !== 'result') return false
    const out: RenderedFeature[] = r.features.map((f) => ({
      id: f.id,
      centroidLngLat: f.centroidLngLat,
      mesh: { positions: f.positions, normals: f.normals, indices: f.indices },
      attributes: f.attributes,
      info: f.info,
    }))
    setActive(q); setTotal(r.total); setRendered(out)
    if (frameCamera) frameToFeatures(out)
    const more = r.total !== undefined && r.total > out.length ? ` of ${r.total}` : ''
    setStatus(`${out.length} rendered${more}`)
    return true
  }, [limit, setActive, setTotal, setRendered, setLoading, setStatus, frameToFeatures])

  // Opening a file also renders it: the map otherwise shows only the basemap.
  const onOpened = useCallback((h: HeaderModel) => {
    setHeader(h)
    setReady(true)
    setSelected(undefined)
    setFetchBbox(undefined)
    if (h.crs.supported && h.crs.code !== null && h.extent) {
      const code = h.crs.code
      const [minX, minY, , maxX, maxY] = h.extent
      const corners: [number, number][] = (
        [[minX, minY], [maxX, minY], [maxX, maxY], [minX, maxY]] as [number, number][]
      ).map((c) => forward(code, c))
      const lngs = corners.map((c) => c[0])
      const lats = corners.map((c) => c[1])
      setViewState((v: ViewState) => ({
        ...v,
        longitude: (Math.min(...lngs) + Math.max(...lngs)) / 2,
        latitude: (Math.min(...lats) + Math.max(...lats)) / 2,
        zoom: zoomForSpan(
          Math.max(...lngs) - Math.min(...lngs),
          Math.max(...lats) - Math.min(...lats),
        ),
      }))
    }
    if (!h.crs.supported || h.crs.code === null) {
      setRendered([]); setTotal(undefined); setActive(undefined)
      setLoading(false)
      setStatus('file opened — CRS not supported, cannot georeference/render')
      return
    }
    void runQuery({}, true) // auto-render the first page (whole dataset, capped by limit)
  }, [setHeader, setReady, setSelected, setFetchBbox, setViewState, setRendered, setTotal, setActive, setLoading, setStatus, runQuery])

  const openUrl = useCallback(async (url: string) => {
    const seq = ++requestSeq
    setReady(false); setRendered([]); setTotal(undefined); setActive(undefined)
    setLoading(true)
    setStatus(`opening ${url} …`)
    const r = await callWorker({ type: 'open', id: ++msgId, url })
    if (seq !== requestSeq) return
    // On success onOpened runs the first query, which owns the indicator until
    // it settles; only the failure path clears it here.
    if (r.type === 'opened') onOpened(r.header)
    else if (r.type === 'error') { setLoading(false); setStatus(`failed to open URL: ${r.message}`) }
  }, [onOpened, setReady, setRendered, setTotal, setActive, setLoading, setStatus])

  const openFile = useCallback(async (file: File) => {
    const seq = ++requestSeq
    setReady(false); setRendered([]); setTotal(undefined); setActive(undefined)
    setLoading(true)
    setStatus(`opening ${file.name} …`)
    const buffer = await file.arrayBuffer()
    const r = await callWorker({ type: 'open', id: ++msgId, buffer }, [buffer])
    if (seq !== requestSeq) return
    if (r.type === 'opened') onOpened(r.header)
    else if (r.type === 'error') { setLoading(false); setStatus(`failed to open file: ${r.message}`) }
  }, [onOpened, setReady, setRendered, setTotal, setActive, setLoading, setStatus])

  const query = useCallback((
    spec: { bboxSource?: [number, number, number, number]; where?: AttrCondition[] },
  ) => {
    setSelected(undefined)
    setFetchBbox(undefined) // not a camera-derived bbox — hide the outline
    void runQuery(spec, true)
  }, [runQuery, setSelected, setFetchBbox])

  // Follow-camera: query the viewport bbox without moving the camera.
  const queryViewport = useCallback((
    bounds: [number, number, number, number], where?: AttrCondition[],
  ) => {
    if (header === undefined || !header.crs.supported || header.crs.code === null) return
    const bboxSource = bboxToSource(header.crs.code, bounds[0], bounds[1], bounds[2], bounds[3])
    // Show the fetched region (inset inside the view) only once it commits, so
    // the outline always matches the features currently on screen.
    void runQuery({ bboxSource, where }, false).then((ok) => {
      if (ok) setFetchBbox(bounds)
    })
  }, [header, runQuery, setFetchBbox])

  const loadNext = useCallback(() => {
    if (active === undefined) return
    setSelected(undefined)
    const seq = ++requestSeq
    const q = { ...active, offset: active.offset + active.limit }
    setLoading(true)
    setStatus('loading next batch…')
    void callWorker({ type: 'query', id: ++msgId, ...q }).then((r) => {
      if (seq !== requestSeq) return
      setLoading(false)
      if (r.type === 'error') { if (!r.aborted) setStatus(`load failed: ${r.message}`); return }
      if (r.type !== 'result') return
      const out: RenderedFeature[] = r.features.map((f) => ({
        id: f.id, centroidLngLat: f.centroidLngLat,
        mesh: { positions: f.positions, normals: f.normals, indices: f.indices },
        attributes: f.attributes, info: f.info,
      }))
      setActive(q); setTotal(r.total); setRendered(out); frameToFeatures(out)
      setStatus(`${out.length} rendered${r.total !== undefined ? ` of ${r.total}` : ''}`)
    })
  }, [active, setSelected, setActive, setTotal, setRendered, setLoading, setStatus, frameToFeatures])

  return { openUrl, openFile, query, queryViewport, loadNext, status, header,
           rendered, total,
           hasMore: total !== undefined && active !== undefined
             && active.offset + active.limit < total }
}
