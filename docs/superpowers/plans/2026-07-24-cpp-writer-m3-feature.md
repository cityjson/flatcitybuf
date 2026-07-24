# C++ writer M3: feature FlatBuffer serialization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Port the feature half of `src/rust/fcb_core/src/writer/serializer.rs` to C++: build a `CityFeature` FlatBuffer from a CityJSON feature JSON object, using M1's attribute encoder and M2's geometry encoder.

**Architecture:** `include/fcb/writer/feature_serializer.hpp` + `src/writer/feature_serializer.cpp`, in `namespace fcb`, gated on `FCB_WITH_JSON`. Consumes the generated `feature_generated.h`/`geometry_generated.h` `Create*` free functions directly (not the `*Builder` classes) — they take named args with defaults, closest to Rust's `*Args` structs.

**Tech Stack:** C++17, `nlohmann::json`, generated FlatBuffers headers, `fcb::AttributeSchema`/`encode_attributes_with_schema`/`to_columns` (M1), `fcb::encode`/`GMBoundaries`/etc. (M2), the existing read-side `fcb::NodeItem` (`packed_rtree.hpp`) reused verbatim for the feature bbox.

## Global Constraints

- Enum name<->string mappings for `CityObjectType` and `SemanticSurfaceType` are maintained SEPARATELY from the read side's `city_object_type_name`/`semantic_surface_type_name` (`cityjson.cpp`), matching Rust's own split between `deserializer.rs`'s `to_cj_co_type` and `serializer.rs`'s `to_co_type` — not DRY, but consistent with the oracle's own structure, and avoids touching already-tested read-side code.
- An unrecognized CityJSON type-name string (not one of CityJSON's ~33 known object types / 18 known surface types) becomes `ExtensionObject`/`ExtraSemanticSurface` plus the verbatim name in `extension_type` — never an error. This mirrors `CjCityObjectType::Extension`/`CjSemanticSurfaceType::Extension`.
- `city_objects` (a JSON object, so unordered in principle) must be visited in ascending id order — same determinism reasoning as M1's `cityfeature_to_index_entries`.
- Vertex coordinates are stored as `int32_t` in the FlatBuffer (already-scaled/translated integers per the file's `Transform`); a CityJSON vertex array holds them as JSON integers already in that scaled form (this writer does not itself apply `transform.scale`/`translate` — that happens once, in the header, and vertices are written as given).
- `fcb::NodeItem` (`include/fcb/packed_rtree.hpp`) is the feature bbox type; reuse it, don't redefine it.

## Testing strategy

Tier 1 (Tasks 1-3, internal building blocks): unit tests constructing a small FlatBufferBuilder, calling the function under test, then reading the result back with the GENERATED accessors directly (`fb_geometry->type()`, `->boundaries()->Get(i)`, etc.) — no need to wait for the full reader.

Tier 2 (Task 4, the entry point `to_fcb_city_feature`): round-trip through the EXISTING, already-conformant C++ reader (`fcb::root_as` style access is internal to `reader.cpp`; instead construct a `Feature`-shaped read using the same low-level generated accessors as Tier 1, OR — simpler and stronger — decode via `fcb::to_cityjson_feature`, which needs a `HeaderView`; since building a full real `HeaderView` requires bytes with a real layout, Task 4's tests read the generated `CityFeature`/`CityObject`/`Geometry` accessors directly rather than going through the full reader stack, and Task 5 (below) is what exercises the full reader).

Task 5 is the byte-exact oracle: build a feature from one `conformance/inputs/*.city.jsonl` fixture, byte-diff the FlatBuffer against a freshly-generated `fcb` CLI reference (`cargo run -p cli -- ser <input> <ref.fcb> --no-spatial-index` then slice out the feature bytes using the known header layout), and cross-read the SAME bytes with the existing C++ reader's `to_cityjson_feature`.

---

### Task 1: Enum mappers + `to_appearance`

**Files:** Create `include/fcb/writer/feature_serializer.hpp` + `src/writer/feature_serializer.cpp`; test `tests/test_writer_feature.cpp`; wire into `CMakeLists.txt`/`tests/CMakeLists.txt`.

**Interfaces produced:**
- `struct CoType { ::CityObjectType type; std::optional<std::string> extension_type; }`, `CoType city_object_type_from_name(const std::string& name);`
- `struct SurfaceType { ::SemanticSurfaceType type; std::optional<std::string> extension_type; }`, `SurfaceType semantic_surface_type_from_name(const std::string& name);`
- `::flatbuffers::Offset<::Appearance> to_appearance(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::json& appearance);`

Mirrors `serializer.rs`'s `to_co_type` (732-792), `FcbSemanticSurfaceType::from` (820-883), `to_appearance` (533-622), `fb_wrap_mode`/`fb_texture_type`/`fb_texture_format` (500-531) — the last three folded into `to_appearance` directly rather than kept as separate free functions, since nothing else calls them.

### Task 2: `to_geometry` + `to_geometry_instance`

**Interfaces produced:**
- `::flatbuffers::Offset<::Geometry> to_geometry(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::json& geometry, const AttributeSchema* semantic_attr_schema);`
- `::flatbuffers::Offset<::GeometryInstance> to_geometry_instance(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::json& geometry);`

Consumes M2's `fcb::encode(geometry)`. `semantic_attr_schema` is `nullptr` when the file carries no semantic attribute schema at all (mirrors Rust's `Option<&AttributeSchema>`); a present-but-empty schema is a valid, distinct state (an object whose semantic surfaces carry no extra attributes yet). Mirrors `serializer.rs`'s `to_geometry` (891-1065) and `to_geometry_instance` (1067-1102) — the latter throws `fcb::Error` on a non-`GeometryInstance` input rather than Rust's `panic!` (a deliberate, disclosed adaptation, per the M2 design doc's error-handling section).

### Task 3: `to_city_object`

**Interfaces produced:**
- `::flatbuffers::Offset<::CityObject> to_city_object(::flatbuffers::FlatBufferBuilder& fbb, const std::string& id, const nlohmann::json& co, const AttributeSchema& attr_schema, const AttributeSchema* semantic_attr_schema);`

Splits `co.geometry` into non-instance vs `GeometryInstance` entries (by each entry's `"type"` field) before calling Task 2's two functions, matching `serializer.rs`'s `to_city_object` (632-730). Attributes: own schema when `co.attributes`'s keys are not all present in `attr_schema` (mirrors `to_fcb_attribute`, `serializer.rs:1151-1174`, already effectively covered by M1's `AttributeSchema` machinery — this task wires it to `CityObject.columns`).

### Task 4: `to_fcb_city_feature` (entry point)

**Interfaces produced:**
- `std::pair<::flatbuffers::Offset<::CityFeature>, NodeItem> to_fcb_city_feature(::flatbuffers::FlatBufferBuilder& fbb, const std::string& id, const nlohmann::json& city_feature, const AttributeSchema& attr_schema, const AttributeSchema* semantic_attr_schema);`

Visits `city_feature["CityObjects"]` in ascending id order (Task 1's determinism note), builds `vertices` as `int32_t` triples, computes the bbox as `fcb::NodeItem{min_x, min_y, max_x, max_y, 0}` over the RAW (untransformed) vertex coordinates — matching Rust's `to_fcb_city_feature`, which computes the bbox in the SAME untransformed space and leaves scale/translate to the caller (`FcbWriter::actual_bbox`, M7). Mirrors `serializer.rs:410-489`.

### Task 5: Byte-exact oracle + cross-reader round trip

Uses `conformance/inputs/single_feature.city.jsonl` (exactly one feature — simplest to isolate). Steps:
1. `cargo run -p cli --release -- ser conformance/inputs/single_feature.city.jsonl /tmp/oracle.fcb --no-spatial-index` (from `src/rust`).
2. Compute the feature's byte range in `/tmp/oracle.fcb` from the header (magic 8B + size-prefix 4B + header bytes; no rtree since `--no-spatial-index`; no attr index since none requested) and extract it.
3. Commit the extracted feature bytes (or the whole tiny oracle file) as a test fixture under `src/cpp/tests/fixtures/` (new directory) alongside the JSON input already in `conformance/inputs/`.
4. C++ test: parse the same JSONL input, build the schema the same way (scan all features), call `to_fcb_city_feature`, `finish_size_prefixed`-equivalent (`fbb.FinishSizePrefixed`), and byte-compare against the committed fixture.
5. Cross-read: feed the same bytes through the existing C++ reader's low-level accessors (`GetSizePrefixedRoot<CityFeature>`) and via `fcb::to_cityjson_feature` (needs a matching schema/HeaderView — construct a minimal one, or compare only the parts `to_cityjson_feature` doesn't need a full header for) to confirm the written bytes decode sensibly.

This task's exact mechanics depend on what Task 1-4 produce, so its detailed steps are written when reached rather than now.

## After all 5 tasks: milestone review

`codex exec -m gpt-5.6-sol --sandbox read-only` on the full M3 diff against `serializer.rs`'s feature half and `feature_writer.rs`. Fix findings, re-verify, commit. Mark M3 complete; move to M4.
