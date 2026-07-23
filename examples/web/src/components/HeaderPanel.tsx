// src/components/HeaderPanel.tsx
import { useFcbData } from '../hooks/useFcbData'
import { columnTypeName } from '../reader/index'

export function HeaderPanel() {
  const { header } = useFcbData()
  if (header === undefined) return null
  return (
    <section className="space-y-1 text-xs">
      <h2 className="text-sm font-semibold">2. Header</h2>
      <div>version: {header.version}</div>
      <div>features: {header.featuresCount || 'unknown'}</div>
      <div className={header.crs.supported ? '' : 'text-red-600'}>
        CRS: {header.crs.label}{header.crs.supported ? '' : ' (unsupported — not georeferenced)'}
      </div>
      <details>
        <summary>columns ({header.columns.length})</summary>
        <ul className="mt-1 space-y-0.5">
          {header.columns.map((c) => (
            <li key={c.name}>{c.name} — {columnTypeName(c.type)}</li>
          ))}
        </ul>
      </details>
    </section>
  )
}
