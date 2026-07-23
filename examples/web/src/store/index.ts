// src/store/index.ts
import type { AttrCondition, FcbReader } from '@cityjson/flatcitybuf'
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
export const limitAtom = atom<number>(200)

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

export const readerAtom = atom<FcbReader | undefined>(undefined)
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
