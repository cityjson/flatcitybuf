// src/components/SourcePanel.tsx
import { useState } from 'react'
import { useFcbData } from '../hooks/useFcbData'

const DEFAULT_URL =
  'https://storage.googleapis.com/flatcitybuf/3dbag_subset_all_index.fcb'

export function SourcePanel() {
  const { openUrl, openFile, status } = useFcbData()
  const [url, setUrl] = useState(DEFAULT_URL)
  return (
    <section className="space-y-2">
      <h2 className="text-sm font-semibold">1. Open a file</h2>
      <div className="flex gap-2">
        <input
          className="flex-1 rounded border px-2 py-1 text-sm"
          value={url} onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void openUrl(url) }}
        />
        <button className="rounded border px-3 py-1 text-sm"
          onClick={() => void openUrl(url)}>Load URL</button>
      </div>
      <label className="block rounded border border-dashed px-3 py-4 text-center text-sm cursor-pointer">
        Choose a local .fcb file
        <input type="file" accept=".fcb" className="hidden"
          onChange={(e) => { const f = e.target.files?.[0]; if (f) void openFile(f) }} />
      </label>
      <p className="text-xs opacity-70">{status}</p>
    </section>
  )
}
