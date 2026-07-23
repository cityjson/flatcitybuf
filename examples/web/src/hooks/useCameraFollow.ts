// src/hooks/useCameraFollow.ts
import { WebMercatorViewport } from '@deck.gl/core'
import type { AttrCondition } from '@cityjson/flatcitybuf'
import { useAtomValue } from 'jotai'
import { useEffect, useRef } from 'react'
import type { HeaderModel } from '../reader/index'
import {
  headerAtom, renderedAtom, spatialModeAtom, totalAtom, viewStateAtom, whereAtom,
} from '../store/index'
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
// The visible area is computed from the real (tilted) camera, so it is already
// generous; a small pad keeps the fetched set close to what is on screen while
// still absorbing minor pans. At 0.15 the query covers the whole visible area
// with the same overshoot the old (mis-positioned) pitch-0 box had.
const PAD = 0.15

// A tilted camera's far corners run toward the horizon. Clamp each corner to
// this multiple of the flat span away from the view centre so one steep glance
// cannot turn into a whole-dataset query.
const MAX_DIST = 1.5

// When the last result was TRUNCATED by the limit we hold only some of the
// features in the fetched area, so being inside it proves nothing — the
// buildings now on screen may never have been fetched. In that case refetch
// once the view has moved this fraction of its span, or changed scale by this
// factor. Both are relative to the current span, so they track zoom.
const MOVE_FRAC = 0.25
const SCALE_FACTOR = 1.35

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

interface LastQuery { padded: Bounds; center: [number, number]; span: number }

/** The ground area the camera can actually see. A pitch-0 rectangle is NOT it:
 *  at pitch 45 the visible trapezoid covers ~1.6x the north-south extent and is
 *  shifted away from the viewer, so querying the flat rectangle leaves the far
 *  part of the screen unfetched. Unprojects the four screen corners at the real
 *  pitch instead, clamping each toward the centre so a near-horizon corner
 *  cannot blow the bbox up. Falls back to the flat bounds if a corner does not
 *  land on the ground. */
function visibleBounds(
  vs: { longitude: number; latitude: number; zoom: number; pitch: number; bearing: number },
  width: number, height: number,
): Bounds {
  const flat = new WebMercatorViewport({ ...vs, pitch: 0, width, height })
  const fb = flat.getBounds() as Bounds
  const flatSpan = Math.max(fb[2] - fb[0], fb[3] - fb[1])
  const maxD = flatSpan * MAX_DIST
  const tilt = new WebMercatorViewport({ ...vs, width, height })
  const corners: [number, number][] = [[0, 0], [width, 0], [width, height], [0, height]]
  const pts: [number, number][] = []
  for (const c of corners) {
    const g = tilt.unproject(c) as [number, number]
    if (!Number.isFinite(g[0]) || !Number.isFinite(g[1])) return fb
    const dx = g[0] - vs.longitude
    const dy = g[1] - vs.latitude
    const d = Math.hypot(dx, dy)
    pts.push(d > maxD && d > 0
      ? [vs.longitude + (dx * maxD) / d, vs.latitude + (dy * maxD) / d]
      : g)
  }
  const lngs = pts.map((p) => p[0])
  const lats = pts.map((p) => p[1])
  return [Math.min(...lngs), Math.min(...lats), Math.max(...lngs), Math.max(...lats)]
}

/** When the spatial mode is `follow`, re-queries the current viewport (plus any
 *  attribute filter) as the camera moves — debounced, and only when the view
 *  has changed enough to need new data. Mount once (in MapView). */
export function useCameraFollow(): void {
  const mode = useAtomValue(spatialModeAtom)
  const viewState = useAtomValue(viewStateAtom)
  const where = useAtomValue(whereAtom)
  const header = useAtomValue(headerAtom)
  const total = useAtomValue(totalAtom)
  const rendered = useAtomValue(renderedAtom)
  const { queryViewport } = useFcbData()

  // The limit cut the last result short, so we hold only part of what is in the
  // fetched area — containment can no longer be trusted as coverage.
  const truncated = total !== undefined && total > rendered.length

  // The last issued query, and the inputs it was for. A new file (new header)
  // or attribute filter invalidates it (force a refetch).
  const last = useRef<LastQuery | null>(null)
  const lastWhere = useRef<AttrCondition[] | undefined>(undefined)
  const lastHeader = useRef<HeaderModel | undefined>(undefined)

  useEffect(() => {
    if (mode !== 'follow' || header === undefined) return
    if (header !== lastHeader.current || where !== lastWhere.current) {
      last.current = null
      lastHeader.current = header
      lastWhere.current = where
    }
    const canvas = document.getElementById('deckgl-overlay')
    const width = canvas?.clientWidth ?? 0
    const height = canvas?.clientHeight ?? 0
    if (width === 0 || height === 0) return
    const timer = setTimeout(() => {
      try {
        const b = visibleBounds(viewState, width, height)
        const center: [number, number] = [(b[0] + b[2]) / 2, (b[1] + b[3]) / 2]
        const span = Math.max(b[2] - b[0], b[3] - b[1])
        const lq = last.current
        if (lq !== null) {
          // Left the fetched area? Always refetch.
          let stale = !contains(lq.padded, b)
          if (!stale && truncated) {
            // Inside the fetched area, but we only hold part of what is there,
            // so zooming in or panning can reveal features we never fetched.
            // Refetch once the view has changed enough to be worth it.
            const moved = Math.hypot(center[0] - lq.center[0], center[1] - lq.center[1])
            const ratio = span / lq.span
            stale = moved > lq.span * MOVE_FRAC
              || ratio < 1 / SCALE_FACTOR || ratio > SCALE_FACTOR
          }
          if (!stale) return
        }
        const padded = pad(b)
        last.current = { padded, center, span }
        void queryViewport(padded, where)
      } catch {
        // viewport not constructible (degenerate size/zoom) — skip this tick
      }
    }, DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [mode, viewState, where, header, truncated, queryViewport])
}
