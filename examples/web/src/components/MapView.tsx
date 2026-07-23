// src/components/MapView.tsx
import { COORDINATE_SYSTEM } from '@deck.gl/core'
import { PolygonLayer } from '@deck.gl/layers'
import { SimpleMeshLayer } from '@deck.gl/mesh-layers'
import { DeckGL } from '@deck.gl/react'
import { useAtom, useAtomValue } from 'jotai'
import { useMemo } from 'react'
import { Map } from 'react-map-gl/maplibre'
import { useCameraFollow } from '../hooks/useCameraFollow'
import { useDrawBbox } from '../hooks/useDrawBbox'
import { colorByAtom, renderedAtom, selectedAtom, viewStateAtom } from '../store/index'
import type { RenderedFeature } from '../store/index'

const BASEMAP = 'https://basemaps.cartocdn.com/gl/positron-gl-style/style.json'

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

  // The drawn bounding box as a visible rectangle (orange outline, faint fill),
  // shown while drawing and after. Without it the only feedback is a text
  // readout, so the box is effectively invisible on the map.
  const layers = useMemo(() => {
    if (bbox === undefined) return meshLayers
    const [minLng, minLat, maxLng, maxLat] = bbox
    const ring: [number, number][] = [
      [minLng, minLat], [maxLng, minLat], [maxLng, maxLat], [minLng, maxLat],
    ]
    const bboxLayer = new PolygonLayer<[number, number][]>({
      id: 'draw-bbox',
      data: [ring],
      getPolygon: (d) => d,
      stroked: true,
      filled: true,
      getFillColor: [255, 140, 0, 35],
      getLineColor: [255, 120, 0, 220],
      getLineWidth: 2,
      lineWidthUnits: 'pixels',
      pickable: false,
    })
    return [...meshLayers, bboxLayer]
  }, [meshLayers, bbox])

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
      {bbox && (
        <div className="pointer-events-none absolute top-2 left-2 rounded bg-black/60 px-2 py-1 text-xs text-white">
          bbox: {bbox.map((n) => n.toFixed(4)).join(', ')}
        </div>
      )}
    </DeckGL>
  )
}
