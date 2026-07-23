// src/store/index.ts
import type { AttrCondition, FcbReader } from '@cityjson/flatcitybuf'
import { atom } from 'jotai'
import type { Mesh } from '../geometry/index'
import type { HeaderModel } from '../reader/index'

export interface RenderedFeature {
  id: string
  centroidLngLat: [number, number]
  mesh: Mesh
  attributes: Record<string, unknown>
}

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
