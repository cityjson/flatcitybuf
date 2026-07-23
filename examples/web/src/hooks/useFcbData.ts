// src/hooks/useFcbData.ts
import {
  type AttrCondition, type FcbReader, type Feature, toCityJSONMetadata,
} from '@cityjson/flatcitybuf'
import { useAtom } from 'jotai'
import { useCallback } from 'react'
import { forward } from '../crs/index'
import { buildFeatureMesh } from '../geometry/index'
import {
  describeError, type HeaderModel, headerModel, openFromBlob, openFromUrl, runQuery,
} from '../reader/index'
import {
  activeQueryAtom, headerAtom, readerAtom, type RenderedFeature, renderedAtom,
  selectedAtom, statusAtom, totalAtom, type ViewState, viewStateAtom,
} from '../store/index'

// How many features the auto-render on open (and a filterless query) shows.
// Kept in step with QueryPanel's default limit; also caps the per-feature
// deck.gl layer count.
const DEFAULT_LIMIT = 200

// Rough Web-Mercator zoom that frames a lng/lat span. Exact fitBounds needs the
// live viewport size; this heuristic is good enough to bring the data on-screen,
// clamped to sane city/building-scale bounds.
function zoomForSpan(spanLng: number, spanLat: number): number {
  const span = Math.max(spanLng, spanLat, 1e-4)
  return Math.min(18, Math.max(11, Math.log2(360 / span) - 1.5))
}

// Builds render-ready features from a SPECIFIC reader + header model rather than
// the hook's atoms, so it works before `setReader`/`setHeader` have committed
// (the auto-render on open needs exactly that). Returns empty when the CRS is
// unsupported — the caller decides the status message.
function buildRenderedFeatures(
  reader: FcbReader, model: HeaderModel, features: Feature[],
): { out: RenderedFeature[]; skipped: number } {
  if (!model.crs.supported || model.crs.code === null) {
    return { out: [], skipped: features.length }
  }
  const code = model.crs.code
  const transform = toCityJSONMetadata(reader.header).transform
  const out: RenderedFeature[] = []
  let skipped = 0
  for (const f of features) {
    const cj = f.toCityJSON(reader.header)
    const fm = buildFeatureMesh(cj, transform, (xy) => forward(code, xy))
    if (fm === null) { skipped++; continue }
    // Reader attribute matching is existential over ALL CityObjects in a
    // feature (parent + children), but a feature renders only one set of
    // attributes. There is no way to recover *which* object matched, so this
    // picks the CityObject with the most attribute keys as a deterministic
    // proxy (ties -> first in iteration order). A demo simplification: it can
    // still surface a different object's values than the one that matched.
    const objects = Object.values(cj.CityObjects)
    const primary = objects.reduce<typeof objects[number] | undefined>(
      (best, obj) => {
        const bestCount = Object.keys(best?.attributes ?? {}).length
        const count = Object.keys(obj.attributes ?? {}).length
        return count > bestCount ? obj : best
      },
      objects[0],
    )
    out.push({
      id: f.id, centroidLngLat: fm.centroidLngLat, mesh: fm.mesh,
      attributes: primary?.attributes ?? {},
    })
  }
  return { out, skipped }
}

// Module-level (not useRef) because useFcbData is called from multiple
// components; a per-instance ref wouldn't see requests issued by siblings.
// Bumped at the start of every open/query/loadNext call; any commit whose
// captured `seq` no longer matches the current counter is stale and is
// dropped instead of overwriting newer state.
let requestSeq = 0

export function useFcbData() {
  const [reader, setReader] = useAtom(readerAtom)
  const [header, setHeader] = useAtom(headerAtom)
  const [rendered, setRendered] = useAtom(renderedAtom)
  const [total, setTotal] = useAtom(totalAtom)
  const [status, setStatus] = useAtom(statusAtom)
  const [active, setActive] = useAtom(activeQueryAtom)
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

  // Opening a file also RENDERS it: without an automatic first query the map
  // shows only the basemap and the file looks "not displayed". Auto-runs a
  // filterless query (first DEFAULT_LIMIT features) so the model appears on
  // load; "Load next batch" then pages through the rest.
  const onOpened = useCallback(async (r: FcbReader, seq: number) => {
    const model = headerModel(r.header)
    setReader(r)
    setHeader(model)
    setSelected(undefined)
    // Frame to the file extent immediately, before features finish building.
    if (model.crs.supported && model.crs.code !== null && model.extent) {
      const code = model.crs.code
      const [minX, minY, , maxX, maxY] = model.extent
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
    if (!model.crs.supported || model.crs.code === null) {
      setRendered([]); setTotal(undefined); setActive(undefined)
      setStatus('file opened — CRS not supported, cannot georeference/render')
      return
    }
    setStatus('rendering features…')
    const spec = { limit: DEFAULT_LIMIT, offset: 0 }
    try {
      const { features, total: t } = await runQuery(r, spec)
      if (seq !== requestSeq) return
      const { out, skipped } = buildRenderedFeatures(r, model, features)
      setRendered(out)
      setTotal(t)
      setActive(spec)
      frameToFeatures(out)
      const more = t !== undefined && t > out.length ? ` of ${t}` : ''
      setStatus(`${out.length} rendered${skipped ? `, ${skipped} skipped` : ''}${more}`)
    } catch (e) {
      if (seq !== requestSeq) return
      setStatus(`file opened, but rendering failed: ${describeError(e)}`)
    }
  }, [setReader, setHeader, setRendered, setTotal, setSelected, setActive,
      setViewState, setStatus, frameToFeatures])

  const openUrl = useCallback(async (url: string) => {
    const seq = ++requestSeq
    setStatus(`opening ${url} ...`)
    try {
      const r = await openFromUrl(url)
      if (seq !== requestSeq) return
      await onOpened(r, seq)
    } catch (e) {
      if (seq !== requestSeq) return
      setStatus(`failed to open URL: ${describeError(e)}`)
    }
  }, [onOpened, setStatus])

  const openFile = useCallback(async (file: File) => {
    const seq = ++requestSeq
    setStatus(`opening ${file.name} ...`)
    try {
      const r = await openFromBlob(file)
      if (seq !== requestSeq) return
      await onOpened(r, seq)
    } catch (e) {
      if (seq !== requestSeq) return
      setStatus(`failed to open file: ${describeError(e)}`)
    }
  }, [onOpened, setStatus])

  const render = useCallback((features: Feature[]) => {
    if (reader === undefined || header === undefined) return
    if (!header.crs.supported || header.crs.code === null) {
      setStatus('CRS not supported — cannot georeference; not rendering')
      return
    }
    const { out, skipped } = buildRenderedFeatures(reader, header, features)
    setRendered(out)
    frameToFeatures(out)
    setStatus(`${out.length} rendered${skipped ? `, ${skipped} skipped` : ''}`)
  }, [reader, header, setRendered, frameToFeatures, setStatus])

  const query = useCallback(async (
    spec: { bboxSource?: [number, number, number, number]
            where?: AttrCondition[]; limit: number },
  ) => {
    if (reader === undefined) return
    const seq = ++requestSeq
    const q = { ...spec, offset: 0 }
    setSelected(undefined)
    setStatus('querying...')
    try {
      const { features, total: t } = await runQuery(reader, q)
      if (seq !== requestSeq) return
      setActive(q); setTotal(t); render(features)
    } catch (e) {
      if (seq !== requestSeq) return
      setStatus(`query failed: ${describeError(e)}`)
    }
  }, [reader, render, setActive, setSelected, setStatus, setTotal])

  const loadNext = useCallback(async () => {
    if (reader === undefined || active === undefined) return
    const seq = ++requestSeq
    const q = { ...active, offset: active.offset + active.limit }
    setSelected(undefined) // the current page (and its selection) is about to be replaced
    setStatus('loading next batch...')
    try {
      const { features, total: t } = await runQuery(reader, q)
      if (seq !== requestSeq) return
      setActive(q); setTotal(t); render(features) // replaces the rendered set with the next page
    } catch (e) {
      if (seq !== requestSeq) return
      setStatus(`load failed: ${describeError(e)}`)
    }
  }, [reader, active, render, setActive, setSelected, setStatus, setTotal])

  return { openUrl, openFile, query, loadNext, status, header, rendered, total,
           hasMore: total !== undefined && active !== undefined
             && active.offset + active.limit < total }
}
