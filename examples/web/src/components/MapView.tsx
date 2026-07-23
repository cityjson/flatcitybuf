// src/components/MapView.tsx
import { COORDINATE_SYSTEM } from '@deck.gl/core'
import { PolygonLayer } from '@deck.gl/layers'
import { SimpleMeshLayer } from '@deck.gl/mesh-layers'
import { DeckGL } from '@deck.gl/react'
import { useAtom, useAtomValue } from 'jotai'
import { useEffect, useMemo, useState } from 'react'
import { Map } from 'react-map-gl/maplibre'
import { useCameraFollow } from '../hooks/useCameraFollow'
import { useDrawBbox } from '../hooks/useDrawBbox'
import {
  colorByAtom, fetchBboxAtom, loadingAtom, renderedAtom, selectedAtom, viewStateAtom,
} from '../store/index'
import type { RenderedFeature } from '../store/index'

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

/** Colours a feature by a numeric attribute, if `colorBy` is set and numeric;
 *  otherwise a steel blue. */
function featureColor(f: RenderedFeature, colorBy: string | undefined): [number, number, number] {
  if (colorBy !== undefined) {
    const v = f.attributes[colorBy]
    if (typeof v === 'number') {
      const t = Math.max(0, Math.min(1, (v % 100) / 100))
      return [Math.round(50 + 200 * t), Math.round(120 * (1 - t) + 60), 180]
    }
  }
  return [70, 130, 180]
}

export function MapView() {
  const rendered = useAtomValue(renderedAtom)
  const colorBy = useAtomValue(colorByAtom)
  const [selected, setSelected] = useAtom(selectedAtom)
  const [viewState, setViewState] = useAtom(viewStateAtom)
  const showLoading = useDelayed(useAtomValue(loadingAtom), 120)
  const fetchBbox = useAtomValue(fetchBboxAtom)
  const { draw, onMapClick, onMapHover, bbox } = useDrawBbox()
  // In follow-camera mode, re-query the viewport (throttled) as the map moves.
  useCameraFollow()

  // Mesh layers are memoised on their own inputs so rubber-banding a bbox
  // (which changes `bbox` on every mouse-move) does not rebuild all N feature
  // layers — only the cheap outline layer below is recreated.
  const meshLayers = useMemo(
    () => rendered.map((f) => new SimpleMeshLayer<RenderedFeature>({
      id: `feat-${f.id}`,
      data: [f],
      mesh: {
        attributes: {
          positions: { value: f.mesh.positions, size: 3 },
          normals: { value: f.mesh.normals, size: 3 },
        },
        indices: { value: f.mesh.indices, size: 1 },
      },
      coordinateSystem: COORDINATE_SYSTEM.METER_OFFSETS,
      coordinateOrigin: [f.centroidLngLat[0], f.centroidLngLat[1], 0],
      getPosition: () => [0, 0, 0],
      getColor: () => {
        const c = featureColor(f, colorBy)
        return f.id === selected?.id ? [255, 160, 0] : c
      },
      pickable: true,
      updateTriggers: { getColor: [colorBy, selected?.id] },
    })),
    [rendered, colorBy, selected],
  )

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
    return extra.length === 0 ? meshLayers : [...meshLayers, ...extra]
  }, [meshLayers, bbox, fetchBbox])

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
    </DeckGL>
  )
}
