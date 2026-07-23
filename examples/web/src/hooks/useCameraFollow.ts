// src/hooks/useCameraFollow.ts
import { WebMercatorViewport } from '@deck.gl/core'
import type { AttrCondition } from '@cityjson/flatcitybuf'
import { useAtomValue } from 'jotai'
import { useEffect, useRef } from 'react'
import type { HeaderModel } from '../reader/index'
import { headerAtom, spatialModeAtom, viewStateAtom, whereAtom } from '../store/index'
import { useFcbData } from './useFcbData'

// How long the camera must settle before a follow query fires. `viewState`
// changes on every camera frame, so the effect below reruns and resets this
// timer each frame — the query runs only once the camera has been still.
const DEBOUNCE_MS = 350

// Each query fetches a bbox padded this fraction of the span BEYOND the visible
// viewport on every side. A new query is only issued once the viewport leaves
// that padded area, so small pans/zooms within it don't refetch. Because the
// pad is a fraction of the current span, the move threshold scales with zoom
// (a small absolute move matters when zoomed in, not when zoomed out).
const PAD = 0.5

type Bounds = [number, number, number, number] // [west, south, east, north]

function contains(outer: Bounds, inner: Bounds): boolean {
  return inner[0] >= outer[0] && inner[1] >= outer[1]
    && inner[2] <= outer[2] && inner[3] <= outer[3]
}

function pad(b: Bounds): Bounds {
  const dx = (b[2] - b[0]) * PAD
  const dy = (b[3] - b[1]) * PAD
  return [b[0] - dx, b[1] - dy, b[2] + dx, b[3] + dy]
}

/** When the spatial mode is `follow`, re-queries the current viewport (plus any
 *  attribute filter) as the camera moves — debounced, and only when the view
 *  leaves the last padded query area. Mount once (in MapView). */
export function useCameraFollow(): void {
  const mode = useAtomValue(spatialModeAtom)
  const viewState = useAtomValue(viewStateAtom)
  const where = useAtomValue(whereAtom)
  const header = useAtomValue(headerAtom)
  const { queryViewport } = useFcbData()

  // The padded bbox of the last issued query, and the inputs it was for. A new
  // file (new header) or attribute filter invalidates it (force a refetch).
  const lastPadded = useRef<Bounds | null>(null)
  const lastWhere = useRef<AttrCondition[] | undefined>(undefined)
  const lastHeader = useRef<HeaderModel | undefined>(undefined)

  useEffect(() => {
    if (mode !== 'follow' || header === undefined) return
    if (header !== lastHeader.current || where !== lastWhere.current) {
      lastPadded.current = null
      lastHeader.current = header
      lastWhere.current = where
    }
    const canvas = document.getElementById('deckgl-overlay')
    const width = canvas?.clientWidth ?? 0
    const height = canvas?.clientHeight ?? 0
    if (width === 0 || height === 0) return
    const timer = setTimeout(() => {
      try {
        // Bounds from a pitch-0 projection of the same centre/zoom: a tilted
        // view's getBounds() extends to the horizon, which would query the
        // whole dataset regardless of zoom.
        const vp = new WebMercatorViewport({ ...viewState, pitch: 0, width, height })
        const b = vp.getBounds() as Bounds
        // Still inside the last fetched area? The data on screen already covers
        // it — don't refetch.
        if (lastPadded.current !== null && contains(lastPadded.current, b)) return
        const padded = pad(b)
        lastPadded.current = padded
        void queryViewport(padded, where)
      } catch {
        // viewport not constructible (degenerate size/zoom) — skip this tick
      }
    }, DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [mode, viewState, where, header, queryViewport])
}
