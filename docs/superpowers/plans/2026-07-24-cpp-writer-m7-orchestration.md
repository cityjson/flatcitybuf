# C++ writer M7: FcbWriter orchestration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans.

**Goal:** Assemble a complete, byte-exact `.fcb` file: wire M1-M6 together into one `FcbWriter`-
equivalent that takes parsed CityJSON (metadata line + feature lines) and writes magic bytes, header,
spatial index, attribute indices, and hilbert-sorted features, in that order.

**Architecture:** `include/fcb/writer/fcb_writer.hpp` + `src/writer/fcb_writer.cpp`. Pure orchestration
-- no new encoding logic; every byte-producing step is M1-M6.

## Ground truth: `FcbWriter::write` (writer/mod.rs:191-278), already read in full

```
out.write_all(&MAGIC_BYTES)?;                                    // 8 bytes, fcb::kMagicBytes... (check exact constant name)
if index_node_size > 0 && !feat_nodes.is_empty() {
    extent = calc_extent(feat_nodes);
    hilbert_sort(&mut feat_nodes, &extent);                       // reorders in place
    index_nodes = feat_nodes.map(|n| { n.offset = running_byte_offset; running += feat_offsets[n.offset].size; n });
    tree = PackedRTree::build(&index_nodes, &extent, index_node_size);
    rtree_buf = tree.stream_write();
}
// re-read unsorted feature bytes from tmp storage, write them out in HILBERT-SORTED order into
// sorted_feature_buf, updating each attribute_index_entries[feat.temp_feature_id].offset/.size
// to its NEW position, as each feature is copied.
for (name, bf) in sorted(attr_indices by schema column index):
    (buf, info) = build_attribute_index_for_attr(name, schema, attribute_index_entries, bf)
    attr_index_buf += buf; attr_index_info.push(info)
header_writer.attribute_indices_info = Some(attr_index_info)
header_buf = header_writer.finish_to_header()
out: magic_bytes, header_buf, rtree_buf, attr_index_buf, sorted_feature_buf
```

Each per-feature step, already ported:
- `to_fcb_city_feature` (M3) -> `(bytes, raw_bbox)`.
- `actual_bbox` (transform.scale/translate applied to raw_bbox) -- NOT yet a named function anywhere;
  this milestone adds it (a few lines, already written twice now in M5's own oracle test -- promote
  that to a real function here instead of a third copy).
- `cityfeature_to_index_entries` (M1) -> per-feature `AttributeIndexEntry` list, tagged with the
  feature's ORIGINAL (pre-sort) temp id so it can be re-keyed to the final sorted offset later.

## A real ordering trap, found while writing M6's oracle test

`build_index_generic` (writer/attr_index.rs) collects `Entry<T>`s for `build_static_btree`/
`Stree::build` by iterating `attribute_index_entries: BTreeMap<usize, AttributeFeatureOffset>`
via `.values()` -- and a `BTreeMap` iterates in KEY order. The key there is each feature's
ORIGINAL (pre-hilbert-sort) temp id (`self.feat_offsets.len()` at insertion time, writer/mod.rs:120),
**not** its final sorted position. So entries reach `Stree::build` in ORIGINAL INPUT order, with
each entry's `.offset` field separately overwritten (during the later sorted-copy pass,
writer/mod.rs:237-241) to its feature's FINAL sorted byte position -- the entry's POSITION in the
vector and the BYTE VALUE stored in its `.offset` field are populated by two different passes, in
two different orders. `Stree::build` then does its OWN stable sort by key, so for a column with
duplicate values, which feature's offset ends up FIRST inside the resulting payload entry depends
on ORIGINAL input order, not sorted order. Getting this backwards (collecting entries in
hilbert-sorted order, which reads as the more "natural" choice given the R-tree already needs that
order) silently reorders every duplicate-key payload's offset list -- caught only because M6's own
oracle test added a fixture with both real hilbert reordering AND real duplicate keys together;
every earlier fixture had one or the other but not both. This milestone's implementation MUST
collect each column's `BtreeEntry` list in original feature order (looking up each feature's final
sorted offset from a precomputed `original_index -> final_offset` table), never by iterating the
sorted `NodeItem` list directly.

## Tasks

1. `FcbWriterOptions` (mirrors `HeaderWriterOptions` plus whichever CLI-only knobs this milestone
   actually needs -- likely just `feature_count`/`index_node_size`/`geographical_extent`/
   `attribute_indices: vector<pair<string, optional<uint16_t>>>`; NOT a CLI, so no `-g`/bbox-filter
   equivalents) + `actual_bbox(transform, raw_bbox) -> NodeItem`.
2. The per-feature accumulation pass: for each input feature, build via `to_fcb_city_feature`, record
   its bytes + size + `actual_bbox`-transformed NodeItem (tagged with its original index) +
   `cityfeature_to_index_entries` (tagged the same way).
3. The sort-and-reassign pass: `calc_extent`+`hilbert_sort` the NodeItems (skip entirely if
   `index_node_size == 0` or there are no features, matching `writer/mod.rs:208`), then walk the
   SORTED order once, concatenating each feature's bytes into the final buffer and recording its
   NEW byte offset both on the NodeItem (for the R-tree) and on that feature's attribute-index
   entries (for the B+tree) -- this is the one pass where R-tree and B+tree offsets both come from
   the same "final feature position" computation, so it must run before either tree is built.
4. Per-column attribute index dispatch: mirrors `writer/attr_index.rs`'s `match *coltype` -- one
   `build_static_btree` call per requested indexed column, using `key_kind_for_column` to pick the
   `KeyKind` and filtering each feature's already-tagged index entries by column. Requested columns
   sorted by SCHEMA COLUMN INDEX first (`writer/mod.rs:195-202`), not by request order.
5. `write_fcb(cj, features, options, attr_schema, semantic_attr_schema) -> vector<uint8_t>` (or a
   stream-writing overload) that calls, in order: magic bytes, R-tree build+encode (via task 3's
   sorted NodeItems), per-column B+tree builds (task 4), `to_fcb_header` (M4, needs the attribute
   index info from task 4 and `feature_count`/`index_node_size` from options), then the sorted
   feature bytes (task 3) -- note header bytes are computed AFTER the index sections logically
   depend on it, but WRITTEN before them, so this function must fully build the R-tree/B+tree bytes
   (or at least their `AttributeIndexInfo`/size) before it can call `to_fcb_header`, then still
   place them after the header in the OUTPUT byte order. Mirrors the somewhat convoluted order in
   `writer/mod.rs:204-275` exactly.
6. Byte-exact oracle: run this whole pipeline over a conformance fixture's ORIGINAL INPUT (metadata +
   features), and diff the ENTIRE output file against the real `.fcb` byte-for-byte -- the strongest
   possible check, subsuming every per-section oracle test from M3-M6. Also verify the C++ reader
   can open and correctly decode a file this writer produced (round-trip through both readers, per
   the project owner's explicit requirement), and that the Rust reader can too if a Rust toolchain
   is available in this environment (check `just check`'s existing conventions for how/whether Rust
   is invoked from C++ CI, since the C++ suite is meant to run without one).

Testing throughout: unit tests for `actual_bbox` and the sort/reassign pass in isolation, Task 6's
whole-file byte-exact oracle as the final milestone gate -- this is the one that proves the writer
end-to-end, so give it the most scrutiny (multiple fixtures: single feature, multi-feature with
attribute indices, a fixture with geometry-templates/extensions/full metadata).
