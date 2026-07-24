# C++ writer M5: packed R-tree builder — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans.

**Goal:** Port the WRITE side of `PackedRTree` (`src/rust/fcb_core/src/packed_rtree/mod.rs`) to
C++: given one `NodeItem` bbox per feature (in real-world, transform-applied coordinates), produce
the exact sequence of `NodeItem` bytes that make up the on-disk R-tree index section.

**Architecture:** `include/fcb/writer/rtree_builder.hpp` + `src/writer/rtree_builder.cpp`. The
READ side already has `fcb::NodeItem` (`packed_rtree.hpp`, decode-only, 40-byte struct) and
`fcb::rtree_num_nodes` (total node count only, no per-level ranges) -- this milestone adds
`encode`, per-level `Range`s, and the actual bottom-up build, as free functions alongside the
existing `fcb::NodeItem` (same struct, extended, not duplicated).

## Ground truth (packed_rtree/mod.rs)

- `NodeItem`: 4x `f64` (min_x, min_y, max_x, max_y) + `u64` offset, LE, 40 bytes, no padding
  (mod.rs:26-33). Already ported read-side (`fcb::NodeItem`); this milestone adds a `write`/
  `encode` counterpart to the existing `decode`.
- `hilbert(x: u32, y: u32) -> u32` (mod.rs:236-289): a fixed bit-twiddling function, no loops,
  every operation `u32` wrapping (C++ `uint32_t` overflow is well-defined wraparound already, so
  this ports as a direct transliteration -- no `wrapping_*` calls needed, unlike a signed type).
- `hilbert_bbox(r, hilbert_max=65535, extent)` (mod.rs:291-298): bbox center scaled into
  `[0, hilbert_max]` via `((center - extent.min) / extent.width() * hilbert_max).floor() as u32`,
  computed independently for x and y, then fed to `hilbert`.
- `hilbert_sort(items, extent)` (mod.rs:300-306): stable sort by `hilbert_bbox` **descending**
  (`hb.partial_cmp(&ha)`, i.e. compares `hb` against `ha` -- higher Hilbert index sorts first).
  This is `std::stable_sort` in C++ (Rust's slice `sort_by` is a stable sort); ties are not
  supposed to happen for real data but stability still matters for reproducibility.
- `calc_extent(nodes)` (mod.rs:308-313): folds `NodeItem::create(0)` (an "empty" node --
  min=+inf, max=-inf) through `expand` over every node. An empty `nodes` slice would fold to the
  identity element (+inf/+inf/-inf/-inf) -- never reached here since M7 only calls this when
  `!feat_nodes.is_empty()`.
- `generate_level_bounds(num_items, node_size)` (mod.rs:342-375): builds `level_num_nodes`
  bottom-up (leaf count, then `n.div_ceil(node_size)` repeatedly until `n == 1`), then converts to
  a **top-down** list of `Range<usize>` (`level_bounds[0]` is the ROOT level range, last element is
  the leaf range) by laying out cumulative offsets so the root sits at the LOWEST index. This
  differs from `fcb::rtree_num_nodes`, which only sums total node count -- this milestone needs
  the actual per-level ranges to build the physical array, so it is a new function, not a
  refactor of the existing one (the existing read-side function stays; this one is additive).
- `generate_nodes` (mod.rs:377-397): walks levels from leaf to root (`0..level_bounds.len()-1`,
  children then parents), grouping every `branching_factor` consecutive children under one parent
  node whose bbox is the union (`NodeItem::expand`) of its children and whose `.offset` is the
  **index of its first child** in the flat array.
- `PackedRTree::build(nodes, extent, node_size)` (mod.rs:432-451): allocates the full flat
  `node_items` array (leaf level pre-filled from `nodes` at the array's TAIL, matching
  `level_bounds` last entry), then calls `generate_nodes`.
- `stream_write` (mod.rs:900-906): writes every node in the flat array, in order, via
  `NodeItem::write` (LE f64 x4 + LE u64).
- Node size / branching factor is clamped to `[2, 65535]` (mod.rs:330,458); `node_size == 0` means
  "no R-tree at all" and is handled by the CALLER (`writer/mod.rs:208`, M7's territory) skipping
  this whole builder, not by this milestone.

## What this milestone does NOT do (out of scope, M7's job)

- Computing each feature's `NodeItem` in the first place (`FcbWriter::actual_bbox`, applying
  `transform.scale`/`translate` to a feature's raw `NodeItem` from `to_fcb_city_feature`) --
  this milestone takes an already-built `std::vector<NodeItem>` as input.
- Reordering the actual FEATURE BYTES to match the hilbert-sorted order, and rewriting each
  sorted node's `.offset` to the feature's new byte position (`writer/mod.rs:211-222`) -- this
  milestone's `hilbert_sort` only reorders the `NodeItem`s themselves; recomputing `.offset` from
  feature sizes is the caller's job (M7), because it needs feature byte sizes this milestone never
  sees.

## Tasks

1. `NodeItem::encode` (LE bytes, mirrors the existing `decode`) + `hilbert(x, y)` + `hilbert_bbox`
   + `calc_extent`. Unit tests: `hilbert(0,0)==0`; a handful of known hilbert-curve values cross-
   checked against the Rust `#[test]`s in `packed_rtree/mod.rs` (grep for `mod tests` there first);
   `calc_extent` over a few nodes.
2. `hilbert_sort` (stable sort, descending hilbert index) + `generate_level_bounds` (returns
   `std::vector<std::pair<std::size_t, std::size_t>>` top-down, matching Rust's `Range` list
   exactly -- unit-test against `generate_level_bounds`'s own doc example and a few
   node_size/num_items combinations, comparing against `fcb::rtree_num_nodes`'s total for a cross
   check).
3. `generate_nodes` + `build_packed_rtree(nodes, extent, node_size) -> std::vector<NodeItem>` (the
   `PackedRTree::build` equivalent) + `encode_packed_rtree(nodes) -> std::vector<uint8_t>` (the
   `stream_write` equivalent, using Task 1's `encode`).
4. Byte-exact oracle: build the R-tree for a multi-feature conformance fixture (need one with
   `>1` feature and a real spatial index -- check `conformance/*.fcb` via `fcb_inspect_header` for
   feature counts >1 and "R-tree yes"; `small.fcb`/`no_count.fcb` looked promising at 44 columns,
   check their feature counts too) using each feature's REAL bbox (read back via the existing
   reader's `select_all`/`FeatureIterator`, or recomputed from the fixture's own
   `.expected.jsonl` vertices + transform) and node_size read from the fixture's own header, then
   byte-compare against the fixture's own rtree section (sliced via
   `[layout.rtree_begin, layout.attr_index_begin)`, never a hardcoded offset).

Testing throughout: unit tests first (TDD red-green), Task 4's byte-exact oracle as the final
milestone gate, matching M3/M4's pattern.
