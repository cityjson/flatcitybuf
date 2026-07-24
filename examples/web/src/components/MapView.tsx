// src/components/MapView.tsx
import { COORDINATE_SYSTEM, WebMercatorViewport } from '@deck.gl/core'
import { PolygonLayer } from '@deck.gl/layers'
import { SimpleMeshLayer } from '@deck.gl/mesh-layers'
import { DeckGL } from '@deck.gl/react'
import { useAtom, useAtomValue } from 'jotai'
import { useEffect, useMemo, useState } from 'react'
import { Map } from 'react-map-gl/maplibre'
import { useCameraFollow } from '../hooks/useCameraFollow'
import { useDrawBbox } from '../hooks/useDrawBbox'
import { BuildingLayer } from '../render/BuildingLayer'
import { mergeFeatures } from '../render/mergeFeatures'
import {
  colorByAtom, fetchBboxAtom, followTooFarAtom, loadingAtom, renderedAtom,
  selectedAtom, spatialModeAtom, viewStateAtom,
} from '../store/index'
import type { RenderedFeature } from '../store/index'
import { FeatureInspector } from './FeatureInspector'

/** Tracks the deck canvas size so the feature popup can be projected. */
function useCanvasSize(): { width: number; height: number } {
  const [size, setSize] = useState({ width: 0, height: 0 })
  useEffect(() => {
    const el = document.getElementById('deckgl-overlay')
    if (el === null) return
    const measure = () => setSize({ width: el.clientWidth, height: el.clientHeight })
    measure()
    const ro = new ResizeObserver(measure)
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  return size
}

const BASEMAP = 'https://basemaps.cartocdn.com/gl/positron-gl-style/style.json'

/** True only once `flag` has been on for `ms`. Warm queries finish in ~10 ms,
 *  so showing the indicator immediately would just flicker. */
function useDelayed(flag: boolean, ms: number): boolean {
  const [shown, setShown] = useState(false)
  useEffect(() => {
    if (!flag) { setShown(false); return }
    const t = setTimeout(() => setShown(true), ms)
    return () => clearTimeout(t)
  }, [flag, ms])
  return shown
}


export function MapView() {
  const rendered = useAtomValue(renderedAtom)
  const colorBy = useAtomValue(colorByAtom)
  const [selected, setSelected] = useAtom(selectedAtom)
  const [viewState, setViewState] = useAtom(viewStateAtom)
  const showLoading = useDelayed(useAtomValue(loadingAtom), 120)
  const fetchBbox = useAtomValue(fetchBboxAtom)
  const mode = useAtomValue(spatialModeAtom)
  const tooFar = useAtomValue(followTooFarAtom)
  const size = useCanvasSize()
  const { draw, onMapClick, onMapHover, bbox } = useDrawBbox()

  // Screen position of the selected feature's centroid, for the popup. Computed
  // at the real pitch so the popup tracks the building as the camera moves.
  const popupPos = useMemo(() => {
    if (selected === undefined || size.width === 0) return null
    const [x, y] = new WebMercatorViewport({ ...viewState, ...size })
      .project(selected.centroidLngLat)
    return { x, y }
  }, [selected, viewState, size])
  // In follow-camera mode, re-query the viewport (throttled) as the map moves.
  useCameraFollow()

  // All buildings in ONE layer: a single merged mesh with per-vertex colour and
  // a per-vertex feature id for picking. Rebuilt only when the result set or the
  // colour-by column changes (selection is a separate highlight layer, so a
  // click doesn't rebuild the whole mesh).
  const buildingLayer = useMemo(() => {
    if (rendered.length === 0) return null
    return new BuildingLayer({
      id: 'buildings',
      mesh: mergeFeatures(rendered, colorBy),
      features: rendered,
      pickable: true,
    })
  }, [rendered, colorBy])

  // The selected building, redrawn on top in orange. One layer for one feature,
  // so highlighting is cheap and never rebuilds the merged mesh.
  const highlightLayer = useMemo(() => {
    if (selected === undefined) return null
    return new SimpleMeshLayer<RenderedFeature>({
      id: 'selected',
      data: [selected],
      mesh: {
        attributes: {
          positions: { value: selected.mesh.positions, size: 3 },
          normals: { value: selected.mesh.normals, size: 3 },
        },
        indices: { value: selected.mesh.indices, size: 1 },
      },
      coordinateSystem: COORDINATE_SYSTEM.METER_OFFSETS,
      coordinateOrigin: [selected.centroidLngLat[0], selected.centroidLngLat[1], 0],
      getPosition: () => [0, 0, 0],
      getColor: [255, 160, 0],
      pickable: false,
    })
  }, [selected])

  // Rectangle outlines drawn over the meshes:
  //  - `fetch-bbox` (blue) is the area the last follow query actually asked
  //    for — inset inside the visible area — so the fetch region is visible
  //    rather than something you have to infer.
  //  - `draw-bbox` (orange) is the rectangle the user drew.
  const layers = useMemo(() => {
    const ringOf = (b: [number, number, number, number]): [number, number][] => [
      [b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]],
    ]
    const extra: PolygonLayer<[number, number][]>[] = []
    if (fetchBbox !== undefined) {
      extra.push(new PolygonLayer<[number, number][]>({
        id: 'fetch-bbox',
        data: [ringOf(fetchBbox)],
        getPolygon: (d) => d,
        stroked: true,
        filled: false,
        getLineColor: [30, 120, 255, 200],
        getLineWidth: 1.5,
        lineWidthUnits: 'pixels',
        pickable: false,
      }))
    }
    if (bbox !== undefined) {
      extra.push(new PolygonLayer<[number, number][]>({
        id: 'draw-bbox',
        data: [ringOf(bbox)],
        getPolygon: (d) => d,
        stroked: true,
        filled: true,
        getFillColor: [255, 140, 0, 35],
        getLineColor: [255, 120, 0, 220],
        getLineWidth: 2,
        lineWidthUnits: 'pixels',
        pickable: false,
      }))
    }
    return [buildingLayer, highlightLayer, ...extra].filter((l) => l !== null)
  }, [buildingLayer, highlightLayer, bbox, fetchBbox])

  return (
    <DeckGL
      viewState={viewState}
      onViewStateChange={(params) => {
        // deck types this as a union (MapViewState | TransitionProps); the
        // interaction always yields a MapViewState with these fields.
        const vs = params.viewState as {
          longitude: number; latitude: number; zoom: number
          pitch?: number; bearing?: number
        }
        setViewState({
          longitude: vs.longitude,
          latitude: vs.latitude,
          zoom: vs.zoom,
          pitch: vs.pitch ?? 0,
          bearing: vs.bearing ?? 0,
        })
      }}
      controller
      layers={layers}
      getCursor={() => (draw.active ? 'crosshair' : 'grab')}
      onClick={(info) => {
        if (draw.active && info.coordinate) {
          onMapClick([info.coordinate[0], info.coordinate[1]])
        } else if (info.layer && (info.object as RenderedFeature)) {
          setSelected(info.object as RenderedFeature)
        }
      }}
      onHover={(info) => {
        if (draw.active && info.coordinate) {
          onMapHover([info.coordinate[0], info.coordinate[1]])
        }
      }}
    >
      <Map mapStyle={BASEMAP} />
      {/* Passive: `pointer-events-none` means it can never swallow a drag or
          click, so panning stays responsive while data is still arriving. */}
      {showLoading && (
        <div
          role="status"
          aria-live="polite"
          className="pointer-events-none absolute top-2 right-2 flex items-center gap-2 rounded-full bg-black/70 px-3 py-1 text-xs text-white shadow"
        >
          <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-white/30 border-t-white" />
          loading features…
        </div>
      )}
      {bbox && (
        <div className="pointer-events-none absolute top-2 left-2 rounded bg-black/60 px-2 py-1 text-xs text-white">
          bbox: {bbox.map((n) => n.toFixed(4)).join(', ')}
        </div>
      )}
      {fetchBbox && (
        <div className="pointer-events-none absolute bottom-2 left-2 flex items-center gap-2 rounded bg-black/60 px-2 py-1 text-xs text-white">
          <span
            className="inline-block h-0 w-4 border-t-2"
            style={{ borderColor: 'rgb(30,120,255)' }}
          />
          fetched area (inset in view)
        </div>
      )}

      {/* Follow mode, too far out to fetch — prompt the user to zoom in. */}
      {mode === 'follow' && tooFar && (
        <div className="pointer-events-none absolute inset-x-0 top-3 flex justify-center">
          <div className="rounded-full bg-black/70 px-4 py-1.5 text-xs text-white shadow">
            Get closer to the ground to fetch features
          </div>
        </div>
      )}

      {/* Attribute inspector as a map popup, anchored above the selected
          building. A deck.gl overlay child so it renders above the deck canvas
          (a react-map-gl Popup would be hidden beneath it). */}
      {selected && popupPos && (
        <div
          className="absolute z-10 w-64 -translate-x-1/2 -translate-y-full"
          style={{ left: popupPos.x, top: popupPos.y - 12 }}
        >
          <div className="max-h-80 overflow-auto rounded-lg bg-white p-3 text-gray-900 shadow-xl ring-1 ring-black/10">
            <button
              className="absolute right-1 top-1 h-6 w-6 rounded text-gray-500 hover:bg-gray-100"
              aria-label="close"
              onClick={() => setSelected(undefined)}
            >
              ✕
            </button>
            <FeatureInspector />
          </div>
          {/* little pointer toward the building */}
          <div className="mx-auto h-0 w-0 border-x-8 border-t-8 border-x-transparent border-t-white" />
        </div>
      )}
    </DeckGL>
  )
}
