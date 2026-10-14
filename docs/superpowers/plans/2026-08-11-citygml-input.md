# CityGML 2.0 Input Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> Every implementer MUST first read `docs/superpowers/specs/2026-08-11-citygml-input-design.md` (the approved spec) and the root `CLAUDE.md`. TDD is mandatory: write the failing test, run it and watch it fail, implement minimally, watch it pass, refactor, commit.

**Goal:** `fcb ser` accepts CityGML 2.0 (`.gml`/`.xml`) inputs — including globs and multiple files — by converting them in memory to cjseq CityJSON structures, preserving geometry, semantics, attributes, and appearance.

**Architecture:** New workspace crate `src/rust/fcb_citygml` (quick-xml + cjseq + thiserror + serde_json only). A streaming scan buffers each `cityObjectMember` subtree into a lightweight owned XML tree; module readers build an intermediate model with real-world f64 coordinates; `convert.rs` quantizes into `CityJSON` metadata + `Vec<CityJSONFeature>` — the same shape the CLI's `InputData` already has. The CLI learns two new extensions and dispatches.

**Tech Stack:** Rust, quick-xml 0.41 (`NsReader`), cjseq2 0.2.0-alpha.1, thiserror, cargo-nextest (`just test` in `src/rust`).

## Global Constraints

- Workspace deps only: add `quick-xml = "0.41"` to `src/rust/Cargo.toml` `[workspace.dependencies]`; crates reference `{ workspace = true }` (per `src/rust/CLAUDE.md`).
- `thiserror` for errors; **no anyhow**; no `unwrap()` outside tests.
- Error policy (spec): malformed structure = hard error; valid-but-unsupported content = warn + skip + count in `ParseReport`. No panics on any input.
- Comparisons in tests: whole-document semantic equality (`serde_json::Value` ==), never selected keys.
- Namespace matching: match **local name + namespace URI**. Accept both CityGML 2.0 (`http://www.opengis.net/citygml/<module>/2.0`) and 1.0 (`…/1.0`) URIs; GML is `http://www.opengis.net/gml`; xlink is `http://www.w3.org/1999/xlink`; appearance module `http://www.opengis.net/citygml/appearance/2.0`.
- Determinism: features in document order; vertices in first-seen order; semantics surfaces in boundedBy document order — expected fixtures rely on this.
- Quantization: default scale `[0.001, 0.001, 0.001]`, translate = per-file minimum of real coordinates, `q = round((x - translate) / scale) as i64`.
- Commit after every task (on `develop`), message style `feat(citygml): …` / `test(citygml): …`.
- Run `cargo nextest run -p fcb_citygml` from `src/rust` for the crate's tests; full gate is `just check` in `src/rust` at Tasks 15–17.

## Shared interfaces (defined once, used by every task)

```rust
// fcb_citygml/src/lib.rs (public API)
pub struct ParseOptions { pub scale: [f64; 3] }          // Default: [0.001; 3]
pub struct CityGmlDocument { pub metadata: cjseq::CityJSON, pub features: Vec<cjseq::CityJSONFeature> }
#[derive(Debug, Default)]
pub struct ParseReport { pub skipped: Vec<Skipped>, pub warnings: Vec<String> }
#[derive(Debug)]
pub struct Skipped { pub element: String, pub gml_id: Option<String>, pub reason: String }
pub fn parse_citygml<R: std::io::BufRead>(reader: R, opts: &ParseOptions)
    -> Result<(CityGmlDocument, ParseReport), CityGmlError>;

// fcb_citygml/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum CityGmlError {
    #[error("XML error at byte {position}: {source}")]
    Xml { position: u64, #[source] source: quick_xml::Error },
    #[error("root element is <{0}>, expected CityModel")]
    UnsupportedRoot(String),
    #[error("unresolvable xlink href {href} in {context}")]
    UnresolvableXlink { href: String, context: String },
    #[error("invalid geometry in {context}: {reason}")]
    InvalidGeometry { context: String, reason: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// fcb_citygml/src/xml.rs — owned subtree built from quick_xml events
pub struct XmlNode {
    pub ns: String,             // namespace URI ("" if none)
    pub local: String,          // local element name
    pub attrs: Vec<(String, String)>,   // (local attr name or "xlink:href"-style resolved, value)
    pub text: String,           // concatenated direct text content, trimmed
    pub children: Vec<XmlNode>,
}
impl XmlNode {
    pub fn attr(&self, name: &str) -> Option<&str>;              // by local attr name
    pub fn child(&self, local: &str) -> Option<&XmlNode>;        // first child by local name
    pub fn children_named(&self, local: &str) -> impl Iterator<Item = &XmlNode>;
    pub fn descendants(&self) -> impl Iterator<Item = &XmlNode>; // depth-first, self included
    pub fn gml_id(&self) -> Option<&str>;                        // attr "id" in gml ns
}

// fcb_citygml/src/gml/mod.rs — geometry parsed to real-world coords
pub struct Ring { pub gml_id: Option<String>, pub pts: Vec<[f64; 3]> }
pub struct Polygon3 {
    pub gml_id: Option<String>,
    pub rings: Vec<Ring>,               // [0] exterior, rest interior
    pub sem_idx: Option<usize>,         // index into IntermediateGeometry.surfaces
}
pub enum GmlGeometry {                  // shells/surfaces flattened to polygon lists
    MultiSurface(Vec<Polygon3>),
    CompositeSurface(Vec<Polygon3>),
    Solid(Vec<Vec<Polygon3>>),          // shells (exterior first)
    MultiSolid(Vec<Vec<Vec<Polygon3>>>),
    CompositeSolid(Vec<Vec<Vec<Polygon3>>>),
}
pub struct XlinkRegistry { /* gml_id -> cloned Polygon3, built per cityObjectMember subtree */ }
impl XlinkRegistry {
    pub fn collect(subtree: &XmlNode) -> Self;   // index every gml:Polygon by gml:id
    pub fn resolve(&self, href: &str, context: &str) -> Result<Polygon3, CityGmlError>;
}

// fcb_citygml/src/model.rs — intermediate model
pub struct SemanticSurface { pub stype: String, pub attributes: serde_json::Map<String, serde_json::Value> }
pub struct IntermediateGeometry {
    pub lod: String,
    pub geometry: GmlGeometry,
    pub surfaces: Vec<SemanticSurface>, // referenced by Polygon3.sem_idx
}
pub struct IntermediateObject {
    pub id: String,
    pub co_type: cjseq::CityObjectType,
    pub attributes: serde_json::Map<String, serde_json::Value>,
    pub geometries: Vec<IntermediateGeometry>,
    pub children: Vec<IntermediateObject>,   // BuildingParts, Installations, …
}

// fcb_citygml/src/crs.rs
pub fn normalize_srs(srs_name: &str) -> Option<NormalizedCrs>;
pub struct NormalizedCrs { pub reference_system: String /* OGC URL form */, pub swap_axes: bool }

// fcb_citygml/src/convert.rs
pub fn convert(
    objects: Vec<IntermediateObject>,
    crs: Option<NormalizedCrs>,
    appearance: Vec<crate::appearance::SurfaceData>,   // empty until Task 13
    opts: &ParseOptions,
    report: &mut ParseReport,
) -> CityGmlDocument;

// fcb_citygml/src/appearance.rs (Tasks 13–14)
pub enum SurfaceData {
    Material { theme: String, material: cjseq::MaterialObject, targets: Vec<String> /* polygon gml:ids */ },
    Texture  { theme: String, texture: cjseq::TextureObject,
               targets: Vec<TextureTarget> },
}
pub struct TextureTarget { pub polygon_id: String, pub ring_coords: Vec<(String, Vec<[f64; 2]>)> /* (ring gml:id, uv per point) */ }
pub fn parse_appearances(city_model_children: &[XmlNode], report: &mut ParseReport) -> Vec<SurfaceData>;
```

Test harness helper, used from Task 6 on (`fcb_citygml/tests/common/mod.rs`):

```rust
use fcb_citygml::{parse_citygml, ParseOptions};

/// Parse tests/fixtures/<name>.gml and compare, as serde_json::Value, the
/// metadata line + each feature line against tests/fixtures/<name>.expected.city.jsonl.
/// Whole-line equality; Value == ignores object key order but not array order.
pub fn assert_fixture(name: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let gml = std::fs::File::open(dir.join(format!("{name}.gml"))).unwrap();
    let (doc, _report) =
        parse_citygml(std::io::BufReader::new(gml), &ParseOptions::default()).unwrap();
    let expected_raw = std::fs::read_to_string(dir.join(format!("{name}.expected.city.jsonl"))).unwrap();
    let mut expected = expected_raw.lines().filter(|l| !l.trim().is_empty());

    let meta_actual: serde_json::Value =
        serde_json::to_value(&doc.metadata).unwrap();
    let meta_expected: serde_json::Value =
        serde_json::from_str(expected.next().expect("expected metadata line")).unwrap();
    pretty_assertions::assert_eq!(meta_expected, meta_actual, "metadata line differs for {name}");

    for (i, feat) in doc.features.iter().enumerate() {
        let actual: serde_json::Value = serde_json::to_value(feat).unwrap();
        let exp_line = expected.next().unwrap_or_else(|| panic!("missing expected feature line {i}"));
        let exp: serde_json::Value = serde_json::from_str(exp_line).unwrap();
        pretty_assertions::assert_eq!(exp, actual, "feature {i} differs for {name}");
    }
    assert!(expected.next().is_none(), "extra expected lines for {name}");
}
```

---

### Task 1: Crate scaffold, error type, root validation

**Files:**
- Modify: `src/rust/Cargo.toml` (add `quick-xml = "0.41"` to `[workspace.dependencies]`, add `"fcb_citygml"` to `[workspace] members`)
- Create: `src/rust/fcb_citygml/Cargo.toml`, `src/rust/fcb_citygml/src/lib.rs`, `src/rust/fcb_citygml/src/error.rs`
- Test: `src/rust/fcb_citygml/tests/parse_root.rs`

**Interfaces:**
- Produces: `parse_citygml`, `ParseOptions`, `CityGmlDocument`, `ParseReport`, `Skipped`, `CityGmlError` exactly as in "Shared interfaces". `CityGmlDocument.metadata` for an empty CityModel: `thetype: CityJSON`, `version: "2.0"`, identity-scaled transform (`scale = opts.scale`, `translate = [0,0,0]`), empty `city_objects`/`vertices`, `metadata: None`.

`fcb_citygml/Cargo.toml`:

```toml
[package]
name = "fcb_citygml"
version = "0.1.0"
edition = "2021"
description = "CityGML 2.0 to CityJSON(Seq) converter used by the FlatCityBuf CLI"
license = "MIT"

[dependencies]
quick-xml = { workspace = true }
cjseq = { workspace = true }
thiserror = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
pretty_assertions = { workspace = true }
```

- [ ] **Step 1: Write the failing tests** (`tests/parse_root.rs`)

```rust
use fcb_citygml::{parse_citygml, CityGmlError, ParseOptions};
use std::io::BufReader;

#[test]
fn empty_city_model_parses_to_empty_document() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:gml="http://www.opengis.net/gml"/>"#;
    let (doc, report) =
        parse_citygml(BufReader::new(xml.as_bytes()), &ParseOptions::default()).unwrap();
    assert_eq!(doc.metadata.version, "2.0");
    assert!(doc.features.is_empty());
    assert!(report.skipped.is_empty());
}

#[test]
fn non_citymodel_root_is_unsupported_root() {
    let xml = r#"<foo xmlns="http://example.com"/>"#;
    let err = parse_citygml(BufReader::new(xml.as_bytes()), &ParseOptions::default()).unwrap_err();
    assert!(matches!(err, CityGmlError::UnsupportedRoot(_)));
}

#[test]
fn malformed_xml_is_xml_error_not_panic() {
    let xml = r#"<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"><unclosed"#;
    let err = parse_citygml(BufReader::new(xml.as_bytes()), &ParseOptions::default()).unwrap_err();
    assert!(matches!(err, CityGmlError::Xml { .. }));
}
```

- [ ] **Step 2: Run to verify failure** — `cd src/rust && cargo nextest run -p fcb_citygml`; expected: compile error (crate does not exist yet), then after scaffolding without logic: assertion failures.
- [ ] **Step 3: Minimal implementation** — workspace edits; `error.rs` as specified; `lib.rs` with `ParseOptions`/`Default`, structs, and `parse_citygml` that opens a `quick_xml::NsReader`, reads to the first start element, errors `UnsupportedRoot` unless local name is `CityModel`, then consumes events to EOF mapping quick-xml errors to `CityGmlError::Xml { position: reader.buffer_position(), .. }` and returns the empty document.
- [ ] **Step 4: Run to verify pass** — same command, 3 tests PASS.
- [ ] **Step 5: Commit** — `git add -A src/rust && git commit -m "feat(citygml): scaffold fcb_citygml crate with root validation"`

### Task 2: CRS normalization (`crs.rs`)

**Files:** Create `src/rust/fcb_citygml/src/crs.rs` (declare `pub mod crs;` in lib.rs). Tests inline `#[cfg(test)]`.

**Interfaces:** Produces `normalize_srs`, `NormalizedCrs` per "Shared interfaces".

- [ ] **Step 1: Failing tests** (in `crs.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn epsg_forms_normalize_to_ogc_url() {
        for form in ["EPSG:25832", "urn:ogc:def:crs:EPSG::25832",
                     "http://www.opengis.net/def/crs/EPSG/0/25832",
                     "https://www.opengis.net/def/crs/EPSG/0/25832"] {
            let c = normalize_srs(form).unwrap();
            assert_eq!(c.reference_system, "https://www.opengis.net/def/crs/EPSG/0/25832");
            assert!(!c.swap_axes);
        }
    }
    #[test]
    fn compound_urn_takes_horizontal_component() {
        let c = normalize_srs("urn:ogc:def:crs,crs:EPSG::25832,crs:EPSG::5783").unwrap();
        assert_eq!(c.reference_system, "https://www.opengis.net/def/crs/EPSG/0/25832");
    }
    #[test]
    fn urn_4326_swaps_axes() {
        let c = normalize_srs("urn:ogc:def:crs:EPSG::4326").unwrap();
        assert_eq!(c.reference_system, "https://www.opengis.net/def/crs/EPSG/0/4326");
        assert!(c.swap_axes);
    }
    #[test]
    fn legacy_epsg_colon_4326_does_not_swap() {
        // "EPSG:4326" is the legacy x=lon convention; only the urn/OGC-URL forms are lat/lon.
        assert!(!normalize_srs("EPSG:4326").unwrap().swap_axes);
    }
    #[test]
    fn unknown_is_none() { assert!(normalize_srs("CRS:84unknown-junk").is_none()); }
}
```

- [ ] **Step 2: Run, verify FAIL** (function missing).
- [ ] **Step 3: Implement** — string parsing only, no external deps. Extract the EPSG code from each accepted form; output `https://www.opengis.net/def/crs/EPSG/0/{code}`. `swap_axes = true` only for urn/OGC-URL forms whose EPSG code is in the lat/lon-ordered geographic set; hardcode the pragmatic list `{4326, 4258, 4269, 4283, 4171, 4617}` (document: extend as datasets demand; comment each code with its CRS name).
- [ ] **Step 4: Run, verify PASS.**
- [ ] **Step 5: Commit** — `git commit -m "feat(citygml): srsName normalization with axis-order detection"`

### Task 3: XML subtree loader + GML rings and polygons

**Files:** Create `src/rust/fcb_citygml/src/xml.rs`, `src/rust/fcb_citygml/src/gml/mod.rs`; wire `pub(crate) mod xml; pub mod gml;`. Tests inline in each file.

**Interfaces:**
- Produces: `XmlNode` API and, in `gml`: `parse_polygon(&XmlNode) -> Result<Option<Polygon3>, CityGmlError>` (Ok(None) = degenerate, caller records skip), `Ring`, `Polygon3`, plus `pub(crate) fn load_subtree<R: BufRead>(reader: &mut NsReader<R>, start: BytesStart) -> Result<XmlNode, CityGmlError>` in `xml.rs` (reads the element whose start tag was just consumed, to matching end).
- Ring repair rules (spec): drop closing point when last == first; drop consecutive duplicates (exact f64 equality); ring with < 3 remaining points → polygon skipped (Ok(None)). `gml:pos` (one point per element) and `gml:posList` (flat list) both supported; `srsDimension` 3 assumed, odd count → `InvalidGeometry`.

- [ ] **Step 1: Failing tests** (`gml/mod.rs`; build `XmlNode` via a test helper that parses a string with `load_subtree`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn node(xml: &str) -> crate::xml::XmlNode { crate::xml::parse_str_for_tests(xml).unwrap() }

    #[test]
    fn polygon_with_poslist_exterior_and_interior() {
        let p = parse_polygon(&node(r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml" gml:id="p1">
          <gml:exterior><gml:LinearRing gml:id="r1">
            <gml:posList>0 0 0 10 0 0 10 10 0 0 10 0 0 0 0</gml:posList>
          </gml:LinearRing></gml:exterior>
          <gml:interior><gml:LinearRing>
            <gml:pos>2 2 0</gml:pos><gml:pos>4 2 0</gml:pos><gml:pos>4 4 0</gml:pos><gml:pos>2 2 0</gml:pos>
          </gml:LinearRing></gml:interior>
        </gml:Polygon>"#)).unwrap().unwrap();
        assert_eq!(p.gml_id.as_deref(), Some("p1"));
        assert_eq!(p.rings.len(), 2);
        assert_eq!(p.rings[0].gml_id.as_deref(), Some("r1"));
        assert_eq!(p.rings[0].pts, vec![[0.,0.,0.],[10.,0.,0.],[10.,10.,0.],[0.,10.,0.]]); // closure dropped
        assert_eq!(p.rings[1].pts.len(), 3);
    }
    #[test]
    fn consecutive_duplicates_dropped() {
        let p = parse_polygon(&node(r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0 0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#)).unwrap().unwrap();
        assert_eq!(p.rings[0].pts.len(), 3);
    }
    #[test]
    fn degenerate_ring_is_none() {
        assert!(parse_polygon(&node(r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0 10 0 0 0 0 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#)).unwrap().is_none());
    }
    #[test]
    fn odd_coordinate_count_is_invalid_geometry() {
        assert!(parse_polygon(&node(r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0 10 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#)).is_err());
    }
}
```

Also in `xml.rs`, tests for `XmlNode`: `child`/`children_named` by local name across prefixes, `attr("id")` resolving `gml:id`, text concatenation, and `parse_str_for_tests` (a `#[cfg(test)]`-gated or `pub(crate)` helper that wraps `NsReader::from_str` + `load_subtree` on the root).

- [ ] **Step 2: Run, verify FAIL.**
- [ ] **Step 3: Implement** — `load_subtree` builds `XmlNode` with resolved namespace URIs (`NsReader::resolve_element`), attribute local names (keep `href` from the xlink namespace exposed as attr name `href`), depth-tracked until matching end. `parse_polygon` walks `exterior`/`interior` → `LinearRing` → `pos|posList`, applies ring repair in a dedicated `fn repair_ring(pts: Vec<[f64;3]>) -> Option<Vec<[f64;3]>>`.
- [ ] **Step 4: Run, verify PASS.**
- [ ] **Step 5: Refactor if needed (shared float parsing), re-run, Commit** — `git commit -m "feat(citygml): XML subtree loader and GML polygon parsing with ring repair"`

### Task 4: GML surface collections, solids, xlink registry

**Files:** Modify `src/rust/fcb_citygml/src/gml/mod.rs` (or split `gml/geometry.rs`). Tests inline.

**Interfaces:**
- Produces: `GmlGeometry`, `parse_geometry(&XmlNode, &XlinkRegistry, &mut ParseReport) -> Result<Option<GmlGeometry>, CityGmlError>` recognizing `MultiSurface`, `CompositeSurface`, `Solid`, `MultiSolid`, `CompositeSolid` nodes; `XlinkRegistry::{collect, resolve}`.
- `surfaceMember`/`baseSurface` etc. may be either an inline `gml:Polygon` or an `xlink:href="#id"` reference; hrefs must start with `#` (others → `UnresolvableXlink`). Solid = `gml:exterior` shell (`CompositeSurface` of members) plus any `gml:interior` shells.
- Degenerate polygons inside collections: skip + count via `report`, do not fail the collection; a Solid whose exterior shell ends up empty → `InvalidGeometry`.

- [ ] **Step 1: Failing tests** — inline XML snippets exercising: MultiSurface of two polygons; Solid with exterior CompositeSurface of 6 unit-cube faces (assert 1 shell, 6 polygons, coordinates survive); surfaceMember with `xlink:href="#p1"` where `p1` is defined under a sibling (registry collected from a common ancestor node); unresolvable href errors with the href in the message; MultiSolid of two cubes.

```rust
#[test]
fn solid_with_xlinked_members() {
    let root = node(r#"
    <root xmlns:gml="http://www.opengis.net/gml" xmlns:xlink="http://www.w3.org/1999/xlink">
      <defs><gml:Polygon gml:id="p1"><gml:exterior><gml:LinearRing>
        <gml:posList>0 0 0 1 0 0 1 1 0 0 1 0</gml:posList>
      </gml:LinearRing></gml:exterior></gml:Polygon></defs>
      <gml:MultiSurface gml:id="ms">
        <gml:surfaceMember xlink:href="#p1"/>
      </gml:MultiSurface>
    </root>"#);
    let reg = XlinkRegistry::collect(&root);
    let ms_node = root.descendants().find(|n| n.local == "MultiSurface").unwrap();
    let mut report = crate::ParseReport::default();
    let g = parse_geometry(ms_node, &reg, &mut report).unwrap().unwrap();
    match g { GmlGeometry::MultiSurface(ps) => { assert_eq!(ps.len(), 1); assert_eq!(ps[0].gml_id.as_deref(), Some("p1")); }, _ => panic!() }
}
```

- [ ] **Step 2: Run, verify FAIL.** 
- [ ] **Step 3: Implement.** `XlinkRegistry::collect` walks `descendants()` indexing every `Polygon` node by `gml:id`, parsing lazily or eagerly (eager is fine; skip degenerates silently at collect time, they're re-skipped with a report entry when referenced).
- [ ] **Step 4: Run, verify PASS. Step 5: Commit** — `git commit -m "feat(citygml): GML surface collections, solids, and xlink resolution"`

### Task 5: Streaming scan + minimal Building reader (intermediate model)

**Files:** Create `src/rust/fcb_citygml/src/model.rs`, `src/rust/fcb_citygml/src/citygml/mod.rs`, `src/rust/fcb_citygml/src/citygml/building.rs`; modify `lib.rs` (`parse_citygml` now: scan top level; on `boundedBy` read Envelope srsName; on `cityObjectMember` → `load_subtree` → dispatch to `citygml::read_member`; on `appearanceMember` collect subtree nodes into a `Vec<XmlNode>` for Task 13, currently unused).
**Test:** `src/rust/fcb_citygml/tests/building_model.rs` (asserts on `IntermediateObject` — expose a `pub fn parse_to_model<R: BufRead>(…) -> Result<(Vec<IntermediateObject>, Option<NormalizedCrs>, ParseReport), CityGmlError>` used by tests and internally by `parse_citygml`).

**Interfaces:**
- Produces: `model.rs` types per "Shared interfaces"; `citygml::read_member(&XmlNode, &mut ParseReport) -> Result<Option<IntermediateObject>, CityGmlError>` (None = unrecognized member, recorded in report); `building.rs` handling `bldg:Building` with `lod{0..4}Solid` / `lod{0..4}MultiSurface` / `lod{0..4}Geometry` (Geometry: accept any single GML geometry child) — LoD digit taken from the property name; `gml:id` → `id`, fallback `format!("{file_stem_or_obj}-{index}")` — use running member index, `"citygml-obj-{index}"`.
- Envelope: first `gml:Envelope` under root `boundedBy`; `srsName` attr → `crs::normalize_srs`; missing/unknown → warning in report, CRS None. Geometry-level fallback: if no envelope srsName, take the first `srsName` seen on any geometry node.

- [ ] **Step 1: Failing test** — LoD1 cube building:

```rust
#[test]
fn lod1_building_to_intermediate_model() {
    let xml = /* CityModel + Envelope srsName="EPSG:7415" + one bldg:Building gml:id="b1"
                 with lod1Solid = unit cube (6 faces, coords offset by (1000, 2000, 0)) */;
    let (objs, crs, report) = fcb_citygml::parse_to_model(BufReader::new(xml.as_bytes()), &ParseOptions::default()).unwrap();
    assert_eq!(objs.len(), 1);
    assert_eq!(objs[0].id, "b1");
    assert_eq!(objs[0].co_type, cjseq::CityObjectType::Building);
    assert_eq!(objs[0].geometries.len(), 1);
    assert_eq!(objs[0].geometries[0].lod, "1");
    assert!(matches!(&objs[0].geometries[0].geometry, GmlGeometry::Solid(shells) if shells[0].len() == 6));
    assert_eq!(crs.unwrap().reference_system, "https://www.opengis.net/def/crs/EPSG/0/7415");
    assert!(report.skipped.is_empty());
    // and: unknown member <foo:Thing> in same file → objs unchanged, report.skipped has 1 entry
}
```

(Write the full cube XML in the test file; 6 faces as `gml:Polygon` inside `gml:CompositeSurface` inside `gml:Solid/gml:exterior`.)

- [ ] **Step 2: Run, verify FAIL. Step 3: Implement. Step 4: verify PASS. Step 5: Commit** — `git commit -m "feat(citygml): streaming member scan and Building LoD geometry to intermediate model"`

### Task 6: convert.rs + first end-to-end fixture

**Files:** Create `src/rust/fcb_citygml/src/convert.rs`; complete `parse_citygml` to run `parse_to_model` → `convert`. Create `tests/common/mod.rs` (the `assert_fixture` helper from "Shared interfaces"), `tests/fixtures/lod1_building.gml`, `tests/fixtures/lod1_building.expected.city.jsonl`, `tests/fixtures.rs` with `mod common;` and one test per fixture as they accrue.

**Interfaces:**
- Produces: `convert(...)` per "Shared interfaces". Behavior: walk features in document order; per feature, dedupe quantized vertices with a `HashMap<[i64;3], usize>`; boundaries reference feature-local vertex indices (CityJSONSeq convention). Axis swap applied to every coordinate first when `crs.swap_axes`. Transform: translate = component-wise min over **all** real coordinates in the file (computed in a first pass), scale from `opts`. Metadata line: `version "2.0"`, `transform`, `metadata.geographical_extent` = [min x,y,z, max x,y,z] of real coords, `metadata.reference_system` from CRS, empty `city_objects` + `vertices`. Feature: `id` = top-level object id; `city_objects` includes the object and (later tasks) its descendants with `children`/`parents`; geometry `lod` set; `GmlGeometry → cjseq::Geometry` by direct depth mapping.
- cjseq specifics the implementer needs: `cjseq::Geometry` is an enum tagged by `type` with per-variant `boundaries` depth (MultiSurface/CompositeSurface depth 3, Solid 4, MultiSolid/CompositeSolid 5) and a flattened `GeometryCommon { semantics, material, texture }`; `CityObject::new(CityObjectType)` then set `pub` fields; `CityObject.other` must stay `Value::Object(empty)`; `CityJSONFeature { thetype: CityJSONFeatureType::CityJSONFeature, id, city_objects, vertices, appearance: None }`; `Metadata` and `ReferenceSystem` from `cjseq::metadata`. Check `cjseq` docs (`~/.cargo/registry/src/*/cjseq2-0.2.0-alpha.1/src/`) for exact variant field spellings before coding.

- [ ] **Step 1: Author fixture + expected file.** `lod1_building.gml`: the Task 5 cube (coords offset (1000, 2000, 0), srsName EPSG:7415). Hand-compute expected JSONL: translate = [1000, 2000, 0]; cube corners quantize to 0/1000 per axis (scale 0.001); 8 distinct vertices; write metadata line then one feature line with the Solid boundaries in first-seen vertex order. **Compute the boundary indices by hand from the fixture's polygon order — do not run the code to produce the expected file.**
- [ ] **Step 2: Failing test** — in `tests/fixtures.rs`: `#[test] fn lod1_building() { common::assert_fixture("lod1_building"); }`. Run, verify FAIL (convert missing/incomplete).
- [ ] **Step 3: Implement `convert.rs`.** Two passes (min-translate, then quantize). Keep functions small: `fn quantize(pt, transform) -> [i64;3]`, `fn polygon_to_indices(&Polygon3, …) -> Vec<Vec<usize>>`, `fn geometry_to_cj(…) -> cjseq::Geometry`.
- [ ] **Step 4: Run, verify PASS** (including all earlier tests). If the expected file disagrees, debug by hand before touching the expected file — the expected file is the oracle.
- [ ] **Step 5: Commit** — `git commit -m "feat(citygml): quantizing converter to CityJSONSeq with first end-to-end fixture"`

### Task 7: Attributes (core + generic)

**Files:** Create `src/rust/fcb_citygml/src/citygml/attributes.rs`; modify `building.rs` to use it. Fixture pair `tests/fixtures/attributes.gml` / `.expected.city.jsonl`; test added to `tests/fixtures.rs`; unit tests inline in `attributes.rs`.

**Interfaces:**
- Produces: `pub(crate) fn read_common_attributes(node: &XmlNode, out: &mut serde_json::Map<String, Value>)` handling, on any thematic object: `gml:name` → `"name"`; simple-text children in the object's own module namespace whose local name is one of `class, function, usage, yearOfConstruction, yearOfDemolition, roofType, storeysAboveGround, storeysBelowGround, storeyHeightsAboveGround, storeyHeightsBelowGround, measuredHeight, averageHeight, species, height, trunkDiameter, crownDiameter, usage, function` — numeric-looking values (`measuredHeight`, `storeys*`, `yearOf*`, heights, diameters) parsed to JSON numbers (integer where the CityGML type is integer/year), rest strings; and `gen:stringAttribute/intAttribute/doubleAttribute/dateAttribute/uriAttribute/measureAttribute` (child `gen:value`, attr `name`) with matching JSON types (date/uri/measure-unit → string; measure numeric value → number).
- Duplicate attribute names: last wins, warning recorded.

- [ ] **Step 1: Failing tests** — unit tests: each gen: type maps to the right JSON type; `measuredHeight` with `uom` attr → number; year → integer. Fixture: a Building with `measuredHeight 9.5`, `yearOfConstruction 1985`, `roofType "1030"`, `function "1000"`, gen attributes of all six kinds; expected JSONL includes `"attributes":{…}` exactly.
- [ ] **Step 2: FAIL. Step 3: Implement. Step 4: PASS. Step 5: Commit** — `git commit -m "feat(citygml): core and generic attribute mapping"`

### Task 8: Semantic surfaces

**Files:** Modify `building.rs`; fixture pair `semantic_surfaces.gml` / `.expected.city.jsonl`.

**Interfaces:**
- Produces: `boundedBy` thematic surfaces on Building(Part): `bldg:WallSurface|RoofSurface|GroundSurface|ClosureSurface|OuterCeilingSurface|OuterFloorSurface` and openings `bldg:Door|Window` (openings nested under a surface's `opening` property become their own `SemanticSurface` entries). Each thematic surface contributes: one `SemanticSurface { stype, attributes }` per surface object into the geometry's `surfaces` vec; its polygons get `sem_idx = Some(i)`. The standard CityGML pattern — `lod2Solid` referencing boundary polygons by xlink while `boundedBy` holds the actual polygons — must work: the Solid's xlink-resolved polygons carry the `sem_idx` assigned where they were defined. Implementation approach: parse `boundedBy` surfaces first into (polygon gml:id → sem_idx) plus polygon list per LoD (`lod2MultiSurface` inside the thematic surface: `lodXMultiSurface` property); then parse `lodXSolid`; polygons resolved via xlink inherit `sem_idx` by looking up their gml_id in that map.
- In `convert.rs`: geometry with any `sem_idx` present → `semantics` with `surfaces` (type + attributes flattened per cjseq `Semantics` types — read `cjseq/src/semantics.rs` for exact shape) and `values` nested to match the geometry depth (Solid: per-shell arrays of per-polygon `Option<usize>`; MultiSurface: per-polygon).

- [ ] **Step 1: Fixture** — LoD2 building: `boundedBy` with 1 RoofSurface (2 polygons), 4 WallSurface, 1 GroundSurface, each polygon with gml:id; `lod2Solid` referencing all 7 polygons via xlink. Expected JSONL: Solid + `semantics.surfaces` (6 entries, boundedBy order) + `values` `[[0,0,1,2,3,4,5]]`-style (hand-derived). Failing test via `assert_fixture("semantic_surfaces")`.
- [ ] **Step 2: FAIL. Step 3: Implement. Step 4: PASS. Step 5: Commit** — `git commit -m "feat(citygml): semantic surface mapping with xlink'd solid boundaries"`

### Task 9: Building hierarchy (parts, installations)

**Files:** Modify `building.rs`, `convert.rs`; fixture pair `building_hierarchy.gml` / `.expected.city.jsonl`.

**Interfaces:**
- Produces: `bldg:consistsOfBuildingPart → bldg:BuildingPart` (recursive: same reader as Building) and `bldg:outerBuildingInstallation → bldg:BuildingInstallation` (geometry via `lodXGeometry`) as `IntermediateObject.children`. `convert.rs` flattens the tree into the feature's `city_objects`: child ids get `parents: vec![parent_id]`, parent gets `children: vec![…]` in document order; feature `id` stays the root's id. Child without gml:id → `"{parent_id}-part-{n}"` / `"{parent_id}-inst-{n}"`.

- [ ] **Step 1: Fixture** — Building `b1` (no own geometry) with two LoD1 BuildingParts and one BuildingInstallation; expected: 4 CityObjects in one feature line, correct `parents`/`children`, vertices pooled across all objects in the feature. Failing test.
- [ ] **Step 2: FAIL. Step 3: Implement. Step 4: PASS. Step 5: Commit** — `git commit -m "feat(citygml): BuildingPart and BuildingInstallation hierarchy"`

### Task 10: Remaining simple thematic modules

**Files:** Create `src/rust/fcb_citygml/src/citygml/simple.rs` (veg, tran, wtr, luse, frn, gen, grp); modify `citygml/mod.rs` dispatch. Fixture pair `thematic_modules.gml` / `.expected.city.jsonl`.

**Interfaces:**
- Produces mapping (namespace family → cjseq `CityObjectType`): `veg:SolitaryVegetationObject`, `veg:PlantCover` → `PlantCover`/`SolitaryVegetationObject`; `tran:Road|Railway|TransportSquare` (geometry `lodXMultiSurface`, semantic children `tran:TrafficArea`/`AuxiliaryTrafficArea` → semantic surfaces `TrafficArea`/`AuxiliaryTrafficArea`); `wtr:WaterBody` (+`wtr:WaterSurface|WaterGroundSurface|WaterClosureSurface` boundedBy → semantics); `luse:LandUse`; `frn:CityFurniture`; `gen:GenericCityObject` → `CityObjectType::Extension("+GenericCityObject".into())`; `grp:CityObjectGroup` with `grp:groupMember xlink:href="#id"` → `children` ids + `children_roles` from the `role` attr (None when absent) — members that are inline rather than href'd: warn + skip member. All reuse `read_common_attributes` and the generic `lodX*` geometry scan (extract that scan into `citygml/mod.rs::read_lod_geometries(node, registry, report) -> Vec<IntermediateGeometry>` during this task's refactor step).
- CityObjectGroup children referencing objects in *other* top-level members: ids are kept verbatim (features are separate lines; cross-feature references are legal in CityJSONSeq via id).

- [ ] **Step 1: Fixture** — one member of each kind (LoD1 or LoD2 MultiSurface geometries; a Road with two TrafficAreas; a group referencing two of the others with one role). Expected file: one feature line per top-level member, document order. Failing test.
- [ ] **Step 2: FAIL. Step 3: Implement (+ the `read_lod_geometries` refactor; re-run ALL tests). Step 4: PASS. Step 5: Commit** — `git commit -m "feat(citygml): vegetation, transport, water, landuse, furniture, generic, group modules"`

### Task 11: TINRelief, Bridge and Tunnel families

**Files:** Create `src/rust/fcb_citygml/src/citygml/relief.rs`, extend `building.rs`-style readers via `src/rust/fcb_citygml/src/citygml/construction.rs` (bridge/tunnel share the Building reader shape: parts, installations, boundedBy). Fixture pairs `tin_relief.gml`, `bridge_tunnel.gml` + expected files.

**Interfaces:**
- Produces: `dem:ReliefFeature` → skipped-with-warning wrapper handling: its `dem:reliefComponent` `dem:TINRelief` members each become their own feature (`TINRelief`); a bare `dem:TINRelief` member also works. TIN geometry: `dem:tin` → `gml:TriangulatedSurface|gml:Tin` → `gml:trianglePatches` → `gml:Triangle` (exterior ring, exactly 3 distinct points after repair) → `CompositeSurface` in CityJSON with `lod` from `dem:lod` child element text (default "1"). New gml support: `parse_triangles(&XmlNode, &mut ParseReport) -> Vec<Polygon3>` in `gml`.
- `brid:Bridge`→`Bridge`, `brid:BridgePart`, `brid:BridgeInstallation`, `brid:BridgeConstructionElement`→`BridgeConstructiveElement`; `tun:Tunnel`/`TunnelPart`/`TunnelInstallation`; boundedBy surface set same local names as building (Roof/Wall/Ground/Closure/OuterCeiling/OuterFloor + Door/Window in their namespaces).

- [ ] **Step 1: Fixtures + failing tests** (two `assert_fixture` tests). TIN fixture: 2 triangles; bridge fixture: Bridge with one BridgePart (LoD1 solid) and boundedBy wall on the part.
- [ ] **Step 2: FAIL. Step 3: Implement. Step 4: PASS. Step 5: Commit** — `git commit -m "feat(citygml): TINRelief, Bridge and Tunnel families"`

### Task 12: ImplicitGeometry flattening

**Files:** Create `src/rust/fcb_citygml/src/gml/implicit.rs`; wire into `read_lod_geometries` (`lodXImplicitRepresentation` properties on veg/frn objects, `core:ImplicitGeometry` child). Fixture pair `implicit_geometry.gml` / `.expected.city.jsonl` + inline matrix unit tests.

**Interfaces:**
- Produces: `pub(crate) fn flatten_implicit(node: &XmlNode, registry: &XlinkRegistry, report: &mut ParseReport) -> Result<Option<GmlGeometry>, CityGmlError>`. Reads `core:transformationMatrix` (16 floats, **row-major 4×4** per CityGML 2.0 §10.2), `core:referencePoint` (`gml:Point/gml:pos`), `core:relativeGMLGeometry` (inline geometry or xlink). Each template point `p` maps to `M · [p,1] + referencePoint` (matrix already includes scale/rotation/translation; reference point added after). Unit test: identity matrix + reference point (10,20,30) translates a template cube; a scale-2 diagonal matrix doubles it.
- Templates referenced by xlink from multiple members: each use flattens independently (duplication accepted, per spec).

- [ ] **Step 1: Failing tests** (matrix unit tests + fixture: SolitaryVegetationObject with lod2ImplicitRepresentation, identity rotation, scale 2, refpoint offset; expected JSONL hand-computed).
- [ ] **Step 2: FAIL. Step 3: Implement. Step 4: PASS. Step 5: Commit** — `git commit -m "feat(citygml): ImplicitGeometry flattening via transformation matrix"`

### Task 13: Appearance — materials

**Files:** Create `src/rust/fcb_citygml/src/appearance.rs`; modify `lib.rs` (feed collected `appearanceMember` subtrees to `parse_appearances`, pass result to `convert`); modify `convert.rs` (join materials to polygons). Fixture pair `material.gml` / `.expected.city.jsonl` + unit tests.

**Interfaces:**
- Produces: `parse_appearances` + `SurfaceData::Material` per "Shared interfaces". CityGML structure: `app:appearanceMember → app:Appearance → app:theme` (default theme name `""` → use `"default"`) `+ app:surfaceDataMember → app:X3DMaterial` with `app:diffuseColor` ("r g b" floats), `app:ambientIntensity`, `app:specularColor`, `app:emissiveColor`, `app:shininess`, `app:transparency`, `app:isSmooth`, and 1..n `app:target>#polygonId</app:target>`. Map to `cjseq::MaterialObject` (name: `gml:name` child or `"material-{n}"`).
- `convert.rs` join: per geometry, per theme: if any of its polygons' gml:ids are targeted, emit `material: {theme: MaterialReference{values: nested Option<usize> matching geometry depth}}` (cjseq `MaterialValues`; surfaces without material → `None`). Materials pooled at **feature** level (`CityJSONFeature.appearance.materials`), indices feature-local; identical MaterialObjects deduped by equality.
- Appearance parsed but targeting no polygon in any feature → warning. `app:appearance` nested inside a CityObject (per-feature appearance, legal in CityGML) is also collected — `XlinkRegistry`-style: collect appearance nodes from both the CityModel level (Task 5 stash) and inside each member subtree.

- [ ] **Step 1: Failing tests** — unit: X3DMaterial node → MaterialObject fields; fixture: LoD2 building with 2 themes ("summer" material red roof, walls gray) targeting semantic-surface polygons; expected JSONL includes `appearance.materials` + per-geometry `material` values. Hand-derive indices.
- [ ] **Step 2: FAIL. Step 3: Implement. Step 4: PASS. Step 5: Commit** — `git commit -m "feat(citygml): X3DMaterial appearance mapping"`

### Task 14: Appearance — textures

**Files:** Modify `appearance.rs`, `convert.rs`. Fixture pair `texture.gml` / `.expected.city.jsonl`.

**Interfaces:**
- Produces: `SurfaceData::Texture` per "Shared interfaces". CityGML: `app:ParameterizedTexture` with `app:imageURI` (copied verbatim), `app:mimeType` → `TextureObject.thetype` (PNG/JPG from mime; unknown → warn+skip surface data), `app:wrapMode`, and per-target `app:textureAssociation`/direct `app:target uri="#polyId"` containing `app:TexCoordList` with `app:textureCoordinates ring="#ringId"` (flat UV list, pairs). `GeoreferencedTexture`/`TexCoordGen` → warn + skip (report).
- `convert.rs` join: pool `TextureObject`s and `vertices-texture` (UV pairs, deduped exact-match) per feature; per polygon ring, `texture` values per theme: `[Some(texture_idx), uv_idx_0, …, uv_idx_n-1]`-shaped nested arrays per cjseq `TextureValues` (read `cjseq/src/appearance.rs` for the exact nesting — depth is geometry depth + 1, innermost = `[texture?, per-vertex uv…]`). UV count must equal ring point count **after ring repair**: when repair dropped points (closure), drop the corresponding trailing UVs; mismatch otherwise → warn + skip texture for that ring.
- Ring-id map: `Ring.gml_id` recorded in Task 3 exists for exactly this join.

- [ ] **Step 1: Failing tests** — fixture: building with one textured roof polygon (4 UVs for 4-point ring, 5 in file with closure), one theme; expected JSONL with `appearance.textures`, `appearance."vertices-texture"`, geometry `texture` values. Include one `GeoreferencedTexture` that must land in the report, not the output.
- [ ] **Step 2: FAIL. Step 3: Implement. Step 4: PASS. Step 5: Commit** — `git commit -m "feat(citygml): ParameterizedTexture appearance mapping"`

### Task 15: CLI integration + FCB round-trip test

**Files:**
- Modify: `src/rust/cli/Cargo.toml` (add `fcb_citygml = { path = "../fcb_citygml" }` via workspace dep entry in root `Cargo.toml`), `src/rust/cli/src/reader.rs`, `src/rust/cli/src/main.rs` (Ser input help text: mention `.gml`), `src/rust/Cargo.toml` (`fcb_citygml = { path = "fcb_citygml", version = "0.1.0" }` under `[workspace.dependencies]`).
- Test: `src/rust/cli/tests/citygml_ser.rs`

**Interfaces:**
- Consumes: `fcb_citygml::{parse_citygml, ParseOptions}`.
- Produces: `InputFormat::CityGML` for extensions `gml`/`xml`; `read_input_file` arm:

```rust
InputFormat::CityGML => {
    let file = File::open(path)?;
    let (doc, report) = fcb_citygml::parse_citygml(BufReader::new(file), &ParseOptions::default())
        .map_err(|e| CliError::CityGml(path.display().to_string(), e.to_string()))?;
    for s in &report.skipped {
        tracing::warn!(file = %path.display(), element = %s.element, reason = %s.reason, "skipped CityGML element");
    }
    for w in &report.warnings { tracing::warn!(file = %path.display(), "{w}"); }
    if !report.skipped.is_empty() {
        eprintln!("  ⚠ {}: skipped {} unsupported element(s)", path.display(), report.skipped.len());
    }
    Ok(InputData { metadata: doc.metadata, features: doc.features })
}
```

plus a `CliError::CityGml(String, String)` variant (`#[error("CityGML parse error in {0}: {1}")]`).

- [ ] **Step 1: Failing integration test** — `cli/tests/citygml_ser.rs`: copy `fcb_citygml/tests/fixtures/semantic_surfaces.gml` (via `include_str!` + tempfile), run the library path the CLI uses: `read_input_file` on the `.gml`, feed through `fcb_core` writer (mirror what `serialize()` does — see `cli/src/main.rs` for the writer setup; use default options, temp output), then read the `.fcb` back with `fcb_core`'s reader + `deserializer::to_cj_feature` and assert the decoded feature equals `semantic_surfaces.expected.city.jsonl`'s feature line as `serde_json::Value` **modulo transform**: dequantize both sides to real coordinates (± 1e-6) before comparing `vertices`, compare everything else exactly. Also test: glob of two `.gml` files merges into 2+ features.
- [ ] **Step 2: FAIL (no CityGML variant). Step 3: Implement. Step 4: PASS; also run full `just test` in `src/rust`. Step 5: Commit** — `git commit -m "feat(cli): CityGML 2.0 input support for fcb ser"`

### Task 16: citygml-tools cross-check corpus

**Files:**
- Create: `src/rust/fcb_citygml/tests/xcheck.rs`, `src/rust/fcb_citygml/tests/xcheck/` (corpus: `<name>.gml` + `<name>.citygml-tools.city.json` per sample), `src/rust/fcb_citygml/tests/xcheck/README.md` (regeneration procedure).
- Java 21 is available at `/usr/bin/java`; citygml-tools: download release zip from `https://github.com/citygml4j/citygml-tools/releases` (latest 2.x) into the session scratchpad, never into the repo.

**Interfaces:**
- Consumes: `parse_citygml`.
- Produces: structural comparison harness `fn assert_structural_match(ours: &CityGmlDocument, reference: &serde_json::Value)` checking per spec: same CityObject id set; per object: same type, same parents/children sets, same attribute map (values compared loosely: numbers ±1e-9, strings exact), same geometry count per lod, per geometry same type + polygon count (+ shell count), same semantics surface types per polygon, and every dequantized vertex of ours must have a counterpart in the reference within 2×scale Euclidean distance (sample up to 500 vertices per object for speed). Appearance: same material names set and texture image set when present.

- [ ] **Step 1: Build corpus.** Download 2–3 small CityGML 2.0 samples from datasets linked at `https://www.cityjson.org/datasets/` (choose small extracts, ≤ ~5 MB each, e.g. a DenHaag tile and a Railway/FZK-Haus sample from the CityGML test datasets). If a file is large, cut it down to the first N `cityObjectMember`s with a scripted XML-aware truncation (keep Envelope + appearance members). Run `citygml-tools to-cityjson --cityjson-version=2.0 <file>` (in scratchpad) and commit the outputs beside the inputs. Document exact commands + URLs in the README.
- [ ] **Step 2: Failing test** — one `#[test]` per sample calling the harness. Run: expect failures that are *our* gaps; fix converter bugs they reveal (each fix = its own minimal red/green cycle with a unit fixture if practical). Structural-comparison looseness may be tuned only where citygml-tools makes a legitimately different choice (document each tolerance in a comment citing the difference).
- [ ] **Step 3: All xcheck tests PASS. Step 4: Commit** — `git commit -m "test(citygml): citygml-tools cross-check corpus and structural harness"`

### Task 17: Docs + final gate

**Files:**
- Modify: `README.md` (CLI usage: CityGML input example), `src/rust/cli/src/main.rs` doc comment for `Ser`, `.llm/docs/projectStructure.md` (add fcb_citygml).
- No changes to `docs/specification.md` (FCB format unchanged).

- [ ] **Step 1: Write docs** (usage: `fcb ser -i "tiles/*.gml" -o city.fcb`; note CityGML 2.0 only, appearance supported, reprojection not).
- [ ] **Step 2: Full gate** — from `src/rust`: `just check` (must be clean: fmt, clippy, tests, build). From repo root: `just test` (other languages unaffected but must still pass).
- [ ] **Step 3: Commit** — `git commit -m "docs: CityGML 2.0 input documentation"`
- [ ] **Step 4:** Delete the plan and spec per project convention **only after user review** — leave both files in place; the user decides after reading the final report.

## Self-Review Notes (already applied)

- Spec coverage: every spec section maps to a task (scaffold/CRS T1–2, geometry T3–4+11–12, model/thematic T5+7–11, convert T6, appearance T13–14, CLI T15, oracle T16, docs T17). Address parsing (`xal`) is not in the spec — out of scope, unrecognized children are silently ignored at attribute level (not reported: too noisy).
- Type consistency: all cross-task names come from "Shared interfaces"; implementers must not rename without updating the plan.
- `parse_to_model` is public for tests (Task 5) — acceptable; documented as `#[doc(hidden)]`.
