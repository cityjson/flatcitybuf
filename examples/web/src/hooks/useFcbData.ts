// src/hooks/useFcbData.ts
import type { AttrCondition } from '@cityjson/flatcitybuf'
import { useAtom, useStore } from 'jotai'
import { useCallback } from 'react'
import { bboxToSource, forward } from '../crs/index'
import { deriveFilename, type ExportFormat, FORMATS } from '../export/index'
import type { HeaderModel } from '../reader/index'
import {
  activeQueryAtom, availableLodsAtom, exportFormatAtom, exportingAtom,
  fetchBboxAtom, headerAtom, limitAtom, loadingAtom, lodAtom, MIN_FETCH_ZOOM,
  readyAtom, type RenderedFeature, renderedAtom, selectedAtom, sourceNameAtom,
  spatialModeAtom, statusAtom, totalAtom, type ViewState, viewStateAtom,
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
// Generation guard for exports: bumped on each export and on every open, so a
// stale export (superseded by a newer export or a new file) discards its result
// instead of downloading under the wrong name or leaving the UI stuck.
let exportSeq = 0

function zoomForSpan(spanLng: number, spanLat: number): number {
  const span = Math.max(spanLng, spanLat, 1e-4)
  return Math.min(18, Math.max(11, Math.log2(360 / span) - 1.5))
}

function triggerDownload(data: string, mime: string, filename: string): void {
  const url = URL.createObjectURL(new Blob([data], { type: mime }))
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  // Revoke after a delay: some browsers need the object URL to stay valid until
  // the download has actually started.
  setTimeout(() => URL.revokeObjectURL(url), 60_000)
}

export function useFcbData() {
  const [header, setHeader] = useAtom(headerAtom)
  const [rendered, setRendered] = useAtom(renderedAtom)
  const [total, setTotal] = useAtom(totalAtom)
  const [status, setStatus] = useAtom(statusAtom)
  const [active, setActive] = useAtom(activeQueryAtom)
  const [limit] = useAtom(limitAtom)
  const [lod, setLod] = useAtom(lodAtom)
  const [availableLods, setAvailableLods] = useAtom(availableLodsAtom)
  const [, setReady] = useAtom(readyAtom)
  const [, setLoading] = useAtom(loadingAtom)
  const [, setFetchBbox] = useAtom(fetchBboxAtom)
  const [, setSelected] = useAtom(selectedAtom)
  const [, setViewState] = useAtom(viewStateAtom)
  const [exporting, setExporting] = useAtom(exportingAtom)
  const [, setSourceName] = useAtom(sourceNameAtom)
  // Read `lod`/`mode` from the store at call time (not via closures): an open is
  // async, and the file may be reset or the mode switched while it is in flight.
  const store = useStore()

  // Union the LoDs seen in a result into the discovered set. The selection is
  // NOT pinned here: `lodAtom` stays undefined ("auto = highest") until the user
  // picks one, so the default keeps tracking the highest as discovery grows.
  const mergeLods = useCallback((lods: string[]) => {
    if (lods.length === 0) return
    setAvailableLods((prev) => {
      const s = new Set(prev)
      for (const l of lods) s.add(l)
      return [...s].sort((a, b) => Number(a) - Number(b))
    })
  }, [setAvailableLods])

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
    spec: {
      bboxSource?: [number, number, number, number]
      where?: AttrCondition[]
      /** Override the current LoD selection (used when applying a new LoD). */
      lod?: string
    },
    frameCamera: boolean,
  ): Promise<boolean> => {
    const seq = ++requestSeq
    const useLod = spec.lod ?? store.get(lodAtom)
    const q = { bboxSource: spec.bboxSource, where: spec.where, limit, offset: 0 }
    setLoading(true)
    setStatus('querying…')
    const r = await callWorker({ type: 'query', id: ++msgId, ...q, lod: useLod })
    // A newer request owns the indicator now — leave it on for that one.
    if (seq !== requestSeq) return false
    setLoading(false)
    if (r.type === 'error') {
      if (!r.aborted) setStatus(`query failed: ${r.message}`)
      return false
    }
    if (r.type !== 'result') return false
    mergeLods(r.lods)
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
  }, [limit, store, mergeLods, setActive, setTotal, setRendered, setLoading, setStatus, frameToFeatures])

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
        // Never land below the fetch gate: a country-scale extent fits at ~zoom
        // 11, which follow mode treats as "too far" and shows the zoom-in hint
        // with nothing on screen. Clamping up puts us at a fetchable zoom on the
        // extent centre, and follow then loads that viewport.
        zoom: Math.max(MIN_FETCH_ZOOM, zoomForSpan(
          Math.max(...lngs) - Math.min(...lngs),
          Math.max(...lats) - Math.min(...lats),
        )),
      }))
    }
    if (!h.crs.supported || h.crs.code === null) {
      setRendered([]); setTotal(undefined); setActive(undefined)
      setLoading(false)
      setStatus('file opened — CRS not supported, cannot georeference/render')
      return
    }
    // In follow mode (the default) the camera-follow effect fetches the framed
    // viewport itself, so skip the whole-dataset first page — for a 10M-feature
    // file it would be an arbitrary, spatially-scattered slice. Other modes
    // render the first page up front. Read the mode from the store, since it may
    // have changed while the (async) open was in flight.
    if (store.get(spatialModeAtom) === 'follow') {
      setStatus('following camera — loading the visible area…')
      return
    }
    void runQuery({}, true) // auto-render the first page (whole dataset, capped by limit)
  }, [store, setHeader, setReady, setSelected, setFetchBbox, setViewState, setRendered, setTotal, setActive, setLoading, setStatus, runQuery])

  const openUrl = useCallback(async (url: string) => {
    const seq = ++requestSeq
    exportSeq++ // invalidate any in-flight export
    setSourceName(url)
    setReady(false); setRendered([]); setTotal(undefined); setActive(undefined)
    setExporting(false)
    setAvailableLods([]); setLod(undefined) // a new file has its own LoD set
    setLoading(true)
    setStatus(`opening ${url} …`)
    const r = await callWorker({ type: 'open', id: ++msgId, url })
    if (seq !== requestSeq) return
    // On success onOpened runs the first query, which owns the indicator until
    // it settles; only the failure path clears it here.
    if (r.type === 'opened') onOpened(r.header)
    else if (r.type === 'error') { setLoading(false); setStatus(`failed to open URL: ${r.message}`) }
  }, [onOpened, setReady, setRendered, setTotal, setActive, setAvailableLods, setLod, setLoading, setStatus, setSourceName, setExporting])

  const openFile = useCallback(async (file: File) => {
    const seq = ++requestSeq
    exportSeq++ // invalidate any in-flight export
    setSourceName(file.name)
    setReady(false); setRendered([]); setTotal(undefined); setActive(undefined)
    setExporting(false)
    setAvailableLods([]); setLod(undefined) // a new file has its own LoD set
    setLoading(true)
    setStatus(`opening ${file.name} …`)
    const buffer = await file.arrayBuffer()
    const r = await callWorker({ type: 'open', id: ++msgId, buffer }, [buffer])
    if (seq !== requestSeq) return
    if (r.type === 'opened') onOpened(r.header)
    else if (r.type === 'error') { setLoading(false); setStatus(`failed to open file: ${r.message}`) }
  }, [onOpened, setReady, setRendered, setTotal, setActive, setAvailableLods, setLod, setLoading, setStatus, setSourceName, setExporting])

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
    void callWorker({ type: 'query', id: ++msgId, ...q, lod }).then((r) => {
      if (seq !== requestSeq) return
      setLoading(false)
      if (r.type === 'error') { if (!r.aborted) setStatus(`load failed: ${r.message}`); return }
      if (r.type !== 'result') return
      mergeLods(r.lods)
      const out: RenderedFeature[] = r.features.map((f) => ({
        id: f.id, centroidLngLat: f.centroidLngLat,
        mesh: { positions: f.positions, normals: f.normals, indices: f.indices },
        attributes: f.attributes, info: f.info,
      }))
      setActive(q); setTotal(r.total); setRendered(out); frameToFeatures(out)
      setStatus(`${out.length} rendered${r.total !== undefined ? ` of ${r.total}` : ''}`)
    })
  }, [active, lod, mergeLods, setSelected, setActive, setTotal, setRendered, setLoading, setStatus, frameToFeatures])

  // Switch the rendered LoD: remember the choice and re-run the current query
  // at that LoD (the mesh is triangulated per LoD in the worker, so this needs
  // a re-fetch). No camera move, so the view — and any follow outline — holds.
  const applyLod = useCallback((newLod: string) => {
    setLod(newLod)
    setSelected(undefined) // the selected feature's mesh is from the old LoD
    if (active === undefined) return
    void runQuery({ bboxSource: active.bboxSource, where: active.where, lod: newLod }, false)
  }, [active, runQuery, setLod, setSelected])

  // Export the current query result. Re-runs the active query in the worker,
  // converts to the chosen format there, and downloads the returned string.
  const exportAs = useCallback(async (format?: ExportFormat) => {
    const fmt = format ?? store.get(exportFormatAtom)
    const a = store.get(activeQueryAtom)
    const count = store.get(renderedAtom).length
    if (a === undefined || count === 0) return
    // Snapshot the generation and source at initiation: opening a new file (or a
    // newer export) bumps exportSeq, and the source name feeds the filename — so
    // a stale export neither downloads under the wrong name nor after a new file.
    const mySeq = ++exportSeq
    const source = store.get(sourceNameAtom)
    setExporting(true)
    setStatus(`preparing ${FORMATS[fmt].label} …`)
    try {
      const r = await callWorker({
        type: 'export', id: ++msgId,
        bboxSource: a.bboxSource, where: a.where, limit: a.limit, offset: a.offset, format: fmt,
      })
      if (mySeq !== exportSeq) return // superseded by a newer export or a new file
      if (r.type === 'error') { setStatus(`export failed: ${r.message}`); return }
      if (r.type !== 'export-result') return
      const filename = deriveFilename(source, fmt)
      triggerDownload(r.data, r.mime, filename)
      setStatus(`downloaded ${filename} (${count} feature${count === 1 ? '' : 's'})`)
    } finally {
      if (mySeq === exportSeq) setExporting(false)
    }
  }, [store, setExporting, setStatus])

  return { openUrl, openFile, query, queryViewport, loadNext, applyLod,
           exportAs, exporting, status,
           header, rendered, total, lod, availableLods,
           hasMore: total !== undefined && active !== undefined
             && active.offset + active.limit < total }
}
