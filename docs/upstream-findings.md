# Defects found in the Rust implementation while porting the C++ reader

Five defects in `fcb_core` surfaced during the native C++ port. Each is
reproducible, each caused a deliberate divergence in the C++ reader, and none
is caught by the existing Rust test suite.

**Status: all of #1-#6 (including #2a and #5) are now FIXED in this branch.**
Each fix has a regression test. #1 turned out to be an upstream `flatbuffers`
bug and is fixed by a version bump, which **changes the written file layout**.
#5 — long deferred as a structural change — now adopts the leaf-level
strict-bound design the C++ reader demonstrated; see its entry.

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

**Scope of this fix:** this item covers only the attribute-*index* decode
path (`reader/attr_query.rs`, used when building/querying a B+tree index
over an attribute column). It does not cover the separate feature-*value*
decode path — see item 2a, which is a distinct, still-open defect in the
same family.

---

## 2a. `Byte` feature-attribute value decode: still decodes as `i8` — FIXED

**Where:** `reader/deserializer.rs:375-383`, inside `decode_attributes`
(the function that turns a `CityObject`'s raw attribute bytes into the
`serde_json::Value` returned for every feature — i.e. the live
`to_cj_feature` path, not the index/query path item 2 covers):

```rust
ColumnType::Byte => {
    map.insert(
        column.name().to_string(),
        serde_json::Value::Number(serde_json::Number::from(
            bytes[offset] as i8,
        )),
    );
    offset += size_of::<u8>();
}
```

**Before/consequence:** the writer stores `Byte` as `u8` (same site as item
2: `writer/attribute.rs:138-141`, `out[offset] = b as u8;`), so a stored
`200` is the single byte `0xC8`. `decode_attributes` reads that byte back
via `bytes[offset] as i8`, so any consumer that reads a feature's
attributes through `fcb_core` (CLI `deser`, `to_cj_feature`, the Rust
library's normal read path) sees `-56` for a stored `200` — the exact
defect item 2 already fixed, but at a different call site that was never
touched. Item 2's "FIXED" applies only to the index/query path; this
value-decode path is a **separate, currently-unfixed** site with the same
bug.

C++ and Python do not reproduce `-56`, but not because they decode `Byte`
as unsigned at this layer — they refuse to decode it at all. Both
feature-value decoders (`src/cpp/src/attribute.cpp:145-156`,
`src/py/flatcitybuf/attribute.py:41-43,123-128`) raise
`UnsupportedColumnType`/`FcbError(UNSUPPORTED_COLUMN_TYPE)` for
`Byte`/`UByte`/`Binary` outright, each citing the same justification in
its own comment: *"the Rust reader hits `unreachable!()` on them
(deserializer.rs:372), so no file in the wild has ever had them read
back."* That justification is itself stale — item 3 above fixed exactly
that `unreachable!()`, and Rust's `decode_attributes` has decoded these
three types (not panicked on them) since. So today: a `Byte` feature
attribute is read (wrongly, as negative) by Rust, and rejected outright by
both C++ and Python, which is a live three-way disagreement beyond the
Rust-only sign bug this item is otherwise about. Fixing either side —
Rust's sign, or C++/Python's blanket rejection — is out of scope here;
recorded so whoever picks either up has both halves of the picture.

**Reproduce:** no existing conformance fixture exercises this, because the
writer's schema inference (`writer/attribute.rs::guess_type`) never
produces `ColumnType::Byte` from parsed CityJSON — every plain JSON integer
is guessed as `Long`/`ULong`, and there is no CLI flag or config to force a
column to `Byte`. A `Byte` column can currently only arise from a caller
that constructs an `AttributeSchema` by hand (bypassing `guess_type`), so
there is no fixture on disk and none of `scripts/gen_conformance.sh`'s
inputs (including `inferable_types.city.jsonl`) can be used as-is to
reproduce this through the CLI.

The following **was run** against this tree's `fcb_core` as an external
crate (constructing the `Header`/`CityFeature` FlatBuffers by hand, since
`guess_type` and the writer's `encode_attributes_with_schema` are
`pub(crate)`/`pub(super)` and cannot build a `Byte` column from outside the
crate) — this is an observed result, not a hypothetical one:

```rust
// Header: one column, `b`, ColumnType::Byte.
// Attributes buffer for one CityObject: [u16 LE col_index = 0][u8 value = 200],
// i.e. exactly what writer/attribute.rs would emit for {"b": 200} against
// a Byte-typed column.
let decoded = decode_attributes(&header_buf.columns().unwrap(), attributes);
println!("{}", decoded);
```
```
stored Byte value: 200
decode_attributes() result: {"b":-56}
```

**Fixed:** `decode_attributes`' `Byte` arm now reads `bytes[offset]` (u8),
matching the writer and the already-`u8` index path — the decision of record
for `Byte` is *unsigned, matching the writer*, the same direction item 2's
fix and the ports had already taken. Pinned by
`test_decode_attributes_byte_ubyte_binary` (`reader/deserializer.rs`), the
first direct unit test `decode_attributes` has ever had; it also pins the
`UByte` and `Binary` arms against the writer's wire format.

The three-way disagreement recorded below is resolved in the same pass: C++
(`src/cpp/src/attribute.cpp`) and Python (`src/py/flatcitybuf/attribute.py`)
now DECODE all three types instead of rejecting them, each mirroring the
Rust test's values (`b=200, ub=200, bin=[1,255]`) and each emitting `Binary`
in Rust's JSON shape (an array of numbers). TypeScript already decoded them;
its stale "the Rust reader disagrees" notes are retired and its
`Array.from` conversion at the CityJSON boundary is now pinned by test. All
four implementations agree. No conformance fixture exercises these types —
cross-language agreement rests on the mirrored unit tests; a shared corpus
fixture (requiring `.fcb` regeneration) is worthwhile follow-up.

A writer-side gap remains, recorded here rather than silently: the writer
reads a `Byte` source value via `as_i64` and wraps it to `u8` without
validation (`writer/attribute.rs`), so a *negative* JSON input, e.g. `-56`,
is stored as `200` and now reads back as `200` in all four implementations.
Round-trip of in-range values is correct; rejecting out-of-range input at
write time is the open improvement.

---

## 3. `Byte`/`UByte`/`Binary` attributes cannot be read back at all — FIXED

**Where:** `reader/deserializer.rs:372` — `unreachable!()`.

The writer emits these column types, but the reader panics on them. Any file
containing such an attribute is unreadable by the implementation that wrote it.

**C++ divergence:** decodes all three. Their widths are unambiguous (1, 1, and
`u32` length + bytes), so there is no reason to refuse them.

> **NOTE — resolved.** The C++/Python rejections described here, and the
> stale comments citing this item's `unreachable!()` (in
> `src/cpp/src/attribute.cpp`, `src/py/flatcitybuf/attribute.py`, and
> `src/cpp/src/key.cpp`'s mirror-image claim about `attr_query.rs`), were
> all reconciled in item 2a's fix pass: both ports now decode all three
> types, matching this item's fixed Rust reader.

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

## 5. `Gt`/`Lt`/`Ne` can drop genuine matches — FIXED

**Where:** `static_btree/query/stream.rs:161-191`.

These are lowered as "range minus `find_exact`", and the subtraction operates
on **feature offsets**. But one feature can appear under several keys when its
CityObjects carry different values of the indexed attribute — the writer
indexes each occurrence.

A feature holding both `k` and `k' > k` is returned by the range scan (via
`k'`) and also by `find_exact(k)` (via `k`), so the subtraction deletes it. It
is a false negative for a feature that genuinely matches.

**C++ divergence (no longer a divergence):** evaluates strict-or-inclusive
bounds at the leaf instead of subtracting. One traversal, no subtraction, no
false negatives.

**Fixed** by adopting exactly that design. All three tree traversal families
(`find_range`, `stream_find_range`, `http_stream_find_range` in
`static_btree/stree.rs`) gained strict-bound variants sharing one
`in_bounds` leaf predicate; the inclusive methods remain as thin wrappers.
All four lowering sites (`query/memory.rs`, `query/stream.rs`, and both cfg
copies in `query/http.rs`) now lower `Gt`/`Lt` to one strict-bound traversal
and `Ne` to two half-open traversals concatenated — the offset subtraction
is gone. The regression tests build the exact scenario this finding
describes (one feature indexed under two keys) and run it through the
memory, stream and http paths; they are also the first `Operator::Ne`
coverage anywhere in the Rust suite.

Two notes for the record:

- String `Gt`/`Lt`/`Ne` semantics on truncation-colliding keys are
  *unchanged*: Rust applies strict bounds uniformly and has no candidate
  post-filter (unlike C++ and TypeScript), so index keys remain its
  contract. C++'s string widening was deliberately not ported.
- The new tests exposed an unrelated pre-existing defect, fixed in the same
  pass: `StreamMultiIndex::query`'s empty-result early return skipped the
  cursor restore, so the *next* query on the same reader resolved index
  offsets from the wrong base (a hard `UnexpectedEof` once any query
  follows an empty-result one).

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

# Defects found while porting the native Python reader

The native Python implementation plan (retired after shipping; see git
history under `docs/superpowers/plans/`) built a
third, independent, pure-Python reader over `fcb_core` (Rust, the oracle) as
ground truth and the C++ reader as the direct porting reference. Comparing
all three over the same bytes — Rust's own CLI output, the C++ conformance
suite, and Python's — surfaced eight further C++-reader defects (§9-16, all
now **FIXED** on this branch as Task 15), one still-open Rust-reader defect
(§17), one FlatBuffers-codegen tooling defect (§18), a class of known,
disclosed limitations left in place (§19), the deliberate behavioural
divergences the new Python reader keeps against C++/Rust (§20), and one
defect in the plan document itself (§21).

Every citation below was re-checked against the source as it stands after
the Task 15 fixes landed (commits `86f0645`, `086acd7`, `0036ab9`); §9-16's
"before" quotes describe pre-fix behaviour that no longer exists in the tree.
Reproductions for §9-16 were re-run live for this write-up (not merely
copied from the implementer's report): `fcb_read_local`'s output on
`conformance/geom_decoder_edges.fcb` was parsed and compared for structural
equality against `conformance/geom_decoder_edges.expected.jsonl` (the Rust
oracle's output for the same file) — `EQUAL`; and
`./build-native/tests/fcb_tests -tc="querying a Json/Binary column is
rejected as unsupported"` was re-run — `1 | 1 passed`. The full suite is
127 test cases / 15915 assertions, all passing, confirmed by running
`./build-native/tests/fcb_tests` directly against the current tree.

## 9. `SemanticObject::parent` never emitted — FIXED

**Where:** `src/cpp/src/cityjson.cpp`'s semantics-surface builder inside
`geometry_to_json` (the `parent` check now sits at line 301, just after the
surface's `type` is set). Rust's `decode_semantics_surfaces` populates it
unconditionally (`src/rust/fcb_core/src/reader/geom_decoder.rs:217`,
`parent: s.parent(),`); the field is `parent:uint = null` in the schema
(`src/fbs/geometry.fbs:90`), and `cjseq2`'s `SemanticsSurface` struct
serializes it with `skip_serializing_if = "Option::is_none"`.

**Before:** the builder read `type`, `attributes`, and `children` off each
`SemanticObject` but never called `so->parent()` at all.

**Consequence for a consumer:** a `Door`/`Window` surface's back-pointer to
its enclosing `WallSurface` silently vanished. `children` survived (it comes
from the *parent* surface), so the tree looked navigable top-down but a
child-to-parent lookup silently returned nothing — no error, just a missing
key.

**Missed by the C++ suite because:** `conformance/geom_decoder_edges.fcb` is
the only fixture whose oracle output contains a semantic-surface `"parent"`
key, and it was not in `test_conformance.cpp`'s case list (now added at
line 86, with a comment explaining exactly this).

**Fix:** `if (const auto p = so->parent()) s["parent"] = *p;` — checked
against the FlatBuffers `Optional<uint32_t>` itself, not truthiness, since
`parent: 0` is a real, non-absent value a `if (so->parent())`-on-a-plain-int
reading would have swallowed just as easily as omitting it entirely.

**Reproduce (re-run for this write-up):**
```
$ ./src/cpp/build-native/fcb_read_local conformance/geom_decoder_edges.fcb \
    > /tmp/out.jsonl
$ python3 -c "
import json
lines = [json.loads(l) for l in open('/tmp/out.jsonl')]
for l in lines[1:]:
    for oid, obj in l['CityObjects'].items():
        for g in obj.get('geometry', []):
            if g.get('semantics'): print(oid, g['semantics'])
"
semantics_parent {'surfaces': [{'children': [1], 'type': 'WallSurface'}, {'parent': 0, 'type': 'Door'}], 'values': [0, 1]}
```
`{'parent': 0, ...}` is present and equals the Rust oracle
(`conformance/geom_decoder_edges.expected.jsonl`) exactly.

## 10. Header `pointOfContact`/`referenceDate` never emitted — FIXED

**Where:** `to_cityjson_metadata` in `src/cpp/src/cityjson.cpp` (now lines
447-524-ish); support added in `src/cpp/include/fcb/header.hpp` (`FileInfo`
gained `reference_date` at line 62, `poc_email` at line 71, plus ten more
`poc_*`/`poc_address_*` fields) and `src/cpp/src/header.cpp`'s
`fill_metadata` (line 80; populates `reference_date` at line 113,
`poc_email`/`has_poc_email` at lines 123-125). Rust builds both
unconditionally from the header in `to_cj_metadata`
(`src/rust/fcb_core/src/reader/deserializer.rs:77-90`), reading
`Header.reference_date` (`src/fbs/header.fbs:143`) and
`Header.poc_contact_name` (`src/fbs/header.fbs:151`, the field whose
presence alone gates the whole `pointOfContact` object on the Rust side) —
both stored natively in the FlatBuffer, nothing derived.

**Before:** `to_cityjson_metadata` built only `geographicalExtent`,
`referenceSystem`, `identifier`, `title` — it never read any `poc_*` field
or `reference_date` at all. This had been acknowledged in a C++ test
comment but never actually tested.

**Consequence for a consumer:** every dataset's contact/provenance metadata
(who to contact about the data, and as-of what date) silently disappeared
on the C++ path, even though the source file carried it.

**Fix + reproduce (re-run for this write-up):**
```
$ ./src/cpp/build-native/fcb_read_local conformance/geom_decoder_edges.fcb \
    | head -1 > /tmp/line0.json
$ python3 -c "
import json
a = json.load(open('/tmp/line0.json'))
b = json.loads(open('conformance/geom_decoder_edges.expected.jsonl').readline())
print('EQUAL' if a == b else 'DIFFERENT')
"
EQUAL
```
The metadata line now carries the full `pointOfContact` (`contactName`,
`contactType`, `role`, `phone`, `emailAddress`, `website`, a nested
`address`) and `referenceDate`, structurally identical to the Rust reader's
output for the same file.

**Evidence is narrower than it looks — flagged explicitly:** `examples/data/delft.fcb`
alone cannot exercise this defect or #9: it has zero `"parent"` occurrences
across 1115 features and no `referenceDate` in its metadata. Only the
purpose-built `conformance/geom_decoder_edges.fcb` fixture (added alongside
the Python geometry/CityJSON task) carries a `parent`-bearing semantic
surface and a full `pointOfContact`/`referenceDate`. `delft.fcb` does confirm
a *partial* form of this defect on real-world data (see its own
`pointOfContact` without an `address`, checked below), but not the address
sub-object or `referenceDate`.

## 11. `select_attr` never type-checks Json/Binary columns — FIXED

**Where:** `FcbReader::select_attr`, `src/cpp/src/reader.cpp:327-459`; the
new guard sits at lines 360-367, immediately after column resolution and
before the "is it indexed" lookup. `key_kind_for_column`
(`src/cpp/src/key.cpp:326-352`) maps `ColumnType::Json` (line 348) and
`ColumnType::Binary` (line 349) to `KeyKind::String100` — a real, working
key kind — so without the new guard `select_attr` would happily execute a
query against a Json/Binary column's index (were one ever built for it) and
return truncated-blob-prefix candidates as if they meant something. Rust's
`attr_query.rs` has no dedicated arm for either type in its two
column-type matches (`src/rust/fcb_core/src/reader/attr_query.rs:38-139` and
`:158-279`); both fall through to the catch-all
`_ => return Err(Error::UnsupportedColumnType(...))` at lines 139 and 279,
i.e. Rust rejects the query outright.

**Consequence for a consumer:** C++ would answer a Json/Binary attribute
query — silently wrong, since a Json/Binary index key is only the first 100
bytes of a serialized blob and says nothing about the decoded value. In
practice this was unreachable through the writer alone (it never builds an
index over a Json/Binary column, `-A` included), which is exactly why the
regression test below has to hand-construct a `HeaderView` carrying one.

**Fix:** reject `ColumnType::Json`/`ColumnType::Binary` unconditionally,
before the index-lookup — so rejection does not depend on whether this
particular writer happened to index the column. Mirrors Python's
`stree.py::_resolve` (its own comment calls this "DIVERGENCE 2" — see §20
below), which does the identical check for the identical reason.

**Reproduce (re-run for this write-up):**
```
$ cd src/cpp && ./build-native/tests/fcb_tests \
    -tc="querying a Json/Binary column is rejected as unsupported"
[doctest] test cases: 1 | 1 passed | 0 failed | 126 skipped
[doctest] assertions: 2 | 2 passed | 0 failed |
```
Test uses `conformance/inferable_types.fcb`'s `a_json` column (confirmed
*not* indexed by the writer), so it specifically pins "reject regardless of
indexed-ness." Before the fix this threw `AttributeIndexNotFound` (code 4)
instead of `UnsupportedColumnType` (code 7) — verified TDD-style by the
implementer (guard reverted, single case rerun, confirmed the `4 != 7`
mismatch, guard restored).

## 12. Top-level `extensions` never emitted — FIXED

**Where:** `to_cityjson_metadata`, `src/cpp/src/cityjson.cpp`; new
`extensions_to_json(const ::Header*)` helper at lines 413-433, wired in at
line 502. Mirrors Rust's extensions block in `to_cj_metadata`
(`src/rust/fcb_core/src/reader/deserializer.rs:33-49`): builds a
name→`{url, version}` map, skips entries with no name (verified by reading
the Rust source directly, lines 33-49 above), and omits the whole
`extensions` key when the map ends up empty — not merely when the header's
`extensions` vector is empty or absent (`if !extensions_map.is_empty()`,
deserializer.rs line 48).

**Consequence for a consumer:** a dataset using a CityJSON Extension (the
fixture `noise_extension.fcb` uses one) lost the `extensions` block
entirely — a consumer had no way to resolve the extension's schema URL or
version, even though every extended attribute (`+noise-buildingLNightMax`
etc.) was still present and decodable.

**Found by:** widening `test_conformance.cpp`'s line-0 metadata check from
four hand-picked keys to a full-object compare (`CHECK(actual[0] ==
expected[0])`, now at line 51) — `noise_extension.fcb`'s Rust-oracle output
carries `"extensions":{"Noise":{"url":"...","version":"1.1"}}` and C++
emitted nothing until this fix.

## 13. `metadata`/`metadata.geographicalExtent` presence-gated, unconditional in Rust — FIXED

**Where:** `to_cityjson_metadata`, `src/cpp/src/cityjson.cpp:465-491`. Rust's
`to_cj_metadata` (`deserializer.rs:81-90`) always sets
`cj.metadata = Some(CjMetadata { geographical_extent: Some(...), ... })`,
where the extent itself is `header.geographical_extent().map(...)
.unwrap_or_default()` — i.e. Rust *always* emits a `metadata` object with at
least a (possibly all-zero) `geographicalExtent`, never omits either key.

**Before:** C++ emitted `metadata` only when at least one sub-field was
non-empty, and omitted `geographicalExtent` specifically when the header
carried no `GeographicalExtent` struct.

**Consequence for a consumer:** a file with no extent metadata at all (e.g.
`noise_extension.fcb`, whose source JSONL has no `"metadata"` key) got a
CityJSON envelope missing `metadata`/`geographicalExtent` entirely instead
of the spec-conformant `[0,0,0,0,0,0]` default Rust emits — a schema
consumer expecting `metadata.geographicalExtent` to always exist would
break specifically on C++ output.

**Fix:** `cj["metadata"]` and `meta["geographicalExtent"]` are now always
set (the latter defaulting to `std::array<double, 6>{}` when
`info.has_extent` is false); every other metadata field stays conditional,
matching the `Option`s on `CjMetadata`.

## 14. `transform` conditional, but unconditional in Rust (defaults `[1,1,1]`/`[0,0,0]`) — FIXED

**Where:** `to_cityjson_metadata`, `src/cpp/src/cityjson.cpp:454-463`. Rust's
`to_cj_metadata` starts from `CityJSON::new()`
(`deserializer.rs:23`), whose `Transform::new()` defaults to
`scale: [1.0, 1.0, 1.0]`, `translate: [0., 0., 0.]` (verified directly
against the `cjseq2` registry source, `lib.rs:1057-1064`, pinned at
version 0.1.1 in `Cargo.lock`), and only overwrites it when
`header.transform()` is `Some` (`deserializer.rs:25-31`) — `CjTransform` is
a plain, non-`Option` field, so the key is **never** omitted from Rust's
output.

**Before:** C++ set `cj["transform"]` only when `info.has_transform`,
omitting the key entirely for a header with no `Transform` struct.

**Consequence for a consumer:** a file written without an explicit
`Transform` (identity scale/translate) lost the `transform` key altogether
on the C++ path, instead of getting the spec-implied identity default a
consumer might rely on being present.

**Fix:** `transform` is now unconditional, defaulting to
`{"scale":[1,1,1],"translate":[0,0,0]}` when `has_transform` is false.
Test: `test_cityjson.cpp`'s `"transform is emitted even when the header
carries none"` (line 92), which calls `to_cityjson_metadata(HeaderView{})` —
a default, byte-less header — and asserts the exact defaults; TDD-verified
failing pre-fix (`REQUIRE(cj.contains("transform"))` false) and passing
after.

## 15. `pointOfContact.emailAddress` treated as optional; required in Rust — FIXED

**Where:** `point_of_contact_to_json`, `src/cpp/src/cityjson.cpp:394-411`.
`poc_contact_name` and `poc_email` are independently optional FlatBuffer
fields (`src/fbs/header.fbs:151,155`), so a header can legally carry a
contact name with no email. Rust's `to_cj_point_of_contact`
(`deserializer.rs:168-177`) treats `email_address` as a hard requirement:
`.ok_or(Error::MissingRequiredField("email_address".to_string()))?`, and
that `?` propagates out of the *entire* `to_cj_metadata` call
(`deserializer.rs:79`, `to_cj_point_of_contact(header)?`) — confirmed
against `cjseq2`'s `PointOfContact` struct, where `email_address` is a
plain `String` (no `skip_serializing_if`), unlike `contact_type`, `role`,
`phone`, `website`, `address`, which are all `Option<T>`.

**Before:** C++ emitted `emailAddress` conditionally, the same as the other
optional fields, so a header with a contact name and no email produced an
*incomplete* `pointOfContact` object instead of Rust's hard failure.

**Consequence if not fixed:** C++ and Rust would disagree on whether such a
file is even readable at the metadata level — C++ silently degrades where
Rust aborts the whole header decode.

**Fix:** `if (!info.has_poc_email) throw Error(ErrorCode::MissingRequiredField,
"email_address");`, then `emailAddress` emission becomes unconditional past
that check. Test: `"pointOfContact without emailAddress fails like Rust's
required field"` (`test_cityjson.cpp:106`) — a synthetic minimal `.fcb`
(hand-built via `CreateHeaderDirect`, since no committed fixture has a
contact name without an email) confirming the throw; TDD-verified against
the pre-fix code (the throw did not happen, test failed as predicted).

## 16. `poc_email` absent-vs-present-but-empty conflated — FIXED

**Where:** the fix for §15 above initially gated on
`info.poc_email.empty()` — but `FileInfo::poc_email` is a plain
`std::string` (`src/cpp/src/header.cpp:123-124`,
`if (hdr->poc_email() != nullptr) info.poc_email = hdr->poc_email()->str();`),
which cannot distinguish "field absent" from "field present with an
explicit empty string": both collapse to `""`. Rust's
`header.poc_email()` returns `Option<&str>`, and a present-but-empty
FlatBuffer string is `Some("")` — which satisfies `.ok_or(...)` and
produces `email_address: ""` with **no error**
(`deserializer.rs:175-177`). Only a genuinely absent field (`None`)
triggers the error.

**Consequence if left as introduced by §15's first pass:** a legitimate
header with `poc_contact_name` set and `poc_email` explicitly set to the
empty string would incorrectly throw `MissingRequiredField`, where Rust
succeeds with `emailAddress: ""` — the fix for §15 would have been *more*
eager than Rust, rejecting files Rust accepts.

**Fix:** added `bool has_poc_email = false;` to `FileInfo`
(`src/cpp/include/fcb/header.hpp:82`), set alongside `poc_email` in
`fill_metadata` (`src/cpp/src/header.cpp:123-125`); the gate in
`point_of_contact_to_json` changed from `info.poc_email.empty()` to
`!info.has_poc_email`. Test:
`"pointOfContact with a present-but-empty emailAddress does not throw"`
(`test_cityjson.cpp:130`) — TDD-verified failing before this second pass
(`ERROR: test case THREW exception: email_address`) and passing after.

**Checked for the same escalation pattern elsewhere:** every other
`FileInfo` string field (`identifier`, `title`, `crs`, the other `poc_*`)
has the identical absent-vs-empty conflation but feeds no `throw` — see §19
below, left as a disclosed, lower-severity limitation rather than fixed
here.

**Superseded (mechanism only, not behaviour):** §19/§20.11 later made
those other fields `std::optional<std::string>` too, so the standalone
`has_poc_email` flag was deleted — `poc_email.has_value()` is the same
test on the field itself. The gate and its throw semantics are unchanged,
and the test above still pins them.

---

## 17. Rust reader: `attr_query.rs` has no `ColumnType::Long` arm — FIXED

**Confirmed directly from source, both sides, for this write-up:**

- Writer: `src/rust/fcb_core/src/writer/attr_index.rs:112` —
  `ColumnType::Long => build_index_generic::<i64, _>(...)` — the writer
  builds a real B+tree index over `i64` values for a `Long` column, no
  different in kind from `Int`/`ULong`/etc.
- Reader: `src/rust/fcb_core/src/reader/attr_query.rs` has two matches on
  `col.type_()` (`grep -n "ColumnType::" attr_query.rs`, lines 38-131 and
  158-270): both list arms for `Int`, `Float`, `Double`, `String`, `Bool`,
  `DateTime`, `Short`, `UShort`, `UInt`, `ULong`, `Byte`, `UByte` — **no
  `Long` arm in either** — falling through to the shared catch-all
  `_ => return Err(Error::UnsupportedColumnType(col.name().to_string()))`
  at lines 139 and 279.

So a `Long` column, indexed correctly by the writer, cannot be queried by
`fcb_core`'s own reader — a genuine Rust-side gap, not merely a C++/Python
port artifact. C++'s `key_kind_for_column`
(`src/cpp/src/key.cpp:342`, `case ::ColumnType::Long: return
KeyKind::Int64;`) maps it to a fully-supported `KeyKind`, so **C++ answers a
`Long`-column query that Rust itself refuses.**

**Consequence for a consumer:** a Rust `fcb_core` caller building an
attribute query against an indexed `i64` column gets
`Error::UnsupportedColumnType` for data the writer happily indexed and that
both C++ and (per the plan's divergence policy) Python answer correctly.

**Fixed:** the `Long` arm was added to all *three* sites — the two matches in
`reader/attr_query.rs` this entry cites, plus a third with the same gap in
`http_reader/mod.rs`'s `add_indices_to_multi_http_index`. Regression test
`test_attr_index_long_column` (`fcb_core/tests/attr_index.rs`) covers the
seekable and sequential query paths; the http arm has compile-time coverage
only (the mock client is `#[cfg(test)]`-gated away from `tests/`). One
nuance worth keeping: `guess_type` checks `is_u64()` before `is_i64()`, so
plain *positive* JSON integers infer as `ULong` — only a negative integer
produces a `Long` column, which is why the test injects negative values and
asserts `col.type_()` on the written header.

## 18. Tooling: `flatc --gen-onefile` emits no cross-schema-file imports — FIXED (generation-time workaround)

**Where:** `flatc`'s Python codegen, one file per top-level `.fbs`, invoked
by `scripts/gen_python_fbs.sh`. A table in one file referencing a type
`include`d from another (`feature.fbs`'s `CityObject.columns: [Column]`,
where `Column` is defined in `header_generated.py`) emits an **unqualified**
`Column()` constructor call in the generated accessor body, with no import
statement. Python resolves that name against the *defining* module's
globals at call time, which does not have it — so calling
`CityObject.Columns(j)`, `CityObject.Geometry(j)`, and
`CityFeature.Appearance()` all raised `NameError: name 'Column' is not
defined` (or the equivalent for `Geometry`/`Appearance`) at runtime.

**Why C++ doesn't have this problem:** C++'s generated headers `#include`
each other transitively, so a cross-file reference just resolves through
the preprocessor. There is no C++ analogue to port a fix from — this is a
Python-codegen-specific defect in the upstream FlatBuffers compiler's
`--gen-onefile` mode, not a port artifact.

**Fix:** `scripts/gen_python_fbs.sh`'s `__init__.py`-generation step appends
a `setattr` backfill loop after the existing re-export lines, setting every
re-exported class as an attribute on every generated submodule that doesn't
already define it under that name (`hasattr`-guarded, so a no-op where the
name is defined locally). Generation-time, not a hand-edit of committed
generated code; regenerating twice produces byte-identical output, and only
`generated/__init__.py` changes — the four `*_generated.py` files are
untouched.

**Reproduce:** before the fix, `python3 -c "..."` calling
`obj.Geometry(0)` on a real fixture raised
`NameError: name 'Geometry' is not defined` inside
`feature_generated.py`'s `Geometry` accessor. This was load-bearing for the
Python port's own Task 7/8 work (`decode_attributes`/`to_cityjson_feature`
cannot resolve per-object schema or geometry without it).

## 19. Known limitations left in place — NOT FIXED, disclosed

- ~~**C++ `FileInfo` conflates absent and empty-string for `identifier`,
  `title`, `crs`, and every `poc_*`/`poc_address_*` field except
  `poc_email`**~~ — **FIXED for every field except `crs`**, together with
  §20.11 (see there for the full contract and the fix). `identifier`,
  `title`, `reference_date` and `poc_contact_name`/`_contact_type`/`_role`/
  `_phone`/`_email`/`_website` are now `std::optional<std::string>` in
  `src/cpp/include/fcb/header.hpp`, populated from `fill_metadata`'s
  existing `!= nullptr` guards, and gated on `has_value()` in
  `to_cityjson_metadata`/`point_of_contact_to_json`; `has_poc_email` is
  deleted, subsumed by `poc_email.has_value()`. Two exclusions are
  deliberate, not oversights:
  - `poc_address_*` stays `std::string`, because non-emptiness — not
    presence — is what Rust itself checks there: `to_cj_address`
    (`deserializer.rs:195-216`) emits each member iff non-empty and the
    sub-object only when at least one member survives.
  - `crs` stays `std::string`, because it is *derived* (authority + code
    from `Header.reference_system`), not read from a schema-optional
    string. Its own gating divergence is §20.10, still open, and needs
    Rust/C++/Python treated together.
- ~~**`std::from_chars` rejects a leading `+` on the address thoroughfare
  number, where Rust's `i64::from_str` accepts it.**~~ — **STALE on both
  premises; struck rather than fixed (2026-08-13 pass, verified against
  the tree).** The parsing reader this bullet cites no longer exists: the
  address model became all-strings (cjseq2 0.2.0-alpha.1), so
  `point_of_contact_address_to_json` (now `src/cpp/src/cityjson.cpp`,
  `~467-481`) does no parsing at all — non-empty test plus verbatim string
  copy, mirroring Rust's `to_cj_address` arm for arm. `thoroughfareNumber`
  is a *string* in the schema (`src/fbs/header.fbs:157`), so no parse can
  reach it on the writer side either: both writers pass `"+42"`, `""`, and
  non-numeric strings through identically. The one `from_chars` remaining
  in `src/cpp` is `parse_i32_whole` on the unrelated `referenceSystem` URL
  path (`src/cpp/src/writer/header_serializer.cpp:72-91`), and it already
  strips a leading `+` to match Rust's `parse::<i32>()`, with regression
  tests (`src/cpp/tests/test_writer_header.cpp:64-81`) — nine edge cases
  checked, C++ and Rust agree on every one.

---

## 20. Deliberate divergences in the new Python reader

These are not defects: each is a considered choice, recorded here so all
three implementations' behaviour on the same input is on record in one
place. Source: `src/py/flatcitybuf/{keys,stree,range_reader,reader,http_reader}.py`.

**The four pre-decided divergences** (named in the plan, implemented and
documented at both the point of use and in `search_stree`'s public
docstring, `src/py/flatcitybuf/stree.py:732+`):

1. **`Byte` index keys decode as `u8`, matching the writer** — and, as of
   this branch, matching Rust's own index reader too. The writer stores
   `Byte` as `u8` and builds `MemoryIndex<u8>` (`writer/attribute.rs:209`,
   `writer/attr_index.rs:240`); Python's key encoding (`keys.py`,
   `column_type_to_key_kind`) and C++'s (`key.cpp:328-335`) both map `Byte`
   to an unsigned key for exactly this reason. **Correction:** earlier text
   here claimed this still disagreed with "Rust's own reader"
   (`reader/attr_query.rs:118`) and called it "the same defect... recorded
   as item 2" — that was wrong on two counts. First, line 118 there is the
   *comment* documenting that fix, not the bug; the actual decode (line
   123) is `MemoryIndex::<u8>`, i.e. already `u8`. Second, item 2 is marked
   FIXED precisely because that index-path mismatch was resolved — so
   Python's index-key choice and Rust's current index reader now agree,
   and there is no live divergence at this layer to record.

   The one remaining divergence in this family — Rust's feature-attribute
   *value* decode returning `-56` for a stored `200`, one layer up from the
   index-key encoding this point is about — is now fixed too; see item 2a,
   whose fix pass also replaced Python's outright `Byte` rejection at that
   layer with a real (unsigned) decode. As of this branch there is no live
   divergence anywhere in the Byte family: writer, index path and value
   path all agree on `u8` in all four implementations.
2. **Json/Binary index queries are rejected** —
   `stree.py::_resolve` (`src/py/flatcitybuf/stree.py:663`, its own comment
   labels this "DIVERGENCE 2"), checked before the "is it indexed" lookup,
   for the identical reason as §11's C++ fix above (and implemented in
   Python *first* — Task 10 predates Task 15). Consequence: **Python
   rejects a Json/Binary attribute query that C++, before Task 15, would
   have silently answered** — that gap is now closed on the C++ side too
   (§11), so as of this branch all three implementations agree on this
   point; it is recorded here because it was, for most of the branch's
   life, a genuine three-way disagreement, not merely a Python quirk.
3. **Float `key_max()` is `+inf`**, so a NaN-keyed feature is invisible to
   any range query (`Ge`/`Le`/etc.) even though it exists in the file.
4. **`DateTime` `key_min()` is epoch 0 (1970-01-01T00:00:00Z)**, so a
   pre-1970 feature is invisible to `Le`/`Lt`/`Ne` range queries.

**Further Python-specific divergences, each with no C++ analogue** (from
`src/py/flatcitybuf/range_reader.py`, `http_reader.py`, `stree.py`):

5. **`FileRangeReader.read` raises `FcbError(INDEX_OUT_OF_BOUNDS)` for
   `offset > total_size()`**, where C++'s `range_reader.cpp:30` treats
   `offset >= total_size()` as "return empty" uniformly. Python matches
   C++ exactly at `offset == total_size()` (returns `b""`) but diverges
   strictly past it — because `_build_tree` in `stree.py` needs an explicit
   bounds check where C++ gets the same protection for free from its
   `RangeReader` wrapper (`range_reader.hpp:56-59`).
6. **A Range-ignoring HTTP `200` response is rejected outright**, where
   C++'s `CurlRangeReader` slices the full body itself and proceeds
   (`src/py/flatcitybuf/http_reader.py:88-91`, docstring). Brief-mandated;
   the tradeoff is a hard failure against a non-conformant server instead
   of quietly reading (and re-fetching) more data than requested.
7. **No `asyncio`/persistent-connection/streaming API exists at all** — the
   Rust crate's async reader was retired in Task 13 alongside the PyO3
   wheel it lived in (`src/rust/fcb_py/src/async_reader.rs`, deleted), and
   the pure-Python reader never had one to begin with. This is the one
   capability genuinely lost by moving off PyO3, not merely a stylistic
   choice, per the Task 13 report.
8. **`_int_key`'s explicit out-of-range rejection** (`keys.py`): Python
   integers are unbounded, so a caller can hand in e.g. `2**70` for a
   `u64` key; C++ would silently truncate via `static_cast`. Python raises
   instead — a strictness with no C++ counterpart to diverge from, since
   the hazard doesn't exist in a fixed-width-integer language the same way.
9. **String keys are `bytes`, not `str`**, and the float/NaN comparator
   (`_cmp_ordered_float`) is *mandatory* in Python where it is merely
   stylistic in C++ — `nan != nan` and Python's `sorted()` is not a total
   order under NaN, so `compare_keys` must be used everywhere a `<`/`==`
   would otherwise silently corrupt tree-ordering invariants. (Detailed in
   the Task 10 report §13; not a behavioural difference in output, just a
   correctness requirement unique to the host language.)

**Two more, found comparing all three implementations during the Python
port, but *not* Python-specific — Python inherited both from C++, which
means fixing Python alone would create a new Python-vs-C++ split rather
than closing one.** Documentation only; no reader code changed for
either.

10. **`referenceSystem` URL construction disagrees with Rust on
    version/authority/code_string, identically in Python and C++.** Rust
    builds the URL from `rs.authority().unwrap_or_default()`,
    `rs.version()` (an `int` per `src/fbs/header.fbs:44`) and `rs.code()`
    (`src/rust/fcb_core/src/reader/deserializer.rs:53-59`), feeding them to
    `ReferenceSystem::to_url`, which formats
    `"{base}/{authority}/{version}/{code}"` (`cjseq2` 0.1.1
    `lib.rs:1128-1133`); this only runs at all when
    `header.reference_system()` is `Some` (`Metadata::reference_system` is
    `Option<ReferenceSystem>` with `skip_serializing_if =
    "Option::is_none"`, `lib.rs:1197-1198`), so an absent
    `ReferenceSystem` table correctly omits the key.

    Python instead derives the whole thing from `FileInfo.crs`, built in
    `src/py/flatcitybuf/header.py:206-215`: authority defaults to the
    literal string `"EPSG"` when `rs.Authority()` is absent (not Rust's
    `unwrap_or_default()`, which is an empty string); the numeric `Code()`
    is preferred whenever non-zero, falling back to `CodeString()` only
    when `Code() == 0`; and `info.crs` is left unset (dropping
    `referenceSystem` entirely) when both `Code() == 0` and
    `CodeString()` is absent. `src/py/flatcitybuf/cityjson.py:782-785`
    then reconstructs the URL by splitting `info.crs` on `:` and
    hardcoding the version segment to the literal `/0/`, regardless of
    the header's actual `Version()`. `src/cpp/src/cityjson.cpp:479-484`
    builds the identical `.../{authority}/0/{code}` string from the
    identical `FileInfo.crs` split — this is not a Python-only shortcut,
    C++ made the same choice.

    Concretely: (a) any header with `version != 0` gives `/0/` in both
    Python and C++ against `/{version}/` in Rust; (b) a `ReferenceSystem`
    table present with `code == 0` and no `code_string` makes Rust emit
    `.../EPSG/0/0` while Python/C++ omit `referenceSystem` outright — the
    same presence-vs-value gating class as §13; (c) a `code_string`-only
    CRS is emitted by Python/C++ and dropped by Rust.

    Matching Rust exactly is not obviously the right fix, and is worth
    saying honestly: it would mean reproducing what
    `unwrap_or_default()` does for a genuinely absent authority — a URL
    like `.../def/crs//0/0` — which reads as a Rust defect (an
    unhelpful literal empty path segment) rather than a contract worth
    porting faithfully.

    **Deliberately deferred (2026-08-13 fix pass):** every other open item
    in this document that was decision-free was closed in that pass; this
    one was left untouched *because* the right contract is a maintainer
    decision — whether Rust's URL construction (actual `version`, empty
    authority segment, `code == 0` emitted) is the behaviour to port, or
    itself the defect to fix in all three implementations at once.

    No test catches any of this because `small.fcb` — the only corpus
    case with a `referenceSystem` at all — happens to carry `EPSG:7415`
    with `version == 0` and a non-zero `code`, the one configuration
    where Python/C++'s reconstruction and Rust's direct field read agree
    by accident (both produce
    `https://www.opengis.net/def/crs/EPSG/0/7415`). A non-zero version, a
    `code_string`-only CRS, `code == 0`, and an absent authority are all
    untested in **all three** implementations. Recommend a follow-up
    upstream finding covering Rust, C++ and Python together, rather than
    a unilateral Python fix.

11. **`identifier`/`title` are gated on non-emptiness, not presence,
    identically in Python and C++ — FIXED.** Rust's `to_cj_metadata` reads
    `header.identifier().map(|i| i.to_string())` and
    `header.title().map(|t| t.to_string())`
    (`src/rust/fcb_core/src/reader/deserializer.rs:85,89`) — both fields
    are `Option<String>` on `cjseq2`'s `Metadata`
    (`lib.rs:1188-1189,1200-1201`) with `skip_serializing_if =
    "Option::is_none"`, so a **present-but-empty** FlatBuffers string
    (`Header.identifier()`/`Header.title()` returning `Some("")`)
    serializes as `"title": ""` — omitted only when the field is
    genuinely absent (`None`).

    Python gates on truthiness instead of presence:
    `src/py/flatcitybuf/cityjson.py:773` (`if info.identifier:`) and
    `:787` (`if info.title:`) both drop the key for an empty string, the
    same as a `None`/absent one. `src/cpp/src/cityjson.cpp:485,490`
    (`if (!info.identifier.empty()) ...`, `if (!info.title.empty())
    ...`) makes the identical choice — this is the same gating class
    §13 already documents for C++'s `metadata`/`geographicalExtent`, just
    on two more fields, and it is not C++-only.

    Worth noting precisely where the information is actually lost in
    Python, since it is not only the truthiness check:
    `src/py/flatcitybuf/header.py:225-231` already reads
    `Identifier()`/`Title()` guarded on `is not None` (so the read itself
    is presence-aware), but `FileInfo.identifier`/`.title`
    (`header.py:105-106`) default to `str = ""` rather than
    `Optional[str] = None` — so the dataclass field cannot represent
    "absent" separately from "present and empty" either. A fix at
    `cityjson.py:773,787` alone (swapping the truthiness check for `is
    not None`) is not sufficient by itself; `FileInfo`'s default would
    also need widening to `Optional[str] = None` for the distinction to
    survive end to end. The follow-up should treat both files together,
    not just the gate.

    Concrete consequence: a source CityJSONSeq with
    `"metadata": {"title": ""}` round-trips through Rust with the header
    line carrying `"title": ""`, while Python's and C++'s header lines
    omit the key entirely — a consumer testing `"title" in metadata`
    gets a different answer per implementation. Not corpus-reachable (no
    fixture has an explicitly-empty `identifier`/`title`), so the
    whole-line conformance compares that caught §12/§13 (a full-object
    `assert actual[0] == expected[0]` on each fixture's header line, in
    both the C++ and Python suites) cannot catch this either — no fixture
    exercises the input in the first place. Recommend the same follow-up
    as #10: one upstream finding covering all three implementations, not
    a unilateral Python change.

    **Fix.** Both readers now gate on presence, and the fix reaches the
    representation as this finding required, not only the gate:

    - **Python.** `FileInfo.identifier`/`.title`
      (`src/py/flatcitybuf/header.py`) are `str | None = None`;
      `_fill_metadata`'s `is not None` guards were already
      presence-aware and now survive into the dataclass. The gates in
      `src/py/flatcitybuf/cityjson.py` became `if info.identifier is not
      None:` / `if info.title is not None:`. `referenceDate` and
      `pointOfContact` there already read the raw header through
      presence-guarded accessors and were correct.
    - **C++.** The nine schema-optional header strings became
      `std::optional<std::string>` — see §19's first bullet for the
      field list, the two deliberate exclusions (`crs`,
      `poc_address_*`), and the deletion of `has_poc_email`.
    - **TypeScript** was already correct (`hdr.identifier() ?? undefined`
      into a `put()` that gates on `!== undefined`) but unpinned; it now
      carries the same test.

    Each implementation pins BOTH halves — present-but-empty emits `""`,
    absent omits the key — against a hand-built header buffer, since the
    corpus cannot express the input. The TS pin was verified to fail when
    `?? undefined` is mutated to `|| undefined`.

    Still open in the same class: `crs`/`referenceSystem` (§20.10), which
    is derived rather than read from a schema-optional string and needs
    Rust, C++ and Python changed together.

---

## 21. Plan-document defect: `rtree_index_size(1, 16)` — the plan asserts 40; the correct value is 80

**Where:** the native Python implementation plan's own test snippet (plan
retired after shipping; see git history under `docs/superpowers/plans/`, line
340 of the retired file),
inside `test_rtree_index_size_matches_the_reference_formula`:
`assert rtree_index_size(1, 16) == 40`. The shipped implementation and its
formula are correct (`docs/specification.md:131` gives
`rtree_index_size`) — only the plan's illustrative test snippet had the
wrong worked example.

**This is wrong.** Tracing the reference loop for `num_items=1,
node_size=16`: `num_nodes = 1` (the leaf level); the loop runs **at least
once regardless of the input** (`n = ceil_div(1, 16) = 1; num_nodes = 1 + 1
= 2;` then `n == 1` breaks) — giving `num_nodes = 2`, i.e.
`2 * 40 = 80`, not `40`. Confirmed directly against the Rust source,
`src/rust/fcb_core/src/packed_rtree/mod.rs:879-898`
(`PackedRTree::index_size`, a `loop { ... if n == 1 { break; } }` — a
do-while shape that always executes its body once, even starting from
`n = num_items = 1`), and against
`src/cpp/tests/test_layout.cpp:38`, which already asserted
`CHECK(rtree_index_size(1, 16) == 80)` before the Python task ever ran —
i.e. the C++ port had this right, and the plan document (written after the
C++ port) introduced the transcription error independently. The intuition
the plan's `== 40` seems to assume — a single item collapsing leaf and root
into one node — is not how this format works: even one leaf item gets a
one-node root summarizing it, which is exactly the behaviour the same test
body's `rtree_index_size(16, 16) == (16 + 1) * 40` and
`rtree_index_size(17, 16) == (17 + 2 + 1) * 40` assertions already rely on
one line below.

**Fix:** the Python implementer corrected the assertion to `== 80` in
`src/py/tests/test_layout.py` (not the plan document itself — out of scope
for that task), with a comment pointing back to this reasoning. **The plan
document itself was left unedited**, per this write-up task's brief
("do not change any code" — the plan is process documentation, not source,
but this task's own scope is `docs/upstream-findings.md` only); recording
the defect here is the intended fix for the process, so nobody re-derives
it from scratch reading the plan a second time.

**Reproduce:**
```
$ cd src/py && uv run pytest tests/test_layout.py::test_rtree_index_size_matches_the_reference_formula -q
1 passed in 0.01s
```
(asserts `rtree_index_size(1, 16) == 80`, matching both the Rust source and
the pre-existing C++ test.)

---

# Defects found while porting the native TypeScript reader

The following surfaced during the TypeScript port (Tasks 1–18). Findings #22–#25
are in the `fcb_wasm` browser binding, which **this branch has since removed**
(`src/rust/wasm/` deleted in Task 18) in favour of the native TypeScript reader
at `src/ts/`. Their line citations are to the crate as it stood at deletion, and
are recorded here because the native reader had to get each of these right where
the wasm binding got it wrong — every one is covered by a TypeScript test. #26
is a live defect in `fcb_core` itself. #27–#29 are writer/CLI defects found and
**fixed on this branch** during Task 2.

## 22. wasm: every JS number is coerced to a `Float64` index key — NOT FIXED (crate removed)

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

## 23. wasm: string query values over 50 bytes are routed into a `StringKey100` — NOT FIXED (crate removed)

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

## 24. wasm: `index_node_size` from the header is ignored on the HTTP path — FIXED in `fcb_core` (crate removed; the twin file-reader bug is fixed too)

**Where:** `wasm/src/lib.rs:275` (`select_spatial_paged`) — and the same hardcode
~~still lives in~~ was shared by `fcb_core/src/http_reader/mod.rs:220`, since
fixed (see below).

Both call `PackedRTree::http_stream_search(..., PackedRTree::DEFAULT_NODE_SIZE, ...)`,
passing the compile-time default (16) instead of `header.index_node_size()`, even
though the very next lines read the header. A file written with any other node size
is traversed as if its R-tree branched by 16, walking the wrong node ranges.

**What a consumer saw:** a spatial query over HTTP against a file written with a
non-default R-tree node size returned wrong or missing features. This is not just a
wasm defect: `fcb_core`'s own HTTP reader ~~shares the hardcode and is still
live~~ shared the hardcode, since fixed — see below. The
native reader threads `header.info.indexNodeSize` into every R-tree traversal
(`src/ts/src/reader.ts`, `searchRtree`/`searchNearest`), and the corpus carries
`appearance_depths_node8.fcb` (node size 8) precisely to exercise it — covered by
`src/ts/test/packed-rtree.test.ts` ("honours a NON-DEFAULT index_node_size from the
header").

**FIXED in `fcb_core` — and the defect was wider than recorded here.** While
fixing the cited HTTP-path hardcode (`http_reader/mod.rs`,
`select_query_paged`), an undocumented *twin* surfaced: the seekable FILE
reader (`reader/mod.rs`, `select_query`) passed the identical
`PackedRTree::DEFAULT_NODE_SIZE` to its traversal, so the defect was
reachable via `FcbReader::open(File).select_query` and through `fcb_api`,
not only over HTTP. The non-seekable `select_query_seq` was never affected.
Both sites now pass `header.index_node_size()`. Regression tests
(`http_spatial_query_honours_header_index_node_size` in-crate,
`read_bbox_honours_header_index_node_size` in `tests/read.rs`) drive
`appearance_depths_node8.fcb` — whose 12 features at node size 8 have level
bounds `[12, 2, 1]` where a hardcoded 16 implies `[12, 1]` — against a
`select_all` oracle. Enabling in-crate HTTP tests over small local fixtures
required teaching the mock range client to truncate at EOF like a real
server instead of panicking. In release builds the pre-fix symptom was
silently wrong results; only debug builds panicked on the
sorted-by-construction assertion.

## 25. wasm: the gloo range client accepts a `200` full-body response as the requested range — NOT FIXED (crate removed)

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

## 26. `PackedRTree::http_stream_search` can emit the extra leaf node twice — FIXED (guard added to all three arms; unreachable in the two intersect arms, reachable in `PointNearest`)

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

**FIXED — with a correction to this entry's premise.** The static analysis
above is accurate (no guard existed), but the double emission turns out to
be **unreachable in practice**: an intersecting sizing leaf implies its own
parent also intersects (a parent's bounds are the union of its children's),
so the sibling range starting at that leaf is always enqueued next and
always takes the overlap-merge branch (`wasted_bytes = 0`, never above any
threshold) — the two ranges become one before the leaf is evaluated twice.
An oracle sweep (~700 boundary-targeted plus 250 random queries against
brute force, `combine_request_threshold = 0`, multi-level trees) found no
duplicate pre-fix, and instrumentation showed the guard path is reached
constantly but never suppresses a hit. The guard was added anyway — each
`NodeRange` now carries a `has_sizing_tail` marker excluding its final node
from candidate evaluation, in **both** the `Query::BBox` and
`Query::PointIntersects` arms (this entry only cited BBox; the same shape
existed in both) — because it encodes the invariant explicitly and closes
the latent worst case: an unguarded sizing-node hit at the end of the last
range would have emitted an unbounded `HttpRange::RangeFrom`, a
read-to-EOF.

The reproduction attempt also exposed a real, unrecorded defect in the same
loops, fixed in the same pass: `debug_assert_eq!(node_pos, num_items - 1)`
compares a position *within the fetched range* against the global item
count, so any hit of the final feature via a non-initial node range
panicked in debug builds. Corrected to assert against the last leaf of the
level. The same wrong `debug_assert` also sat in the third arm,
`Query::PointNearest`, and is now fixed there too, with its own oracle test
(`nearest_search_reaches_the_final_leaf_through_a_non_initial_range`: 17
leaves at branching factor 4, whose final level-1 node owns a single child,
so the level's last leaf is only ever reached through the range `24..25` —
`node_pos` is `0`, never `16`).

That arm was **initially** left without the `has_sizing_tail` guard, on the
reading that evaluating a borrowed sizing leaf as a nearest *candidate* is
result-correct. That reading turned out to be only half right, so **the
guard is now in all three arms.** A brute-force sweep over the same
fixtures (`nearest_search_matches_brute_force_at_leaf_boundaries`) showed
the nearest *feature* was indeed always identified correctly — 0 wrong
features and no duplicates in 1506 boundary queries — but in 126 of them
the hit landed on a range's sizing leaf, which by construction is the last
node of the fetched batch, so `node_items.get(node_pos + 1)` was `None` and
the arm emitted an unbounded `HttpRange::RangeFrom` instead of the bounded
range: the right feature, read to EOF. That is exactly the latent worst
case cited above as the reason the guard was added to the other two arms —
except here it was not latent but reachable on ~8% of boundary queries, and
it is what the corrected `debug_assert` caught once it started asserting
the true invariant. `QueueItem` now carries the same `has_sizing_tail`
marker, set when the leaf range is extended, cleared when clamping to
`level_bounds[0].end` proves the extra node is not there, and honoured by
the same `emit_len` skip; there is no merge-inheritance step to mirror,
because the nearest search pushes every child range onto its heap
separately and never merges two. The sweep additionally asserts, on its
own, that an unbounded `RangeFrom` is only ever returned for the level's
final leaf. The C++/Python/TypeScript analogues of this assertion — and of
the sizing-tail handling — have not been checked; follow-up.

## 27. `HeaderWriter::new_with_options` overwrote the caller's `index_node_size` — FIXED (this branch, Task 2)

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
finding #24. Now fixed with the `appearance_depths_node8.fcb` fixture as its
regression witness.

## 28. The CLI conflated the R-tree node size with `attr_branching_factor` — FIXED (this branch, Task 2)

**Where:** `fcb_cli` write path, now `cli/src/main.rs:498-500`.

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
not match the caller's intent — again feeding finding #24.

## 29. `fcb_cli deser` broke its loop on `features_count`, truncating count-0 files — FIXED (this branch, Task 2)

**Where:** `fcb_cli` deserialize path, now `cli/src/main.rs:735-738`.

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

## 30. `serde_json` loses 1 ULP parsing certain `Double` attribute values — NOT FIXED, `fcb_cli` ingestion only

**Where:** not `fcb_core` at all — `serde_json` 1.0.133's own JSON-number-to-`f64`
parser, exercised wherever the CLI (or any Rust caller) deserializes CityJSON
text into a `serde_json::Value` before handing attribute/extent doubles to the
writer.

**Found by:** the new C++ writer's value-exact round-trip check (write
`examples/data/delft.city.jsonl` with `fcb_write_cityjson`, `deser` the result
back with the Rust CLI, diff against `deser`-ing the checked-in
`examples/data/delft.fcb`). 7 of 1115 features disagreed, always by exactly 1
ULP on a `Double` value:

```
$ fcb_write_cityjson delft.city.jsonl delft_cpp.fcb   # C++ writer
$ cargo run -p fcb_cli -- ser   delft.city.jsonl delft_rust.fcb -A -g   # same input, fresh Rust writer
$ cargo run -p fcb_cli -- deser delft_cpp.fcb  delft_cpp.jsonl
$ cargo run -p fcb_cli -- deser delft_rust.fcb delft_rust.jsonl
# delft_cpp.jsonl:  "b3_volume_lod12": 20.652481079101562   <- matches the original input text exactly
# delft_rust.jsonl: "b3_volume_lod12": 20.65248107910156    <- 1 ULP below (bits 0x...ffffffff vs 0x...00000000)
```

Isolated to `serde_json` itself, independent of `fcb_core`:

```rust
let s = "20.652481079101562";
s.parse::<f64>().unwrap().to_bits();                          // 0x4034a70900000000 — correctly rounded
serde_json::from_str::<serde_json::Value>(s)
    .unwrap().as_f64().unwrap().to_bits();                     // 0x4034a708ffffffff — 1 ULP low
```

Rust's own `str::parse::<f64>` and nlohmann::json (what the C++ writer uses)
both produce the IEEE‑754 correctly-rounded double for this decimal string;
`serde_json`'s number parser does not, for certain decimal strings landing near
a rounding boundary. This reproduced identically whether the fresh Rust-written
file was built with the CLI's own `ser -A -g` — so it is not specific to how
`delft.fcb` was originally generated, and not something the writer/reader
format logic controls at all.

**Consequences:** negligible in practice (~1e-16 relative error, well under any
real query threshold or display precision), but it means a `Double` attribute's
bytes are not always byte-identical between a Rust-CLI-written file and a
C++-writer-written file for the *same* input JSON text — the C++ writer is
incidentally *more* faithful to the source text here, not less. Not fixed:
changing `serde_json`'s float parsing is outside this port's scope, and
"correctly-rounded" is the behavior worth keeping, not replicating the
imprecision.

# Defects found while auditing `ser`/`deser` round-trip fidelity

Found by driving the release binaries over the four checked-in source datasets
(`fcb_core/tests/data/*.city.jsonl`) and deep-diffing input against output —
matching features **by `id`**, because `ser` reorders features along the Hilbert
curve and a positional comparison silently compares unrelated features.

## 31. The header's `appearance` palette was written but never read back — FIXED (this branch)

**Where:** `fcb_core/src/reader/deserializer.rs`, `to_cj_metadata`.

`Header` has an `appearance` field, and `to_fcb_header` fills it from
`cj.appearance` (`writer/serializer.rs:114`, used at `:208`). The reader never
read it: `Header::appearance()` (`fb/header_generated.rs:2688`) was not called
anywhere under `src/reader/`, and only the *feature* path decoded an appearance
(`deserializer.rs`, `feature.appearance()`). `to_cj_metadata` had no reference
to appearance at all.

**Repro** — the bytes really are on disk, so this is a read defect, not a write
one:

```console
$ strings conformance/geom_temp.fcb | grep -c UUID_e58d9d68   # the material name
1
$ fcb deser conformance/geom_temp.fcb out.jsonl
$ head -1 out.jsonl | python3 -c "import json,sys; print('appearance' in json.load(sys.stdin))"
False
```

**Why it is worse than a fidelity loss.** A header's geometry templates index
*into the header's palette* — a template belongs to no feature, so its
`material`/`texture` mapping can refer to nothing else. The reader emitted the
templates and dropped the palette, so `geom_temp` round-tripped to a header with
three templates carrying material mappings to indices 1 and 2 and texture
mappings, and **no `appearance` object at all**: dangling indices in output that
still looks schema-valid.

This also disproves the rationale the three ports recorded for mirroring the
omission ("each feature carries the slice of the palette it actually uses").
In `geom_temp` the header palette and the per-feature palettes are disjoint:
the header holds materials `UUID_e58d9d68…`/`UUID_f55b5612…` and textures
`Vegetation_Juniper2.jpg`/`MaerZ-0.png`, none of which appear in any feature,
and the features carry entirely different textures.

**Fix:** the appearance decode is now one shared
`to_cj_appearance(Appearance) -> Result<CjAppearance, Error>`, called by both
`to_cj_feature` and `to_cj_metadata`. Shared rather than duplicated on purpose:
the `"image": "" -> absent` choice and the empty-vector semantics must stay
identical on both paths, and duplicating them is what let the header path drift
unnoticed.

**Corpus:** only `conformance/geom_temp.expected.jsonl` changed — regenerated
from the **existing** `.fcb` (no `.fcb` was rebuilt, which would churn bytes and
break `test_writer_oracle.cpp`). The restored palette matches the original
`geom_temp.city.jsonl` header appearance exactly.

**Ports — all three deliberately mirror the old behaviour and are now wrong:**

| Port | Site | Status |
|---|---|---|
| C++ | `src/cpp/src/cityjson.cpp`, `to_cityjson_metadata` | **FIXED** — emits `appearance` beside the templates that index it; the stale comment at the per-geometry mapping site is corrected. 279/279 doctest cases, 292/292 with the HTTP adapter, ASan/UBSan clean |
| Python | `src/py/flatcitybuf/cityjson.py`, `to_cityjson_metadata` | **FIXED** — emits `appearance` beside the templates, via the existing `_appearance_to_json`; stale comment corrected. 255 pass (251 without numpy), `mypy --strict` and `ruff` clean |
| TypeScript | `src/ts/src/cityjson/index.ts`, `toCityJSONMetadata` | **FIXED** — emits `appearance` via the existing `appearanceToJson`; the `CityJSON` interface in `cityjson/types.ts` gained the `appearance?` member it was missing (`tsc` caught that the type never modelled it). 244 Node + 3 browser tests, `tsc --noEmit` clean |

Each unfixed port's `geom_temp` conformance case fails until it emits the header
appearance; Python's `CASES` in `tests/test_conformance.py` includes `geom_temp`.

## 32. The writer dropped `appearance` and **all** geometry templates when a header had no `metadata` — FIXED (this branch)

**Where:** `fcb_core/src/writer/serializer.rs`, the `else` arm of
`if let Some(meta) = cj.metadata.as_ref()`.

`to_fcb_header` builds `appearance`, `templates` and `templates_vertices` from
`cj.appearance` and `cj.geometry_templates` — siblings of `metadata`, not
children of it — and then chooses between two `Header::create` calls. The
metadata-present arm passed all three; the metadata-absent arm left them to
`..Default::default()`, i.e. absent. So a CityJSON header carrying geometry
templates but no `metadata` lost **every template and the whole palette**, with
no error.

Found while fixing #31: the new regression test wrote a header with an
appearance and templates but no `metadata`, and still decoded nothing — the
reader fix alone did not make it pass.

**Fix:** the `else` arm now passes `appearance`, `templates` and
`templates_vertices` too.

**Coverage:** `fcb_core/tests/appearance_roundtrip.rs::the_header_appearance_palette_roundtrips`
exercises both defects at once — a header with a palette *and* templates that
index it, and no `metadata` — and compares the whole palette as one value rather
than a chosen subset of its members.

**Ports (writers only; the Python and TypeScript readers are N/A):**

| Port | Status |
|---|---|
| C++ | **Verified unaffected.** `src/cpp/src/writer/header_serializer.cpp` computes `appearance`, `templates` and `templates_vertices` before, and independently of, the `metadata` block, then has a **single** unconditional `CreateHeader` that always passes all three. The metadata `if` only populates metadata-derived optionals. The two-branch shape that caused this bug in Rust does not exist there. |

**Why the writer oracle could not have caught this.** `test_writer_oracle.cpp`
compares C++ output byte-for-byte against the checked-in `.fcb` fixtures — but
those were produced by the *old, buggy* Rust writer, and no fixture is a
metadata-absent header carrying appearance or templates. Byte-identical-to-Rust
would have meant byte-identical-to-the-bug. The C++ writer is clear here by
construction, not by oracle agreement; a fixture exercising that shape would be
needed to keep it that way.

# Defects found while exercising the ports against live data

## 33. Python: the bbox phase was the one read phase with no buffering — FIXED (this branch)

**Where:** `src/py/flatcitybuf/packed_rtree.py`, `search_rtree`.

Four phases read over a `RangeReader`, and three of them wrap it in a
per-query `BufferedRangeReader` sized to the reference's own constant:

| Phase | Wrapper | Window | Reference |
|---|---|---|---|
| open | `header.py:309` | `_OPEN_PREFETCH_SIZE` = 12944 | `http_reader/mod.rs:80-98` |
| features | `reader.py:248`, `:300` | `_FEATURE_FETCH_SIZE` = 1 MiB | — |
| attribute index | `stree.py:774` | `_INDEX_FETCH_SIZE` = 1 MiB | `http_reader/mod.rs:363` |
| **bbox / R-tree** | **none** | — | `http_reader/mod.rs:213` says 256 KiB |

`search_rtree` issued one physical `reader.read` per R-tree node. Against a
`FileRangeReader` that is merely extra syscalls; over HTTP it is one request
per node. The spec's "HTTP constants" table
(`docs/specification.md:341`) documents a bbox combine threshold of
`256*1024` — Python implemented the attribute one beside it but not this.

**Measured** — the live 68 GB 3DBAG file, bbox `120000 486000 121000 487000`,
identical results throughout (2762 features):

| | open | R-tree traversal | wall clock |
|---|---|---|---|
| Python, before | 2 requests | **240 requests** | **32.4 s** |
| Python, after | 2 requests | **10 requests** | **1.4 s** |

These count the **index traversal only**, not the feature reads that follow.
An earlier revision of this entry put C++'s 37 in the same table; that number
is C++'s total for the whole query *including* decoding all 2762 features, so
it was never comparable to a traversal-only figure. The end-to-end comparison
belongs to finding #34, which is where the rest of the gap turned out to be.

Not a correctness defect — the answer was always right, and matches C++ and
Rust exactly (and on `delft.fcb`, all three agree on the same 170 features).
It is a divergence in the one behaviour the format exists for: fetching only
the bytes you need, in as few requests as possible.

**Fix:** one `BufferedRangeReader(reader, _NODE_FETCH_SIZE)` at the top of
`search_rtree`, mirroring `stree.py` exactly, with `_NODE_FETCH_SIZE =
256 * 1024` cited to `http_reader/mod.rs:213`. The window only ever widens a
read the caller already asked for, so the existing bounds checks still govern
what is decoded.

**Coverage:** `tests/test_packed_rtree.py::test_a_multi_level_search_buffers_instead_of_reading_per_node`
asserts strictly fewer physical reads than nodes visited — the property, not
the window size. Verified to fail without the fix (2 reads vs 1).

**Ports:** C++ and Rust already combine on this path and are unaffected. The
TypeScript reader has the same four phases and has **not** been checked for
this — worth doing when it is next touched.

## 34. Python: `feature_at` read through the raw reader, costing 2 HTTP requests per feature — FIXED (this branch)

**Where:** `src/py/flatcitybuf/reader.py`, `FcbReader.feature_at`.

`feature_at` is the documented way back from an index hit to a feature — both
`search_rtree` and `select_attr` return byte offsets, and `docs/py.md` shows
the loop:

```python
for hit in hits:
    cj = fcb.to_cityjson_feature(reader.feature_at(hit), reader.header)
```

It read through `self._reader`, the **raw** reader. `select_all` and
`select_attr`'s post-filter each wrapped that reader in a
`BufferedRangeReader(_FEATURE_FETCH_SIZE)`, but `feature_at` did not — and a
per-call wrapper could not have helped anyway, since one call has nothing to
amortise. Every call therefore issued its own physical reads: a length prefix
and a body, **2.0 HTTP requests per feature**, measured.

This is the same class as finding #33 and was found immediately after it, by
asking why the numbers there did not match C++'s once features were actually
decoded rather than just counted.

**Measured** — live 68 GB 3DBAG file, bbox `120000 486000 121000 487000`,
2762 features, identical results throughout:

| | requests | wall clock |
|---|---|---|
| Python, before | ~5764 (10 index + ~5524 features) | >10 min (timed out) |
| Python, after | **39** | **9.2 s** |
| C++ `fcb_read_http` | 37 | ~6.4 s |
| TypeScript | 34 | 5.2 s |

Before both fixes the same query was ~5764 requests; all four implementations
now agree on 2762 features within a few requests of each other.

**Fix:** one `BufferedRangeReader(reader, _FEATURE_FETCH_SIZE)` created in
`FcbReader.__init__` and reused by every `feature_at` call, so walking a sorted
hit list costs a fetch per window rather than per feature. An out-of-order or
distant offset simply misses the window and refills it, exactly as a random
read would — the method stays random-access.

**Coverage:** `tests/test_public_api.py::test_walking_a_hit_list_with_feature_at_does_not_read_per_feature`
asserts fewer physical reads than features walked — the property, not the
window size.

**Ports:** C++ and TypeScript were measured on the same query and do not have
this gap (37 and 34 requests end to end). TypeScript in particular wraps a
single `BufferedRangeReader` around the source at open
(`src/ts/src/reader.ts`), so every phase inherits buffering by construction
rather than needing a wrapper per phase.

## 35. The encoded geometry was reachable but not usable, in all three ports — FIXED (this branch)

**Where:** `src/ts/src/index.ts`, `src/py/flatcitybuf/__init__.py`,
`src/cpp/include/fcb/feature.hpp`.

Every reader could produce CityJSON, and every reader could reach the
FlatBuffers tables internally — but a *consumer* wanting to compute over the
format's own representation (the five count arrays plus the flat vertex-index
list, which is what you want for analysis: no nesting, no allocation, no JSON)
could not. Each port blocked it differently, and none of them on purpose:

| Port | What was wrong |
|---|---|
| C++ | `Feature::raw()` was **private**, reachable only through the `detail::FeatureAccess` friend |
| Python | `raw_city_object`/`raw_city_feature` existed but were absent from `__all__` and documented "only cityjson.py should call this"; there was no public `semantic_surface_type_name` at all |
| TypeScript | `rawObject()` was public, but every enum and constant needed to INTERPRET it — `GeometryType`, `SemanticSurfaceType`, `NULL_INDEX`, the name helpers — was unexported, and the generated modules are not reachable by subpath (`ERR_PACKAGE_PATH_NOT_EXPORTED`) |

So the fastest path through the format was the one path a user could not take.

**Fix:** each port now exposes it, with the same warning attached — NESTING
DEPTH COMES FROM `Geometry.type()`, NEVER FROM THE ARRAYS (finding #8). C++
moves `raw()` to the public section; Python exports the two gateways plus
`geometry_type_name`/`semantic_surface_type_name` (the latter newly written,
lifted from `cityjson.py`'s private list); TypeScript re-exports the enums,
constants, decoders and table types, following the precedent already set by
`ColumnType`.

**Demonstrated and validated by** `geometry_analysis` in all three ports:
surface area per semantic surface type, walked from the flat arrays. Each one
self-checks two ways — against its own nested-CityJSON path, and against
`b3_opp_grond`, 3DBAG's published ground area, which no implementation here
produced.

Cross-implementation agreement over 200 features of `delft.fcb`, all three
byte-identical at every LoD:

| lod | RoofSurface | GroundSurface | WallSurface |
|---|---|---|---|
| 1.2 | 144646.86 | 144646.86 | 256929.12 |
| 1.3 | 144647.20 | 144647.20 | 279442.01 |
| 2.2 | 160166.79 | 144649.17 | 269946.15 |

The physics is a check in itself: at lod 1.2 and 1.3 the roof area *equals* the
footprint, because those levels are flat extrusions; at lod 2.2 it exceeds it,
because the roofs are sloped. A walk that mis-grouped surfaces or misread the
semantics would not reproduce that, and both properties are asserted in the
Python and TypeScript example tests.

## 36. Writer diagnostics go to stdout, corrupting `ser <input> -` — FIXED (this branch)

**Where:** `fcb_core/src/writer/attribute.rs:266`.

Library code in the attribute-index builder prints a diagnostic with
`println!` — that is, to **stdout**:

```rust
None => {
    println!("Attribute {attr} not found in schema");
    continue;
}
```

It fires once per (indexed attribute, feature) whenever a feature lacks a
column that is in the header schema — the *normal* case for heterogeneous
real-world data. `fcb_cli ser` documents `<OUTPUT>` as "use '-' for stdout",
so on that path the diagnostic lines interleave with the binary stream.

**Reproduction** (first 2000 features of `data/cjseq_data/3dbag_subset.city.jsonl`):

```
$ head -n 2001 3dbag_subset.city.jsonl > slice.jsonl
$ fcb ser slice.jsonl slice.fcb -A -g          # 8 416 814 B, inspects fine
$ fcb ser slice.jsonl -        -A -g > bad.fcb # 8 481 920 B, exit 0
$ fcb inspect bad.fcb --static
FCB core error: Missing magic bytes in FCB file header
$ od -c bad.fcb | head -1
0000000  A t t r i b u t e   b 3 _ b a g _ b a g _ o v e r l a p ...
```

65 106 bytes of diagnostic text injected, exit status 0, no warning. The CLI's
own progress banner correctly uses stderr; only this library `println!` was
wrong (it also violated the crate's "use `tracing`" rule). Scope is limited to
inputs with per-feature attribute variation — `delft.city.jsonl` emits no
diagnostics, and `ser delft.city.jsonl -` was byte-identical to the file-output
path even before the fix.

The fix routes the diagnostic through `tracing`, so it is emitted only where a
subscriber asks for it — and the destination becomes the embedder's choice, not
the library's. It also corrects the message, which described the wrong
condition: the attribute *is* in the schema, it is the feature that lacks it:

```rust
None => {
    // Never `println!`: library diagnostics on stdout corrupt
    // `fcb_cli ser <input> -`, which writes the binary there.
    debug!("feature is missing indexed attribute {attr}");
    continue;
}
```

After the fix, `ser slice.jsonl -` emits 8 416 814 B — the same size as the
file-output path — and `inspect --static` reads 2 000 features from it.
`fcb_cli` installs no `tracing` subscriber, so for CLI users the diagnostic
now vanishes entirely; it was per-feature noise on normal heterogeneous input,
and library code has no business writing to a stream the caller owns.
Two sibling `println!`s of the same defect class went in the same pass:
`attribute.rs`'s "not supported for indexing" arm is now `debug!`, and
`attr_index.rs`'s "Unsupported column type for indexing" line is deleted
outright — it was redundant with the `Error::UnsupportedColumnType` returned on
the next line. The `eprintln!` in the DateTime-parse fallback is now a `warn!`,
since silently defaulting a bad timestamp to the epoch is worth surfacing.

Note that `cmp`-ing the two outputs is **not** a valid check here: the writer is
not byte-reproducible run to run (file→file twice with the same binary differs
at the same offset, in header column-name territory), so the equality evidence
above is size, `inspect`, and a `deser` round-trip compared line by line.

**What a consumer saw:** piping the writer's output (`ser … - | …`) on real
heterogeneous data produced a file that no reader can open, with a clean exit
status. No justfile recipe uses the `-` path, which is why it went unnoticed.
