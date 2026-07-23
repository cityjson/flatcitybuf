import { FeatureInspector } from './components/FeatureInspector'
import { HeaderPanel } from './components/HeaderPanel'
import { MapView } from './components/MapView'
import { QueryPanel } from './components/QueryPanel'
import { SourcePanel } from './components/SourcePanel'

export function App() {
  return (
    <div className="flex h-full">
      <aside className="w-80 shrink-0 space-y-4 overflow-y-auto border-r p-4">
        <h1 className="text-base font-bold">FlatCityBuf viewer</h1>
        <p className="text-xs opacity-70">
          Native TypeScript reader (@cityjson/flatcitybuf) — no WASM, no server.
        </p>
        <SourcePanel />
        <HeaderPanel />
        <QueryPanel />
        <FeatureInspector />
      </aside>
      <main className="relative flex-1">
        <MapView />
      </main>
    </div>
  )
}
