# C++ writer M6: static B+tree builder — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans.

**Goal:** Port the WRITE side of the static B+tree (`src/rust/fcb_core/src/static_btree/{stree,payload}.rs`,
plus the per-column-type dispatch in `writer/attr_index.rs`) to C++: given a column's `(key, feature
byte offset)` pairs, produce the exact node-array + payload-section bytes that make up one attribute
index blob.

**Architecture:** `include/fcb/writer/btree_builder.hpp` + `src/writer/btree_builder.cpp`. Reuses the
read side's `fcb::KeyValue`/`encode_key`/`decode_key`/`compare_keys`/`key_max`/`key_kind_for_column`
(`key.hpp`), M1's `fcb::AttributeIndexEntry`/`cityfeature_to_index_entries` (`writer/attribute.hpp`),
and (like M5 did for the R-tree) the read side's file-private `generate_level_bounds` in `stree.cpp`,
exposed the same way `rtree_level_bounds` was.

## Ground truth, and the semantics worked out by hand before writing any code

This is NOT a classical B+tree with separate key/child-pointer arrays. Everything is one flat array
of `NodeItem { key, offset }`, laid out level by level exactly like the packed R-tree (leaves at the
array's tail, root at index 0), via `generate_level_bounds` (stree.rs:474-508) -- already re-derived
once for the R-tree in M5, same shape here except the loop divides by `branching_factor` and stops at
`n < branching_factor` (not `n == 1`), because each node holds `branching_factor - 1` **separator**
entries for `branching_factor` **children** -- the classic "n keys separate n+1 children" rule, applied
to a flat array instead of nested structs.

Traced by hand against both `generate_nodes` (build, stree.rs:510-603) and `find_exact` (search,
already ported/conformant on the read side) together, since either reads ambiguously alone:

- An entry's `.offset` points to its own LEFT child's node (a contiguous run of up to
  `branching_factor - 1` entries at the level below, starting at that index).
- An entry's `.key` is an **upper-bound separator**: the smallest key that belongs to the NEXT node
  over, not to this entry's own child. `find_exact`'s binary search + "index found -> add node_size
  before recursing, unless that overruns the level, then use the entry's own offset" mirrors this
  exactly.
- The LAST child-node in a group of `branching_factor` siblings gets NO entry of its own (it's the
  implicit "else, nothing matched, keep going right" case) -- that's what `is_right_most_child`
  detects and skips (`child_idx_diff % skip_size` landing in `[node_size^2, branching_factor^2)`,
  where `skip_size = branching_factor * node_size`); `child_idx` still advances by `node_size` on a
  skip, since a skipped node still occupies its own span of the level below.
- `parent_min_key` exists because a LEAF's own stored key already IS that leaf-node's minimum (so a
  leaf-level parent can read `node_items[right_node_idx].key` directly for its separator), but an
  INTERNAL node's stored key is a separator, NOT its subtree's true minimum -- so a parent of
  INTERNAL nodes must instead look up the precomputed minimum via `parent_min_key[child_idx +
  node_size]` (the right sibling's min, computed one level down) for its OWN separator, and propagate
  `parent_min_key[child_idx]` (the left child's own min) forward under the new parent's own index, so
  the level above can find it in turn. The chain only works because each level's pass runs to
  completion (and populates every index a later pass will look up) before the next level's pass reads
  it -- do not parallelize or reorder the two nested loops.
- `K::max_value()` (this codebase's `key_max(kind)`) marks "no separator needed, everything from here
  rightward belongs to this child" -- used for the last parent entry when there IS no next sibling
  (`!has_next_node` / `right_node_idx >= children_level.end`). `key_max`'s documented quirks (float
  max is +inf even though NaN sorts higher; DateTime min is epoch 0) do not interact with this: the
  sentinel only needs to compare greater than every REAL key of that kind for binary search's
  upper-bound logic to keep working, which +inf still does against any finite float, NaN included in
  the actual data or not (a NaN-keyed leaf's own findability is a pre-existing, disclosed limitation
  of `key_max`, not something this milestone changes).

## Duplicate keys and the payload section (stree.rs:638-691, payload.rs)

`Stree::build` sorts all `(key, offset)` entries by key first, then groups consecutive equal keys:
a key with exactly one occurrence becomes an ordinary leaf entry (`offset` = the real feature byte
offset); a key with more becomes a leaf entry whose `offset` has its MSB set (`PAYLOAD_TAG = 1u64 <<
63`, already defined read-side in `stree.hpp`) and whose low 63 bits are a byte offset INTO a separate
payload section (appended after the whole node array in `stream_write`), where a `PayloadEntry` is a
`u32` count then that many LE `u64` offsets (`decode_payload_entry` already ported read-side; this
milestone adds the encode counterpart). `unique_leaves.len()` (not the original entry count) is what
feeds `generate_level_bounds`/`num_unique_items` -- the header's `AttributeIndexInfo.num_unique_items`
counts unique KEYS, not features.

## Tasks

1. `encode_payload_entry` (mirrors `PayloadEntry::serialize`, the write counterpart to the existing
   `decode_payload_entry`) + expose `stree.cpp`'s file-private `generate_level_bounds` as
   `fcb::stree_level_bounds` (same treatment `rtree_level_bounds` got in M5) + a plain-data `BtreeEntry
   { KeyValue key; std::uint64_t offset; }` input type.
2. The sort-and-group-duplicates step: given `vector<BtreeEntry>`, produce `unique_leaves` (one
   `NodeItem`-equivalent per distinct key, tagged offset for duplicates) + the payload byte buffer, in
   the exact order `Stree::build` produces them (sorted by key via `compare_keys`, first-seen-offset-
   first within a duplicate group, matching a stable sort over the ALREADY-sorted-by-key input).
3. `generate_nodes`: the traced algorithm above, using `std::unordered_map<std::uint64_t, KeyValue>`
   for `parent_min_key` (keyed by flat array index) -- throw `fcb::Error` rather than `.expect()`-
   panicking on a missing lookup, since a missing entry there means this port's own bookkeeping is
   wrong, not a malformed-input condition.
4. `build_static_btree(entries, branching_factor) -> BuiltIndex { vector<uint8_t> bytes;
   std::uint16_t branching_factor; std::uint32_t num_unique_items; }` (the `MemoryIndex::build` +
   `.serialize()` + `.num_items()` equivalent) orchestrating tasks 2-3 plus `stream_write` (node array
   then payload, matching M5's `encode_packed_rtree` style).
5. Byte-exact oracle: build a real attribute index from a conformance fixture with attribute indexing
   enabled and DUPLICATE key values (need to check the corpus for one, or generate a fresh fixture via
   the real Rust CLI with `--index-all-attributes` or `-a <col>`, like M3/M4/M5 did) and byte-compare
   against the fixture's own attribute-index section (sliced via `[layout.attr_index_begin,
   layout.feature_begin)`).

Testing throughout: unit tests first (a small hand-traced tree like the one above, ported as a
literal test case, plus edge cases -- single item, all-duplicate keys, a tree whose leaf count is
exactly a multiple of `branching_factor`), Task 5's byte-exact oracle as the final milestone gate.
