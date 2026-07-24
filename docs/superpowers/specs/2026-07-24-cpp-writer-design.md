# C++ writer for FlatCityBuf — design

## Goal

`src/cpp` currently has a from-scratch, conformant *reader* only. This adds a
from-scratch *writer*, matching what `src/rust/fcb_core/src/writer/` does, so
a C++ program can produce `.fcb` files without any Rust toolchain or FFI —
same philosophy as the existing reader (`docs/upstream-findings.md`,
`.llm/docs/specification.md`).

No CLI: the repo already has `fcb` (Rust) for command-line conversion. This is
a library surface only, consumed like the existing reader's public headers.

## Scope (approved: full parity, 8 milestones)

| # | Milestone | Rust source ported |
|---|---|---|
| M1 | Attribute schema + byte encoding | `writer/attribute.rs` |
| M2 | Geometry flattening (boundaries/semantics/material/texture) | `writer/geom_encoder.rs` |
| M3 | Feature FlatBuffer serialization | `writer/serializer.rs` (feature half), `writer/feature_writer.rs` |
| M4 | Header FlatBuffer serialization | `writer/serializer.rs` (header half), `writer/header_writer.rs` |
| M5 | Packed R-tree builder (hilbert sort + bottom-up build + stream write) | `packed_rtree/mod.rs` write side |
| M6 | Static B+tree builder (per key type, with payload packing) | `writer/attr_index.rs`, `static_btree/stree.rs` write side |
| M7 | `FcbWriter` orchestration (full file assembly, hilbert-ordered features) | `writer/mod.rs` |
| M8 | Polish: example, docs, `just` wiring | — |

Each milestone is its own TDD cycle (red → green → refactor), its own commit
(or small stack of commits) on `develop`, and gets a `codex exec -m
gpt-5.6-sol` review before moving to the next milestone.

## Architecture

New `writer/` module under the existing `fcb_core_cpp` target, gated on
`FCB_WITH_JSON` (writer input is CityJSON-shaped `nlohmann::json`, same gate
the decode side already requires):

| New file | Milestone | Reuses (already exists, read-side) |
|---|---|---|
| `include/fcb/writer/attribute_schema.hpp` + `src/writer/attribute_schema.cpp` | M1 | — |
| `include/fcb/writer/geom_encoder.hpp` + `src/writer/geom_encoder.cpp` | M2 | — |
| `include/fcb/writer/feature_serializer.hpp` + `src/writer/feature_serializer.cpp` | M3 | generated `feature_generated.h` |
| `include/fcb/writer/header_serializer.hpp` + `src/writer/header_serializer.cpp` | M4 | generated `header_generated.h` |
| `include/fcb/writer/rtree_builder.hpp` + `src/writer/rtree_builder.cpp` | M5 | `fcb::rtree_index_size` (`layout.hpp`) |
| `include/fcb/writer/attr_index_builder.hpp` + `src/writer/attr_index_builder.cpp` | M6 | `fcb::encode_key`, `fcb::compare_keys`, `fcb::key_kind_for_column`, `fcb::stree_num_nodes` (`key.hpp`, `stree.hpp`) |
| `include/fcb/writer.hpp` + `src/writer.cpp` (`FcbWriter`) | M7 | `fcb::compute_layout` (`layout.hpp`) |

The read-side headers already exported the primitives a writer needs
(`encode_key`, `compare_keys`, `key_kind_for_column`, `rtree_index_size`,
`stree_num_nodes`, `compute_layout`) — these are reused, not reimplemented.

## Public API

Mirrors Rust's `FcbWriter` shape, adapted to idiomatic C++:

```cpp
namespace fcb {

struct WriterOptions {
    bool write_index = true;
    std::uint16_t index_node_size = kDefaultNodeSize;  // 16
    // (field name, branching factor); nullopt branching factor -> DEFAULT_BRANCHING_FACTOR
    std::vector<std::pair<std::string, std::optional<std::uint16_t>>> attribute_indices;
    std::optional<std::array<double, 6>> geographical_extent;
};

class FcbWriter {
  public:
    // cj_metadata is the first line of a CityJSONSeq: type/version/transform/metadata/...
    // attr_schema / semantic_attr_schema are built by the caller scanning features first,
    // exactly as the Rust `write.rs` example does today.
    FcbWriter(nlohmann::json cj_metadata, WriterOptions options,
              AttributeSchema attr_schema, std::optional<AttributeSchema> semantic_attr_schema);

    void add_feature(const nlohmann::json& city_json_feature);  // one CityJSONFeature line
    std::vector<std::uint8_t> write();  // finalizes: hilbert sort, index build, section assembly
};

}  // namespace fcb
```

Internally, `add_feature` spools each feature's encoded bytes to temp storage
(mirroring Rust's `tempfile` use, for scalability against large datasets) and
keeps an in-memory `NodeItem` array plus offset/size bookkeeping, plus
per-feature attribute-index entries if `attribute_indices` is set. `write()`
hilbert-sorts the `NodeItem`s, builds the packed R-tree, re-reads features in
sorted order into the final buffer, builds each configured attribute's B+tree
blob (in schema-column order) against the *final* sorted offsets, builds the
header (with `AttributeIndex` metadata), and emits
`MAGIC_BYTES + header + rtree + attr_index + features` back to back, no
padding — exactly the file layout in `.llm/docs/specification.md`.

## Testing strategy (TDD; two oracle tiers)

**Tier 1 — pure data transforms (M1, M2).** No FlatBuffers involved yet, so
Rust's own unit-test vectors (`test_add_attributes`,
`test_attribute_serialization`, `test_encode_boundaries`,
`types_of_equal_depth_flatten_identically`, `test_encode_semantics`,
`a_null_semantics_shell_expands_to_one_null_per_surface`,
`test_encode_material`, `a_null_material_shell_or_solid_is_recorded_as_a_null_count`,
`test_encode_texture`) are ported verbatim as C++ doctest cases: same input,
same expected flattened arrays / byte blobs. No Rust toolchain needed to run
these.

**Tier 2 — anything touching bytes (M3–M7).** This is where the user's two
requirements are enforced directly, using the Rust CLI (`fcb ser` / `fcb
deser`) as the interop harness:

1. **Byte-exact parity with the Rust writer.** For a given
   `conformance/inputs/*.city.jsonl` fixture (or a small new hand-authored
   fixture for an edge case not in the corpus) and a fixed set of writer
   options, run `cargo run -p cli --release -- ser <input> <ref.fcb> [flags]`
   to produce a reference file, and diff our C++-produced bytes against it —
   the whole file for M7, the relevant section (header / one feature / rtree
   / attr index) for earlier milestones. Both writers sort city objects by id,
   attribute schemas by column index, and attribute-index columns by schema
   index identically, so this is a real byte-for-byte comparison, not a
   semantic one.
2. **Cross-reader interop.** A file produced by the C++ writer must be
   readable by *both* readers, each independently, not just re-decodable by
   its own writer's mental model (this is the "two readers agreeing on a
   wrong answer" trap `CLAUDE.md` calls out):
   - `fcb deser <cpp-written.fcb> out.jsonl` (Rust reader) — diff against the
     original input / `.expected.jsonl`, whole lines.
   - The existing C++ reader (`fcb::read_header` + feature iteration +
     `to_cityjson_feature`) — diff the same way.
3. These generated oracle bytes/outputs are produced once (Rust toolchain
   needed only at test-authoring time) and **committed** alongside the C++
   test fixtures, the same way `conformance/*.expected.jsonl` is committed
   today — a clean checkout with no Rust toolchain still runs `just test`.

M7's test suite runs this full loop (C++ write → Rust write, byte diff; C++
write → both readers, JSON diff) over every `conformance/inputs/*.city.jsonl`
fixture, plus at least one case with a non-default `index_node_size` and one
with `attribute_indices` configured, since those are the parts of the format
a corpus-only reader test never exercises on the write side.

## Advisor / reviewer workflow

- **Fable** (`Agent` tool, `model: "fable"`) for the two genuinely hard
  algorithmic ports: the hilbert-sort bottom-up R-tree construction (M5,
  `packed_rtree/mod.rs:233`, `:291-298`, `:342-375`) and the static B+tree's
  bottom-up level construction plus payload-section packing for duplicate
  keys (M6, `stree.rs`). Not used for routine, mechanical porting.
- **codex CLI** (`codex exec -m gpt-5.6-sol --sandbox read-only`) reviews the
  diff once tests are green for a milestone, before starting the next one.
  Findings get triaged (per `superpowers:receiving-code-review` — verify
  before applying) and fixed before the milestone's commit.

## Error handling

Reuse `fcb::Error` / `fcb::ErrorCode` (`error.hpp`); add a new code only if a
genuinely new failure mode appears (existing codes already cover e.g.
`UnsupportedColumnType` for attr-index rejection parity). Rust's `panic!` on a
mismatched `GeometryInstance` variant (`serializer.rs`'s
`to_geometry_instance`) is a programmer-error assertion in Rust; the C++ port
throws `fcb::Error` instead of aborting, since this is a library — flagged
explicitly in the M3 codex review as a deliberate behavioral choice, not a
silent port artifact.

## Build wiring

- Add new sources to `fcb_core_cpp` in `CMakeLists.txt` under
  `if(FCB_WITH_JSON)`.
- Add new test files to `tests/CMakeLists.txt` following the existing
  doctest pattern (`tests/test_writer_*.cpp`).
- No new `just` verbs: `just test` / `just check` already run the whole C++
  suite; `just fix` only formats.

## Out of scope

- CLI wiring (explicit user instruction — the Rust `fcb` CLI already covers
  this).
- Python/TypeScript writers (separate future work; this design doesn't
  block them, and new writer-side conformance fixtures this creates are
  placed so they're reusable later, but building those ports is not part of
  this scope).
