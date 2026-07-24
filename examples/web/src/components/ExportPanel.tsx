// src/components/ExportPanel.tsx
import { useAtom } from 'jotai'
import { type ExportFormat, FORMATS } from '../export/index'
import { useFcbData } from '../hooks/useFcbData'
import { exportFormatAtom } from '../store/index'
import { PrimaryButton, SectionHeading } from './ui'

const ORDER: ExportFormat[] = ['cityjson', 'cityjsonseq', 'obj']

export function ExportPanel() {
  const { header, rendered, exportAs, exporting } = useFcbData()
  const [format, setFormat] = useAtom(exportFormatAtom)
  if (header === undefined) return null
  const disabled = rendered.length === 0 || exporting
  return (
    <section className="space-y-2 text-sm">
      <SectionHeading>4. Export</SectionHeading>
      <div className="flex flex-wrap gap-1">
        {ORDER.map((f) => {
          const on = format === f
          return (
            <button key={f} type="button" onClick={() => setFormat(f)}
              aria-pressed={on}
              className={
                'rounded border px-2 py-1 text-xs transition-colors '
                + (on
                  ? 'border-cj-gold bg-cj-gold-soft font-semibold text-cj-charcoal'
                  : 'border-cj-charcoal/20 text-cj-charcoal-soft hover:border-cj-gold')
              }>
              {FORMATS[f].label}
            </button>
          )
        })}
      </div>
      <PrimaryButton disabled={disabled} onClick={() => void exportAs(format)}>
        {exporting ? 'preparing…' : `Download ${rendered.length} feature${rendered.length === 1 ? '' : 's'}`}
      </PrimaryButton>
      <p className="text-xs text-cj-charcoal-soft">
        Exports the current result. OBJ includes every LoD present in the data.
      </p>
    </section>
  )
}
