// src/hooks/useFcbData.ts
import { type Feature, toCityJSONMetadata } from '@cityjson/flatcitybuf'
import { useAtom } from 'jotai'
import { useCallback } from 'react'
import { forward } from '../crs/index'
import { buildFeatureMesh } from '../geometry/index'
import {
  describeError, headerModel, openFromBlob, openFromUrl, runQuery,
} from '../reader/index'
import {
  activeQueryAtom, headerAtom, readerAtom, renderedAtom, selectedAtom,
  statusAtom, totalAtom,
} from '../store/index'

export function useFcbData() {
  const [reader, setReader] = useAtom(readerAtom)
  const [header, setHeader] = useAtom(headerAtom)
  const [rendered, setRendered] = useAtom(renderedAtom)
  const [total, setTotal] = useAtom(totalAtom)
  const [status, setStatus] = useAtom(statusAtom)
  const [active, setActive] = useAtom(activeQueryAtom)
  const [, setSelected] = useAtom(selectedAtom)

  const onOpened = useCallback(async (r: Awaited<ReturnType<typeof openFromUrl>>) => {
    setReader(r)
    setHeader(headerModel(r.header))
    setRendered([]); setTotal(undefined); setSelected(undefined)
    setActive(undefined)
    setStatus('file opened')
  }, [setReader, setHeader, setRendered, setTotal, setSelected, setActive, setStatus])

  const openUrl = useCallback(async (url: string) => {
    setStatus(`opening ${url} ...`)
    try { await onOpened(await openFromUrl(url)) }
    catch (e) { setStatus(`failed to open URL: ${describeError(e)}`) }
  }, [onOpened, setStatus])

  const openFile = useCallback(async (file: File) => {
    setStatus(`opening ${file.name} ...`)
    try { await onOpened(await openFromBlob(file)) }
    catch (e) { setStatus(`failed to open file: ${describeError(e)}`) }
  }, [onOpened, setStatus])

  const render = useCallback((features: Feature[]) => {
    if (reader === undefined || header === undefined) return
    if (!header.crs.supported || header.crs.code === null) {
      setStatus('CRS not supported — cannot georeference; not rendering')
      return
    }
    const code = header.crs.code
    const transform = toCityJSONMetadata(reader.header).transform
    const out = []
    let skipped = 0
    for (const f of features) {
      const cj = f.toCityJSON(reader.header)
      const fm = buildFeatureMesh(cj, transform, (xy) => forward(code, xy))
      if (fm === null) { skipped++; continue }
      const primary = Object.values(cj.CityObjects)[0]
      out.push({
        id: f.id, centroidLngLat: fm.centroidLngLat, mesh: fm.mesh,
        attributes: primary?.attributes ?? {},
      })
    }
    setRendered(out)
    setStatus(`${out.length} rendered${skipped ? `, ${skipped} skipped` : ''}`)
  }, [reader, header, setRendered, setStatus])

  const query = useCallback(async (
    spec: { bboxSource?: [number, number, number, number]
            where?: import('@cityjson/flatcitybuf').AttrCondition[]; limit: number },
  ) => {
    if (reader === undefined) return
    const q = { ...spec, offset: 0 }
    setActive(q); setSelected(undefined)
    setStatus('querying...')
    try {
      const { features, total: t } = await runQuery(reader, q)
      setTotal(t); render(features)
    } catch (e) { setStatus(`query failed: ${describeError(e)}`) }
  }, [reader, render, setActive, setSelected, setStatus, setTotal])

  const loadNext = useCallback(async () => {
    if (reader === undefined || active === undefined) return
    const q = { ...active, offset: active.offset + active.limit }
    setActive(q)
    setStatus('loading next batch...')
    try {
      const { features } = await runQuery(reader, q)
      render(features) // replaces the rendered set with the next page
    } catch (e) { setStatus(`load failed: ${describeError(e)}`) }
  }, [reader, active, render, setActive, setStatus])

  return { openUrl, openFile, query, loadNext, status, header, rendered, total,
           hasMore: total !== undefined && active !== undefined
             && active.offset + active.limit < total }
}
