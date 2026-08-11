# CityGML 2.0 input support — design

Date: 2026-08-11
Status: approved by user (brainstorming session)

## Goal

`fcb ser` accepts CityGML 2.0 files (`.gml`/`.xml`) as input, alongside the
existing CityJSON/CityJSONSeq support, including glob patterns and multiple
files. CityGML 3.0 is out of scope. Mixing CityGML and CityJSON inputs in one
invocation is undocumented/untested (it will incidentally work, but we make no
promises). Output formats are unchanged; no GML output.

Strategy: CityGML → in-memory CityJSONSeq structures (cjseq types) → existing
FCB writer path. The CityJSON model is the semantic target; acknowledged as a
lossy-but-acceptable compromise where CityGML concepts have no CityJSON
equivalent.

## Fidelity scope (v1)

Preserve **everything the mapping below covers**, including appearance:

- All thematic CityObject types listed under "Mapping rules"
- Per-LoD geometry (all LoDs present in the file)
- Semantic surfaces
- Core + generic attributes
- Appearance: X3DMaterial and ParameterizedTexture (TexCoordList).
  `GeoreferencedTexture` and `TexCoordGen` are warned and skipped in v1.

CRS: pass-through, no reprojection. srsName parsed and normalized; axis order
fixed for lat/lon-ordered geographic CRSs. Reprojection (`--crs`) is a possible
future addition (proj deps already in the workspace) — not v1.

## Architecture

New workspace member `src/rust/fcb_citygml` (crate name `fcb_citygml`).
Dependencies: `quick-xml`, `cjseq`, `thiserror`, `serde_json` only. No
FlatBuffers, no CLI knowledge.

Decision record: hand-rolled parser chosen over `nusamai-citygml` (git-only
dependency — blocks crates.io publishing; PLATEAU-oriented codegen; we'd still
write the whole mapping layer) and over shelling out to citygml-tools (JVM
runtime dependency). citygml-tools is still used *offline* as a test oracle.

```
fcb_citygml/
  src/
    lib.rs          // pub fn parse_citygml<R: BufRead>(r, &ParseOptions)
                    //   -> Result<(CityGmlDocument, ParseReport), CityGmlError>
    gml/            // GML 3.1.1 geometry: pos/posList, LinearRing, Polygon,
                    //   Multi/CompositeSurface, Solid, Multi/CompositeSolid,
                    //   within-file xlink registry
    model.rs        // intermediate CityObject model: typed geometry, semantics,
                    //   attributes, appearance refs; real-world f64 coords
    citygml/        // thematic readers: bldg, brid, tun, veg, tran, wtr, luse,
                    //   frn, dem, gen, grp + generic attributes
    appearance.rs   // X3DMaterial + ParameterizedTexture → CityJSON appearance
    convert.rs      // intermediate model → cjseq CityJSON + Vec<CityJSONFeature>
                    //   (quantization, vertex dedup, transform, extent)
    crs.rs          // srsName parsing → OGC URL form, axis-order fix
```

`CityGmlDocument = { metadata: CityJSON, features: Vec<CityJSONFeature> }` —
the same shape as the CLI's `InputData`, so integration is a thin dispatch.

Parsing model: single streaming pass over the document; each
`cityObjectMember` subtree is buffered (DOM-lite) so xlink resolution and
semantics stay tractable. xlinks resolve within-file only; unresolvable ones
are hard errors with the href and context in the message.

Quantization: CityGML has no transform. Compute one per file: scale `0.001`
(default), translate = per-file coordinate minimum. Vertices deduplicated per
feature. The CLI merger already reconciles differing transforms across files.

## Mapping rules (CityGML 2.0 → CityJSON 2.0)

Types:

| CityGML | CityJSON |
|---|---|
| bldg:Building / BuildingPart / BuildingInstallation | Building / BuildingPart / BuildingInstallation |
| brid:Bridge (+Part/Installation/ConstructionElement) | Bridge family |
| tun:Tunnel (+Part/Installation) | Tunnel family |
| veg:SolitaryVegetationObject / PlantCover | same names |
| tran:Road / Railway / TransportSquare | same names |
| wtr:WaterBody | WaterBody |
| luse:LandUse | LandUse |
| frn:CityFurniture | CityFurniture |
| dem:TINRelief | TINRelief |
| gen:GenericCityObject | +GenericCityObject (extension type) |
| grp:CityObjectGroup | CityObjectGroup |

Unrecognized members: warn, skip, count in `ParseReport` — never silent.

Structure: one top-level CityObject = one `CityJSONFeature` line; nested
parts/installations live in the same feature's `CityObject` map with
`parents`/`children`. `gml:id` becomes the object key (generated stable key if
absent: `<file-stem>-<index>`).

Geometry: LoD from property name (`lod2Solid` → `"2"`). GML Solid → Solid,
Multi/CompositeSurface → Multi/CompositeSurface, Multi/CompositeSolid → same.
Interior rings preserved. Ring closure repaired per CityJSON spec (drop
last==first); consecutive duplicate points dropped; a ring left with <3
distinct points is degenerate — its polygon is warned, skipped, and counted in
`ParseReport` (not a hard error).

ImplicitGeometry (used heavily by PLATEAU-style datasets for vegetation and
furniture): v1 flattens it — apply the transformation matrix and reference
point to the template geometry — rather than mapping to CityJSON
`geometry-templates`. Proper template pooling is a possible follow-up.

Semantic surfaces: Wall/Roof/Ground/Closure/OuterCeiling/OuterFloorSurface,
Door, Window, and water/transport equivalents → geometry `semantics`
(surfaces + per-polygon indices). Generic attributes on the surface ride along.

Attributes: module core attributes (class/function/usage/yearOfConstruction/
measuredHeight/roofType/…) and `gen:*Attribute` (string, int, double, date,
uri, measure) → typed JSON values. `gml:name` → `attributes.name` if present.

Appearance: X3DMaterial → `material` themes; ParameterizedTexture with
TexCoordList → `texture` themes + per-feature `vertices-texture`. Texture image
URIs copied verbatim (relative stays relative).

## CLI integration, CRS & metadata

- `cli/src/reader.rs`: `InputFormat::CityGML` for `.gml`/`.xml`;
  `read_input_file` dispatches to `fcb_citygml::parse_citygml`. Globs and
  multi-file merging are untouched (they operate on paths / InputData).
- No new CLI flags for the happy path. CLI summary output gains a
  skipped/warnings section fed by `ParseReport`.
- srsName source: top-level `gml:Envelope`, else first geometry that carries
  one. Accepted forms: `EPSG:nnnn`, `urn:ogc:def:crs:EPSG::nnnn`, compound
  `urn:ogc:def:crs,crs:…` (horizontal component wins),
  `http(s)://www.opengis.net/def/crs/EPSG/0/nnnn`. Normalized to the OGC URL
  form. Geographic CRSs with official lat/lon axis order get coordinates
  swapped to x=lon/y=lat. No srsName → warn, omit `referenceSystem`.
- Output metadata line: `version: "2.0"`, computed `transform`, computed
  `metadata.geographicalExtent` (from actual vertices, not the file Envelope),
  `referenceSystem` as above.

## Error handling

`CityGmlError` (thiserror; no anyhow, per workspace policy):

- `Xml` — malformed XML, with byte offset
- `UnresolvableXlink { href, context }`
- `InvalidGeometry` — odd coordinate count, missing posList, degenerate ring
- `Io`, `UnsupportedRoot` (not a CityModel), etc.

Policy: **malformed structure = hard error; valid-but-unsupported content =
warn + skip + count** (surfaced via `ParseReport` and the CLI summary). No
panics; malformed-input tests assert errors, not crashes.

## Testing

Three rings, all under `just test` in `src/rust`:

1. **Hand-authored unit fixtures** (`fcb_citygml/tests/fixtures/*.gml`, one
   concern each: LoD1 building, interior ring, xlink'd surface, semantic
   surfaces, every gen:attribute type, material, texture, every srsName form,
   every thematic module, malformed inputs) with hand-written
   `.expected.city.jsonl`. Comparison: whole-line semantic JSON equality
   (canonicalized `serde_json::Value`), never selected keys.
2. **citygml-tools cross-check**: committed corpus of real-dataset snippets
   (cityjson.org dataset list) with `citygml-tools to-cityjson` outputs
   committed as expected. Structural comparison (object set, types, hierarchy,
   attributes, semantics, polygon counts, dequantized vertices within
   tolerance) because quantization/vertex order legitimately differ.
   Regeneration is a documented offline step (Java only needed then).
3. **End-to-end FCB round-trip**: fixture → `fcb ser` → `.fcb` → Rust reader →
   compare against the same expected JSONL. Guards against two stages agreeing
   on a wrong answer.

Process: TDD (Red → Green → Refactor) enforced per task; implementation by
Opus subagents orchestrated and reviewed before committing to `develop`.

## Out of scope (v1)

- CityGML 3.0; GML output; reprojection; ADE extensions (unknown-namespace
  children of known objects are warned+skipped); GeoreferencedTexture /
  TexCoordGen; cross-file xlinks; CityJSON `geometry-templates` output
  (ImplicitGeometry is flattened instead — see "Geometry" above).
