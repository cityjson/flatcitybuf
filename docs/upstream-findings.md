# Defects found in the Rust implementation while porting the C++ reader

Five defects in `fcb_core` surfaced during the native C++ port. Each is
reproducible, each caused a deliberate divergence in the C++ reader, and none
is caught by the existing Rust test suite.

They are written up here rather than filed directly — **filing them as GitHub
issues is a maintainer decision**, since they are public and some are
arguably behaviour changes rather than bugs.

---

## 1. `Transform` is written at a misaligned offset

**Where:** the header writer, observable in any `.fcb` file.

The `Transform` struct (six doubles, requiring 8-byte alignment) is placed at
body+68 in the header FlatBuffer — an odd multiple of 4.

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

**C++ workaround:** struct doubles are read via `memcpy` (well-defined at any
alignment) and only `check_alignment` is disabled; all other verification runs.

---

## 2. `Byte` attribute index: writer stores `u8`, reader decodes `i8`

**Where:** `writer/attribute.rs:209`, `writer/attr_index.rs:240` vs
`reader/attr_query.rs:118`.

The writer stores `Byte` values as `u8` and builds the index as
`MemoryIndex<u8>`; the reader decodes that same index as `i8`. A stored `200`
reads back as `-56`.

**C++ divergence:** matches the writer (`u8`), so it decodes files correctly
and disagrees with the Rust reader for values above 127.

---

## 3. `Byte`/`UByte`/`Binary` attributes cannot be read back at all

**Where:** `reader/deserializer.rs:372` — `unreachable!()`.

The writer emits these column types, but the reader panics on them. Any file
containing such an attribute is unreadable by the implementation that wrote it.

**C++ divergence:** decodes all three. Their widths are unambiguous (1, 1, and
`u32` length + bytes), so there is no reason to refuse them.

---

## 4. `find_range` silently drops its upper boundary item

**Where:** `static_btree/stree.rs:954`.

`end_idx = min(upper_idx + node_size, leaf_end)`. Because `find_partition`
descends *left* on an exact hit, when `upper` is itself a separator key the
matching leaf entry sits at exactly `upper_idx + node_size` — one past the scan
end — and is dropped.

This affects every `Le(k)` and range query where `k` is a separator: roughly
1-in-`branching_factor` of unique keys.

**The existing test encodes the bug.** `test_range_search`
(`stree.rs:1915-2032`) builds keys 0..18, comments *"expects to find exactly 19
items"*, then asserts `len() == 18`. Later cases in the same test hedge with
"we might get different counts … at least 5/3" and never assert the boundary
keys.

**C++ divergence:** widens the scan by one node. Safe because the leaf filter
already rejects out-of-range keys; costs at most one extra node read.

---

## 5. `Gt`/`Lt`/`Ne` can drop genuine matches

**Where:** `static_btree/query/stream.rs:161-191`.

These are lowered as "range minus `find_exact`", and the subtraction operates
on **feature offsets**. But one feature can appear under several keys when its
CityObjects carry different values of the indexed attribute — the writer
indexes each occurrence.

A feature holding both `k` and `k' > k` is returned by the range scan (via
`k'`) and also by `find_exact(k)` (via `k`), so the subtraction deletes it. It
is a false negative for a feature that genuinely matches.

**C++ divergence:** evaluates strict-or-inclusive bounds at the leaf instead of
subtracting. One traversal, no subtraction, no false negatives.

---

## 6. Suspected: `find_exact` on a maximum-valued key walks off the level

**Status: derived by inspection, not yet reproduced.** Worth a small Rust test
before filing.

Separator entries with no right sibling carry `K::max_value()` as a sentinel
whose offset already points at the last child group. `find_exact`'s
`Ok(i) → offset + node_size` right-descent then overshoots the level, which
should produce an inverted slice (panic) in the in-memory path or a `usize`
underflow in the streaming path.

`Eq(true)` on a bool-indexed column ought to be enough to trigger it, since
`true` is `bool::max_value()`.

**C++ workaround:** clamps the child index back to the entry's own offset when
the computed child would leave the level. A no-op for ordinary keys.
