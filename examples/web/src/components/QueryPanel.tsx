// src/components/QueryPanel.tsx
import type { AttrCondition, Operator } from '@cityjson/flatcitybuf'
import { useAtom } from 'jotai'
import { useEffect, useState } from 'react'
import { bboxToSource } from '../crs/index'
import { useDrawBbox } from '../hooks/useDrawBbox'
import { useFcbData } from '../hooks/useFcbData'
import { coerceAttrValue } from '../reader/index'
import {
  colorByAtom, limitAtom, type SpatialMode, spatialModeAtom, whereAtom,
} from '../store/index'
import { PrimaryButton, SectionHeading } from './ui'

const INPUT =
  'rounded border border-cj-charcoal/20 px-1 py-1 text-xs '
  + 'focus:border-cj-purple focus:outline-none focus:ring-1 focus:ring-cj-purple'
const SECONDARY_BTN =
  'rounded border border-cj-charcoal/20 px-2 py-1 text-xs text-cj-charcoal '
  + 'transition-colors hover:border-cj-purple hover:text-cj-purple'

const OPERATORS: Operator[] = ['Eq', 'Ne', 'Gt', 'Ge', 'Lt', 'Le']
const MODES: { value: SpatialMode; label: string; hint: string }[] = [
  { value: 'all', label: 'Whole dataset', hint: 'first N features; page with Load next batch' },
  { value: 'bbox', label: 'Draw bbox', hint: 'draw a rectangle on the map, then Run' },
  { value: 'follow', label: 'Follow camera', hint: 'auto-queries the visible area as you move' },
]

export function QueryPanel() {
  const { header, query, loadNext, applyLod, total, rendered, hasMore, lod,
    availableLods } = useFcbData()
  const { draw, start, clear, bbox } = useDrawBbox()
  const [colorBy, setColorBy] = useAtom(colorByAtom)
  const [mode, setMode] = useAtom(spatialModeAtom)
  const [limit, setLimit] = useAtom(limitAtom)
  const [, setWhere] = useAtom(whereAtom)
  const [field, setField] = useState('')
  const [op, setOp] = useState<Operator>('Eq')
  const [value, setValue] = useState('')
  const [err, setErr] = useState('')

  // A new file's columns differ; drop any stale attribute selection.
  useEffect(() => { setField(''); setWhere(undefined) }, [header, setWhere])

  if (header === undefined) return null
  const cols = header.columns
  const queryable = header.queryable

  const changeMode = (m: SpatialMode) => {
    if (m !== 'bbox') clear() // leaving bbox mode drops the drawn rectangle
    setMode(m)
  }

  // Builds the attribute filter from the current inputs, or throws a message.
  const buildWhere = (): AttrCondition[] | undefined => {
    if (field === '') return undefined
    if (!queryable.some((q) => q.name === field)) {
      throw new Error('field is not queryable (not an indexed, supported column)')
    }
    const col = cols.find((c) => c.name === field)
    if (col === undefined) throw new Error('unknown field')
    return [{ field, operator: op, value: coerceAttrValue(col, value) }]
  }

  const run = () => {
    setErr('')
    let where: AttrCondition[] | undefined
    try { where = buildWhere() } catch (e) {
      setErr(e instanceof Error ? e.message : String(e)); return
    }
    // Share the attribute filter so follow-camera re-queries include it.
    setWhere(where)
    if (mode === 'follow') return // live: the follow effect re-queries on this change
    let bboxSource: [number, number, number, number] | undefined
    if (mode === 'bbox') {
      if (bbox === undefined) { setErr('draw a bbox on the map first'); return }
      if (header.crs.code === null || !header.crs.supported) {
        setErr('cannot run a spatial query: CRS is unsupported'); return
      }
      bboxSource = bboxToSource(header.crs.code, bbox[0], bbox[1], bbox[2], bbox[3])
    }
    void query({ bboxSource, where })
  }

  // The LoD shown as selected: the explicit choice, else the highest known.
  const activeLod = lod ?? availableLods[availableLods.length - 1]

  return (
    <section className="space-y-3 text-sm">
      <SectionHeading>3. Query</SectionHeading>

      <fieldset className="space-y-1 rounded border border-cj-charcoal/15 p-2">
        <legend className="px-1 text-xs font-semibold text-cj-charcoal-soft">Spatial</legend>
        {MODES.map((m) => (
          <label key={m.value} className="flex items-start gap-2 text-xs">
            <input type="radio" name="spatial-mode" className="mt-0.5 accent-cj-purple"
              checked={mode === m.value} onChange={() => changeMode(m.value)} />
            <span>
              <span className="font-medium">{m.label}</span>
              <span className="block text-cj-charcoal-soft">{m.hint}</span>
            </span>
          </label>
        ))}
        {mode === 'bbox' && (
          <div className="flex items-center gap-2 pt-1">
            <button className={SECONDARY_BTN}
              onClick={() => (draw.active ? clear() : start())}>
              {draw.active ? 'cancel draw' : 'draw bbox'}
            </button>
            <span className="text-xs text-cj-charcoal-soft">{bbox ? 'bbox set' : 'no bbox'}</span>
          </div>
        )}
      </fieldset>

      {/* Exclusive LoD selector — appears once the data reveals its LoDs. */}
      {availableLods.length > 0 && (
        <fieldset className="space-y-1 rounded border border-cj-charcoal/15 p-2">
          <legend className="px-1 text-xs font-semibold text-cj-charcoal-soft">
            Level of Detail
          </legend>
          <div className="flex flex-wrap gap-1">
            {availableLods.map((l) => {
              const on = activeLod === l
              return (
                <button key={l} type="button" onClick={() => applyLod(l)}
                  aria-pressed={on}
                  className={
                    'rounded border px-2 py-1 text-xs transition-colors '
                    + (on
                      ? 'border-cj-gold bg-cj-gold-soft font-semibold text-cj-charcoal'
                      : 'border-cj-charcoal/20 text-cj-charcoal-soft hover:border-cj-gold')
                  }>
                  LoD {l}
                </button>
              )
            })}
          </div>
        </fieldset>
      )}

      <fieldset className="space-y-1 rounded border border-cj-charcoal/15 p-2">
        <legend className="px-1 text-xs font-semibold text-cj-charcoal-soft">
          Attribute (optional)
        </legend>
        <div className="grid grid-cols-3 gap-1">
          <select className={INPUT}
            value={field} onChange={(e) => setField(e.target.value)}>
            <option value="">(no attribute)</option>
            {queryable.map((c) => (
              <option key={c.name} value={c.name}>{c.name} ({c.typeName})</option>
            ))}
          </select>
          <select className={INPUT}
            value={op} onChange={(e) => setOp(e.target.value as Operator)}>
            {OPERATORS.map((o) => <option key={o} value={o}>{o}</option>)}
          </select>
          <input className={INPUT} value={value}
            onChange={(e) => setValue(e.target.value)} placeholder="value" />
        </div>
      </fieldset>

      <div className="flex items-center gap-2 text-xs">
        <label>limit</label>
        <input type="number" min={1} className={`w-20 ${INPUT}`}
          value={limit}
          onChange={(e) => setLimit(Math.max(1, Number(e.target.value) || 1))} />
        <PrimaryButton onClick={run}>Run query</PrimaryButton>
      </div>
      {mode === 'follow' && (
        <p className="text-xs text-cj-charcoal-soft">
          Following the camera — pan/zoom the map to load the visible area.
        </p>
      )}

      {hasMore && (
        <button className={SECONDARY_BTN}
          onClick={() => void loadNext()}>Load next batch</button>
      )}

      <div className="flex items-center gap-2 text-xs">
        <label>colour by</label>
        <select className={INPUT}
          value={colorBy ?? ''}
          onChange={(e) => setColorBy(e.target.value || undefined)}>
          <option value="">(uniform)</option>
          {cols.map((c) => <option key={c.name} value={c.name}>{c.name}</option>)}
        </select>
      </div>

      {err && <p className="text-xs text-red-600">{err}</p>}
      <p className="text-xs text-cj-charcoal-soft">
        showing {rendered.length}{total !== undefined ? ` of ${total}` : ''}
      </p>
    </section>
  )
}
