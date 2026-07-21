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

The C++ reader still infers, and this change has not yet been mirrored there.
