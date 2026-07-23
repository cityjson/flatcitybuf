// src/components/FeatureInspector.tsx
import { useAtomValue } from 'jotai'
import { selectedAtom } from '../store/index'

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-3">
      <span className="opacity-60">{label}</span>
      <span className="text-right">{value}</span>
    </div>
  )
}

/** The selected feature's general info + attributes. Rendered inside the map
 *  popup (see MapView); returns null when nothing is selected. */
export function FeatureInspector() {
  const selected = useAtomValue(selectedAtom)
  if (selected === undefined) return null
  const { info } = selected
  const geom = info.geometryType
    ? `${info.geometryType}${info.lod ? ` (LoD ${info.lod})` : ''}`
    : '(none)'
  const attrs = Object.entries(selected.attributes)
  return (
    <div className="text-xs">
      <div className="mb-1 font-semibold break-all">{selected.id}</div>
      <div className="space-y-0.5">
        <InfoRow label="type" value={info.objectType ?? '(unknown)'} />
        <InfoRow label="geometry" value={geom} />
        <InfoRow label="vertices" value={String(info.vertexCount)} />
        <InfoRow label="triangles" value={String(info.triangleCount)} />
      </div>
      <div className="mt-2 mb-1 font-semibold opacity-70">
        Attributes ({attrs.length})
      </div>
      {attrs.length === 0
        ? <p className="opacity-60">(no attributes)</p>
        : (
          <table className="w-full">
            <tbody>
              {attrs.map(([k, v]) => (
                <tr key={k} className="align-top">
                  <td className="pr-2 opacity-70">{k}</td>
                  <td className="break-all">{v === null ? 'null' : String(v)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
    </div>
  )
}
