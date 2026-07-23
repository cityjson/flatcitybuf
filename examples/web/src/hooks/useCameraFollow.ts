// src/hooks/useCameraFollow.ts
import { WebMercatorViewport } from '@deck.gl/core'
import { useAtomValue } from 'jotai'
import { useEffect } from 'react'
import { readerAtom, spatialModeAtom, viewStateAtom, whereAtom } from '../store/index'
import { useFcbData } from './useFcbData'

// How long the camera must settle before a follow query fires. `viewState`
// changes on every camera frame, so the effect below reruns and resets this
// timer each frame — the query runs only once the camera has been still.
const DEBOUNCE_MS = 400

/** When the spatial mode is `follow`, re-queries the current viewport (plus any
 *  attribute filter) as the camera moves, debounced. Mount once (in MapView).
 *  Does nothing in the other modes. */
export function useCameraFollow(): void {
  const mode = useAtomValue(spatialModeAtom)
  const viewState = useAtomValue(viewStateAtom)
  const where = useAtomValue(whereAtom)
  const reader = useAtomValue(readerAtom)
  const { queryViewport } = useFcbData()

  useEffect(() => {
    if (mode !== 'follow' || reader === undefined) return
    const canvas = document.getElementById('deckgl-overlay')
    const width = canvas?.clientWidth ?? 0
    const height = canvas?.clientHeight ?? 0
    if (width === 0 || height === 0) return
    const timer = setTimeout(() => {
      try {
        // Compute bounds from a pitch-0 projection of the same centre/zoom. A
        // tilted view's getBounds() extends to the horizon, which would query
        // the whole dataset regardless of zoom (massive over-fetch); the flat
        // bounds are a tight rectangle around what the user is looking at.
        const vp = new WebMercatorViewport({
          ...viewState, pitch: 0, width, height,
        })
        const b = vp.getBounds() // [west, south, east, north]
        void queryViewport([b[0], b[1], b[2], b[3]], where)
      } catch {
        // viewport not constructible (degenerate size/zoom) — skip this tick
      }
    }, DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [mode, viewState, where, reader, queryViewport])
}
