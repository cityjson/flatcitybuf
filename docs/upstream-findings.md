# Defects found in the Rust implementation while porting the C++ reader

Five defects in `fcb_core` surfaced during the native C++ port. Each is
reproducible, each caused a deliberate divergence in the C++ reader, and none
is caught by the existing Rust test suite.

**Status: all six are now FIXED in this branch** except #5, which is a
structural change to the query lowering that the C++ reader demonstrates the
alternative for. Each fix has a regression test. #1 turned out to be an
upstream `flatbuffers` bug and is fixed by a version bump, which **changes the
written file layout**.

A seventh defect surfaced while regenerating fixtures: indexing an attribute
with no values panicked (`assert!(num_leaf_nodes > 0, "Cannot create empty
tree")`) instead of returning an error, so `--index-all-attributes` aborted on
any heterogeneous dataset. Library code should not panic; it now returns an
error and the writer skips such columns.

Filing these as public GitHub issues remains a maintainer call.

---

## 1. `Transform` is written at a misaligned offset — FIXED (flatbuffers bump)

**Where:** `flatbuffers` 24.12.23's `finish_size_prefixed`, not FlatCityBuf's
own code. Observable in any `.fcb` written before the bump.

Two structs that both require 8-byte alignment were laid out relative to
*different* bases, so no placement of the buffer could align both:

```
field                  off(buf) off(body)  buf%8 body%8
transform                    72       68      0      4   <- aligned to buf
geographical_extent         132      128      4      0   <- aligned to body
```

They sit 60 bytes apart, and 60 % 8 == 4. Shifting the buffer to fix one
necessarily breaks the other, which is why the C++ verifier's
`check_alignment` failed at every possible residue.

**Consequences:**

- The C++ FlatBuffers verifier's `check_alignment` rejects every Rust-written
  header, at every possible buffer placement. The offset is internal, so no
  allocation strategy fixes it.
- Reading the field through the generated accessor is undefined behaviour.
  UBSan reports `member call on misaligned address ... for type 'Transform'`.
- Rust's own verifier does not check this, which is why it went unnoticed.

**Reproduce:** build the C++ suite with `-fsanitize=undefined` before the
memcpy workaround, or verify any header buffer with C++ `flatbuffers::Verifier`
defaults.

**Fix:** bump the Rust `flatbuffers` pin from 24.3.25 (resolving to 24.12.23)
to 25.9.23. The newer builder aligns everything relative to the size-prefixed
buffer start:

```
transform                    72       68      0   <- both consistent
geographical_extent         128      124      0
VerifySizePrefixedHeaderBuffer (check_alignment ON) = 1
```

**This is a breaking change to the written layout** — files produced before the
bump keep the old, internally inconsistent alignment. All fixtures in this repo
were regenerated.

The C++ reader now enables full alignment verification and needs no padding:
FlatBuffers aligns relative to the buffer start, and `std::vector`'s allocation
is already 8-aligned. `memcpy` reads for struct doubles are retained as cheap
defence, since they compile to the same load.

---

## 2. `Byte` attribute index: writer stores `u8`, reader decodes `i8` — FIXED

**Where:** `writer/attribute.rs:209`, `writer/attr_index.rs:240` vs
`reader/attr_query.rs:118`.

The writer stores `Byte` values as `u8` and builds the index as
`MemoryIndex<u8>`; the reader decodes that same index as `i8`. A stored `200`
reads back as `-56`.

**C++ divergence:** matches the writer (`u8`), so it decodes files correctly
and disagrees with the Rust reader for values above 127.

---

## 3. `Byte`/`UByte`/`Binary` attributes cannot be read back at all — FIXED

**Where:** `reader/deserializer.rs:372` — `unreachable!()`.

The writer emits these column types, but the reader panics on them. Any file
containing such an attribute is unreadable by the implementation that wrote it.

**C++ divergence:** decodes all three. Their widths are unambiguous (1, 1, and
`u32` length + bytes), so there is no reason to refuse them.

---

## 4. `find_range` silently drops its upper boundary item — FIXED

**Where:** `static_btree/stree.rs:954`.

`end_idx = min(upper_idx + node_size, leaf_end)`. Because `find_partition`
descends *left* on an exact hit, when `upper` is itself a separator key the
matching leaf entry sits at exactly `upper_idx + node_size` — one past the scan
end — and is dropped.

This affects every `Le(k)` and range query where `k` is a separator: roughly
1-in-`branching_factor` of unique keys.

**Two existing tests encoded the bug**, each contradicting its own comment:

- `test_range_search` builds keys 0..18, comments *"expects to find exactly 19
  items"*, then asserts `len() == 18`.
- `test_memory_index_with_complex_data` comments *"1(x2), 2, 3"* and asserts
  `3`; and comments *"17, 18"* and asserts `1`.

Both are now corrected to match their comments, and two regression tests were
added.

**C++ divergence:** widens the scan by one node. Safe because the leaf filter
already rejects out-of-range keys; costs at most one extra node read.

---

## 5. `Gt`/`Lt`/`Ne` can drop genuine matches — NOT FIXED upstream

**Where:** `static_btree/query/stream.rs:161-191`.

These are lowered as "range minus `find_exact`", and the subtraction operates
on **feature offsets**. But one feature can appear under several keys when its
CityObjects carry different values of the indexed attribute — the writer
indexes each occurrence.

A feature holding both `k` and `k' > k` is returned by the range scan (via
`k'`) and also by `find_exact(k)` (via `k`), so the subtraction deletes it. It
is a false negative for a feature that genuinely matches.

**C++ divergence (still live):** evaluates strict-or-inclusive bounds at the
leaf instead of subtracting. One traversal, no subtraction, no false negatives.

Not fixed upstream: it is a structural change to the query lowering rather
than a localised correction, and the C++ reader demonstrates the alternative.

---

## 6. `find_exact` on a maximum-valued key walks off the level — FIXED

**Confirmed and fixed**, with a regression test (`test_find_exact_on_max_valued_key`).

Separator entries with no right sibling carry `K::max_value()` as a sentinel
whose offset already points at the last child group. `find_exact`'s
`Ok(i) → offset + node_size` right-descent then overshoots the level, which
should produce an inverted slice (panic) in the in-memory path or a `usize`
underflow in the streaming path.

`Eq(true)` on a bool-indexed column ought to be enough to trigger it, since
`true` is `bool::max_value()`.

**C++ workaround:** clamps the child index back to the entry's own offset when
the computed child would leave the level. A no-op for ordinary keys.

---

## 7. `cjseq2` wraps every material/texture index in a one-element array — FIXED (0.1.1)

**Where:** `cjseq2` 0.1.0, `impl JsonIndex for Option<usize>::to_value`, not
FlatCityBuf's own code. Surfaced while porting appearance decoding to C++.

```rust
fn to_value(&self) -> Value {
    Value::Array(self.iter().map(|x| x.to_value()).collect())  // Option::iter
}
```

`Option::iter` yields zero or one element, so `Some(1)` serialized as `[1]`
and `None` as `[]`. The `Option<u32>` impl directly above it is correct
(`Number` / `Null`); only `Option<usize>` — used by `MaterialValues` and
`TextureValues`, and by nothing else — was wrong. Semantics values and
boundaries were unaffected, which is why this survived: every fixture with
appearance data agreed with a reader that had the same bug.

The emitted CityJSON was invalid against the spec, which wants
`"values": [null, 1]` for a MultiSurface's materials and
`[[0, 16, 17, 18, 19]]` for a surface's texture ring, not
`[[], [1]]` and `[[[0], [16], [17], [18], [19]]]`.

**Fix:** mirror the `Option<u32>` impl. Released as cjseq2 0.1.1 and merged
into `hideba/cjseq`'s `develop`; `src/rust/Cargo.toml` depends on `0.1.1`
from crates.io, so a fresh clone builds with no local checkout.

0.1.1 was cut from the published 0.1.0 tree plus this one function, NOT from
`develop` HEAD. `develop` also carries an unreleased change (`759b86a`,
"introduce CityObjectType and SemanticSurfaceType enums") that retypes
`thetype` on `CityJSON`, `CityObject` and `SemanticsSurface` from `String` to
enums; `fcb_core` still assigns strings there and fails to compile against it
with five `E0308` mismatches. Releasing that is a 0.2.0, and needs `fcb_core`
updated in the same change.

`src/cpp/tests/conformance/geom_temp.expected.jsonl` was regenerated after
the fix; both readers now emit spec-correct CityJSON. `small.expected.jsonl`
changed only in key order.

---

## 8. Two appearance shapes lost a nesting level on round trip — FIXED

**Where:** `fcb_core/src/reader/geom_decoder.rs`, `decode_materials` and
`decode_textures`. Found while porting the decoders to C++, confirmed by
round-tripping real CityJSON through our own writer and reader
(`fcb_core/tests/appearance_roundtrip.rs`).

Both decoders pick a nesting depth from which count arrays are populated,
and two of those guards were too strict:

**Materials, `solids == [1]`.** The single-Solid branch was guarded on
`solids.len() == 1 && solids[0] > 1`, so a Solid with exactly ONE shell fell
into the MultiSolid branch:

```
in:  "material": {"winter": {"values": [[0, 1]]}}
out: "material": {"winter": {"values": [[[0, 1]]]}}
```

One exterior shell is the commonest geometry there is, so this affected
most buildings carrying materials. Guard is now `solids.len() == 1`.

**Textures, a single-string MultiLineString.** The MultiLineString branch
required `strings.len() > 1`, so one string fell through to the MultiSurface
branch and likewise gained a level. The two shapes ARE distinguishable: the
MultiSurface encoding also carries `shells == [1]`, claimed by an earlier
branch. Guard is now `!strings.is_empty()`.

Both fixes are mirrored in the C++ reader (`src/cpp/src/geometry.cpp`); the
two decoders must change together. No conformance fixture changed, because
`geom_temp` happens to exercise neither branch — which is exactly why unit
tests over the reference's own output could not have caught this, and only a
round trip through the writer did.

### The whole class is gone: depth now comes from the geometry type

Both fixes above were still guesses — better guesses, but guesses. Two further
quirks of the same shape were documented here as unreachable-from-our-writer:
`decode_textures` skipping its shell branch when `shells.len() > 1`, and the
MultiLineString branch iterating `surfaces[0]` rather than `strings.len()`.

The decoders no longer infer anything. `decode_materials` and `decode_textures`
take the `GeometryType` — which was always there, in the enclosing `Geometry`
table — and select the depth from it alone. Every `solids.len() == 1`,
`shells.len() == 1` and `strings.len() > 1` guard is deleted, and so are the
two quirks, which no longer have a branch to be reachable into.

That the guessing could not have been made correct is now proved by test rather
than argued: a `Solid` and a one-solid `MultiSolid` are shown to flatten to
byte-identical arrays, as are `MultiSurface`/`CompositeSurface` and
`MultiSolid`/`CompositeSolid`. Against the previous decoder, six geometry ×
appearance combinations came back wrong — the four where a one-solid
`MultiSolid` or `CompositeSolid` decoded as a `Solid`, plus a dropped `null`
solid and a dropped explicit `"values": null`.

The depths, from `geomprimitives.schema.json`:

| type                               | boundaries | semantics.values | material.values | texture.values |
|------------------------------------|-----------:|-----------------:|----------------:|---------------:|
| `MultiPoint`                       |          1 |                1 |     *forbidden* |    *forbidden* |
| `MultiLineString`                  |          2 |                1 |     *forbidden* |    *forbidden* |
| `MultiSurface`, `CompositeSurface` |          3 |                1 |               1 |              3 |
| `Solid`                            |          4 |                2 |               2 |              4 |
| `MultiSolid`, `CompositeSolid`     |          5 |                3 |               3 |              5 |

`MultiPoint` and `MultiLineString` are typed with no `material` and no
`texture` member and with `additionalProperties: false`, so appearance on one
of them is not valid CityJSON — which is why the second bug above, a textured
single-string `MultiLineString`, describes an input that should never have been
accepted in the first place.

This is mirrored in the C++ reader (`src/cpp/src/geometry.cpp`), which no
longer infers either: `decode_boundaries`, `decode_semantics_values`,
`decode_material_values` and `decode_texture_values` all take a `GeometryKind`
and switch on it, exactly as the Rust decoders switch on `GeometryType`. As
with the two fixes above, the two decoders must change together — a depth rule
that holds in one reader and not the other is a file that round-trips through
`fcb_core` and not through the C++ reader, which is the harder bug of the two
to find.

---

# Defects found while porting the native TypeScript reader

The following surfaced during the TypeScript port (Tasks 1–18). Findings #9–#12
are in the `fcb_wasm` browser binding, which **this branch has since removed**
(`src/rust/wasm/` deleted in Task 18) in favour of the native TypeScript reader
at `src/ts/`. Their line citations are to the crate as it stood at deletion, and
are recorded here because the native reader had to get each of these right where
the wasm binding got it wrong — every one is covered by a TypeScript test. #13
is a live defect in `fcb_core` itself. #14–#16 are writer/CLI defects found and
**fixed on this branch** during Task 2.

## 9. wasm: every JS number is coerced to a `Float64` index key — NOT FIXED (crate removed)

**Where:** `wasm/src/lib.rs:1110-1112` (`WasmAttrQuery::new`).

```rust
} else if let Some(n) = value_js.as_f64() {
    // All JS numbers are f64.
    KeyType::Float64(Float(n))
```

Every numeric query value from JavaScript became a `Float64` key, because in JS
all numbers are IEEE-754 doubles and the binding never consulted the column's
declared type. But the attribute index for an `Int`/`UInt`/`Short`/`Long`/… column
is built over that column's *native* key type, so a query like
`["building_id", "Eq", 42]` reached an `HttpIndex<i32>` carrying a `Float64` key
and failed the type check.

**What a consumer saw:** an attribute query against any non-`Double` numeric
column failed from the browser with a "key type mismatch" error — i.e. the whole
class of integer-column queries was unusable. The native reader instead picks the
key encoding from the column's `ColumnType` (`src/ts/src/static-btree/`), so
`42` against an `Int` column is queried as an `i32`; covered by the attribute-query
tests in `src/ts/test/stree.test.ts`.

## 10. wasm: string query values over 50 bytes are routed into a `StringKey100` — NOT FIXED (crate removed)

**Where:** `wasm/src/lib.rs:1114-1118` (`WasmAttrQuery::new`).

```rust
} else if let Some(s) = value_js.as_string() {
    if s.len() > 50 {
        KeyType::StringKey100(FixedStringKey::<100>::from_str(&s))
    } else {
        KeyType::StringKey50(FixedStringKey::<50>::from_str(&s))
```

A string query value longer than 50 bytes was encoded as a `FixedStringKey<100>`,
but the writer only ever builds string attribute indices as `FixedStringKey<50>`
(see `add_indices_to_multi_http_index`, same file, which registers every `String`
column as `HttpIndex<FixedStringKey<50>>`). The 100-byte key could not be compared
against a 50-byte index.

**What a consumer saw:** any attribute query whose string value exceeded 50 bytes
failed with a key-type/length mismatch. The native reader always encodes a string
condition as the 50-byte key the index actually uses, then treats the index result
as *candidates* and post-filters them against each feature's full untruncated
string (`src/ts/src/post-filter.ts`); covered by `src/ts/test/stree.test.ts`
("string keys are truncated, so the index returns candidates") and
`post-filter.test.ts`.

## 11. wasm: `index_node_size` from the header is ignored on the HTTP path — NOT FIXED (crate removed; the same bug is live in `fcb_core`)

**Where:** `wasm/src/lib.rs:275` (`select_spatial_paged`) — and the same hardcode
still lives in `fcb_core/src/http_reader/mod.rs:220`.

Both call `PackedRTree::http_stream_search(..., PackedRTree::DEFAULT_NODE_SIZE, ...)`,
passing the compile-time default (16) instead of `header.index_node_size()`, even
though the very next lines read the header. A file written with any other node size
is traversed as if its R-tree branched by 16, walking the wrong node ranges.

**What a consumer saw:** a spatial query over HTTP against a file written with a
non-default R-tree node size returned wrong or missing features. This is not just a
wasm defect: `fcb_core`'s own HTTP reader shares the hardcode and is still live. The
native reader threads `header.info.indexNodeSize` into every R-tree traversal
(`src/ts/src/reader.ts`, `searchRtree`/`searchNearest`), and the corpus carries
`appearance_depths_node8.fcb` (node size 8) precisely to exercise it — covered by
`src/ts/test/packed-rtree.test.ts` ("honours a NON-DEFAULT index_node_size from the
header").

## 12. wasm: the gloo range client accepts a `200` full-body response as the requested range — NOT FIXED (crate removed)

**Where:** `wasm/src/gloo_client.rs:29-44` (`WasmHttpClient::get_range`).

```rust
let response = GlooRequest::new(url).header("Range", range).send().await…?;
if !response.ok() {              // 200 is "ok"
    return Err(HttpError::HttpStatus(response.status()));
}
response.binary().await…        // whole body, taken as the requested range
```

`response.ok()` is true for any 2xx, including a `200 OK` that ignored the `Range`
header and returned the *entire file*. The client then treated that full body as
the bytes for the requested `[offset, offset+len)` window, so every subsequent
offset was computed against data that started at byte 0 — silent corruption of all
later reads.

**What a consumer saw:** against a server (or CDN/proxy) that does not honour range
requests, reads appeared to succeed but returned bytes from the wrong offset,
producing garbage features or decode errors with no indication of the cause. The
native `FetchRangeReader` (`src/ts/src/io/fetch.ts`) requires a `206 Partial
Content` with a `Content-Range` that matches what it asked for, and raises
`RangeHeadersNotExposed`/an error otherwise — covered by
`src/ts/test/http.test.ts` ("THROWS when the server ignores Range and returns 200"
/ "throws when the server returns a DIFFERENT range than requested") and the
browser CORS test in `src/ts/test/browser/fetch.browser.test.ts`.

## 13. `PackedRTree::http_stream_search` can emit the extra leaf node twice — NOT FIXED (live in `fcb_core`)

**Where:** `fcb_core/src/packed_rtree/mod.rs:956-966`, with the `+1` at line 986.

When descending to the leaf level, the search extends a child node range by one
(`children_nodes.end += 1`, line 986) so it can read the *next* leaf's offset and
thereby size the last feature in the batch. But when that extended range is later
popped and iterated (the `for (node_pos, node_item) in node_items.iter().enumerate()`
loop, line 956), the loop evaluates the `bounds.intersects` predicate against **all**
fetched nodes, including that extra `+1` leaf — there is no guard restricting result
emission to the logical `[start, end)` of the range. The extra leaf is also the first
leaf of the adjacent sibling range and is evaluated again when that range is
processed. If it intersects the query box, it is emitted as a hit **twice**.

**What a consumer saw:** a bbox/point query that straddles a leaf-node boundary can
return a duplicate feature. The native reader evaluates only the `[start, end)`
half-open range of each node group and never re-evaluates the sizing leaf, so it does
not duplicate — covered by the bbox brute-force oracle tests in
`src/ts/test/packed-rtree.test.ts`, which compare the hit *set* against an exhaustive
scan.

## 14. `HeaderWriter::new_with_options` overwrote the caller's `index_node_size` — FIXED (this branch, Task 2)

**Where:** `fcb_core/src/writer/header_writer.rs:80-94`.

The constructor unconditionally reassigned `options.index_node_size =
PackedRTree::DEFAULT_NODE_SIZE`, so the field was write-only: whatever node size a
caller passed was discarded and every file was written with node size 16. The fix
keeps the caller's value and only forces it to `0` when `write_index` is false (the
header's way of saying "no R-tree"):

```rust
if !options.write_index {
    options.index_node_size = 0;
}
```

**What a consumer saw:** it was impossible to write a file with a non-default R-tree
node size, which also meant no reader could be tested against one — the bug hid
finding #11. Now fixed with the `appearance_depths_node8.fcb` fixture as its
regression witness.

## 15. The CLI conflated the R-tree node size with `attr_branching_factor` — FIXED (this branch, Task 2)

**Where:** `fcb_cli` write path, now `cli/src/main.rs:493-495`.

The R-tree `index_node_size` and the attribute B+tree `attr_branching_factor` are
unrelated knobs, but the CLI drove the header's `index_node_size` from the
attribute branching flag. The fix reads the R-tree node size from its own option:

```rust
// The R-tree node size, NOT the attribute B+tree branching factor:
// they are unrelated knobs and were previously driven by one flag.
index_node_size: options.index_node_size.unwrap_or(16),
```

**What a consumer saw:** setting the attribute branching factor silently changed the
spatial index node size (and vice versa), producing files whose header node size did
not match the caller's intent — again feeding finding #11.

## 16. `fcb_cli deser` broke its loop on `features_count`, truncating count-0 files — FIXED (this branch, Task 2)

**Where:** `fcb_cli` deserialize path, now `cli/src/main.rs:730-733`.

The decode loop stopped after `features_count` features. A header may legitimately
declare `0`, which means "unknown", not "empty" (see `conformance/no_count.fcb`,
which declares 0 and holds three features), so a count-0 file was truncated to zero
(or, under the older `while let Ok(..)` shape, a mid-file decode error was swallowed
and mistaken for a clean end of stream). The fix drives the loop off the iterator to
EOF and propagates errors with `?`:

```rust
while let Some(feat_buf) = fcb_reader.next()? {
    let feature = feat_buf.cur_cj_feature()?;
    writeln!(writer, "{}", serde_json::to_string(&feature)?)?;
}
```

**What a consumer saw:** `fcb_cli deser` on a file with an unknown (0) feature count
emitted only the metadata line and dropped every feature, exiting `0` as if it had
succeeded. The native reader's `scan` has the same EOF-not-count semantics
(`src/ts/src/reader.ts`), covered by the `no_count` conformance case in
`src/ts/test/conformance.test.ts`.
