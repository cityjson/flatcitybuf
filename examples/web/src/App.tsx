import { HeaderPanel } from './components/HeaderPanel'
import { MapView } from './components/MapView'
import { QueryPanel } from './components/QueryPanel'
import { SourcePanel } from './components/SourcePanel'

export function App() {
  return (
    <div className="flex h-full text-cj-charcoal">
      <aside className="w-80 shrink-0 space-y-4 overflow-y-auto border-r border-cj-charcoal/15 bg-white p-4">
        <header className="space-y-1">
          <div className="flex items-center gap-2">
            {/* the three logo marks: gold, purple, green */}
            <span className="flex gap-0.5" aria-hidden>
              <span className="h-5 w-1.5 rounded-sm bg-cj-gold" />
              <span className="h-5 w-1.5 rounded-sm bg-cj-purple" />
              <span className="h-5 w-1.5 rounded-sm bg-cj-green" />
            </span>
            <h1 className="text-base font-bold">FlatCityBuf viewer</h1>
          </div>
          <p className="text-xs text-cj-charcoal-soft">
            Native TypeScript reader (@cityjson/flatcitybuf) — no WASM, no server.
          </p>
        </header>
        <SourcePanel />
        <HeaderPanel />
        <QueryPanel />
      </aside>
      <main className="relative flex-1">
        <MapView />
      </main>
    </div>
  )
}
