// src/store/index.ts
import type { AttrCondition } from '@cityjson/flatcitybuf'
import { atom } from 'jotai'
import type { Mesh } from '../geometry/index'
import type { HeaderModel } from '../reader/index'

/** General facts about a rendered feature, for the inspector. */
export interface FeatureInfo {
  objectType?: string
  geometryType?: string
  lod?: string
  vertexCount: number
  triangleCount: number
}

export interface RenderedFeature {
  id: string
  centroidLngLat: [number, number]
  mesh: Mesh
  attributes: Record<string, unknown>
  info: FeatureInfo
}

/** How the spatial extent of a query is chosen. `all` = the whole dataset
 *  (first `limit`, paged); `bbox` = a rectangle drawn on the map; `follow` =
 *  the current camera viewport, re-queried as the camera moves. */
export type SpatialMode = 'all' | 'bbox' | 'follow'
export const spatialModeAtom = atom<SpatialMode>('follow')

/** Max features rendered per query (and "Load next batch" page size). */
export const limitAtom = atom<number>(1000)

/** The LoD to render, exclusive. `undefined` means "highest available" (the
 *  default until an LoD is picked). Set from the LoD selector; changing it
 *  re-runs the current query, since the mesh is triangulated per LoD in the
 *  worker. */
export const lodAtom = atom<string | undefined>(undefined)

/** The distinct LoD labels discovered so far (unioned across query results),
 *  sorted ascending. Empty until the first result arrives; drives whether the
 *  LoD selector is shown and what options it lists. */
export const availableLodsAtom = atom<string[]>([])

/** True when follow-camera is on but the view is zoomed too far out to fetch
 *  (the area would be too large). Drives a "zoom in" hint instead of a query. */
export const followTooFarAtom = atom<boolean>(false)

/** Below this zoom, follow-camera treats the visible area as too large to fetch
 *  (a whole city/region) and shows a "zoom in" hint instead of querying. Shared
 *  with the open-time framing so a fresh file never lands below it — otherwise a
 *  country-scale dataset would frame to zoom ~11 and show the hint with nothing
 *  on screen. Higher = closer to the ground; set to a street/neighbourhood zoom
 *  so the visible area stays close to what the per-query feature limit covers. */
export const MIN_FETCH_ZOOM = 15

/** The active attribute filter, applied on top of every spatial mode. `[]`/
 *  undefined means no attribute filter. Held in an atom (not QueryPanel local
 *  state) so follow-camera re-queries can include it. */
export const whereAtom = atom<AttrCondition[] | undefined>(undefined)

/** The deck.gl camera. Controlled (not `initialViewState`) so the app can fly
 *  the camera to the loaded data — otherwise a dataset that isn't already under
 *  the hard-coded start view renders entirely off-screen and looks empty. */
export interface ViewState {
  longitude: number
  latitude: number
  zoom: number
  pitch: number
  bearing: number
}
export const INITIAL_VIEW: ViewState = {
  longitude: 4.36, latitude: 52.0, zoom: 13, pitch: 45, bearing: 0,
}
export const viewStateAtom = atom<ViewState>(INITIAL_VIEW)

/** True once a file is open in the worker (the reader itself lives there). */
export const readyAtom = atom<boolean>(false)

/** A read (open or query) is in flight. Drives a passive loading indicator —
 *  it never blocks input, so the user can keep panning while data arrives. */
export const loadingAtom = atom<boolean>(false)

/** The lng/lat bbox actually sent to the last follow-camera query (inset inside
 *  the visible area). Drawn on the map so the fetched region is visible rather
 *  than something you have to infer. Undefined when the current results did not
 *  come from a camera-derived bbox. */
export const fetchBboxAtom = atom<[number, number, number, number] | undefined>(undefined)
export const headerAtom = atom<HeaderModel | undefined>(undefined)
export const renderedAtom = atom<RenderedFeature[]>([])
export const totalAtom = atom<number | undefined>(undefined)
export const statusAtom = atom<string>('open a .fcb file to begin')
export const selectedAtom = atom<RenderedFeature | undefined>(undefined)
export const colorByAtom = atom<string | undefined>(undefined)

/** The active query, remembered so "Load Next Batch" can advance the offset. */
export interface ActiveQuery {
  bboxSource?: [number, number, number, number]
  where?: AttrCondition[]
  limit: number
  offset: number
}
export const activeQueryAtom = atom<ActiveQuery | undefined>(undefined)

/** Draw state: the two lng/lat corners the user is placing. */
export interface DrawState {
  active: boolean
  a?: [number, number]
  b?: [number, number]
}
export const drawAtom = atom<DrawState>({ active: false })
