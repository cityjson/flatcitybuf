// src/hooks/useDrawBbox.ts
import { useAtom } from 'jotai'
import { useCallback } from 'react'
import { drawAtom } from '../store/index'

/** Two-click rectangle draw. `onMapClick` receives a lng/lat coordinate from
 *  deck.gl's pick info; first click sets corner A, second sets B and finishes.
 *  `onMapHover` rubber-bands B while placing. */
export function useDrawBbox() {
  const [draw, setDraw] = useAtom(drawAtom)

  const start = useCallback(() => setDraw({ active: true }), [setDraw])
  const clear = useCallback(() => setDraw({ active: false }), [setDraw])

  const onMapClick = useCallback((coord: [number, number]) => {
    setDraw((d) => {
      if (!d.active) return d
      if (d.a === undefined) return { ...d, a: coord, b: coord }
      return { active: false, a: d.a, b: coord }
    })
  }, [setDraw])

  const onMapHover = useCallback((coord: [number, number]) => {
    setDraw((d) => (d.active && d.a !== undefined ? { ...d, b: coord } : d))
  }, [setDraw])

  const bbox = draw.a && draw.b
    ? [Math.min(draw.a[0], draw.b[0]), Math.min(draw.a[1], draw.b[1]),
       Math.max(draw.a[0], draw.b[0]), Math.max(draw.a[1], draw.b[1])] as
       [number, number, number, number]
    : undefined

  return { draw, start, clear, onMapClick, onMapHover, bbox }
}
