// src/components/FeatureInspector.tsx
import { useAtomValue } from 'jotai'
import { selectedAtom } from '../store/index'

export function FeatureInspector() {
  const selected = useAtomValue(selectedAtom)
  if (selected === undefined) return null
  return (
    <section className="space-y-1 text-xs">
      <h2 className="text-sm font-semibold">4. Selected: {selected.id}</h2>
      <table className="w-full">
        <tbody>
          {Object.entries(selected.attributes).map(([k, v]) => (
            <tr key={k}><td className="pr-2 opacity-70">{k}</td>
              <td>{String(v)}</td></tr>
          ))}
        </tbody>
      </table>
    </section>
  )
}
