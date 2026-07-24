# C++ writer M2: geometry flattening encoder — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port `src/rust/fcb_core/src/writer/geom_encoder.rs` to C++: flatten a CityJSON geometry's boundaries, semantics, material and texture into the parallel count/index arrays FlatCityBuf stores.

**Architecture:** One new header/source pair, `include/fcb/writer/geom_encoder.hpp` + `src/writer/geom_encoder.cpp`, in `namespace fcb`, gated on `FCB_WITH_JSON`. Operates directly on the raw CityJSON JSON shape (`nlohmann::json`), unlike Rust which operates on cjseq's typed `CjGeometry` enum — there is no typed CityJSON model in C++, so every accessor reads a named JSON field instead of matching an enum variant.

**Tech Stack:** C++17, `nlohmann::json`, the existing read-side `fcb::GeometryKind` enum and `fcb::decode_boundaries`/`decode_semantics_values`/`decode_material_values`/`decode_texture_values` (`include/fcb/geometry.hpp`) as a round-trip oracle.

## Global Constraints

- `u32::MAX` (4294967295) means null in semantics/material null-shell markers and appearance indices (`CLAUDE.md`).
- Depth (how deeply `boundaries`/`semantics.values`/`material.values`/`texture.values` nest) comes from the geometry's `type`, via `fcb::GeometryKind` -- **never** inferred from which arrays are populated (`geometry.hpp`'s documented policy, and `CLAUDE.md`'s spirit of not re-deriving the format). This is a deliberate adaptation from Rust, which dispatches on `serde`'s untagged-enum shape-sniffing of the *values* JSON instead (`CjSemanticsValues`/`CjMaterialValues`/`CjTextureValues`); dispatching on `GeometryKind` instead is simpler in the absence of a typed cjseq-equivalent, more robust against an empty-array edge case that would defeat shape-sniffing, and produces byte-identical output for every valid input, since the format's own depth-by-type table is exactly what the sniffing was recovering anyway.
- A JSON `null` value inside a boundary/semantics/material/texture array element must round-trip as `NULL` (`u32::MAX`), never as an omitted entry or an empty array -- getting this wrong desyncs every downstream index in the same array.
- Reuse `fcb::GeometryKind` (`geometry.hpp`) for the type enum; do not define a second one.

## Testing strategy

Every encoder function gets TWO checks, both required to pass:
1. **Direct value check** against Rust's own unit-test vectors (ported verbatim from `geom_encoder.rs`'s `#[cfg(test)] mod tests`), so the exact flattened arrays are pinned, not just "some array came out."
2. **Round-trip through the existing, already-conformant C++ *reader*** (`fcb::decode_boundaries`/`decode_semantics_values`/`decode_material_values`/`decode_texture_values`): `decode(encode(x)) == x`. This is a stronger oracle than Rust's own tests reach alone, and it is a genuinely independent code path (written and tested before this milestone existed), so it guards against the "two implementations agree on a wrong answer" failure mode `CLAUDE.md` warns about.

---

### Task 1: `GMBoundaries` + `encode_boundaries` + `geometry_kind_from_name`

**Files:**
- Create: `src/cpp/include/fcb/writer/geom_encoder.hpp`
- Create: `src/cpp/src/writer/geom_encoder.cpp`
- Test: `src/cpp/tests/test_writer_geometry.cpp`
- Modify: `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Produces: `struct fcb::GMBoundaries { vector<uint32_t> solids, shells, surfaces, strings, indices; }`, `fcb::GeometryKind fcb::geometry_kind_from_name(const std::string&)` (throws `fcb::Error{InvalidAttributeValue}` on an unknown name -- CityJSON's 8 known geometry type strings only), `fcb::GMBoundaries fcb::encode_boundaries(GeometryKind kind, const nlohmann::json& boundaries)`.

- [ ] Write failing tests in `test_writer_geometry.cpp`: port `test_encode_boundaries` and `types_of_equal_depth_flatten_identically` from `geom_encoder.rs` verbatim (same input JSON, same expected `solids`/`shells`/`surfaces`/`strings`/`indices` vectors), for MultiPoint, MultiLineString, MultiSurface, Solid, CompositeSolid. Add one round-trip case per kind: `fcb::decode_boundaries(kind, encode(kind, input).solids, ..., .indices) == input` (parse the same input JSON, compare via `==` on `nlohmann::json`).
- [ ] Run `just test` from `src/cpp`, confirm compile failure (symbols undeclared).
- [ ] Implement `geometry_kind_from_name` (switch/if-chain over the 8 CityJSON type strings: "MultiPoint", "MultiLineString", "MultiSurface", "CompositeSurface", "Solid", "MultiSolid", "CompositeSolid", "GeometryInstance") and `encode_boundaries` (`push_ring`/`push_surface`/`push_shell`/`push_solid` helpers in an anonymous namespace, each appending one count entry per level then recursing, exactly mirroring `geom_encoder.rs:126-189`).
- [ ] Run `just test`, confirm pass. `clang-format -i`, `just lint`, commit.

### Task 2: `GMSemantics` + `encode_semantics`

**Interfaces:**
- Consumes: `GMBoundaries` (Task 1, for shell/surface counts when expanding a null shell/solid).
- Produces: `struct fcb::GMSemantics { nlohmann::json surfaces; std::optional<std::vector<std::uint32_t>> values; }` (`surfaces` is the CityJSON `semantics.surfaces` array, passed through verbatim -- FlatBuffers `SemanticObject` encoding is M3's job, not this one's), `fcb::GMSemantics fcb::encode_semantics(const nlohmann::json& semantics, GeometryKind kind, const GMBoundaries& boundaries)`.

- [ ] Write failing tests: port `test_encode_semantics` and `a_null_semantics_shell_expands_to_one_null_per_surface` verbatim. Add round-trip: `fcb::decode_semantics_values(kind, boundaries.solids, boundaries.shells, *encoded.values) == original_semantics_values_json` for the non-null case (skip round-trip for the null case, since a `null` shell deliberately does NOT round-trip its spelling -- documented in Rust's own doc comment, reproduce that comment here too).
- [ ] Run `just test`, confirm failure.
- [ ] Implement: `encode_semantics_values` dispatches on `kind` into 3 depth buckets (`{MultiPoint, MultiLineString, MultiSurface, CompositeSurface}` = flat array of index-or-null; `{Solid}` = one level of shells; `{MultiSolid, CompositeSolid}` = one level of solids-of-shells), each mirroring `push_semantics_shell`'s null-expansion using `boundaries.shells`/`boundaries.solids` for counts (`geom_encoder.rs:379-432`). `NULL` sentinel is `std::numeric_limits<std::uint32_t>::max()`.
- [ ] Run `just test`, confirm pass. Format, lint, commit.

### Task 3: `MaterialMapping` + `encode_material`

**Interfaces:**
- Produces: `struct fcb::MaterialMapping { enum class Kind { Value, Values, NullValues } kind; std::string theme; std::uint32_t value = 0; std::vector<std::uint32_t> solids, shells, vertices; }` (tagged-struct style, matching this codebase's existing `AttrValue` convention rather than `std::variant`), `std::vector<fcb::MaterialMapping> fcb::encode_material(const nlohmann::json& material, GeometryKind kind)` (`material` is the CityJSON `geometry.material` object: theme name -> `{"value": N}` or `{"values": [...]}` or `{"values": null}`; themes are visited in ascending name order, matching Rust's determinism fix for its `HashMap` source).

- [ ] Write failing tests: port `test_encode_material` and `a_null_material_shell_or_solid_is_recorded_as_a_null_count` verbatim (all 6 Rust sub-cases: single value, MultiSurface-depth values, Solid-depth values, multiple themes, CompositeSolid-depth values, null shell/solid). Add round-trip via `decode_material_values` for the non-null-count cases.
- [ ] Run `just test`, confirm failure.
- [ ] Implement, mirroring `geom_encoder.rs:204-284` exactly, keyed on `kind` instead of the value's own shape (per the Global Constraints note).
- [ ] Run `just test`, confirm pass. Format, lint, commit.

### Task 4: `TextureMapping` + `encode_texture`

**Interfaces:**
- Produces: `struct fcb::TextureMapping { std::string theme; bool has_values = false; std::vector<std::uint32_t> solids, shells, surfaces, strings, vertices; }`, `std::vector<fcb::TextureMapping> fcb::encode_texture(const nlohmann::json& texture, GeometryKind kind)`.

- [ ] Write failing tests: port `test_encode_texture` verbatim (MultiSurface-depth, Solid-depth, CompositeSolid-depth, multiple themes). Add round-trip via `decode_texture_values`.
- [ ] Run `just test`, confirm failure.
- [ ] Implement, mirroring `geom_encoder.rs:290-361` exactly, keyed on `kind`.
- [ ] Run `just test`, confirm pass. Format, lint, commit.

### Task 5: `EncodedGeometry` + `encode` (top-level entry point)

**Interfaces:**
- Produces: `struct fcb::EncodedGeometry { GMBoundaries boundaries; std::optional<GMSemantics> semantics; std::optional<std::vector<MaterialMapping>> materials; std::optional<std::vector<TextureMapping>> textures; }`, `fcb::EncodedGeometry fcb::encode(const nlohmann::json& geometry)` (`geometry` is one CityJSON geometry object; reads `type`/`boundaries`/`semantics`/`material`/`texture` fields directly; a `GeometryInstance` type yields an all-empty `EncodedGeometry`, matching Rust -- it is encoded separately by M3's `to_geometry_instance`).

- [ ] Write a failing test combining a `MultiSurface` with both semantics and material into one `encode()` call, checking all four output members are populated correctly.
- [ ] Run `just test`, confirm failure.
- [ ] Implement `encode`: resolve `GeometryKind` via `geometry_kind_from_name(geometry.at("type"))`, call `encode_boundaries`, then conditionally call `encode_semantics`/`encode_material`/`encode_texture` when the corresponding JSON key is present and non-null.
- [ ] Run full `just check` (lint + build + both test suites). Format, commit -- this closes M2.

## After all 5 tasks: milestone review

- [ ] `codex exec -m gpt-5.6-sol --sandbox read-only`, reviewing `git diff <M2-start>..HEAD -- src/cpp`, against `geom_encoder.rs` and the depth table in `geometry.hpp`. Ask specifically about the `GeometryKind`-dispatch-instead-of-shape-sniffing adaptation, and any `u32::MAX` null-handling mistakes.
- [ ] Triage findings (verify before applying), fix, commit.
- [ ] Mark M2 complete; move to M3.
