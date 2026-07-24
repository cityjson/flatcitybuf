// src/components/SourcePanel.tsx
import { useState } from 'react'
import { useFcbData } from '../hooks/useFcbData'
import { PrimaryButton, SectionHeading } from './ui'

const DEFAULT_URL =
  'https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb'

const INPUT =
  'rounded border border-cj-charcoal/20 px-2 py-1 text-sm '
  + 'focus:border-cj-purple focus:outline-none focus:ring-1 focus:ring-cj-purple'

export function SourcePanel() {
  const { openUrl, openFile, status } = useFcbData()
  const [url, setUrl] = useState(DEFAULT_URL)
  return (
    <section className="space-y-2">
      <SectionHeading>1. Open a file</SectionHeading>
      <div className="flex gap-2">
        <input
          className={`flex-1 ${INPUT}`}
          value={url} onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void openUrl(url) }}
        />
        <PrimaryButton onClick={() => void openUrl(url)}>Load URL</PrimaryButton>
      </div>
      <label className="block cursor-pointer rounded border border-dashed border-cj-charcoal/30 px-3 py-4 text-center text-sm text-cj-charcoal-soft hover:border-cj-purple hover:text-cj-purple">
        Choose a local .fcb file
        <input type="file" accept=".fcb" className="hidden"
          onChange={(e) => { const f = e.target.files?.[0]; if (f) void openFile(f) }} />
      </label>
      <p className="text-xs text-cj-charcoal-soft">{status}</p>
    </section>
  )
}
