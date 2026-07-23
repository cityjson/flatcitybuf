// src/components/MapView.tsx
import { COORDINATE_SYSTEM } from '@deck.gl/core'
import { SimpleMeshLayer } from '@deck.gl/mesh-layers'
import { DeckGL } from '@deck.gl/react'
import { useAtom, useAtomValue } from 'jotai'
import { useMemo } from 'react'
import { Map } from 'react-map-gl/maplibre'
import { useDrawBbox } from '../hooks/useDrawBbox'
import { colorByAtom, renderedAtom, selectedAtom } from '../store/index'
import type { RenderedFeature } from '../store/index'

const BASEMAP = 'https://basemaps.cartocdn.com/gl/positron-gl-style/style.json'
const INITIAL_VIEW = { longitude: 4.36, latitude: 52.0, zoom: 13, pitch: 45, bearing: 0 }

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
  const { draw, onMapClick, onMapHover, bbox } = useDrawBbox()

  const layers = useMemo(() => {
    const meshLayers = rendered.map((f) => new SimpleMeshLayer<RenderedFeature>({
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
    }))
    return meshLayers
  }, [rendered, colorBy, selected])

  return (
    <DeckGL
      initialViewState={INITIAL_VIEW}
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
