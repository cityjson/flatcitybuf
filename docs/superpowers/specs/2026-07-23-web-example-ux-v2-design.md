# Web Example UX v2 — Design

**Date:** 2026-07-23
**Status:** Approved
**Builds on:** the shipped `examples/web` React/deck.gl viewer.

Four UX additions requested after the viewer shipped.

## A. Configurable feature limit
The number of features shown is user-configurable. A shared `limitAtom` (default
200) drives the initial render on open, manual queries, follow-camera queries,
and "Load next batch" page size.

## B. Grouped query UI with a spatial mode selector
Replace the flat query panel with one **Query** container:
- **Spatial mode** (radio, `spatialModeAtom`): `all` (Whole dataset, first
  *limit*, paged) · `bbox` (Draw bbox) · `follow` (Follow camera) — **default
  `follow`**.
- **Attribute filter** (optional): field (indexed columns only) / operator /
  value, applied on top of the spatial mode.
- **Limit** input + a single **Run query** button. Draw-bbox controls appear
  only in `bbox` mode.

## C. Follow-camera mode (default)
Moving/zooming the map auto-queries the visible viewport:
- Trigger on `viewState` change when mode is `follow`; **debounce ~400 ms**
  after the camera settles.
- Viewport bounds computed deck-natively from the current `viewState` +
  canvas size via `WebMercatorViewport(...).getBounds()`, then
  `bboxToSource` → `reader.select`.
- **Abort in-flight requests**: a shared `AbortController` is replaced on each
  new query; its `signal` is threaded `runQuery → reader.select({signal})`.
  Aborted requests resolve silently (no error status).
- The **limit** caps results when zoomed far out.
- **Follow queries never move the camera** — `render` takes a `frameCamera`
  flag; follow passes `false` to avoid a feedback loop (re-framing would change
  `viewState`, retriggering the follow query).
- Manual **Run query** in `follow` mode queries the current viewport once.

## D. Feature inspector
Clicking a building shows, in `FeatureInspector`:
- **General info**: feature id, primary CityObject type, geometry type + LoD,
  vertex count, triangle count.
- **Attributes**: the full attribute table (existing).

`RenderedFeature` gains an `info` object populated in `buildRenderedFeatures`:
`{ objectType?, geometryType?, lod?, vertexCount, triangleCount }`.

## Files
- `store/index.ts` — `limitAtom`, `spatialModeAtom`; extend `RenderedFeature`.
- `reader/index.ts` — thread optional `signal` through `QuerySpec`/`runQuery`.
- `hooks/useFcbData.ts` — read `limitAtom`; `queryViewport(bounds)`;
  `render(features, { frameCamera })`; shared AbortController; populate `info`.
- `hooks/useCameraFollow.ts` (new) — debounced viewport query when mode is
  `follow`.
- `components/QueryPanel.tsx` — grouped modes + single Run button.
- `components/MapView.tsx` — the follow trigger; keep the drawn-bbox rectangle.
- `components/FeatureInspector.tsx` — general info + attributes.

## Non-goals
Single attribute condition only (no AND/OR); no state persistence; pitch-exact
viewport bounds (an approximate bbox is fine — the limit caps over-fetch).

## Verification
`tsc --noEmit` + `npm test` (pure modules unchanged) + `npm run build`, then an
in-browser pass against the remote `delft.city.fcb`: load → buildings render;
switch modes; follow-camera re-queries on pan/zoom (throttled); draw-bbox +
attribute query; click a building → inspector shows info + attributes.
