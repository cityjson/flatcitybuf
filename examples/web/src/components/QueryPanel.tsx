// src/components/QueryPanel.tsx
import type { AttrCondition, Operator } from '@cityjson/flatcitybuf'
import { useAtom } from 'jotai'
import { useEffect, useState } from 'react'
import { bboxToSource } from '../crs/index'
import { useDrawBbox } from '../hooks/useDrawBbox'
import { useFcbData } from '../hooks/useFcbData'
import { coerceAttrValue } from '../reader/index'
import { colorByAtom } from '../store/index'

const OPERATORS: Operator[] = ['Eq', 'Ne', 'Gt', 'Ge', 'Lt', 'Le']

export function QueryPanel() {
  const { header, query, loadNext, total, rendered, hasMore } = useFcbData()
  const { draw, start, clear, bbox } = useDrawBbox()
  const [colorBy, setColorBy] = useAtom(colorByAtom)
  const [field, setField] = useState('')
  const [op, setOp] = useState<Operator>('Eq')
  const [value, setValue] = useState('')
  const [limit, setLimit] = useState(200)
  const [err, setErr] = useState('')

  useEffect(() => {
    setField('')
  }, [header])

  if (header === undefined) return null
  const cols = header.columns
  const queryable = header.queryable

  const run = () => {
    setErr('')
    let bboxSource: [number, number, number, number] | undefined
    if (bbox && header.crs.code !== null && header.crs.supported) {
      bboxSource = bboxToSource(header.crs.code, bbox[0], bbox[1], bbox[2], bbox[3])
    }
    let where: AttrCondition[] | undefined
    if (field !== '') {
      if (!queryable.some((q) => q.name === field)) {
        setErr('field is not queryable (not an indexed, supported column)')
        return
      }
      const col = cols.find((c) => c.name === field)
      if (col === undefined) { setErr('unknown field'); return }
      try {
        where = [{ field, operator: op, value: coerceAttrValue(col, value) }]
      } catch (e) { setErr(String(e instanceof Error ? e.message : e)); return }
    }
    if (bboxSource === undefined && where === undefined) {
      setErr('draw a bbox or set an attribute condition'); return
    }
    void query({ bboxSource, where, limit })
  }

  return (
    <section className="space-y-2 text-sm">
      <h2 className="text-sm font-semibold">3. Query</h2>
      <div className="flex items-center gap-2">
        <button className="rounded border px-2 py-1"
          onClick={() => (draw.active ? clear() : start())}>
          {draw.active ? 'cancel draw' : 'draw bbox'}
        </button>
        <span className="text-xs opacity-70">
          {bbox ? 'bbox set' : 'no bbox'}
        </span>
      </div>
      <div className="grid grid-cols-3 gap-1">
        <select className="rounded border px-1 py-1 text-xs"
          value={field} onChange={(e) => setField(e.target.value)}>
          <option value="">(no attribute)</option>
          {queryable.map((c) => (
            <option key={c.name} value={c.name}>{c.name} ({c.typeName})</option>
          ))}
        </select>
        <select className="rounded border px-1 py-1 text-xs"
          value={op} onChange={(e) => setOp(e.target.value as Operator)}>
          {OPERATORS.map((o) => <option key={o} value={o}>{o}</option>)}
        </select>
        <input className="rounded border px-1 py-1 text-xs" value={value}
          onChange={(e) => setValue(e.target.value)} placeholder="value" />
      </div>
      <div className="flex items-center gap-2 text-xs">
        <label>limit</label>
        <input type="number" className="w-20 rounded border px-1 py-1"
          value={limit} onChange={(e) => setLimit(Number(e.target.value))} />
        <button className="rounded border px-2 py-1" onClick={run}>Run</button>
        {hasMore && (
          <button className="rounded border px-2 py-1"
            onClick={() => void loadNext()}>Load next batch</button>
        )}
      </div>
      <div className="flex items-center gap-2 text-xs">
        <label>colour by</label>
        <select className="rounded border px-1 py-1"
          value={colorBy ?? ''} onChange={(e) => setColorBy(e.target.value || undefined)}>
          <option value="">(uniform)</option>
          {cols.map((c) => <option key={c.name} value={c.name}>{c.name}</option>)}
        </select>
      </div>
      {err && <p className="text-xs text-red-600">{err}</p>}
      <p className="text-xs opacity-70">
        showing {rendered.length}{total !== undefined ? ` of ${total}` : ''}
      </p>
    </section>
  )
}
