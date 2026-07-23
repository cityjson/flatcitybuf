// src/components/FeatureInspector.tsx
import { useAtomValue } from 'jotai'
import { selectedAtom } from '../store/index'

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-2">
      <span className="opacity-60">{label}</span>
      <span className="text-right">{value}</span>
    </div>
  )
}

export function FeatureInspector() {
  const selected = useAtomValue(selectedAtom)
  if (selected === undefined) return null
  const { info } = selected
  const geom = info.geometryType
    ? `${info.geometryType}${info.lod ? ` (LoD ${info.lod})` : ''}`
    : '(none)'
  const attrs = Object.entries(selected.attributes)
  return (
    <section className="space-y-2 text-xs">
      <h2 className="text-sm font-semibold">4. Selected feature</h2>

      <div className="rounded border p-2 space-y-0.5">
        <InfoRow label="id" value={selected.id} />
        <InfoRow label="type" value={info.objectType ?? '(unknown)'} />
        <InfoRow label="geometry" value={geom} />
        <InfoRow label="vertices" value={String(info.vertexCount)} />
        <InfoRow label="triangles" value={String(info.triangleCount)} />
      </div>

      <div>
        <div className="mb-1 font-semibold opacity-70">
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
    </section>
  )
}
