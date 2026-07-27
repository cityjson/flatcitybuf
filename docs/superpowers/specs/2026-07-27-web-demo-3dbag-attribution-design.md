# Web demo: 3DBAG data attribution

## Problem

The web viewer (`examples/web`) renders 3DBAG buildings over a CARTO/MapLibre
basemap but shows no attribution for the 3DBAG data. 3DBAG is CC BY 4.0 and
requires a visible credit: <https://docs.3dbag.nl/en/copyright>.

3DBAG's stated requirements for electronic maps:

- Credit text: **© 3DBAG by tudelft3d and 3DGI**
- Digital media must link to the copyright page
  (`https://docs.3dbag.nl/en/copyright`)
- Attribution appears in the bottom-right corner

## Approach

Use MapLibre's own attribution control via `react-map-gl/maplibre`'s
`AttributionControl`, feeding the 3DBAG credit through `customAttribution`.
`customAttribution` **augments** — it does not replace — the basemap
attribution the control reads from the style's sources (CARTO / OpenStreetMap),
so the single bottom-right control shows both:

> © OpenStreetMap contributors © CARTO | © 3DBAG by tudelft3d and 3DGI

The 3DBAG entry is a link to the copyright page, satisfying the link
requirement. Bottom-right is MapLibre's default position, satisfying placement.

Shown unconditionally (independent of the currently-open source), since the
demo's primary showcased dataset is 3DBAG.

## Changes

`src/components/MapView.tsx` only:

1. Import `AttributionControl` alongside `Map` from `react-map-gl/maplibre`.
2. Set `attributionControl={false}` on `<Map>` so MapLibre does not also
   auto-create its own control — otherwise there would be two overlapping
   controls (one auto with just basemap credits, one explicit with 3DBAG).
3. Add one `<AttributionControl compact={false} customAttribution="…" />` child
   of `<Map>`.

No other files change.

## Testing

Manual, in the browser: confirm the bottom-right control shows both the basemap
attribution and the 3DBAG credit, and that the 3DBAG credit links to the
copyright page. No unit test — this is a static UI credit with no logic.
