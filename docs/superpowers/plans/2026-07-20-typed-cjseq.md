# Typed cjseq Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace cjseq's untyped `serde_json::Value` geometry with types that encode CityJSON's nesting depth, so a wrong-depth document fails to deserialize instead of silently decoding wrong — then migrate `fcb_core` onto it and retire the `cjseq2` fork.

**Architecture:** `Geometry` becomes an internally-tagged enum whose variants carry depth-correct boundaries (`Ring`/`Surface`/`Shell` aliases). Semantics and appearance values follow the same depth ladder. `fcb_core`'s FlatBuffers decoder then reads nesting depth from the geometry type — which is stored in the file — instead of inferring it from which count arrays happen to be populated, which is what caused two round-trip bugs fixed in this session.

**Tech Stack:** Rust 2021, serde/serde_json, criterion (existing benches), doctest; the downstream consumers are `fcb_core` (Rust), the C++ reader (`src/cpp`), and the WASM/npm package.

---

## READ THIS FIRST: context from the session that produced this plan

A fresh session has none of this. It is all load-bearing.

### The three repos/crates involved

| Thing | Where | State as of 2026-07-20 |
|---|---|---|
| upstream cjseq | `github.com/cityjson/cjseq`, branch `main` | `e3b198a`, crate `cjseq` 0.4.1 |
| the fork | `github.com/HideBa/cjseq` (git remote spells it `hideba`) | `main` is **identical** to upstream `e3b198a` |
| fork's typed lineage | same fork, branch `develop` | crate renamed `cjseq2` 0.1.0; **41 ahead of `main`, 30 BEHIND** |
| published fork crate | crates.io `cjseq2` 0.1.1 | cut from `fix/option-usize-serialization` = published 0.1.0 + one fix |
| local clone | `~/tudelft/cityjson/cjseq` | has `upstream` remote added; local `develop` has an **unpushed merge** `4415c79` |
| consumer | `~/tudelft/cityjson/flatcitybuf`, branch `develop` | depends on `cjseq2 = "0.1.1"` from crates.io |

**Correction to a common assumption:** the fork's `main` is NOT stale — it matches upstream exactly. The staleness is in the `cjseq2` lineage (`develop`), which forked off long ago and never merged `main`'s later 30 commits. Since the published 0.1.1 sits on that lineage, `fcb_core` today runs on a base missing those 30 upstream commits. They are almost entirely CLI/WASM/CI/doc work (globbing, multi-CityJSONSeq stdin, geographical-extent updates, WASM bindings, publish workflows, first unit tests) — **no type changes** — so rebuilding the type work on top of current `main` is the right move rather than merging `main` into `develop`.

**`develop` does not compile against `fcb_core`.** Commit `759b86a` ("introduce CityObjectType and SemanticSurfaceType enums") retyped `thetype` on `CityJSON`, `CityObject` and `SemanticsSurface` from `String` to enums. `fcb_core` still assigns strings, giving five `E0308` errors. That is why 0.1.1 was cut from the published 0.1.0 tree plus one fix, not from `develop` HEAD. Task 9 is where `fcb_core` finally absorbs that change.

### What this session already fixed (do not redo)

Five commits on `flatcitybuf` `develop` (`692f7d3`, `c63b2b0`, `ac46026`, `af6abc4`, `0a7fd0e`):

1. **The C++ reader gained appearance + geometry-template decoding**, ported from `fcb_core/src/reader/geom_decoder.rs`. `src/cpp/src/geometry.cpp` now has `decode_material_values` / `decode_texture_values`.
2. **cjseq2 0.1.1** fixed `impl JsonIndex for Option<usize>::to_value`, which used `Option::iter` and wrapped every material/texture index in a one-element array (`Some(1)` → `[1]`, `None` → `[]`), emitting CityJSON that validators reject. Written up as finding #7 in `docs/upstream-findings.md`.
3. **Two round-trip data losses fixed in both readers** (finding #8): material values on a Solid with exactly one shell, and texture values on a single-string MultiLineString, each came back one nesting level deeper than written. Two further quirks were *proved* unreachable from our own writer and documented rather than changed.
4. **The npm/WASM build stopped committing generated artifacts** (`src/ts/` now holds only `package.json`; `scripts/build_wasm.sh` is the one build path).

**The deepest lesson, and the reason this plan exists:** those bugs were invisible to every test that compared the reader against the reference reader's own output, because both agreed on the wrong answer. Only a round trip through the *writer* caught them (`src/rust/fcb_core/tests/appearance_roundtrip.rs`). The root cause is that nesting depth is inferred at decode time from which count arrays are populated, rather than being determined by the geometry type. This plan removes that inference.

### Known gotchas that will bite

- **`Cargo.lock` is gitignored** in flatcitybuf (`.gitignore:28`) and has never been tracked. Do not "fix" it into a commit.
- **`.fcb` files are now tracked** (`.gitignore`'s `!conformance/*.fcb` negation, added in `a20f371`, since CI has no Rust toolchain to regenerate them on a clean checkout). The conformance corpus tracks `conformance/*.fcb` and `*.expected.jsonl` directly, alongside `inputs/*.city.jsonl`; `scripts/gen_conformance.sh` regenerates them but regeneration is not byte-reproducible (`cjseq2` iterates CityObjects from a `HashMap`), so a regeneration diff is expected churn, not a signal.
- **Regenerating fixtures produces key-order churn** even when nothing semantic changed (serde `HashMap` iteration). Always diff *parsed* JSON before committing a fixture change; revert files whose JSON is equal.
- **`[patch.crates-io]` with a relative path breaks fresh clones.** It was used briefly this session and removed in `c63b2b0`. If you need it again while iterating, remove it before the final commit.
- **`geom_temp` exercises neither of the two fixed branches**, so a green conformance run does not prove appearance correctness. Round-trip tests do.
- Two decode quirks remain by design (documented, unreachable from our writer): textures skipping the shell branch when `shells.len() > 1`, and the MultiLineString branch iterating `surfaces[0]` rather than `strings.len()`. Task 8 makes both moot.

### Working conventions to keep

- **TDD, strictly.** Write the failing test, run it, watch it fail for the *right* reason, implement, watch it pass, commit. Several steps below say "verify it fails" — do not skip them; a test that never failed has proved nothing.
- **Use Fable as an advisor for the hard analytical passes** (`Agent` tool, `model: "fable"`). It did two jobs well this session: producing an exact branch-condition spec from Rust source before the C++ port, and the round-trip investigation that classified the four quirks. Give it a narrow question, forbid it from changing behaviour, and demand concrete evidence (printed arrays, side-by-side JSON) rather than conclusions.
- **Get a code review from codex before declaring a stage done:**
  `codex exec --model gpt-5.6-sol --sandbox read-only "<focused prompt>"`.
  It found three real defects this session (an empty-vector divergence, a null-pointer deref, missing coverage) and no false positives. Give it the same context a human reviewer would need.
- **The oracle technique** (use it whenever porting behaviour between the two readers): temporarily inject a test into the Rust source that prints the reference function's actual output for each case, run it, pin those values in the port's tests, then revert the injection. Do not hand-derive expected values — this session caught one wrong hand-derivation that way.

---

## Global Constraints

- Rust edition 2021. Keep the crate `no_std`-free but WASM-compatible: `develop` already builds for `wasm32-unknown-unknown` and that must keep working.
- **Breaking changes are explicitly allowed**, including to the FlatCityBuf data format. The project is experimental. Do not contort a design to preserve compatibility.
- **Never hand-write `Serialize`/`Deserialize` for an index type.** Derive it, or use `#[serde(untagged)]`/`#[serde(tag)]`. The one hand-written impl in this codebase's history (`JsonIndex for Option<usize>`) shipped invalid CityJSON for months.
- Output must validate against the CityJSON 2.0 spec: material values are integers or `null`, never single-element arrays; texture rings are `[texture_index, uv_index, ...]`.
- Every public type derives `Debug, Clone, PartialEq` (fork already does; upstream partly does not — keep it).
- Target crate name for the fork releases: `cjseq2`. Final destination: upstream `cjseq`.

---

## File Structure

`src/lib.rs` in cjseq is ~2000 lines holding every type plus the CLI-facing logic. Split it as the type work lands — files that change together, and small enough to hold in context:

- `src/lib.rs` — re-exports only, plus crate docs.
- `src/error.rs` — `CjseqError`, `Result` (already exists on `develop` as part of `5a8102c`; port it).
- `src/geometry.rs` — `Geometry` enum, `Ring`/`Surface`/`Shell` aliases, `GeometryType`, `GeometryTemplates`.
- `src/semantics.rs` — `Semantics`, `SemanticsSurface`, `SemanticSurfaceType`, semantics values.
- `src/appearance.rs` — `Appearance`, `MaterialObject`, `TextureObject`, `MaterialReference`, `TextureReference`, `TextFormat`, `WrapMode`, `TextType`.
- `src/city_object.rs` — `CityObject`, `CityObjectType`.
- `src/metadata.rs` — `Metadata`, `PointOfContact`, `Address`, `ReferenceSystem`, `Transform`, `GeographicalExtent`.
- `src/cityjson.rs` — `CityJSON`, `CityJSONFeature`, `CityJSONType`, the sequencing/collect/filter logic that currently lives in `lib.rs`.
- `tests/roundtrip.rs` — real-file round trips (new).
- `tests/depth_rejection.rs` — wrong-depth documents must fail to deserialize (new).

---

## The type design

The flaw shared by both existing designs: nesting depth is not in the type.

- Upstream: `boundaries: Value` — no checking at all.
- Fork (`cjseq2`): `NestedArray<T>` is a shape-agnostic recursive enum. It can hold *any* depth, so nothing rejects a `MultiSurface` whose boundaries are 4 deep, and a decoder that guesses depth wrong produces a value that type-checks perfectly.

**Put the depth in the variant.** CityJSON already tags geometry by type, so an internally-tagged enum maps 1:1 onto the wire format:

```rust
pub type Ring    = Vec<usize>;
pub type Surface = Vec<Ring>;
pub type Shell   = Vec<Surface>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Geometry {
    MultiPoint      { lod: Option<String>, boundaries: Ring,           #[serde(flatten)] common: GeometryCommon },
    MultiLineString { lod: Option<String>, boundaries: Surface,        #[serde(flatten)] common: GeometryCommon },
    MultiSurface    { lod: Option<String>, boundaries: Shell,          #[serde(flatten)] common: GeometryCommon },
    CompositeSurface{ lod: Option<String>, boundaries: Shell,          #[serde(flatten)] common: GeometryCommon },
    Solid           { lod: Option<String>, boundaries: Vec<Shell>,     #[serde(flatten)] common: GeometryCommon },
    MultiSolid      { lod: Option<String>, boundaries: Vec<Vec<Shell>>,#[serde(flatten)] common: GeometryCommon },
    CompositeSolid  { lod: Option<String>, boundaries: Vec<Vec<Shell>>,#[serde(flatten)] common: GeometryCommon },
    GeometryInstance{ boundaries: Vec<usize>, template: usize, transformation_matrix: [f64; 16] },
}
```

Appearance values ride the same ladder, one level shallower for materials (one index per surface, not per ring) and exactly parallel for textures (leaf = `[texture_index, uv...]`):

```rust
pub enum MaterialValues {                 pub enum TextureValues {
    Surfaces(Vec<Option<usize>>),             Surface(Vec<Vec<Option<usize>>>),
    Shells(Vec<Vec<Option<usize>>>),          Shell(Vec<Vec<Vec<Option<usize>>>>),
    Solids(Vec<Vec<Vec<Option<usize>>>>),     Solid(Vec<Vec<Vec<Vec<Option<usize>>>>>),
}                                         }
```

`Option<usize>` stays — CityJSON genuinely uses `null` for "no material here" — but **derived**, so `None` serializes as `null` and nothing else.

**Known trade-offs, accept them knowingly:**
- `#[serde(tag)]` buffers the input, so it is slower than an externally-tagged enum and incompatible with `deny_unknown_fields`. Fine here: CityJSON documents are read once and the spec allows extra members.
- Deserialize errors on deeply nested `Vec` become verbose ("invalid type: integer, expected a sequence"). Task 4 adds a wrapping error that names the geometry type and the expected depth.
- A `MultiSolid` containing exactly one solid and a `Solid` are structurally distinguishable now (different variants), which the FlatBuffers encoding could not express before. Task 8 must store the geometry type in the mapping or read it from the enclosing `Geometry` table — it already does the latter.

---

## Task 1: Branch off current upstream main, split lib.rs

**Files:**
- Create: `src/error.rs`, `src/geometry.rs`, `src/semantics.rs`, `src/appearance.rs`, `src/city_object.rs`, `src/metadata.rs`, `src/cityjson.rs` (all in `~/tudelft/cityjson/cjseq`)
- Modify: `src/lib.rs` (becomes re-exports + docs), `Cargo.toml` (name `cjseq2`, version `0.2.0-alpha.0`)

**Interfaces:**
- Produces: every public type currently exported from `cjseq` 0.4.1, re-exported from `lib.rs` at the same paths, so `use cjseq::CityJSON` keeps working.

- [ ] **Step 1: Create the branch off upstream main, not off develop**

```bash
cd ~/tudelft/cityjson/cjseq
git fetch upstream && git fetch origin
git checkout -b feat/typed-cityjson upstream/main
git log --oneline -1          # expect e3b198a
```

- [ ] **Step 2: Confirm the baseline is green before touching anything**

Run: `cargo test`
Expected: PASS (upstream `main` has ~15 tests). Record the count; it must not drop.

- [ ] **Step 3: Set crate identity**

In `Cargo.toml`: `name = "cjseq2"`, `version = "0.2.0-alpha.0"`, keep `description` noting it is a typed fork of cjseq pending upstream merge.

- [ ] **Step 4: Move types into modules, no behaviour change**

Cut and paste only. Each new module gets `use super::*;`-free explicit imports. `lib.rs` ends with `pub use {error::*, geometry::*, semantics::*, appearance::*, city_object::*, metadata::*, cityjson::*};`

- [ ] **Step 5: Verify nothing changed**

Run: `cargo test`
Expected: same test count, all PASS. Then `cargo public-api diff` if available, or eyeball that `lib.rs`'s re-exports cover every previously-public name.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "refactor: split lib.rs into per-concern modules

Pure code motion off upstream main, no behaviour change, as the base for
the typed rewrite."
```

---

## Task 2: Depth-typed boundaries

**Files:**
- Modify: `src/geometry.rs`
- Test: `tests/depth_rejection.rs` (create), plus unit tests in `src/geometry.rs`

**Interfaces:**
- Consumes: Task 1's module layout.
- Produces: `pub type Ring = Vec<usize>; pub type Surface = Vec<Ring>; pub type Shell = Vec<Surface>;` and the `Geometry` enum exactly as sketched in "The type design" above. `Geometry::geometry_type(&self) -> GeometryType` for callers that need the tag without matching.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/depth_rejection.rs
use cjseq2::Geometry;

#[test]
fn multisurface_accepts_three_levels() {
    let g: Geometry = serde_json::from_value(serde_json::json!({
        "type": "MultiSurface", "lod": "2",
        "boundaries": [[[0, 3, 2, 1]], [[4, 5, 6, 7]]]
    })).expect("valid MultiSurface must deserialize");
    match g {
        Geometry::MultiSurface { ref boundaries, .. } => assert_eq!(boundaries.len(), 2),
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn multisurface_rejects_solid_depth() {
    // One level too deep for a MultiSurface. Previously decoded happily.
    let r: Result<Geometry, _> = serde_json::from_value(serde_json::json!({
        "type": "MultiSurface", "lod": "2",
        "boundaries": [[[[0, 3, 2, 1]]]]
    }));
    assert!(r.is_err(), "wrong-depth boundaries must not deserialize");
}

#[test]
fn solid_roundtrips_through_json() {
    let input = serde_json::json!({
        "type": "Solid", "lod": "2",
        "boundaries": [[[[0, 3, 2, 1]], [[4, 5, 6, 7]]]]
    });
    let g: Geometry = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(serde_json::to_value(&g).unwrap(), input);
}
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test --test depth_rejection`
Expected: FAIL — `Geometry` is still a struct with `boundaries: Value`, so `multisurface_rejects_solid_depth` fails (it accepts) and the variant match does not compile.

- [ ] **Step 3: Implement the enum**

Replace the `Geometry` struct in `src/geometry.rs` with the internally-tagged enum from "The type design". Keep `GeometryType` as a standalone enum for callers that only need the tag, and add:

```rust
impl Geometry {
    pub fn geometry_type(&self) -> GeometryType {
        match self {
            Geometry::MultiPoint { .. } => GeometryType::MultiPoint,
            Geometry::MultiLineString { .. } => GeometryType::MultiLineString,
            Geometry::MultiSurface { .. } => GeometryType::MultiSurface,
            Geometry::CompositeSurface { .. } => GeometryType::CompositeSurface,
            Geometry::Solid { .. } => GeometryType::Solid,
            Geometry::MultiSolid { .. } => GeometryType::MultiSolid,
            Geometry::CompositeSolid { .. } => GeometryType::CompositeSolid,
            Geometry::GeometryInstance { .. } => GeometryType::GeometryInstance,
        }
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --test depth_rejection && cargo test`
Expected: all PASS. Upstream's own tests will need mechanical updates where they construct `Geometry`; that is expected and in scope for this task.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat!: put nesting depth in the Geometry type

BREAKING: Geometry is now an internally-tagged enum whose variants carry
depth-correct boundaries. A document whose boundaries do not match its
declared type no longer deserializes."
```

---

## Task 3: Depth-typed semantics values

**Files:**
- Modify: `src/semantics.rs`, `src/geometry.rs` (semantics field types per variant)
- Test: unit tests in `src/semantics.rs`

**Interfaces:**
- Consumes: `Ring`/`Surface`/`Shell` from Task 2.
- Produces: `Semantics { surfaces: Vec<SemanticsSurface>, values: SemanticsValues }` where `SemanticsValues` is depth-typed the same way boundaries are, one level shallower (one index per surface).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn solid_semantics_values_are_one_per_surface_per_shell() {
    let s: Semantics = serde_json::from_value(serde_json::json!({
        "surfaces": [{"type": "RoofSurface"}, {"type": "WallSurface"}],
        "values": [[0, 1, null]]
    })).unwrap();
    assert_eq!(serde_json::to_value(&s).unwrap()["values"],
               serde_json::json!([[0, 1, null]]));
}

#[test]
fn null_semantics_index_stays_null() {
    let s: Semantics = serde_json::from_value(serde_json::json!({
        "surfaces": [{"type": "RoofSurface"}], "values": [null, 0]
    })).unwrap();
    // Must be `null`, never `[]` -- see finding #7.
    assert_eq!(serde_json::to_value(&s).unwrap()["values"],
               serde_json::json!([null, 0]));
}
```

- [ ] **Step 2: Run and watch fail** — `cargo test semantics`
- [ ] **Step 3: Implement** the depth-typed `SemanticsValues` with derived serde only.
- [ ] **Step 4: Run** — `cargo test`, expect PASS.
- [ ] **Step 5: Commit** — `feat!: depth-type semantics values`

---

## Task 4: Depth-typed appearance values, and a readable depth error

**Files:**
- Modify: `src/appearance.rs`, `src/error.rs`
- Test: unit tests in `src/appearance.rs`

**Interfaces:**
- Produces: `MaterialValues` / `TextureValues` as sketched above; `MaterialReference { value: Option<usize>, values: Option<MaterialValues> }`; `TextureReference { values: TextureValues }`. `CjseqError::GeometryDepth { geometry_type: GeometryType, expected: usize, found: String }`.

- [ ] **Step 1: Write the failing tests — these are the regression tests for finding #7 and #8**

```rust
#[test]
fn material_indices_serialize_as_numbers_and_null() {
    // cjseq2 0.1.0 emitted [[], [1]] here. That is invalid CityJSON.
    let m: MaterialReference = serde_json::from_value(serde_json::json!({
        "values": [null, 1]
    })).unwrap();
    assert_eq!(serde_json::to_value(&m).unwrap(),
               serde_json::json!({"values": [null, 1]}));
}

#[test]
fn solid_material_values_keep_their_shell_level() {
    // Finding #8: this shape came back as [[[0, 1]]] through FCB.
    let m: MaterialReference = serde_json::from_value(serde_json::json!({
        "values": [[0, 1]]
    })).unwrap();
    assert_eq!(serde_json::to_value(&m).unwrap(),
               serde_json::json!({"values": [[0, 1]]}));
}

#[test]
fn texture_ring_is_index_then_uvs() {
    let t: TextureReference = serde_json::from_value(serde_json::json!({
        "values": [[[0, 10, 11, 12]]]
    })).unwrap();
    assert_eq!(serde_json::to_value(&t).unwrap(),
               serde_json::json!({"values": [[[0, 10, 11, 12]]]}));
}
```

- [ ] **Step 2: Run and watch fail** — `cargo test appearance`
- [ ] **Step 3: Implement.** Delete `NestedArray` and the `JsonIndex` trait entirely — do not port them. Derived serde only.
- [ ] **Step 4: Run** — `cargo test`, expect PASS.
- [ ] **Step 5: Commit** — `feat!: depth-type appearance values, drop NestedArray`

---

## Task 5: Typed tags with extension round-trip

**Files:**
- Modify: `src/city_object.rs`, `src/semantics.rs`, `src/cityjson.rs`
- Test: unit tests per enum

**Interfaces:**
- Produces: `CityObjectType`, `SemanticSurfaceType`, `CityJSONType`, `CityJSONFeatureType`. Each has an `Extension(String)` variant that round-trips a leading `+` exactly.

This is a re-implementation of `develop`'s `759b86a` on the new base. The critical requirement it must not lose: `flatcitybuf`'s `noise_extension` conformance fixture contains `"+NoiseCityFurnitureSegment"`-style types, and they must survive byte-for-byte.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn extension_city_object_type_roundtrips_with_its_plus() {
    let t: CityObjectType = serde_json::from_value(
        serde_json::json!("+NoiseCityFurnitureSegment")).unwrap();
    assert_eq!(t, CityObjectType::Extension("+NoiseCityFurnitureSegment".into()));
    assert_eq!(serde_json::to_value(&t).unwrap(),
               serde_json::json!("+NoiseCityFurnitureSegment"));
}

#[test]
fn known_city_object_type_is_a_unit_variant() {
    let t: CityObjectType = serde_json::from_value(serde_json::json!("BuildingPart")).unwrap();
    assert_eq!(t, CityObjectType::BuildingPart);
}
```

- [ ] **Step 2: Run and watch fail.** - [ ] **Step 3: Implement** with `#[serde(untagged)]` over a known-variants enum plus `Extension(String)`, or a custom `Deserialize` that dispatches on the leading `+`. If custom, add a proptest-style test over all known names. - [ ] **Step 4: Run.** - [ ] **Step 5: Commit** — `feat!: typed CityObject/semantic-surface tags with extension support`

---

## Task 6: Port the rest of the fork's value-adds

**Files:** `src/error.rs`, `src/city_object.rs`, `src/metadata.rs`

Cherry-pick from `origin/develop` onto the new base, one commit per concept, dropping anything superseded by Tasks 2-5:

| Keep | From |
|---|---|
| custom error type | `5a8102c` |
| `children_roles` field | `828c7c7` |
| `geographicalExtent` as `[f64; 6]` | `8f7fbaf` |
| `PartialEq`/`Eq` derives | `6a844e3`, `6917631` |
| `SemanticsSurface` public fields, `others` optional | `5eaf8f5`, `aff25a1` |
| CityJSON → OBJ conversion | `bfd8761` — **only if you still want it**; it is unrelated to typing and will complicate the upstream PR. Recommend leaving it on `develop` and out of the PR. |
| ~~`NestedArray`, `JsonIndex`~~ | superseded — do not port |

- [ ] **Step 1-N:** For each row: write a test that exercises the feature, watch it fail, cherry-pick/reimplement, watch it pass, commit.

---

## Task 7: Real-file round trips and a codex review

**Files:**
- Create: `tests/roundtrip.rs`
- Test data: reuse `flatcitybuf/src/cpp/tests/conformance/inputs/*.city.jsonl` and `flatcitybuf/src/rust/fcb_core/tests/data/*.city.jsonl` (copy a few in, or point at them via a path constant)

- [ ] **Step 1: Write the failing test**

```rust
/// Parse -> serialize -> parse must be a fixpoint, and the first
/// serialization must equal the input's parsed form. Anything that changes
/// on the way through is data loss.
#[test]
fn every_fixture_roundtrips_exactly() {
    for path in fixture_paths() {
        for (lineno, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
            if line.trim().is_empty() { continue }
            let original: serde_json::Value = serde_json::from_str(line).unwrap();
            let typed: CityJSONFeatureOrHeader = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{}:{lineno}: {e}", path.display()));
            let reserialized = serde_json::to_value(&typed).unwrap();
            assert_eq!(reserialized, original, "{}:{lineno} changed on round trip", path.display());
        }
    }
}
```

- [ ] **Step 2: Run.** Expect failures — this is the task's real work. Each one is either a genuine bug or a field cjseq drops. Fix until green.
- [ ] **Step 3: Ask codex to review the whole type design before it calcifies**

```bash
codex exec --model gpt-5.6-sol --sandbox read-only "Review the typed CityJSON model in src/geometry.rs, src/semantics.rs and src/appearance.rs against the CityJSON 2.0 spec. Focus on: (1) any legal CityJSON document this model would reject; (2) any illegal one it would accept; (3) serde attributes that silently drop unknown or optional members; (4) whether the internally-tagged enum handles GeometryInstance and geometry templates correctly. Cite file:line and give a concrete document for each finding."
```

- [ ] **Step 4: Act on the findings, then commit.**

---

## Task 8: Migrate fcb_core's decoder to type-driven depth

**Files:** (in `~/tudelft/cityjson/flatcitybuf`)
- Modify: `src/rust/Cargo.toml` (point at the fork by path while iterating), `src/rust/fcb_core/src/reader/geom_decoder.rs`, `src/rust/fcb_core/src/reader/deserializer.rs`, `src/rust/fcb_core/src/writer/geom_encoder.rs`, `src/rust/fcb_core/src/writer/serializer.rs`
- Test: `src/rust/fcb_core/tests/appearance_roundtrip.rs` (exists — extend it)

**Interfaces:**
- Consumes: `Geometry::geometry_type()`, the depth-typed values from Tasks 2-4.
- Produces: `decode_materials`/`decode_textures` taking the geometry type as a parameter.

**This is the payoff task.** Today both decoders infer depth from which of `solids`/`shells`/`surfaces`/`strings` are non-empty, and that inference is what produced finding #8's two bugs and the two documented-unreachable quirks. The geometry type is right there in the enclosing `Geometry` table (`g.type_()`).

- [ ] **Step 1: Extend the round-trip tests to cover every geometry type × appearance combination**

`appearance_roundtrip.rs` already has 6 cases and the `roundtrip_geometry` helper that dumps the raw mapping arrays. Add MultiPoint, CompositeSurface, MultiSolid and CompositeSolid cases with both material and texture. Watch the new ones fail where depth inference is wrong.

- [ ] **Step 2: Change the decoder signatures to take the type**

```rust
pub(crate) fn decode_materials(
    geometry_type: GeometryType,
    material_mappings: &[MaterialMapping],
) -> Option<HashMap<String, CjMaterialReference>>
```

and select the depth from `geometry_type` alone. Delete every `solids.len() == 1`, `shells.len() == 1`, `strings.len() > 1` guard — they are the inference being removed.

- [ ] **Step 3: Run** — `cargo test -p fcb_core`. Expect PASS including all round trips.
- [ ] **Step 4: Delete finding #8's "unreachable quirks" section** from `docs/upstream-findings.md`, replacing it with a note that type-driven depth removed the class.
- [ ] **Step 5: Commit.**

---

## Task 9: Absorb the typed tags in fcb_core

**Files:** `src/rust/fcb_core/src/reader/deserializer.rs`, `src/rust/fcb_core/src/writer/serializer.rs` (~5 sites)

This is the change that made `develop` uncompilable against `fcb_core` (five `E0308`s: `thetype` expecting `CityJSONType`/`CityObjectType`/`SemanticSurfaceType` where a `String` is assigned, and two `&str`-vs-`&Enum` comparisons).

- [ ] **Step 1: Build and collect the errors** — `cd src/rust && cargo build -p fcb_core 2>&1 | grep -E "^error"`. Expect five.
- [ ] **Step 2: Fix each site** by constructing the enum instead of a string; `serializer.rs:772`'s `to_semantic_surface_type(&str)` becomes a `From<&SemanticSurfaceType>`.
- [ ] **Step 3: Run** — `cargo test -p fcb_core`, then the C++ conformance corpus:

```bash
./scripts/gen_conformance.sh
cd src/cpp && cmake --build build-native -j8 && ./build-native/tests/fcb_tests
```

Diff *parsed* JSON on the fixtures; commit only files whose JSON actually changed.
- [ ] **Step 4: Commit.**

---

## Task 10: Mirror any decoder change into the C++ reader

**Files:** `src/cpp/src/geometry.cpp`, `src/cpp/include/fcb/geometry.hpp`, `src/cpp/tests/test_geometry.cpp`

The C++ decoders are a deliberate line-by-line port of the Rust ones and **must move together** — `src/cpp/tests/test_geometry.cpp` says so at the top of its appearance section. If Task 8 made depth type-driven, C++ must read `g->type()` the same way.

- [ ] **Step 1: Use the oracle technique** to get expected values (see "Working conventions"). Do not hand-derive.
- [ ] **Step 2: Update `decode_material_values`/`decode_texture_values` to take a geometry type**, and update their tests.
- [ ] **Step 3: Run** — native and ASan:

```bash
cd src/cpp && cmake --build build-native -j8 && ./build-native/tests/fcb_tests
cmake --build build-asan -j8 && ./build-asan/tests/fcb_tests
```

Expected: all green (122 cases as of this plan).
- [ ] **Step 4: Commit.**

---

## Task 11: Release, migrate, upstream, retire

- [ ] **Step 1: Publish `cjseq2` 0.2.0** from `feat/typed-cityjson` (`cargo publish`). Push the branch.
- [ ] **Step 2: Point flatcitybuf at the release** — `src/rust/Cargo.toml`: `cjseq = { package = "cjseq2", version = "0.2.0" }`. Remove any `[patch.crates-io]`. Verify a fresh-clone build: `cargo tree --locked --offline -p fcb_core` must succeed.
- [ ] **Step 3: Full verification** — `cargo test -p fcb_core`, `cargo test -p fcb_core --doc`, C++ native + ASan, `just build-wasm`, `cd src/ts && npm pack --dry-run`.
- [ ] **Step 4: Open the upstream PR** from `HideBa/cjseq:feat/typed-cityjson` to `cityjson/cjseq:main`. The PR body should lead with the two invalid-CityJSON bugs this typing prevents (findings #7 and #8 in `docs/upstream-findings.md`) — that is the argument for the breaking change, not "stricter types are nicer". Leave the OBJ conversion out of the PR if it is still on the branch.
- [ ] **Step 5: After upstream merges and releases**, switch `fcb_core` to `cjseq = "<new version>"`, re-run everything in Step 3, then `cargo yank --version 0.1.0 cjseq2`, `cargo yank --version 0.1.1 cjseq2`, `cargo yank --version 0.2.0 cjseq2`. Yank rather than delete: yanked versions stop being selected but existing lockfiles keep resolving.
- [ ] **Step 6: Update `docs/upstream-findings.md`** — findings #7 and #8 become "fixed upstream in cjseq <version>", and the cjseq2 fork note goes away.

---

## Self-review notes

- **Coverage:** the user's four asks map to Task 1 + 6 (sync the fork with upstream), Tasks 2-5 (the type design), Tasks 8-10 (migrate `fcb_core`, and the C++ reader it forgot to mention but which must move in lockstep), Task 11 (upstream PR, then retire `cjseq2`).
- **Sequencing risk:** Task 8 is the only task that can invalidate the type design; if type-driven decoding turns out to need something the enum cannot express, stop and revisit Task 2 rather than adding inference back.
- **Not in this plan:** the header-level `appearance` object that FCB stores but neither reader emits (`header.fbs:118-122`); `geom_temp`'s header carries 2 materials, 2 textures and 1015 UV vertices that never reach the output. Features carry their own appearance so nothing is broken today. Decide separately whether the writer should stop writing it or the readers should start reading it.
