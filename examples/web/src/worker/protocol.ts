// src/worker/protocol.ts
// Messages exchanged between the main thread and the FCB worker. The reader,
// the query, and the (CPU-heavy) triangulation all run in the worker; the main
// thread only sends requests and turns the returned meshes into deck.gl layers.
import type { AttrCondition } from '@cityjson/flatcitybuf'
import type { HeaderModel } from '../reader/index'
import type { FeatureInfo } from '../store/index'

export interface OpenRequest {
  type: 'open'
  id: number
  url?: string
  /** For a local file: the whole file bytes, transferred to the worker. */
  buffer?: ArrayBuffer
}
export interface QueryRequest {
  type: 'query'
  id: number
  bboxSource?: [number, number, number, number]
  where?: AttrCondition[]
  limit: number
  offset: number
}
export type WorkerRequest = OpenRequest | QueryRequest

/** One rendered feature, with its mesh as raw typed arrays whose backing
 *  buffers are transferred (zero-copy) from the worker. */
export interface WorkerFeature {
  id: string
  centroidLngLat: [number, number]
  positions: Float32Array
  normals: Float32Array
  indices: Uint32Array
  info: FeatureInfo
  attributes: Record<string, unknown>
}
export interface OpenedResponse { type: 'opened'; id: number; header: HeaderModel }
export interface ResultResponse {
  type: 'result'; id: number; total: number | undefined; features: WorkerFeature[]
}
export interface ErrorResponse {
  type: 'error'; id: number; message: string; aborted: boolean
}
export type WorkerResponse = OpenedResponse | ResultResponse | ErrorResponse
