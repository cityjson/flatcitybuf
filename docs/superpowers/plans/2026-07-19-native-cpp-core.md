# Native C++ FlatCityBuf Core Library — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Rust-FFI C++ bindings with a from-scratch native C++17 implementation of the FlatCityBuf **reader** core (local files + HTTP range access), with no Rust dependency at build or run time.

**Architecture:** A sans-IO core. All parsing, R-tree traversal and B+tree traversal operate on byte buffers handed to them by a synchronous, user-implementable `RangeReader` interface exposing a batched multi-range read. Local files and HTTP are two adapters behind that one interface, so there is a single traversal code path. FlatBuffers accessors are never exposed publicly — every returned view owns a `shared_ptr` to its backing buffer. CityJSON emission is a separate, optional component so embedders who only want FlatBuffers access pay nothing for JSON.

**Tech Stack:** C++17, CMake ≥ 3.16, FlatBuffers (official C++ runtime, generated headers committed), nlohmann/json (CityJSON component), libcurl (optional HTTP adapter), doctest (tests only).

---

## Global Constraints

Every task's requirements implicitly include this section.

- **Branch:** all work happens on `develop`, branched from `main`. `develop` does not exist yet; Task 1 creates it.
- **Commit at every milestone.** Each task ends with a commit. Do not batch tasks into one commit.
- **TDD is mandatory.** Every task follows red → green: write the failing test, run it and *see it fail for the stated reason*, write the minimal implementation, run it and see it pass, commit. Never write implementation before a failing test exists.
- **C++ standard: C++17.** Not C++20. The existing `src/cpp/CMakeLists.txt` is C++17 and the GIS ecosystem consuming this still ships C++17 compilers. No coroutines, no `std::span` (use a local `fcb::span` shim), no concepts.
- **No async runtime in the core.** No `std::future`, no callbacks, no coroutines in any public API. Batching, not asynchrony, is the concurrency primitive — see Task 4.
- **No TLS inside the library, ever.** The vcpkg curated registry rejected the previous artifact because the static lib link-depended on OpenSSL (see `remove_openssl_dependency.md.resolved`). libcurl brings its own platform TLS (Schannel / SecureTransport / system OpenSSL) and is opt-in via `FCB_WITH_CURL` (default `OFF`). The default build's dependency set is exactly: flatbuffers + nlohmann/json.
- **Required dependency versions:** `flatbuffers` — **`flatc` must match the C++ flatbuffers runtime version it generates against** (currently 25.9.23 via both brew and vcpkg). It does *not* need to match the `flatbuffers` crate pin in `src/rust/Cargo.toml` (24.3.25): the FlatBuffers wire format is stable across versions by design, so a file written by Rust 24.3.25 is readable by C++ 25.9.23. What breaks on mismatch is generated-code-vs-runtime API drift, which is why flatc and the runtime must agree with each other. Also: `nlohmann_json` ≥ 3.2.0, `doctest` ≥ 2.4.11, `libcurl` ≥ 7.68 (optional).
- **All hand-serialized data is little-endian.** FlatBuffers handles its own endianness; the R-tree, B+tree and payload readers are hand-rolled and must byteswap on big-endian hosts (or `static_assert` LE — see Task 3).
- **Generated FlatBuffers types live in the GLOBAL namespace.** Every `namespace FlatCityBuf;` declaration in `src/fbs/*.fbs` is commented out (`header.fbs:1`, `feature.fbs:8`), and `flatc --cpp` emits no namespace — verified empirically. Refer to `::Header`, `::CityFeature`, `::Column`, `::ColumnType`, `::AttributeIndex`, and the free functions `GetSizePrefixedHeader`, `VerifySizePrefixedHeaderBuffer`, `GetSizePrefixedCityFeature`, `VerifySizePrefixedCityFeatureBuffer`. **Never write `FlatCityBuf::`** — it does not exist. Because these names are unqualified in the global namespace, keep generated headers out of public `fcb/*.hpp` headers wherever possible (see the ownership rule below), so consumers do not inherit them.
- **Never expose raw generated FlatBuffers pointers or `flatbuffers::Vector` in a public header — this is a hard rule with no exceptions.** A returned `const ::Header*` outlives nothing: the caller can retain it past the owning view's destruction and read freed memory. Public view types (`HeaderView`, `Feature`) hold `std::shared_ptr<const std::vector<std::uint8_t>>` and expose **value accessors** that copy out (`std::string id()`, `std::uint64_t features_count()`), or nested view objects that themselves carry the `shared_ptr`. Internal access to the generated pointer goes through a `friend` struct, since C++ cannot bolt a private member onto an already-defined class from another header. The pattern: the public class declares `private: const ::CityFeature* raw() const; friend struct detail::FeatureAccess;`, and `src/cpp/src/detail/feature_access.hpp` defines `struct FeatureAccess { static const ::CityFeature* get(const Feature&); };` for the decoders. The generated type is forward-declared in the public header, so consumers never include the generated headers.
- **Always run the FlatBuffers `Verifier`** before accessing any root, on both header and features. The Rust reader does this via `size_prefixed_root_as_*`.
- **All input is untrusted; all size arithmetic is checked.** `features_count`, `header_size`, `index_node_size`, `branching_factor`, `AttributeIndex.length`, payload counts and the per-feature 4-byte prefix all come from the file and may be hostile or corrupt. Every add and multiply feeding a section bound, an allocation, or a range request goes through `checked_add`/`checked_mul` helpers that throw `fcb::Error` on overflow rather than wrapping. Before any index is read, validate: `feature_begin <= total_size`; each attribute tree's computed node region `<= AttributeIndex.length`; no duplicate `AttributeIndex.index` values; every computed range against `total_size()`; payload count and offset against that index's payload region. A single crafted feature prefix must not be able to provoke a multi-gigabyte allocation — enforce `kMaxFeatureSize` (default 256 MB, configurable) before allocating for any feature.
- **Reject malformed values; do not silently reinterpret them.** Rust's `index_size` functions `assert!(node_size >= 2)` *before* clamping (`packed_rtree/mod.rs:879`, `stree.rs:1480`), so a clamp in C++ would invent behaviour Rust never exhibits. A `node_size`/`branching_factor` of 0 or 1 is a corrupt file: throw `fcb::Error`.
- **Byte-identity scope.** Do NOT promise byte-identical output vs Rust for FlatBuffers sections — flatc's Rust and C++ builders differ in vtable dedup and field emission order, so two semantically identical Headers can legitimately differ in bytes. Byte-identity is required and tested only for the *hand-serialized* sections (R-tree, B+tree, payload), and only if a writer is ever added (out of scope here).
- **Golden comparisons are on parsed JSON trees, never strings.** Key order and float formatting differ between implementations.
- **Reference implementation is the Rust source, not `specification.md`.** The spec is under- and mis-specified in several places (see Task 12). When they disagree, the Rust code at `src/rust/fcb_core/src/` wins.
- **Naming:** namespace `fcb`, headers under `include/fcb/`, `snake_case` for functions and variables, `PascalCase` for types, `.hpp` for public headers and `.cpp` for sources. Matches nothing existing (the current `src/cpp/include/fcb.h` is a generated-code wrapper being deleted), so this is the new convention.

---

## Format Reference (ground truth, verified against Rust source)

Every constant below is cited. Tasks reference this section rather than repeating derivations.

### File layout

```
[ magic 8B ][ header_size 4B LE ][ Header FlatBuffer ][ R-tree ][ Attr index ][ Features ]
```

**There is no padding or alignment between any of these sections.** The writer emits back-to-back `write_all` calls (`src/rust/fcb_core/src/writer/mod.rs:266-271`). The spec's claim that sections are aligned is false.

**There are no section offsets stored anywhere.** They must be computed.

| Quantity | Value / formula | Citation |
|---|---|---|
| `MAGIC_BYTES` | `{'f','c','b',0x01,'f','c','b',0x00}` (8 bytes) | `const_vars.rs:5` |
| `VERSION` | `1`, at magic byte index **3** | `const_vars.rs:2` |
| Magic validation | `b[0..3]=="fcb" && b[4..7]=="fcb" && b[3] <= 1`. Byte 7 is written as 0 but **never validated** | `lib.rs:56-58` |
| `header_size` | 4 bytes **LE u32**. This is the **FlatBuffers size prefix**, not a custom field. It excludes itself. | `reader/mod.rs:97-102` |
| Header size guard | `8 <= header_size <= 536870912` (512 MB) else `IllegalHeaderSize` | `const_vars.rs:8`, `reader/mod.rs:97-102` |
| Header root accessor | `GetSizePrefixedRoot<Header>` — buffer passed **includes** the 4 prefix bytes | `reader/mod.rs:104-110` |
| `header_len` | `8 + (4 + header_size)` | `http_reader/mod.rs:136` |
| `rtree_begin` | `header_len` | — |
| `rtree_size` | `0` if `index_node_size == 0 \|\| features_count == 0`, else `rtree_index_size(features_count, index_node_size)` | `reader/mod.rs:266-275` |
| `attr_index_begin` | `header_len + rtree_size` | `http_reader/mod.rs:279` |
| `attr_index_size` | plain sum of `AttributeIndex.length()` over all header entries; `0` if absent | `reader/mod.rs:276-295` |
| `feature_begin` | `header_len + rtree_size + attr_index_size` | `http_reader/mod.rs:280` |

### Features

- Each feature is a **size-prefixed FlatBuffer**: 4-byte **LE u32** prefix excluding itself, then the buffer.
  (`reader/mod.rs:539-545`, `:569-572`; written by `finish_size_prefixed` at `writer/feature_writer.rs:83`)
- Root accessor: `GetSizePrefixedRoot<CityFeature>`; buffer passed **includes** the prefix.
- **No padding between features** (`writer/mod.rs:225-244`, `:271`).
- Features are stored in **Hilbert order**, not input order (`writer/mod.rs:202-203`).
- **Feature byte length is not stored in the index.** It is either (a) the feature's own 4-byte prefix, or (b) `next_leaf.offset - this_leaf.offset`. For the *last* feature only (a) is available, which is why `RangeReader` must expose `total_size()`.

### Packed R-tree

| Quantity | Value / formula | Citation |
|---|---|---|
| `NodeItem` | `{ f64 min_x, f64 min_y, f64 max_x, f64 max_y, u64 offset }`, all **LE**, **40 bytes**, no padding | `packed_rtree/mod.rs:23-33`, `:56-77` |
| `DEFAULT_NODE_SIZE` | `16`, clamped to `[2, 65535]` | `packed_rtree/mod.rs:325`, `:330` |
| `rtree_index_size(n, ns)` | `ns=clamp(ns,2,65535); num_nodes=n; loop { n=ceil_div(n,ns); num_nodes+=n; if n==1 break } return num_nodes*40` | `packed_rtree/mod.rs:879-898` |
| Level bounds | `level_bounds[0]` is the **leaf** level and is **last in storage order**; `level_bounds.back()` is the root `0..1` | `packed_rtree/mod.rs:342-375` |
| Hilbert curve | **Writer-only — not ported.** `HILBERT_MAX = 65535`; ordering is `floor(65535.0 * (centroid - extent.min) / extent.size)`. A reader never computes this; it compares stored bboxes and follows offsets. Listed only so a future writer plan has the reference. | `packed_rtree/mod.rs:233`, `:291-298` |
| Internal node `offset` | a **child node index**, not a byte offset | `packed_rtree/mod.rs:385`, `:531` |
| Leaf node `offset` | byte offset **relative to `feature_begin`** | `writer/mod.rs:207-215` |
| Leaf test (stream) | `node_index >= num_nodes - num_items` | `packed_rtree/mod.rs:702` |
| Last-feature range | `RangeFrom(start..)` — read the 4-byte prefix first | `packed_rtree/mod.rs:962-975` |
| Leaf fetch +1 rule | when descending into level 0, extend the node range by one extra node (clamped to `level_bounds[0].end`) so the next offset is available | `packed_rtree/mod.rs:979-987` |

### Attribute B+tree

The attribute index section is a **bare concatenation of per-column blobs in ascending `Column.index` order**, with no per-index header and no separator.

Per-column blob: `[ num_all_nodes × Entry<K> ][ payload section ]` (`static_btree/stree.rs:1520-1535`).

| Quantity | Value / formula | Citation |
|---|---|---|
| `AttributeIndex` (header struct) | `{ ushort index; uint length; ushort branching_factor; uint num_unique_items; }` — **16 bytes, not 12**: field order forces 2 bytes of padding after each `ushort`. Wire layout: `0:u16 index, 2:pad, 4:u32 length, 8:u16 branching_factor, 10:pad, 12:u32 num_unique_items`. Confirmed in the generated code: Rust `pub struct AttributeIndex(pub [u8; 16])` (`fb/header_generated.rs:810`) and C++ `FLATBUFFERS_MANUALLY_ALIGNED_STRUCT(4)` with explicit `padding0__`/`padding1__` members. | `src/fbs/header.fbs:65-70` |
| `length` | byte length of the whole blob **including** its payload section | — |
| `num_unique_items` | number of **unique keys** (= leaf count), NOT feature count | — |
| Locating column `i` | `attr_index_begin + Σ length of preceding entries` (sorted by `index()`) | `reader/attr_query.rs:309-337` |
| `Entry<K>` | `key: K` then `offset: u64 LE`. `SERIALIZED_SIZE = K::SERIALIZED_SIZE + 8` | `static_btree/entry.rs:25-52` |
| Node size for **search** | `branching_factor - 1` entries | `stree.rs:743`, `:826`, `:1087` |
| Level-bounds divisor | `branching_factor`, and the loop breaks when **`n < branching_factor`** (NOT `n == 1` — this differs from the R-tree and is intentional) | `stree.rs:462-497` |
| `stree_index_size(n, bf, payload)` | `bf=clamp(bf,2,65535); num_nodes=n; loop { n=ceil_div(n,bf); num_nodes+=n; if n<bf break } return num_nodes*ENTRY + payload` | `stree.rs:1480-1501` |
| `payload_data_start` | `index_begin + num_all_nodes * Entry<K>::SERIALIZED_SIZE` | `stree.rs:1442-1444` |
| payload size | `length - num_all_nodes * Entry<K>::SERIALIZED_SIZE` | derived |
| `PAYLOAD_TAG` | `1u64 << 63`; `PAYLOAD_MASK = ~PAYLOAD_TAG` | `stree.rs:15-17` |
| Payload entry | `u32 count LE` then `count × u64 LE`; size `4 + count*8` | `static_btree/payload.rs:36-61` |
| Payload offset base | tagged value's low 63 bits are **relative to the payload section start** | `stree.rs:652-659` |
| Leaf sibling pointers | **none.** Range scans walk the contiguous leaf array by index. The doc comment at `entry.rs:15` claiming otherwise is stale and false. | `stree.rs:626-679` |
| `SearchResultItem.offset` | feature-section-relative byte offset | `stree.rs:378-384` |

**Key encodings** (`static_btree/key.rs`), all integers LE:

| KeyType | Size | Encoding | Citation |
|---|---|---|---|
| Int8 / UInt8 | 1 | raw byte | `key.rs:284-314` |
| Int16/UInt16/Int32/UInt32/Int64/UInt64 | 2/2/4/4/8/8 | LE two's complement | `key.rs:260-280` |
| Float32 | 4 | **raw IEEE-754 LE bits** | `key.rs:323-345` |
| Float64 | 8 | **raw IEEE-754 LE bits** | `key.rs:347-370` |
| Bool | 1 | `0`/`1`; read as `byte != 0` | `key.rs:373-393` |
| DateTime | **12** | `i64 LE` UNIX seconds, then `u32 LE` subsec nanos | `key.rs:396-425` |
| FixedStringKey\<N\>, N ∈ {20,50,100} | N | raw N bytes, zero-padded, silently truncated at the **byte** level (can split UTF-8). No length, no terminator. | `key.rs:434-464`, `:483-489` |

**There is NO sign-flip / total-order bit transform for floats.** On-disk bytes are the plain IEEE-754 bit pattern. Ordering is `ordered_float` semantics applied *after* decode: NaN sorts greatest, NaN == NaN, `-0.0 == +0.0`.

**Column type → key type**, as the writer actually emits (`writer/attr_index.rs:240`, `:272`, `:288`):
`Bool→bool, Byte→u8, UByte→u8, Short→i16, UShort→u16, Int→i32, UInt→u32, Long→i64, ULong→u64, Float→f32, Double→f64, String→FixedStringKey<50>, DateTime→DateTime, Json→FixedStringKey<100>, Binary→FixedStringKey<100>`.

`StringKey20` is defined but **never produced by the writer**.

### Attribute schema resolution — PER OBJECT, not per file

**Corrected during execution of Task 7a.** Attributes must be decoded against the schema of the `CityObject` that owns them:

- `CityObject.columns` overrides `Header.columns` whenever it is set (`src/fbs/feature.fbs`, and the comment there says so explicitly).
- This is the normal case, not an edge case: in `examples/data/delft.fcb`, **all 1115 objects that carry attributes declare their own columns**, and the header's 44 columns are never used for decoding.
- Objects within one feature differ: the `Building` parent carries no attributes while its `BuildingPart` child carries them all. Code must walk all objects rather than assuming object 0.
- Getting this wrong does **not** fail loudly. Attribute records are not self-delimiting — each value's width comes from its column's type — so a wrong schema desynchronises the remainder of the blob and yields plausible-looking garbage. It surfaced as a nonsense column index (28777, which is ASCII `"ip"` from the middle of a string value).

Every task that decodes attributes (7a, 10's post-filter, 12's conformance) must resolve the schema this way.

### Known divergences from the Rust reader (deliberate)

These are cases where Rust's reader disagrees with Rust's own writer, or where a sentinel is arguably wrong. Each is a decision, not an oversight — a future implementer must not "fix" C++ to match Rust without reading this.

1. **`Byte` columns: C++ decodes `u8`, Rust's reader decodes `i8`.** The writer stores `Byte` as `u8` (`writer/attribute.rs:209`) and builds its index as `MemoryIndex<u8>` (`writer/attr_index.rs:240`), but the reader decodes that index as `i8` (`reader/attr_query.rs:118`). For stored values > 127 the Rust reader therefore returns a negative number that was never written. **C++ matches the writer (`u8`)** — decoding files correctly beats bug-compatibility. Consequence: C++ and Rust disagree on `Byte` queries for values > 127 until Rust is fixed. File this as a Rust bug (Task 14) and reference the issue here.
   Note also that normal attribute extraction does not even create index entries for `Byte`, `UByte`, `Short`, `UShort` and several other declared types — they fall through to "not supported" (`writer/attribute.rs:327`). So in practice this path is rarely exercised; correctness still matters for hand-built and third-party files.
2. **`Json`/`Binary` columns are indexed by the writer but rejected by the Rust reader** with `UnsupportedColumnType` (`reader/attr_query.rs:273`). **C++ mirrors the rejection** — these are `FixedStringKey<100>` over a JSON/binary blob, so index hits are near-meaningless without post-verification, and rejecting is honest.
3. **Float `max_value()` is `+inf`, but NaN sorts above `+inf`** in the `ordered_float` total order (`static_btree/key.rs:139`). Range-lowered operators (`Ge`, `Ne`) therefore silently **exclude NaN-keyed features**. C++ reproduces this so query results match Rust. Document it in the public API docs for `select_attr`.
4. **DateTime `min_value()` is epoch 0** (`static_btree/key.rs:242`) even though the wire format stores a signed `i64` and permits negative seconds. Pre-1970 timestamps are therefore invisible to `Le`/`Ne` range queries. C++ reproduces this.

**Operator lowering** (`static_btree/query/stream.rs:161-191`):
`Eq→find_exact`; `Ge→find_range(key, MAX)`; `Le→find_range(MIN, key)`; `Gt→find_range(key, MAX) minus find_exact(key)`; `Lt→find_range(MIN, key) minus find_exact(key)`; `Ne→find_range(MIN, MAX) minus find_exact(key)`. Multi-condition queries are **AND**-intersected sequentially with early exit on empty (`stream.rs:402-423`).

### HTTP constants

| Quantity | Value | Citation |
|---|---|---|
| `DEFAULT_HTTP_FETCH_SIZE` | `1048576` (1 MB) | `http_reader/mod.rs:42` |
| Open prefetch | `2024 + (1+16+256)*40 = 12944` bytes | `http_reader/mod.rs:80-98` |
| Combine threshold (bbox) | `256*1024` | `http_reader/mod.rs:213` |
| Combine threshold (attr) | `1024*1024` | `http_reader/mod.rs:363` |
| Feature batching rule | `wasted = next.start - prev_end`; same batch if `wasted < threshold` | `http_reader/mod.rs:612-650` |
| Batch request size | `(first.start .. last.start + last.len.value_or(4))`, capped at 1 MB | `http_reader/mod.rs:659-681` |
| Payload prefetch size | `clamp(ceil(num_items*0.1) * 64, 16*1024, 4*1024*1024)` | `stree.rs:417-443` |

---

## File Structure

Everything below lives under `src/cpp/`. The existing contents of `src/cpp/` (the CXX-bridge `CMakeLists.txt`, `include/fcb.h`, `examples/`, `tests/roundtrip_test.cpp`, `build/`, `example_output.fcb`) are deleted in **Task 13**, not before — the bridge must keep working for vcpkg consumers until the native reader passes conformance.

| Path | Responsibility |
|---|---|
| `src/cpp/CMakeLists.txt` | Top-level build; options `FCB_WITH_CURL`, `FCB_WITH_JSON`, `FCB_BUILD_TESTS`; install/export rules |
| `src/cpp/cmake/flatcitybufConfig.cmake.in` | `find_package(flatcitybuf CONFIG)` support |
| `src/cpp/vcpkg.json` | vcpkg manifest (flatbuffers, nlohmann-json; curl in a feature) |
| `src/cpp/generated/*.h` | flatc `--cpp` output for `src/fbs/*.fbs`, **committed** |
| `src/cpp/include/fcb/error.hpp` | `fcb::Error` exception hierarchy, error codes mirroring Rust's `Error` enum |
| `src/cpp/include/fcb/span.hpp` | C++17 `fcb::span<T>` shim (no C++20) |
| `src/cpp/include/fcb/range_reader.hpp` | `RangeRequest`, `RangeReader`, `FileRangeReader`, `BufferedRangeReader` |
| `src/cpp/include/fcb/layout.hpp` | `FileLayout` — section offset arithmetic |
| `src/cpp/include/fcb/header.hpp` | `HeaderView` (owns buffer), `FileInfo`, `ColumnInfo` |
| `src/cpp/include/fcb/feature.hpp` | `Feature` (owns buffer, wraps `CityFeature`) |
| `src/cpp/include/fcb/query.hpp` | `BBox`, `SpatialQuery`, `Operator`, `AttrValue`, `AttrCondition`, `AttrQuery` |
| `src/cpp/include/fcb/reader.hpp` | `FcbReader`, `FeatureIterator` — the public entry point |
| `src/cpp/include/fcb/cityjson.hpp` | CityJSON emission (only when `FCB_WITH_JSON`) |
| `src/cpp/include/fcb/http/curl_range_reader.hpp` | libcurl adapter (only when `FCB_WITH_CURL`) |
| `src/cpp/src/layout.cpp` | Magic/header-size validation, section offset computation |
| `src/cpp/src/range_reader.cpp` | `FileRangeReader`, `BufferedRangeReader`, default `read_batch` |
| `src/cpp/src/packed_rtree.cpp` | Level bounds, index size, streaming bbox search, range coalescing |
| `src/cpp/src/key.cpp` | Key encode/decode/compare for every `KeyType` |
| `src/cpp/src/stree.cpp` | B+tree level bounds, `find_exact`, `find_partition`, `find_range`, operator lowering |
| `src/cpp/src/payload.cpp` | Payload entry decode, tag handling, prefetch cache |
| `src/cpp/src/attribute_decoder.cpp` | Feature attribute blob → `AttrValue` per `Column` schema |
| `src/cpp/src/geom_decoder.cpp` | Boundaries/semantics/templates decode |
| `src/cpp/src/cityjson.cpp` | `Feature`/`HeaderView` → nlohmann JSON |
| `src/cpp/src/reader.cpp` | `FcbReader` orchestration, `FeatureIterator` |
| `src/cpp/src/http/curl_range_reader.cpp` | libcurl multi-range implementation |
| `src/cpp/tests/CMakeLists.txt` | doctest registration |
| `src/cpp/tests/fake_range_reader.hpp` | In-memory `RangeReader` + request log, for deterministic IO tests |
| `src/cpp/tests/test_*.cpp` | One per source module |
| `src/cpp/tests/conformance/` | Golden corpus: `.fcb` + Rust-reader-emitted `.json` pairs |
| `scripts/gen_conformance.sh` | Regenerates the corpus using the Rust CLI |
| `scripts/gen_cpp_flatbuffers.sh` | Regenerates `src/cpp/generated/` via flatc |

---

## Task 1: Branch, skeleton build, and first green test

**Files:**
- Create: `src/cpp/CMakeLists.txt` — note this **overwrites** the existing CXX-bridge CMakeLists. Preserve the old one first (Step 2).
- Create: `src/cpp/include/fcb/span.hpp`, `src/cpp/include/fcb/error.hpp`
- Create: `src/cpp/tests/CMakeLists.txt`, `src/cpp/tests/test_error.cpp`
- Create: `src/cpp/vcpkg.json`

**Interfaces:**
- Consumes: nothing.
- Produces: `fcb::span<T>`; `fcb::Error` with `fcb::ErrorCode`; CMake targets `fcb_core_cpp` (static lib) and `fcb_tests`; CMake options `FCB_WITH_CURL` (default OFF), `FCB_WITH_JSON` (default ON), `FCB_BUILD_TESTS` (default ON).

- [ ] **Step 1: Create the `develop` branch**

`develop` does not exist locally or on the remote. Verify, then create it from `main`:

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git branch -a --list '*develop*'   # expect: no output
git checkout main
git checkout -b develop
```

- [ ] **Step 2: Preserve the CXX bridge so it keeps building**

The native library takes over `src/cpp/`, but the bridge must stay alive until Task 13. Move it aside:

```bash
git mv src/cpp/CMakeLists.txt src/cpp/CMakeLists.bridge.txt
git mv src/cpp/include/fcb.h src/cpp/include/fcb_bridge.h
git commit -m "chore(cpp): park CXX bridge build under .bridge names ahead of native port"
```

Then update the one reference to it in the justfile so CI still builds the bridge. In `justfile`, replace the `pre-commit-cpp` recipe body:

```make
# Run C++ binding checks (legacy CXX bridge — removed in Task 13)
pre-commit-cpp:
    cd src/cpp && cmake -B build-bridge -S . -DCMAKE_PROJECT_INCLUDE=CMakeLists.bridge.txt && cmake --build build-bridge
```

If that `-DCMAKE_PROJECT_INCLUDE` form does not work on the first try, instead move the bridge wholesale to `src/cpp_bridge/` with `git mv` and point the recipe there. Do not spend more than 15 minutes on this — the bridge is deleted in Task 13 regardless.

- [ ] **Step 3: Write the failing test**

Create `src/cpp/tests/test_error.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/error.hpp>
#include <string>

TEST_CASE("Error carries a code and a message") {
    fcb::Error e(fcb::ErrorCode::MissingMagicBytes, "bad magic");
    CHECK(e.code() == fcb::ErrorCode::MissingMagicBytes);
    CHECK(std::string(e.what()) == "bad magic");
}

TEST_CASE("Error is throwable as std::runtime_error") {
    bool caught = false;
    try {
        throw fcb::Error(fcb::ErrorCode::IllegalHeaderSize, "too big");
    } catch (const std::runtime_error& e) {
        caught = true;
        CHECK(std::string(e.what()) == "too big");
    }
    CHECK(caught);
}
```

Create `src/cpp/tests/CMakeLists.txt`:

```cmake
find_package(doctest CONFIG REQUIRED)

add_executable(fcb_tests
    test_error.cpp
)
target_link_libraries(fcb_tests PRIVATE fcb_core_cpp doctest::doctest)
target_compile_definitions(fcb_tests PRIVATE DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN)

include(CTest)
add_test(NAME fcb_tests COMMAND fcb_tests)
```

Create `src/cpp/vcpkg.json`:

```json
{
  "name": "flatcitybuf",
  "version": "0.8.0",
  "description": "Native C++ reader for the FlatCityBuf cloud-optimized CityJSON format",
  "homepage": "https://github.com/cityjson/flatcitybuf",
  "license": "MIT",
  "dependencies": [
    "flatbuffers",
    "nlohmann-json"
  ],
  "features": {
    "curl": {
      "description": "HTTP range-request reader backed by libcurl",
      "dependencies": ["curl"]
    },
    "tests": {
      "description": "Build the test suite",
      "dependencies": ["doctest"]
    }
  }
}
```

Create `src/cpp/CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.16)
project(flatcitybuf VERSION 0.8.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

option(FCB_WITH_JSON  "Build the CityJSON conversion component" ON)
option(FCB_WITH_CURL  "Build the libcurl HTTP range reader"     OFF)
option(FCB_BUILD_TESTS "Build tests"                            ON)

find_package(flatbuffers CONFIG REQUIRED)

add_library(fcb_core_cpp STATIC
    src/layout.cpp
)

target_include_directories(fcb_core_cpp PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/generated>
    $<INSTALL_INTERFACE:include>
)
target_link_libraries(fcb_core_cpp PUBLIC flatbuffers::flatbuffers)

if(FCB_WITH_JSON)
    find_package(nlohmann_json 3.2.0 CONFIG REQUIRED)
    target_link_libraries(fcb_core_cpp PUBLIC nlohmann_json::nlohmann_json)
    target_compile_definitions(fcb_core_cpp PUBLIC FCB_WITH_JSON=1)
endif()

if(FCB_WITH_CURL)
    find_package(CURL REQUIRED)
    target_link_libraries(fcb_core_cpp PUBLIC CURL::libcurl)
    target_compile_definitions(fcb_core_cpp PUBLIC FCB_WITH_CURL=1)
endif()

if(FCB_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
```

`src/layout.cpp` does not exist yet, so create a placeholder containing exactly:

```cpp
// Implemented in Task 3.
namespace fcb { namespace detail { void layout_translation_unit_anchor() {} } }
```

- [ ] **Step 4: Run the test and verify it fails**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp
cmake -B build -S . \
  -DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake \
  -DVCPKG_MANIFEST_FEATURES=tests
cmake --build build
```

`VCPKG_MANIFEST_FEATURES=tests` is required: `FCB_BUILD_TESTS` defaults to `ON` and unconditionally `find_package(doctest ...)`, but doctest lives in an optional vcpkg feature and is otherwise never installed. Add `curl` to the list whenever configuring with `-DFCB_WITH_CURL=ON`.

If you would rather use system packages than vcpkg (faster locally, and what CI does): `brew install flatbuffers nlohmann-json doctest` on macOS, or `apt-get install libflatbuffers-dev nlohmann-json3-dev doctest-dev` on Debian/Ubuntu, then configure without the toolchain file.

Expected: **compile failure**, `fatal error: 'fcb/error.hpp' file not found`.

- [ ] **Step 5: Write the minimal implementation**

Create `src/cpp/include/fcb/span.hpp`:

```cpp
#pragma once
#include <cstddef>
#include <type_traits>
#include <vector>

namespace fcb {

// Minimal C++17 stand-in for std::span. Non-owning view over contiguous memory.
template <typename T>
class span {
public:
    span() noexcept : data_(nullptr), size_(0) {}
    span(T* data, std::size_t size) noexcept : data_(data), size_(size) {}

    template <typename U, typename = std::enable_if_t<std::is_same<const U, T>::value>>
    span(const std::vector<U>& v) noexcept : data_(v.data()), size_(v.size()) {}

    span(std::vector<std::remove_const_t<T>>& v) noexcept : data_(v.data()), size_(v.size()) {}

    T* data() const noexcept { return data_; }
    std::size_t size() const noexcept { return size_; }
    bool empty() const noexcept { return size_ == 0; }
    T& operator[](std::size_t i) const noexcept { return data_[i]; }
    T* begin() const noexcept { return data_; }
    T* end() const noexcept { return data_ + size_; }

    span subspan(std::size_t offset, std::size_t count) const noexcept {
        return span(data_ + offset, count);
    }

private:
    T* data_;
    std::size_t size_;
};

using bytes_view = span<const std::uint8_t>;

}  // namespace fcb
```

Note: `bytes_view` needs `<cstdint>`; add it to the includes.

Create `src/cpp/include/fcb/error.hpp`. The codes mirror `src/rust/fcb_core/src/error.rs`:

```cpp
#pragma once
#include <stdexcept>
#include <string>

namespace fcb {

enum class ErrorCode {
    MissingMagicBytes,
    IllegalHeaderSize,
    InvalidFlatbuffer,
    NoIndex,
    AttributeIndexNotFound,
    NoColumnsInHeader,
    MissingRequiredField,
    UnsupportedColumnType,
    InvalidAttributeValue,
    QueryExecutionError,
    IoError,
    HttpError,
    JsonError,
};

class Error : public std::runtime_error {
public:
    Error(ErrorCode code, const std::string& message)
        : std::runtime_error(message), code_(code) {}

    ErrorCode code() const noexcept { return code_; }

private:
    ErrorCode code_;
};

}  // namespace fcb
```

- [ ] **Step 6: Run the test and verify it passes**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp
cmake --build build && ctest --test-dir build --output-on-failure
```

Expected: `2 test cases passed`, `100% tests passed, 0 tests failed out of 1`.

- [ ] **Step 7: Commit (milestone)**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git add src/cpp/CMakeLists.txt src/cpp/vcpkg.json src/cpp/include/fcb src/cpp/src src/cpp/tests justfile
git commit -m "feat(cpp): scaffold native C++ core with error and span primitives"
```

---

## Task 2: FlatBuffers code generation

**Files:**
- Create: `scripts/gen_cpp_flatbuffers.sh`
- Create: `src/cpp/include/fcb/generated/header_generated.h`, `feature_generated.h`, `geometry_generated.h`, `extension_generated.h` (flatc output, committed). They live under `include/` from the start so the same include path works in-tree and installed.
- Create: `src/cpp/tests/test_generated_schema.cpp`
- Modify: `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: C++ types in the **global namespace** (every `namespace` declaration in the schemas is commented out; verified by running flatc): `::Header`, `::Column`, `::ColumnType`, `::AttributeIndex`, `::Transform`, `::GeographicalExtent`, `::CityFeature`, `::CityObject`, `::Geometry`, `::SemanticObject`, plus the free functions `GetSizePrefixedHeader`, `VerifySizePrefixedHeaderBuffer`, `GetSizePrefixedCityFeature`, `VerifySizePrefixedCityFeatureBuffer`.

- [ ] **Step 1: Write the generation script**

Create `scripts/gen_cpp_flatbuffers.sh`:

```bash
#!/usr/bin/env bash
# Regenerate the committed C++ FlatBuffers headers from src/fbs/*.fbs.
# flatc MUST match the flatbuffers version pinned in src/rust/Cargo.toml (24.3.25).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${REPO_ROOT}/src/cpp/include/fcb/generated"

# flatc must match the C++ flatbuffers RUNTIME the generated code compiles
# against -- generated code calls runtime APIs that change across versions.
# It does NOT need to match the Rust crate pin (24.3.25): the FlatBuffers wire
# format is stable across versions, so Rust-written files read fine here.
EXPECTED_FLATC_VERSION="${FCB_FLATC_VERSION:-25.9.23}"
ACTUAL="$(flatc --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')"
if [[ "${ACTUAL}" != "${EXPECTED_FLATC_VERSION}" ]]; then
  echo "ERROR: flatc ${ACTUAL} found, but ${EXPECTED_FLATC_VERSION} is required." >&2
  echo "       It must match the flatbuffers C++ runtime you build against." >&2
  echo "       Override with FCB_FLATC_VERSION=<v> if you have bumped both." >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
flatc --cpp --scoped-enums -o "${OUT_DIR}" -I "${REPO_ROOT}/src/fbs" \
  "${REPO_ROOT}/src/fbs/header.fbs" \
  "${REPO_ROOT}/src/fbs/feature.fbs" \
  "${REPO_ROOT}/src/fbs/geometry.fbs" \
  "${REPO_ROOT}/src/fbs/extension.fbs"

echo "Generated C++ headers in ${OUT_DIR}"
```

```bash
chmod +x scripts/gen_cpp_flatbuffers.sh
```

- [ ] **Step 2: Write the failing test**

Create `src/cpp/tests/test_generated_schema.cpp`. The generated types are in the **global namespace** — every `namespace FlatCityBuf;` in the schemas is commented out (`header.fbs:1`, `feature.fbs:8`) and flatc emits none. This has been verified empirically; do not add a namespace qualifier.

```cpp
#include <doctest/doctest.h>
#include <fcb/generated/header_generated.h>
#include <fcb/generated/feature_generated.h>

TEST_CASE("AttributeIndex has the padded 16-byte wire layout") {
    // Field order (ushort, uint, ushort, uint) forces 2 bytes of padding after
    // each ushort. Rust's generated type is [u8; 16]; flatc's C++ struct has
    // explicit padding0__/padding1__ members. If this ever fails, the SCHEMA
    // changed -- fix the Format Reference in the plan, not this number.
    CHECK(sizeof(AttributeIndex) == 16);
    CHECK(alignof(AttributeIndex) == 4);
}

TEST_CASE("ColumnType enumerators match the schema's declaration order") {
    // header.fbs:9-26 declares `enum ColumnType: ubyte` in this exact order.
    // The B+tree key mapping depends on these values, so pin them.
    CHECK(static_cast<std::uint8_t>(ColumnType::Byte)     == 0);
    CHECK(static_cast<std::uint8_t>(ColumnType::UByte)    == 1);
    CHECK(static_cast<std::uint8_t>(ColumnType::Bool)     == 2);
    CHECK(static_cast<std::uint8_t>(ColumnType::Short)    == 3);
    CHECK(static_cast<std::uint8_t>(ColumnType::UShort)   == 4);
    CHECK(static_cast<std::uint8_t>(ColumnType::Int)      == 5);
    CHECK(static_cast<std::uint8_t>(ColumnType::UInt)     == 6);
    CHECK(static_cast<std::uint8_t>(ColumnType::Long)     == 7);
    CHECK(static_cast<std::uint8_t>(ColumnType::ULong)    == 8);
    CHECK(static_cast<std::uint8_t>(ColumnType::Float)    == 9);
    CHECK(static_cast<std::uint8_t>(ColumnType::Double)   == 10);
    CHECK(static_cast<std::uint8_t>(ColumnType::String)   == 11);
    CHECK(static_cast<std::uint8_t>(ColumnType::Json)     == 12);
    CHECK(static_cast<std::uint8_t>(ColumnType::DateTime) == 13);
    CHECK(static_cast<std::uint8_t>(ColumnType::Binary)   == 14);
}

TEST_CASE("the size-prefixed root accessors the reader needs exist and are global") {
    // Compile-time surface check. These are free functions in the global
    // namespace: GetSizePrefixedHeader / VerifySizePrefixedHeaderBuffer and
    // the CityFeature equivalents.
    const ::Header* (*get_header)(const void*) = &GetSizePrefixedHeader;
    const ::CityFeature* (*get_feature)(const void*) = &GetSizePrefixedCityFeature;
    CHECK(get_header != nullptr);
    CHECK(get_feature != nullptr);

    // A buffer too short to hold a root must fail verification, not crash.
    const std::uint8_t stub[4] = {0, 0, 0, 0};
    flatbuffers::Verifier v(stub, sizeof(stub));
    CHECK_FALSE(VerifySizePrefixedHeaderBuffer(v));
}
```

Note the include path: `<fcb/generated/...>`. Generated headers live at `src/cpp/include/fcb/generated/` from the start, so the same path works in-tree and after `cmake --install` (see Task 13).

Add `test_generated_schema.cpp` to the `add_executable(fcb_tests ...)` list in `src/cpp/tests/CMakeLists.txt`.

- [ ] **Step 3: Run and verify it fails**

```bash
cd src/cpp && cmake --build build
```

Expected: `fatal error: 'header_generated.h' file not found`.

- [ ] **Step 4: Generate the headers**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
./scripts/gen_cpp_flatbuffers.sh
```

If `flatc` is absent: `brew install flatbuffers` on macOS (currently ships 25.9.23, which matches), or `apt-get install flatbuffers-compiler`. The script fails if flatc's version differs from the C++ **runtime** you build against — install a matching pair rather than relaxing the check. It does **not** need to match the Rust crate pin (24.3.25); the wire format is stable across versions.

- [ ] **Step 5: Run and verify it passes**

```bash
cd src/cpp && cmake --build build && ctest --test-dir build --output-on-failure
```

Expected: all tests pass. If `sizeof(AttributeIndex) != 16`, the **schema itself has changed** — read the generated struct, update the Format Reference in this plan, and work out what else the layout change breaks. Do not simply edit the number to match.

- [ ] **Step 6: Commit (milestone)**

```bash
git add scripts/gen_cpp_flatbuffers.sh src/cpp/include/fcb/generated src/cpp/tests
git commit -m "feat(cpp): generate and commit FlatBuffers C++ headers"
```

---

## Task 2b: Generate the conformance corpus (before any decoder needs it)

Fixture generation is pure Rust-CLI work with **no C++ dependency**, so it comes early. Tasks 7a, 8, 10 and 12 all consume these files; creating them in Task 12 (as originally sequenced) would make the earlier tasks' tests uncompilable and TDD impossible.

**Files:**
- Create: `scripts/gen_conformance.sh` (Class A), `src/rust/fcb_conformance/` (Class B generator), `scripts/gen_malformed.py` (Class C)
- Create: `src/cpp/tests/conformance/` fixtures and `src/cpp/tests/conformance/inputs/*.city.jsonl`
- Modify: `src/rust/Cargo.toml` (add `fcb_conformance` to workspace members), `src/cpp/tests/CMakeLists.txt` (define `FCB_CONFORMANCE_DIR`)

**Interfaces:**
- Consumes: the Rust CLI and `fcb_core`.
- Produces: `FCB_CONFORMANCE_DIR` pointing at a directory of `<name>.fcb` plus `<name>.expected.jsonl` (Class A) or `<name>.expected.json` (Class B), and `malformed/<name>.fcb` (Class C).

- [ ] **Step 1: Confirm the real CLI subcommand names**

The plan uses `ser`/`deser`/`info` as placeholders. Establish the truth before scripting anything:

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust
cargo run -p fcb_cli -- --help
```

Record the actual subcommands and flags, and use them verbatim in every script below and in Tasks 7c, 8, 10 and 12.

- [ ] **Step 2: Author the Class A inputs and generator**

Write the Class A input files and `scripts/gen_conformance.sh` exactly as specified in Task 12 (which now only *documents* the corpus rather than creating it). Run it and confirm each `.fcb` and `.expected.jsonl` pair exists and is non-empty.

- [ ] **Step 3: Build the Class B binary generator**

Add `src/rust/fcb_conformance/` as a dev-only workspace member (`publish = false`) that constructs `Header` column schemas and attribute indexes directly, bypassing JSON inference, and emits the Class B fixtures plus hand-authored `.expected.json` files. Add it to `[workspace] members`.

Verify it does not break the workspace:

```bash
cd src/rust && cargo check --workspace --exclude fcb_wasm --exclude fcb_py
```

- [ ] **Step 4: Build the Class C malformed generator**

Write `scripts/gen_malformed.py`, which copies a valid `.fcb` and mutates it into each malformed case listed in Task 12. Each output goes to `src/cpp/tests/conformance/malformed/`.

- [ ] **Step 5: Wire up `FCB_CONFORMANCE_DIR`**

Add to `src/cpp/tests/CMakeLists.txt`:

```cmake
target_compile_definitions(fcb_tests PRIVATE
    FCB_CONFORMANCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}/conformance"
)
```

- [ ] **Step 6: Decide what is committed**

Commit the Class A/B/C **outputs**, not just the generators — the C++ test suite must run without a Rust toolchain, and CI has no Rust job for the C++ path. Keep them small: cap `delft` derivatives and prefer `small.city.jsonl` where a large file adds nothing. If total fixture size exceeds ~20 MB, commit the small ones and generate `delft` on demand behind `FCB_DIFFERENTIAL_TESTS`.

- [ ] **Step 7: Commit (milestone)**

```bash
git add scripts/gen_conformance.sh scripts/gen_malformed.py src/rust/fcb_conformance \
        src/rust/Cargo.toml src/cpp/tests/conformance src/cpp/tests/CMakeLists.txt
git commit -m "test(cpp): generate conformance corpus (valid, binary, malformed)"
```

---

## Task 3: File layout — magic, header size, section offsets

This is the highest-risk arithmetic in the whole port. An off-by-one here silently corrupts everything downstream, so it gets its own task with pure-function tests and no IO.

**Files:**
- Create: `src/cpp/include/fcb/layout.hpp`
- Modify: `src/cpp/src/layout.cpp` (replace the Task 1 placeholder)
- Create: `src/cpp/tests/test_layout.cpp`
- Modify: `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `fcb::Error`, `fcb::ErrorCode`, `fcb::bytes_view`.
- Produces:
  - `constexpr std::size_t fcb::kMagicBytesSize = 8;`
  - `constexpr std::size_t fcb::kHeaderSizeSize = 4;`
  - `constexpr std::size_t fcb::kHeaderMaxBufferSize = 536870912;`
  - `constexpr std::uint8_t fcb::kVersion = 1;`
  - `bool fcb::check_magic_bytes(bytes_view b);`
  - `std::uint64_t fcb::rtree_index_size(std::uint64_t num_items, std::uint16_t node_size);`
  - `struct fcb::FileLayout { uint64_t header_len, rtree_begin, rtree_size, attr_index_begin, attr_index_size, feature_begin; };`
  - `FileLayout fcb::compute_layout(std::uint32_t header_size, std::uint64_t features_count, std::uint16_t index_node_size, std::uint64_t attr_index_size);`

- [ ] **Step 1: Write the failing test**

Create `src/cpp/tests/test_layout.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/layout.hpp>
#include <vector>

using namespace fcb;

TEST_CASE("magic bytes validation mirrors Rust check_magic_bytes") {
    // Only [0..3) and [4..7) are compared; byte 3 must be <= VERSION; byte 7 is ignored.
    std::vector<std::uint8_t> ok = {'f','c','b',1,'f','c','b',0};
    CHECK(check_magic_bytes(bytes_view(ok)));

    std::vector<std::uint8_t> byte7_garbage = {'f','c','b',1,'f','c','b',0xAB};
    CHECK(check_magic_bytes(bytes_view(byte7_garbage)));  // byte 7 is never validated

    std::vector<std::uint8_t> version_zero = {'f','c','b',0,'f','c','b',0};
    CHECK(check_magic_bytes(bytes_view(version_zero)));   // 0 <= 1

    std::vector<std::uint8_t> future_version = {'f','c','b',2,'f','c','b',0};
    CHECK_FALSE(check_magic_bytes(bytes_view(future_version)));  // 2 > 1

    std::vector<std::uint8_t> bad_prefix = {'x','c','b',1,'f','c','b',0};
    CHECK_FALSE(check_magic_bytes(bytes_view(bad_prefix)));

    std::vector<std::uint8_t> bad_second = {'f','c','b',1,'f','c','x',0};
    CHECK_FALSE(check_magic_bytes(bytes_view(bad_second)));

    std::vector<std::uint8_t> too_short = {'f','c','b',1};
    CHECK_FALSE(check_magic_bytes(bytes_view(too_short)));
}

TEST_CASE("rtree_index_size matches the Rust formula") {
    // n=1: num_nodes=1; loop: n=ceil(1/16)=1, num_nodes=2, n==1 -> break. => 2*40
    CHECK(rtree_index_size(1, 16) == 80);
    // n=16: num_nodes=16; n=1, num_nodes=17, break. => 17*40
    CHECK(rtree_index_size(16, 16) == 680);
    // n=17: num_nodes=17; n=2, num_nodes=19; n=1, num_nodes=20, break. => 20*40
    CHECK(rtree_index_size(17, 16) == 800);
    // n=257: 257 -> 17 (274) -> 2 (276) -> 1 (277). => 277*40
    CHECK(rtree_index_size(257, 16) == 11080);
    // A node_size of 0 or 1 is a CORRUPT FILE, not something to clamp.
    // Rust asserts node_size >= 2 before clamping (packed_rtree/mod.rs:879),
    // so clamping in C++ would invent behaviour Rust never exhibits.
    CHECK_THROWS_AS(rtree_index_size(4, 0), Error);
    CHECK_THROWS_AS(rtree_index_size(4, 1), Error);
    CHECK_NOTHROW(rtree_index_size(4, 2));
}

TEST_CASE("size arithmetic is checked against overflow on hostile input") {
    // features_count is an untrusted u64 straight from the file. num_nodes
    // grows past it and is then multiplied by 40; both must be checked.
    CHECK_THROWS_AS(rtree_index_size(UINT64_MAX, 2), Error);
    CHECK_THROWS_AS(rtree_index_size(UINT64_MAX / 8, 2), Error);
    // A plausible-looking but absurd count must not wrap into a small number.
    CHECK_THROWS_AS(compute_layout(100, UINT64_MAX, 16, 0), Error);
}

TEST_CASE("compute_layout rejects attribute index sizes that overflow the file") {
    CHECK_THROWS_AS(compute_layout(100, 1, 16, UINT64_MAX), Error);
}

TEST_CASE("compute_layout stacks sections with no padding") {
    // header_size = 100 -> header_len = 8 + 4 + 100 = 112
    FileLayout l = compute_layout(/*header_size=*/100, /*features_count=*/17,
                                  /*index_node_size=*/16, /*attr_index_size=*/500);
    CHECK(l.header_len == 112);
    CHECK(l.rtree_begin == 112);
    CHECK(l.rtree_size == 800);
    CHECK(l.attr_index_begin == 912);
    CHECK(l.attr_index_size == 500);
    CHECK(l.feature_begin == 1412);
}

TEST_CASE("compute_layout suppresses the rtree when it is absent") {
    FileLayout no_index = compute_layout(100, 17, /*index_node_size=*/0, 0);
    CHECK(no_index.rtree_size == 0);
    CHECK(no_index.feature_begin == 112);

    FileLayout no_features = compute_layout(100, /*features_count=*/0, 16, 0);
    CHECK(no_features.rtree_size == 0);
    CHECK(no_features.feature_begin == 112);
}

TEST_CASE("compute_layout rejects illegal header sizes") {
    CHECK_THROWS_AS(compute_layout(7, 1, 16, 0), Error);
    CHECK_THROWS_AS(compute_layout(536870913, 1, 16, 0), Error);
    CHECK_NOTHROW(compute_layout(8, 1, 16, 0));
    CHECK_NOTHROW(compute_layout(536870912, 1, 16, 0));
}
```

Add `test_layout.cpp` to `src/cpp/tests/CMakeLists.txt`.

- [ ] **Step 2: Run and verify it fails**

```bash
cd src/cpp && cmake --build build
```

Expected: `fatal error: 'fcb/layout.hpp' file not found`.

- [ ] **Step 3: Write the implementation**

First create `src/cpp/src/detail/checked.hpp` — an **internal** header (not installed) that every module uses at its trust boundaries. Putting these in an anonymous namespace inside `layout.cpp` would leave the feature, R-tree, B+tree, payload, cache and HTTP code without them, which is exactly where hostile arithmetic lands.

```cpp
#pragma once
#include <cstdint>
#include <fcb/error.hpp>
#include <string>

namespace fcb {
namespace detail {

// These operate on untrusted, file-supplied values. Overflow must THROW,
// never wrap -- a wrapped size becomes an under-allocated buffer, which is
// how a length check turns into a heap overflow.

inline std::uint64_t checked_add(std::uint64_t a, std::uint64_t b,
                                 const char* what = "add") {
    if (a > UINT64_MAX - b) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    std::string("size arithmetic overflow (") + what + ")");
    }
    return a + b;
}

inline std::uint64_t checked_mul(std::uint64_t a, std::uint64_t b,
                                 const char* what = "mul") {
    if (a != 0 && b > UINT64_MAX / a) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    std::string("size arithmetic overflow (") + what + ")");
    }
    return a * b;
}

/// ceil(a / b) without the (a + b - 1) overflow hazard. Throws on b == 0
/// rather than trapping -- callers pass file-supplied divisors.
inline std::uint64_t ceil_div(std::uint64_t a, std::uint64_t b) {
    if (b == 0) {
        throw Error(ErrorCode::IllegalHeaderSize, "division by zero in size arithmetic");
    }
    return a / b + (a % b != 0 ? 1 : 0);
}

/// End of a range, checked. Use this at EVERY place that forms offset+length:
/// cache coverage tests, feature cursor advance, node slab bounds, payload
/// entry bounds, and HTTP Range header construction.
inline std::uint64_t range_end(std::uint64_t offset, std::uint64_t length) {
    return checked_add(offset, length, "range_end");
}

/// Throws unless [offset, offset+length) lies wholly within `limit`.
inline void require_within(std::uint64_t offset, std::uint64_t length,
                           std::uint64_t limit, const char* what) {
    if (range_end(offset, length) > limit) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    std::string("range out of bounds: ") + what);
    }
}

}  // namespace detail
}  // namespace fcb
```

Then create `src/cpp/include/fcb/layout.hpp`:

```cpp
#pragma once
#include <cstdint>
#include <fcb/error.hpp>
#include <fcb/span.hpp>

namespace fcb {

constexpr std::size_t  kMagicBytesSize      = 8;
constexpr std::size_t  kHeaderSizeSize      = 4;
constexpr std::size_t  kHeaderMinBufferSize = 8;
constexpr std::size_t  kHeaderMaxBufferSize = 1024ull * 1024ull * 512ull;  // 512 MB
constexpr std::uint8_t kVersion             = 1;
constexpr std::size_t  kNodeItemSize        = 40;
constexpr std::uint16_t kDefaultNodeSize    = 16;

/// Mirrors fcb_core::check_magic_bytes (src/rust/fcb_core/src/lib.rs:56-58).
/// Compares only bytes [0,3) and [4,7); byte 7 is never validated.
bool check_magic_bytes(bytes_view b);

/// Mirrors PackedRTree::index_size (packed_rtree/mod.rs:879-898). Returns bytes.
/// Throws fcb::Error on node_size < 2, num_items == 0, or arithmetic overflow.
std::uint64_t rtree_index_size(std::uint64_t num_items, std::uint16_t node_size);

struct FileLayout {
    std::uint64_t header_len;
    std::uint64_t rtree_begin;
    std::uint64_t rtree_size;
    std::uint64_t attr_index_begin;
    std::uint64_t attr_index_size;
    std::uint64_t feature_begin;
};

/// Throws fcb::Error{IllegalHeaderSize} when header_size is out of range or
/// any size arithmetic overflows.
FileLayout compute_layout(std::uint32_t header_size,
                          std::uint64_t features_count,
                          std::uint16_t index_node_size,
                          std::uint64_t attr_index_size);

/// Throws unless the computed sections fit inside the resource. Call this
/// immediately after compute_layout, before issuing any index read.
void validate_layout_against_size(const FileLayout& l, std::uint64_t total_size);

/// Hard ceiling on a single feature's byte length, enforced before allocating.
/// A crafted 4-byte prefix would otherwise request up to 4 GiB.
constexpr std::uint64_t kMaxFeatureSize = 256ull * 1024ull * 1024ull;

}  // namespace fcb
```

Replace `src/cpp/src/layout.cpp` entirely:

```cpp
#include <fcb/layout.hpp>
#include "detail/checked.hpp"

#include <algorithm>
#include <cstring>
#include <string>

namespace fcb {

using detail::checked_add;
using detail::checked_mul;
using detail::ceil_div;

bool check_magic_bytes(bytes_view b) {
    if (b.size() < kMagicBytesSize) return false;
    static const std::uint8_t kFcb[3] = {'f', 'c', 'b'};
    if (std::memcmp(b.data() + 0, kFcb, 3) != 0) return false;
    if (std::memcmp(b.data() + 4, kFcb, 3) != 0) return false;
    return b[3] <= kVersion;
}

std::uint64_t rtree_index_size(std::uint64_t num_items, std::uint16_t node_size) {
    // Rust asserts node_size >= 2 (packed_rtree/mod.rs:879). 0 or 1 means the
    // file is corrupt: reject rather than clamp, so we never invent a layout.
    if (node_size < 2) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    "invalid index_node_size: " + std::to_string(node_size));
    }
    if (num_items == 0) {
        throw Error(ErrorCode::IllegalHeaderSize, "rtree_index_size requires num_items > 0");
    }
    const std::uint64_t ns = node_size;
    std::uint64_t n = num_items;
    std::uint64_t num_nodes = n;
    for (;;) {
        n = ceil_div(n, ns);
        num_nodes = checked_add(num_nodes, n);
        if (n == 1) break;
    }
    return checked_mul(num_nodes, kNodeItemSize);
}

FileLayout compute_layout(std::uint32_t header_size,
                          std::uint64_t features_count,
                          std::uint16_t index_node_size,
                          std::uint64_t attr_index_size) {
    if (header_size < kHeaderMinBufferSize || header_size > kHeaderMaxBufferSize) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    "illegal header size: " + std::to_string(header_size));
    }

    FileLayout l{};
    l.header_len = kMagicBytesSize + kHeaderSizeSize + header_size;
    l.rtree_begin = l.header_len;
    // index_node_size == 0 means "no spatial index" and is legal; any other
    // value < 2 is corrupt and rtree_index_size will reject it.
    l.rtree_size = (index_node_size == 0 || features_count == 0)
                       ? 0
                       : rtree_index_size(features_count, index_node_size);
    l.attr_index_begin = checked_add(l.rtree_begin, l.rtree_size);
    l.attr_index_size = attr_index_size;
    l.feature_begin = checked_add(l.attr_index_begin, l.attr_index_size);
    return l;
}

void validate_layout_against_size(const FileLayout& l, std::uint64_t total_size) {
    if (l.feature_begin > total_size) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    "sections extend past end of file: feature_begin=" +
                        std::to_string(l.feature_begin) +
                        " total_size=" + std::to_string(total_size));
    }
}

}  // namespace fcb
```

Note the `rtree_index_size` loop: `num_items == 0` would loop forever, which is why `compute_layout` short-circuits on `features_count == 0` before calling it. Do not call `rtree_index_size(0, ...)` directly.

- [ ] **Step 4: Run and verify it passes**

```bash
cd src/cpp && cmake --build build && ctest --test-dir build --output-on-failure
```

Expected: all `test_layout.cpp` cases pass.

- [ ] **Step 5: Cross-check against a real file**

Add a temporary sanity check against the committed fixture — this catches a wrong formula that happens to satisfy the synthetic cases:

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
xxd -l 16 examples/data/delft.fcb
```

Expected first 8 bytes: `6663 6201 6663 6200` (`fcb\x01fcb\x00`). Bytes 8–11 are the LE header size. Confirm the header size falls in `[8, 512MB]`.

- [ ] **Step 6: Commit (milestone)**

```bash
git add src/cpp/include/fcb/layout.hpp src/cpp/src/layout.cpp src/cpp/tests
git commit -m "feat(cpp): implement file layout arithmetic and magic validation"
```

---

## Task 4: The `RangeReader` interface, file adapter, and buffered decorator

This is the architectural centrepiece — the reason for the whole port. Everything above the IO layer speaks only `RangeReader`.

**Files:**
- Create: `src/cpp/include/fcb/range_reader.hpp`
- Create: `src/cpp/src/range_reader.cpp`
- Create: `src/cpp/tests/fake_range_reader.hpp`, `src/cpp/tests/test_range_reader.cpp`
- Modify: `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `fcb::Error`, `fcb::bytes_view`.
- Produces:
  - `struct fcb::RangeRequest { std::uint64_t offset; std::uint64_t length; std::vector<std::uint8_t> data; };`
  - `class fcb::RangeReader` — pure virtual `total_size()`, `read(offset, length) -> std::vector<uint8_t>`; virtual `read_batch(std::vector<RangeRequest>&)` defaulting to a loop over `read`.
  - `class fcb::FileRangeReader : public RangeReader` — ctor `FileRangeReader(const std::string& path)`.
  - `class fcb::BufferedRangeReader : public RangeReader` — ctor `BufferedRangeReader(std::shared_ptr<RangeReader> inner, std::uint64_t min_req_size)`. **No `set_min_req_size`**: the window size is fixed at construction because this is a per-query object (see the ownership note on the class).

- [ ] **Step 1: Write the failing test**

Create `src/cpp/tests/fake_range_reader.hpp` first — every later task depends on it:

```cpp
#pragma once
#include <fcb/range_reader.hpp>

#include <cstdint>
#include <stdexcept>
#include <vector>

namespace fcb {
namespace testing {

/// In-memory RangeReader that records every request, so tests can assert on
/// IO behaviour (coalescing, prefetch, request counts) deterministically.
class FakeRangeReader : public RangeReader {
public:
    explicit FakeRangeReader(std::vector<std::uint8_t> data) : data_(std::move(data)) {}

    std::uint64_t total_size() override { return data_.size(); }

    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override {
        requests.push_back({offset, length});
        if (offset > data_.size()) throw std::out_of_range("offset past end");
        const std::uint64_t end = std::min<std::uint64_t>(offset + length, data_.size());
        return std::vector<std::uint8_t>(data_.begin() + static_cast<std::ptrdiff_t>(offset),
                                         data_.begin() + static_cast<std::ptrdiff_t>(end));
    }

    struct Req { std::uint64_t offset; std::uint64_t length; };
    std::vector<Req> requests;

private:
    std::vector<std::uint8_t> data_;
};

}  // namespace testing
}  // namespace fcb
```

Create `src/cpp/tests/test_range_reader.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/range_reader.hpp>
#include "fake_range_reader.hpp"

#include <cstdio>
#include <fstream>
#include <numeric>

using namespace fcb;

static std::vector<std::uint8_t> iota_bytes(std::size_t n) {
    std::vector<std::uint8_t> v(n);
    for (std::size_t i = 0; i < n; ++i) v[i] = static_cast<std::uint8_t>(i & 0xFF);
    return v;
}

TEST_CASE("FileRangeReader reads exact ranges and reports total size") {
    const std::string path = "test_frr.bin";
    auto data = iota_bytes(1000);
    { std::ofstream f(path, std::ios::binary);
      f.write(reinterpret_cast<const char*>(data.data()), 1000); }

    FileRangeReader r(path);
    CHECK(r.total_size() == 1000);

    auto chunk = r.read(100, 10);
    REQUIRE(chunk.size() == 10);
    CHECK(chunk[0] == 100);
    CHECK(chunk[9] == 109);

    // Reading past EOF truncates rather than throwing.
    auto tail = r.read(995, 50);
    CHECK(tail.size() == 5);

    std::remove(path.c_str());
}

TEST_CASE("default read_batch fills every request") {
    testing::FakeRangeReader fake(iota_bytes(1000));
    std::vector<RangeRequest> reqs = {{10, 4, {}}, {500, 2, {}}};
    fake.read_batch(reqs);

    REQUIRE(reqs[0].data.size() == 4);
    CHECK(reqs[0].data[0] == 10);
    REQUIRE(reqs[1].data.size() == 2);
    CHECK(reqs[1].data[0] == 244);  // 500 & 0xFF
    CHECK(fake.requests.size() == 2);
}

TEST_CASE("BufferedRangeReader over-fetches to min_req_size and serves hits from cache") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(10000));
    BufferedRangeReader buf(fake, /*min_req_size=*/1024);

    auto a = buf.read(0, 8);
    REQUIRE(a.size() == 8);
    REQUIRE(fake->requests.size() == 1);
    CHECK(fake->requests[0].offset == 0);
    CHECK(fake->requests[0].length == 1024);  // over-fetched

    // Inside the cached window -> no new upstream request.
    auto b = buf.read(500, 20);
    REQUIRE(b.size() == 20);
    CHECK(b[0] == 244);
    CHECK(fake->requests.size() == 1);

    // Outside the window -> exactly one more request.
    auto c = buf.read(5000, 4);
    REQUIRE(c.size() == 4);
    CHECK(c[0] == 136);  // 5000 & 0xFF
    CHECK(fake->requests.size() == 2);
}

TEST_CASE("BufferedRangeReader::read_batch serves cache hits without upstream reads") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(10000));
    BufferedRangeReader buf(fake, /*min_req_size=*/1024);

    buf.read(0, 8);                       // primes the cache with [0, 1024)
    REQUIRE(fake->requests.size() == 1);

    std::vector<RangeRequest> reqs = {{10, 4, {}}, {900, 4, {}}};
    buf.read_batch(reqs);                 // both inside the cached window

    CHECK(fake->requests.size() == 1);    // no new upstream traffic
    REQUIRE(reqs[0].data.size() == 4);
    CHECK(reqs[0].data[0] == 10);
    REQUIRE(reqs[1].data.size() == 4);
    CHECK(reqs[1].data[0] == 132);        // 900 & 0xFF
}

TEST_CASE("BufferedRangeReader::read_batch forwards only misses, preserving order") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(10000));
    BufferedRangeReader buf(fake, /*min_req_size=*/1024);

    buf.read(0, 8);                       // caches [0, 1024)
    fake->requests.clear();

    std::vector<RangeRequest> reqs = {{10, 4, {}}, {5000, 4, {}}, {20, 4, {}}};
    buf.read_batch(reqs);

    // Only the 5000 request should have gone upstream.
    REQUIRE(fake->requests.size() == 1);
    CHECK(fake->requests[0].offset == 5000);

    // Order preserved, every request filled.
    CHECK(reqs[0].data[0] == 10);
    CHECK(reqs[1].data[0] == 136);        // 5000 & 0xFF
    CHECK(reqs[2].data[0] == 20);
}

TEST_CASE("BufferedRangeReader honours reads larger than min_req_size") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(10000));
    BufferedRangeReader buf(fake, /*min_req_size=*/64);

    auto big = buf.read(100, 2000);
    CHECK(big.size() == 2000);
    REQUIRE(fake->requests.size() == 1);
    CHECK(fake->requests[0].length == 2000);
}
```

Add both files to `src/cpp/tests/CMakeLists.txt` (`fake_range_reader.hpp` needs no entry; add `test_range_reader.cpp` to the sources and add `target_include_directories(fcb_tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})`).

- [ ] **Step 2: Run and verify it fails**

Expected: `fatal error: 'fcb/range_reader.hpp' file not found`.

- [ ] **Step 3: Write the implementation**

Create `src/cpp/include/fcb/range_reader.hpp`:

```cpp
#pragma once
#include <cstdint>
#include <fstream>
#include <memory>
#include <string>
#include <vector>

namespace fcb {

/// One range in a batched read. `data` is filled by read_batch().
struct RangeRequest {
    std::uint64_t offset;
    std::uint64_t length;
    std::vector<std::uint8_t> data;
};

/// Synchronous byte-range source. Implement this to plug in any transport
/// (file, HTTP, memory, engine VFS). The core never assumes asynchrony;
/// batching is the concurrency primitive.
///
/// CONTRACT -- implementors must honour all of it:
///
///  * read(offset, length) returns EXACTLY `length` bytes unless the range
///    extends past the end of the resource, in which case it returns exactly
///    the bytes that exist (possibly zero). It must never return a short
///    buffer for any other reason; a truncated network response is an error,
///    not a short read, and must throw fcb::Error{HttpError}.
///  * offset > total_size() returns empty; it is not an error.
///  * length == 0 returns empty without contacting the transport.
///  * Errors are reported by throwing fcb::Error. Returning garbage is not
///    an option -- the core cannot distinguish it from data.
///  * read_batch fills every element's `data` in place. Request ORDER IS
///    PRESERVED; the i-th request's bytes land in the i-th element. An
///    implementation may reorder its internal fetches freely.
///  * Partial batch failure is all-or-nothing: if any request cannot be
///    satisfied, read_batch throws and the caller must not inspect `data`.
///  * The resource must be STABLE for the lifetime of the reader. The core
///    issues many ranges against one logical file and assumes they come from
///    the same bytes. HTTP implementations must pin the representation (see
///    CurlRangeReader: Accept-Encoding: identity plus ETag/If-Match).
///  * THREAD SAFETY: instances are NOT thread-safe. One RangeReader serves
///    one query at a time. Concurrent queries need separate instances.
///  * There is no cancellation mechanism. A transport that needs one should
///    implement it out-of-band (e.g. a flag its read() checks and throws on).
class RangeReader {
public:
    virtual ~RangeReader() = default;

    /// Total byte length of the resource.
    ///
    /// This is a REQUIRED bounds/security contract, not merely a convenience:
    /// every computed section offset and every range request is validated
    /// against it before use, so a corrupt header cannot make the core read
    /// or allocate out of bounds. (It is NOT needed to size the last feature
    /// -- every feature carries its own 4-byte size prefix.)
    virtual std::uint64_t total_size() = 0;

    /// Read `length` bytes at `offset`, subject to the contract above.
    virtual std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) = 0;

    /// Fill every request, preserving order. Transports that can pipeline or
    /// multiplex should override this; the default is a sequential loop.
    virtual void read_batch(std::vector<RangeRequest>& requests);
};

/// Local-file adapter.
class FileRangeReader : public RangeReader {
public:
    explicit FileRangeReader(const std::string& path);

    std::uint64_t total_size() override;
    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override;

private:
    std::string path_;
    std::ifstream stream_;
    std::uint64_t size_;
};

/// Caching decorator: over-fetches to `min_req_size` and serves subsequent
/// reads inside the cached window without touching the inner reader.
/// This is what makes HTTP traversal cheap and file traversal unchanged.
///
/// OWNERSHIP NOTE: this is a PER-QUERY object. Each query (select_all,
/// select_bbox, select_attr) constructs its own BufferedRangeReader around
/// the shared transport with the min_req_size appropriate to its phase.
/// Never wrap once in FcbReader::open() and mutate min_req_size later --
/// that makes concurrent iterators silently alter each other's buffering
/// policy, and invites a decorator wrapping a decorator.
class BufferedRangeReader : public RangeReader {
public:
    BufferedRangeReader(std::shared_ptr<RangeReader> inner, std::uint64_t min_req_size);

    std::uint64_t total_size() override;
    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override;
    void read_batch(std::vector<RangeRequest>& requests) override;

private:
    // Checked arithmetic: an overflowing offset+length must not wrap into a
    // false cache hit, which would then build invalid iterators in
    // slice_from_buffer(). Uses detail::range_end, which throws on overflow.
    bool covers(std::uint64_t offset, std::uint64_t length) const;
    std::vector<std::uint8_t> slice_from_buffer(std::uint64_t offset,
                                                std::uint64_t length) const;

    std::shared_ptr<RangeReader> inner_;
    std::uint64_t min_req_size_;
    std::uint64_t buf_offset_ = 0;
    std::vector<std::uint8_t> buf_;
};

}  // namespace fcb
```

Create `src/cpp/src/range_reader.cpp`:

```cpp
#include <fcb/range_reader.hpp>
#include <fcb/error.hpp>

#include <algorithm>

namespace fcb {

void RangeReader::read_batch(std::vector<RangeRequest>& requests) {
    for (auto& r : requests) {
        r.data = read(r.offset, r.length);
    }
}

FileRangeReader::FileRangeReader(const std::string& path)
    : path_(path), stream_(path, std::ios::binary | std::ios::ate) {
    if (!stream_) {
        throw Error(ErrorCode::IoError, "cannot open file: " + path);
    }
    size_ = static_cast<std::uint64_t>(stream_.tellg());
}

std::uint64_t FileRangeReader::total_size() { return size_; }

std::vector<std::uint8_t> FileRangeReader::read(std::uint64_t offset, std::uint64_t length) {
    if (offset >= size_) return {};
    const std::uint64_t n = std::min<std::uint64_t>(length, size_ - offset);
    std::vector<std::uint8_t> out(static_cast<std::size_t>(n));
    stream_.clear();
    stream_.seekg(static_cast<std::streamoff>(offset), std::ios::beg);
    stream_.read(reinterpret_cast<char*>(out.data()), static_cast<std::streamsize>(n));
    if (stream_.gcount() != static_cast<std::streamsize>(n)) {
        throw Error(ErrorCode::IoError, "short read from " + path_);
    }
    return out;
}

BufferedRangeReader::BufferedRangeReader(std::shared_ptr<RangeReader> inner,
                                         std::uint64_t min_req_size)
    : inner_(std::move(inner)), min_req_size_(min_req_size) {}

std::uint64_t BufferedRangeReader::total_size() { return inner_->total_size(); }

bool BufferedRangeReader::covers(std::uint64_t offset, std::uint64_t length) const {
    if (buf_.empty() || offset < buf_offset_) return false;
    // Throws rather than wrapping; both ends are file-supplied.
    return detail::range_end(offset, length) <=
           detail::range_end(buf_offset_, buf_.size());
}

std::vector<std::uint8_t> BufferedRangeReader::read(std::uint64_t offset, std::uint64_t length) {
    if (length == 0) return {};  // contract: never contact the transport
    if (!covers(offset, length)) {
        const std::uint64_t fetch = std::max<std::uint64_t>(length, min_req_size_);
        buf_ = inner_->read(offset, fetch);
        buf_offset_ = offset;
    }
    const std::uint64_t rel = offset - buf_offset_;
    if (rel >= buf_.size()) return {};
    const std::uint64_t n = std::min<std::uint64_t>(length, buf_.size() - rel);
    return std::vector<std::uint8_t>(buf_.begin() + static_cast<std::ptrdiff_t>(rel),
                                     buf_.begin() + static_cast<std::ptrdiff_t>(rel + n));
}

std::vector<std::uint8_t> BufferedRangeReader::slice_from_buffer(
    std::uint64_t offset, std::uint64_t length) const {
    const std::uint64_t rel = offset - buf_offset_;
    return std::vector<std::uint8_t>(
        buf_.begin() + static_cast<std::ptrdiff_t>(rel),
        buf_.begin() + static_cast<std::ptrdiff_t>(rel + length));
}

void BufferedRangeReader::read_batch(std::vector<RangeRequest>& requests) {
    // Serve what the cache already covers, forward only the misses. Blindly
    // forwarding everything would defeat the decorator exactly when tree
    // traversal batches -- which is its whole reason to exist.
    struct Miss { std::size_t index; std::uint64_t offset, want, fetch; };
    std::vector<Miss> misses;
    misses.reserve(requests.size());

    for (std::size_t i = 0; i < requests.size(); ++i) {
        auto& r = requests[i];
        if (covers(r.offset, r.length)) {
            r.data = slice_from_buffer(r.offset, r.length);
        } else {
            misses.push_back(Miss{i, r.offset, r.length,
                                  std::max(r.length, min_req_size_)});
        }
    }
    if (misses.empty()) return;

    // Over-fetch each miss to min_req_size, exactly as read() does; otherwise
    // the cache seeded below would be one request wide and buy nothing.
    std::vector<RangeRequest> fetches;
    fetches.reserve(misses.size());
    for (const auto& m : misses) fetches.push_back(RangeRequest{m.offset, m.fetch, {}});

    inner_->read_batch(fetches);

    // Hand back only the bytes each caller asked for -- never the over-fetch.
    for (std::size_t k = 0; k < misses.size(); ++k) {
        const auto& m = misses[k];
        auto& got = fetches[k].data;
        const std::uint64_t n = std::min<std::uint64_t>(m.want, got.size());
        requests[m.index].data.assign(
            got.begin(), got.begin() + static_cast<std::ptrdiff_t>(n));
    }

    // Seed the single-window cache from the last over-fetched block. Traversal
    // walks forward, so the most recently fetched range is the likeliest hit.
    buf_offset_ = misses.back().offset;
    buf_ = std::move(fetches.back().data);
}

}  // namespace fcb
```

Add `src/range_reader.cpp` to `add_library(fcb_core_cpp ...)` in `src/cpp/CMakeLists.txt`.

- [ ] **Step 4: Run and verify it passes**

```bash
cd src/cpp && cmake --build build && ctest --test-dir build --output-on-failure
```

Expected: all `test_range_reader.cpp` cases pass.

- [ ] **Step 5: Commit (milestone)**

```bash
git add src/cpp/include/fcb/range_reader.hpp src/cpp/src/range_reader.cpp src/cpp/tests src/cpp/CMakeLists.txt
git commit -m "feat(cpp): add RangeReader abstraction with file and buffered adapters"
```

---

## Task 5: Header parsing and `FileInfo`

**Files:**
- Create: `src/cpp/include/fcb/header.hpp`, `src/cpp/src/header.cpp`
- Create: `src/cpp/tests/test_header.cpp`
- Modify: `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `RangeReader`, `check_magic_bytes`, `compute_layout`, `FileLayout`, generated `Header`/`Column`/`AttributeIndex`.
- Produces:
  - `struct fcb::ColumnInfo { std::uint16_t index; std::string name; int type; bool nullable; };`
  - `struct fcb::AttrIndexInfo { std::uint16_t column_index; std::uint32_t length; std::uint16_t branching_factor; std::uint32_t num_unique_items; std::uint64_t begin; };`
  - `struct fcb::FileInfo { std::uint64_t features_count; std::vector<ColumnInfo> columns; std::array<double,6> geographical_extent; bool has_extent; std::array<double,3> scale, translate; bool has_transform; std::string crs; std::string cityjson_version; };`
  - `class fcb::HeaderView` — owns its buffer; `const FileInfo& info() const`, `const FileLayout& layout() const`, `const std::vector<AttrIndexInfo>& attr_indices() const`.
  - `HeaderView fcb::read_header(std::shared_ptr<RangeReader> reader);` — takes **shared** ownership so it can construct its own per-query `BufferedRangeReader(reader, 12944)` internally. `FcbReader::open()` passes the bare transport; it does not wrap.

- [ ] **Step 1: Write the failing test**

Create `src/cpp/tests/test_header.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/header.hpp>
#include <fcb/range_reader.hpp>
#include "fake_range_reader.hpp"

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("read_header parses the committed delft fixture") {
    FileRangeReader r(kFixture);
    HeaderView h = read_header(r);

    CHECK(h.info().features_count > 0);
    CHECK_FALSE(h.info().columns.empty());
    CHECK(h.layout().header_len > 12);
    CHECK(h.layout().feature_begin >= h.layout().attr_index_begin);
    CHECK(h.layout().feature_begin < r.total_size());
}

TEST_CASE("read_header rejects a file with bad magic") {
    std::vector<std::uint8_t> junk(64, 0xAB);
    testing::FakeRangeReader fake(junk);
    CHECK_THROWS_AS(read_header(fake), Error);

    try {
        testing::FakeRangeReader f2(junk);
        read_header(f2);
    } catch (const Error& e) {
        CHECK(e.code() == ErrorCode::MissingMagicBytes);
    }
}

TEST_CASE("attribute index entries carry absolute begin offsets") {
    FileRangeReader r(kFixture);
    HeaderView h = read_header(r);

    std::uint64_t expected = h.layout().attr_index_begin;
    for (const auto& ai : h.attr_indices()) {
        CHECK(ai.begin == expected);
        expected += ai.length;
    }
    CHECK(expected == h.layout().feature_begin);
}
```

In `src/cpp/tests/CMakeLists.txt`, define the fixture path:

```cmake
target_compile_definitions(fcb_tests PRIVATE
    DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN
    FCB_TEST_DATA_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../../../examples/data"
)
```

- [ ] **Step 2: Run and verify it fails**

Expected: `fatal error: 'fcb/header.hpp' file not found`.

- [ ] **Step 3: Write the implementation**

`src/cpp/src/header.cpp` performs exactly this sequence — mirroring `reader/mod.rs:97-110` and `http_reader/mod.rs:99-171`:

1. `reader.read(0, 8)`; if `!check_magic_bytes(...)` throw `Error{MissingMagicBytes}`.
2. `reader.read(8, 4)`; decode LE `uint32_t header_size` (use explicit shifts, not `memcpy` of a `uint32_t`, so the code is endian-correct by construction).
3. Read `kHeaderSizeSize + header_size` bytes starting at offset `8` — the buffer handed to FlatBuffers **must include the 4-byte prefix**.
4. `flatbuffers::Verifier v(buf.data(), buf.size()); if (!VerifySizePrefixedHeaderBuffer(v)) throw Error{InvalidFlatbuffer}`.
5. `const ::Header* hdr = GetSizePrefixedHeader(buf.data());` — kept in an internal detail header, never exposed publicly.
6. Sum `AttributeIndex.length()` over `hdr->attribute_index()` (null-safe → 0), sorting entries by `index()` first, and record a running `begin` for each.
7. `compute_layout(header_size, hdr->features_count(), hdr->index_node_size(), attr_index_size)`, then **immediately** `validate_layout_against_size(layout, reader.total_size())`. Never issue an index or feature read before this call returns.
   While summing in step 6, also reject **duplicate `AttributeIndex.index()` values** (two indexes claiming the same column makes the cumulative-offset walk ambiguous) and sum the lengths with `detail::checked_add`.
8. Populate `FileInfo` from `hdr` (columns, transform, geographical extent, reference system → `"EPSG:<code>"`, CityJSON version).
9. Store `buf` in the `HeaderView` by value so `raw()` stays valid for the object's lifetime. `HeaderView` must be movable but **not** copyable-by-pointer — hold `std::shared_ptr<const std::vector<std::uint8_t>>`.

Exact field names on `Header` come from `src/fbs/header.fbs`; read that file and use them verbatim rather than guessing.

- [ ] **Step 4: Run and verify it passes**

```bash
cd src/cpp && cmake --build build && ctest --test-dir build --output-on-failure
```

Expected: all three cases pass. The third case is the real prize — it proves the section arithmetic against a real file end-to-end.

- [ ] **Step 5: Cross-check `features_count` against Rust**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust
cargo run -p fcb_cli -- info ../../examples/data/delft.fcb
```

(Recipe name may differ; check `justfile` for the `fcb_info` recipe.) The `features_count` it prints must equal what the C++ test reads. If they differ, stop and fix before proceeding — everything downstream depends on it.

- [ ] **Step 6: Commit (milestone)**

```bash
git add src/cpp/include/fcb/header.hpp src/cpp/src/header.cpp src/cpp/tests src/cpp/CMakeLists.txt
git commit -m "feat(cpp): parse FCB header and derive section layout"
```

---

## Task 6: Sequential feature scan

**Files:**
- Create: `src/cpp/include/fcb/feature.hpp`, `src/cpp/src/feature.cpp`
- Create: `src/cpp/include/fcb/reader.hpp`, `src/cpp/src/reader.cpp`
- Create: `src/cpp/tests/test_sequential_scan.cpp`
- Modify: `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `HeaderView`, `RangeReader`, `FileLayout`.
- Produces:
  - `class fcb::Feature` — owns `std::shared_ptr<const std::vector<std::uint8_t>>`; `std::string id() const`, `std::uint64_t byte_offset() const`, `std::size_t city_object_count() const`. The `const ::CityFeature* raw()` accessor is **private**, declared in `src/cpp/src/detail/feature_access.hpp` and `friend`ed to the decoders — never in a public header.
  - `class fcb::FeatureIterator` — `bool next()`, `const Feature& current() const`, `std::uint64_t features_count() const`. Single-pass, non-copyable.
  - `class fcb::FcbReader` — `static FcbReader open_file(const std::string& path)`, `static FcbReader open(std::shared_ptr<RangeReader>)`, `const HeaderView& header() const`, `FeatureIterator select_all()`.

- [ ] **Step 1: Write the failing test**

Create `src/cpp/tests/test_sequential_scan.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/reader.hpp>

#include <set>
#include <string>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("select_all visits exactly features_count features") {
    FcbReader r = FcbReader::open_file(kFixture);
    const std::uint64_t expected = r.header().info().features_count;
    REQUIRE(expected > 0);

    FeatureIterator it = r.select_all();
    CHECK(it.features_count() == expected);

    std::uint64_t seen = 0;
    while (it.next()) {
        const Feature& f = it.current();
        CHECK_FALSE(f.id().empty());
        ++seen;
    }
    CHECK(seen == expected);
}

TEST_CASE("feature ids are non-empty and unique") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();

    std::set<std::string> ids;
    while (it.next()) {
        std::string id = it.current().id();
        CHECK_FALSE(id.empty());
        ids.insert(id);
    }
    CHECK(ids.size() == r.header().info().features_count);
}

TEST_CASE("a Feature outlives the iterator that produced it") {
    Feature kept;
    {
        FcbReader r = FcbReader::open_file(kFixture);
        FeatureIterator it = r.select_all();
        REQUIRE(it.next());
        kept = it.current();  // copies the shared_ptr, not the bytes
    }
    // Iterator and reader are destroyed; the backing buffer must still be
    // valid. Exercised through PUBLIC value accessors only -- raw() is private.
    CHECK_FALSE(kept.id().empty());
    CHECK(kept.city_object_count() > 0);
}
```

The third case is the lifetime guarantee that the global constraints demand. It must pass under ASan.

- [ ] **Step 2: Run and verify it fails**

Expected: `fatal error: 'fcb/reader.hpp' file not found`.

- [ ] **Step 3: Write the implementation**

`FeatureIterator::next()` reads from `layout.feature_begin`, advancing a cursor:

1. `auto prefix = reader_->read(cursor_, 4)`; if `prefix.size() < 4` → end of iteration, return `false`.
2. `uint32_t len` = LE decode of `prefix` (explicit shifts). **Reject `len > kMaxFeatureSize` before allocating** — a crafted prefix of `0xFFFFFFFF` would otherwise request ~4 GiB. Also reject `cursor_ + 4 + len > reader_->total_size()`.
3. `auto buf = reader_->read(cursor_, 4 + len)` — one read including the prefix, so the buffer handed to FlatBuffers has the prefix.
4. `flatbuffers::Verifier v(buf.data(), buf.size()); if (!VerifySizePrefixedCityFeatureBuffer(v)) throw Error{InvalidFlatbuffer};`
5. Wrap `buf` in `std::make_shared<const std::vector<std::uint8_t>>(std::move(buf))`, construct the `Feature`, record `byte_offset = cursor_ - layout.feature_begin`.
6. `cursor_ += 4 + len`.

Iteration ends **only** after exactly `features_count` features have been produced. Reaching EOF earlier is a truncated file, not a normal end: throw `Error{IoError}`. Specifically, a prefix read returning fewer than 4 bytes, or a feature body shorter than its prefix claims, is an error whenever fewer than `features_count` features have been seen. (Task 12's Class C fixtures assert exactly this; treating early EOF as success would silently accept truncation.) Conversely, if `features_count` features have been read but bytes remain, that is also a malformed file — report it rather than reading on.

`Feature` must be default-constructible (for the lifetime test), copyable (shared_ptr copy), and `raw()` returns `nullptr` when default-constructed.

Wrap the reader in a `BufferedRangeReader` with `min_req_size = 1048576` (`DEFAULT_HTTP_FETCH_SIZE`) inside `select_all()`, so sequential scanning does one big read per megabyte rather than two reads per feature. This is the same value the Rust HTTP path uses for `SelectAll` (`http_reader/mod.rs:560`).

- [ ] **Step 4: Run and verify it passes**

```bash
cd src/cpp && cmake --build build && ctest --test-dir build --output-on-failure
```

- [ ] **Step 5: Run under sanitizers**

```bash
cd src/cpp
cmake -B build-asan -S . -DCMAKE_BUILD_TYPE=Debug \
  -DCMAKE_CXX_FLAGS="-fsanitize=address,undefined -fno-omit-frame-pointer" \
  -DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake \
  -DVCPKG_MANIFEST_FEATURES=tests
cmake --build build-asan && ctest --test-dir build-asan --output-on-failure
```

Expected: clean. UBSan matters specifically here — FlatBuffers accessors on an unaligned buffer are UB, and the format has **no inter-feature padding**, so features land at arbitrary alignments. If UBSan reports misaligned access, the fix is to ensure each feature's bytes are copied into a fresh `std::vector` (which is suitably aligned) rather than viewed in place — which the implementation above already does.

- [ ] **Step 6: Commit (milestone)**

```bash
git add src/cpp/include/fcb src/cpp/src src/cpp/tests src/cpp/CMakeLists.txt
git commit -m "feat(cpp): implement sequential feature iteration"
```

---

## Task 7: CityJSON emission

**Split this into three commits, not one.** As originally written this task bundled attribute decoding, every geometry nesting depth, semantics, appearances, extensions, templates, metadata *and* JSON emission — far too much to review or bisect as a single change, and its shape-only unit tests would not have caught a wrong nesting level. Each sub-task below lands its own golden comparison against the Rust reader immediately, rather than deferring all real verification to Task 12.

- **Task 7a — Attributes.** `src/cpp/src/attribute_decoder.cpp` plus `decode_attributes()`. Test against `inferable_types` for every writer-producible `ColumnType`, and against a feature with no attributes at all. Commit: `feat(cpp): decode feature attributes`.
- **Task 7b — Geometry and semantics.** `src/cpp/src/geom_decoder.cpp`: boundaries for every geometry type (`MultiPoint`, `MultiLineString`, `MultiSurface`, `Solid`, `CompositeSolid`), semantic surfaces, and geometry templates/instances. Test each nesting depth explicitly, plus `geom_temp` for templates. Commit: `feat(cpp): decode geometry boundaries and semantics`.
- **Task 7c — CityJSON emission.** `src/cpp/src/cityjson.cpp`: metadata envelope, feature assembly, appearances, extensions. This is where the full parsed-tree diff against the Rust reader lands. Commit: `feat(cpp): emit CityJSON from header and features`.

Steps 1–6 below apply to 7c; 7a and 7b follow the same red-green-commit shape against their own narrower tests. Do not proceed to 7b until 7a's golden comparison passes.

**Files:**
- Create: `src/cpp/include/fcb/cityjson.hpp`, `src/cpp/src/cityjson.cpp`
- Create: `src/cpp/src/attribute_decoder.cpp`, `src/cpp/src/geom_decoder.cpp`
- Create: `src/cpp/tests/test_cityjson.cpp`
- Modify: `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `Feature`, `HeaderView`, `ColumnInfo`.
- Produces (all guarded by `#ifdef FCB_WITH_JSON`):
  - `nlohmann::json fcb::to_cityjson_metadata(const HeaderView&);`
  - `nlohmann::json fcb::to_cityjson_feature(const Feature&, const HeaderView&);`
  - `nlohmann::json fcb::decode_attributes(fcb::bytes_view blob, const std::vector<ColumnInfo>& schema);`

- [ ] **Step 1: Write the failing test**

Create `src/cpp/tests/test_cityjson.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/cityjson.hpp>
#include <fcb/reader.hpp>

#include <nlohmann/json.hpp>

using namespace fcb;
using nlohmann::json;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("metadata emits a valid CityJSON envelope") {
    FcbReader r = FcbReader::open_file(kFixture);
    json cj = to_cityjson_metadata(r.header());

    CHECK(cj["type"] == "CityJSON");
    CHECK(cj.contains("version"));
    REQUIRE(cj.contains("transform"));
    CHECK(cj["transform"]["scale"].size() == 3);
    CHECK(cj["transform"]["translate"].size() == 3);
}

TEST_CASE("a feature emits a valid CityJSONFeature") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();
    REQUIRE(it.next());

    json f = to_cityjson_feature(it.current(), r.header());

    CHECK(f["type"] == "CityJSONFeature");
    CHECK(f.contains("id"));
    REQUIRE(f.contains("CityObjects"));
    CHECK(f["CityObjects"].is_object());
    CHECK_FALSE(f["CityObjects"].empty());
    REQUIRE(f.contains("vertices"));
    CHECK(f["vertices"].is_array());

    // Every vertex is a 3-element integer array (CityJSON stores quantized ints).
    for (const auto& v : f["vertices"]) {
        REQUIRE(v.is_array());
        CHECK(v.size() == 3);
        CHECK(v[0].is_number_integer());
    }
}

TEST_CASE("geometry boundaries respect the dimensional hierarchy") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();

    bool checked_a_solid = false;
    while (it.next() && !checked_a_solid) {
        json f = to_cityjson_feature(it.current(), r.header());
        for (const auto& co : f["CityObjects"]) {
            if (!co.contains("geometry")) continue;
            for (const auto& g : co["geometry"]) {
                if (g["type"] == "Solid") {
                    // Solid -> shells -> surfaces -> rings -> vertex indices
                    REQUIRE(g["boundaries"].is_array());
                    REQUIRE_FALSE(g["boundaries"].empty());
                    const auto& shell = g["boundaries"][0];
                    REQUIRE(shell.is_array());
                    REQUIRE_FALSE(shell.empty());
                    const auto& surface = shell[0];
                    REQUIRE(surface.is_array());
                    REQUIRE_FALSE(surface.empty());
                    const auto& ring = surface[0];
                    REQUIRE(ring.is_array());
                    CHECK(ring[0].is_number_integer());
                    checked_a_solid = true;
                    break;
                }
            }
            if (checked_a_solid) break;
        }
    }
    CHECK(checked_a_solid);
}
```

- [ ] **Step 2: Run and verify it fails**

Expected: `fatal error: 'fcb/cityjson.hpp' file not found`.

- [ ] **Step 3: Write the implementation**

Port `src/rust/fcb_core/src/reader/deserializer.rs` (`to_cj_metadata` at `:22`, `to_cj_feature` at `:380`) and `reader/geom_decoder.rs`. Three sub-parts:

**Boundaries decode** — reconstruct the nested arrays from the flat `boundaries` / `strings` / `surfaces` / `shells` / `solids` count arrays, per `specification.md` §boundaries-encoding. The dimensional hierarchy is: `solids[i]` = number of shells in solid `i`; `shells[i]` = number of surfaces in shell `i`; `surfaces[i]` = number of rings in surface `i`; `strings[i]` = number of vertex indices in ring `i`. Consume the count arrays with running cursors, innermost first. Emit `MultiPoint`/`MultiLineString`/`MultiSurface`/`Solid`/`CompositeSolid` nesting depth according to the geometry `type` field.

**Attribute decode** — walk the feature's attribute blob. Each value is prefixed by its `Column.index` (u16 LE), then the value encoded per the column's `ColumnType`: numeric types in native LE binary; `String`/`Json` as `u32 LE length` + UTF-8 bytes; `Bool` as one byte; `Binary` as `u32 LE length` + bytes. Read `src/rust/fcb_core/src/reader/attribute.rs` for the exact prefix width and ordering and mirror it — do not infer from the spec.

**Metadata** — map `Header` → CityJSON top level: `type`, `version`, `transform{scale,translate}`, `metadata{geographicalExtent, referenceSystem}`, `extensions`, `appearance`, `geometry-templates`.

- [ ] **Step 4: Run and verify it passes**

- [ ] **Step 5: Compare against the Rust reader's output for the same file**

This is the first real conformance check. Generate the Rust side:

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust
cargo run -p fcb_cli -- ser -i ../../examples/data/delft.fcb -o /tmp/rust_delft.city.jsonl
```

(Check `justfile`/`cli` for the exact subcommand and flags.) Then write a throwaway C++ target that emits the same, and diff **as parsed JSON**, never as text:

```bash
python3 - <<'PY'
import json
rust = [json.loads(l) for l in open('/tmp/rust_delft.city.jsonl')]
cpp  = [json.loads(l) for l in open('/tmp/cpp_delft.city.jsonl')]
assert len(rust) == len(cpp), (len(rust), len(cpp))
diffs = [i for i,(a,b) in enumerate(zip(rust,cpp)) if a != b]
print(f"{len(diffs)} differing records out of {len(rust)}")
if diffs: print(json.dumps(rust[diffs[0]], indent=2)[:2000])
PY
```

Expected: `0 differing records`. Anything else is a bug in the C++ decoder — investigate before moving on. Note the first line of the Rust output is the CityJSON metadata envelope, the rest are features; account for that offset.

- [ ] **Step 6: Commit (milestone)**

```bash
git add src/cpp/include/fcb/cityjson.hpp src/cpp/src src/cpp/tests src/cpp/CMakeLists.txt
git commit -m "feat(cpp): emit CityJSON from header and features"
```

---

## Task 8: Packed R-tree spatial query

**No Hilbert curve implementation.** The Hilbert function exists only to *order* features at write time. A reader compares the query bbox against stored node bboxes and follows offsets; it never computes a Hilbert value. Porting it would add a verbatim-transcription risk and a genuine UB trap (Rust's `as u32` saturates and maps NaN to 0; the C++ cast is undefined) for zero reader benefit. If a writer is added later, that plan adds `hilbert.cpp` and a `saturating_cast_u32` helper then.

**Files:**
- Create: `src/cpp/include/fcb/packed_rtree.hpp`, `src/cpp/src/packed_rtree.cpp`
- Create: `src/cpp/include/fcb/query.hpp`
- Create: `src/cpp/tests/test_packed_rtree.cpp`
- Modify: `src/cpp/include/fcb/reader.hpp`, `src/cpp/src/reader.cpp`, `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `RangeReader`, `FileLayout`, `HeaderView`.
- Produces:
  - `struct fcb::NodeItem { double min_x, min_y, max_x, max_y; std::uint64_t offset; };` with `static NodeItem decode(bytes_view);`, `bool intersects(const BBox&) const;` and `static constexpr std::size_t kSize = 40;`
  - `struct fcb::BBox { double min_x, min_y, max_x, max_y; };`
  - `struct fcb::SearchResultItem { std::uint64_t offset; std::uint64_t index; };`
  - `std::vector<SearchResultItem> fcb::rtree_stream_search(RangeReader&, std::uint64_t index_begin, std::uint64_t num_items, std::uint16_t node_size, const BBox& q);`
  - `FeatureIterator fcb::FcbReader::select_bbox(const BBox&);`

- [ ] **Step 1: Write the failing NodeItem/intersects test**

Create `src/cpp/tests/test_packed_rtree.cpp` starting with the decode and predicate, which need no file:

```cpp
#include <doctest/doctest.h>
#include <fcb/packed_rtree.hpp>
#include <fcb/reader.hpp>

#include <cstring>
#include <set>
#include <string>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("NodeItem decodes 40 little-endian bytes") {
    std::vector<std::uint8_t> raw(40, 0);
    const std::uint8_t one[8] = {0,0,0,0,0,0,0xF0,0x3F};  // 1.0
    std::memcpy(raw.data() + 0,  one, 8);
    std::memcpy(raw.data() + 8,  one, 8);
    std::memcpy(raw.data() + 16, one, 8);
    std::memcpy(raw.data() + 24, one, 8);
    raw[32] = 0x2A;  // offset = 42

    NodeItem n = NodeItem::decode(bytes_view(raw));
    CHECK(n.min_x == doctest::Approx(1.0));
    CHECK(n.max_y == doctest::Approx(1.0));
    CHECK(n.offset == 42u);
    CHECK(NodeItem::kSize == 40);
}

TEST_CASE("intersects matches Rust NodeItem::intersects boundary semantics") {
    // Port the comparison operators EXACTLY from packed_rtree/mod.rs -- an
    // inclusive/exclusive slip here silently changes every query result.
    NodeItem n{0.0, 0.0, 10.0, 10.0, 0};

    CHECK(n.intersects(BBox{5.0, 5.0, 6.0, 6.0}));      // fully inside
    CHECK(n.intersects(BBox{-5.0, -5.0, 5.0, 5.0}));    // overlapping
    CHECK(n.intersects(BBox{-5.0, -5.0, 20.0, 20.0}));  // enclosing

    // Edge contact: Rust uses >/< on the opposing bounds, so touching
    // edges DO intersect. Verify against the Rust source before trusting.
    CHECK(n.intersects(BBox{10.0, 10.0, 20.0, 20.0}));  // corner touch
    CHECK(n.intersects(BBox{-5.0, -5.0, 0.0, 0.0}));    // corner touch

    CHECK_FALSE(n.intersects(BBox{10.1, 0.0, 20.0, 10.0}));  // just past max_x
    CHECK_FALSE(n.intersects(BBox{-20.0, 0.0, -0.1, 10.0})); // just before min_x
    CHECK_FALSE(n.intersects(BBox{0.0, 10.1, 10.0, 20.0}));  // just past max_y
}
```

Before finalising the edge-contact expectations, open `NodeItem::intersects` in `src/rust/fcb_core/src/packed_rtree/mod.rs` and transcribe its exact comparison operators. Do not guess whether it is `>` or `>=`.

- [ ] **Step 2: Run and verify it fails**

Expected: `fatal error: 'fcb/packed_rtree.hpp' file not found`.

- [ ] **Step 3: Implement NodeItem decode and intersects**

Create `src/cpp/include/fcb/packed_rtree.hpp` and `src/cpp/src/packed_rtree.cpp` with `BBox`, `NodeItem::decode` (explicit little-endian byte assembly, not `memcpy` of a `double`, so the code is endian-correct by construction) and `NodeItem::intersects` transcribed from Rust.

- [ ] **Step 4: Run and verify the decode tests pass**

- [ ] **Step 5: Write the failing R-tree test**

Create `src/cpp/tests/test_packed_rtree.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/packed_rtree.hpp>
#include <fcb/reader.hpp>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("NodeItem decodes 40 little-endian bytes") {
    std::vector<std::uint8_t> raw(40, 0);
    // min_x = 1.0 -> IEEE754 LE
    const std::uint8_t one[8] = {0,0,0,0,0,0,0xF0,0x3F};
    std::memcpy(raw.data() + 0,  one, 8);
    std::memcpy(raw.data() + 8,  one, 8);
    std::memcpy(raw.data() + 16, one, 8);
    std::memcpy(raw.data() + 24, one, 8);
    raw[32] = 0x2A;  // offset = 42

    NodeItem n = NodeItem::decode(bytes_view(raw));
    CHECK(n.min_x == doctest::Approx(1.0));
    CHECK(n.max_y == doctest::Approx(1.0));
    CHECK(n.offset == 42u);
    CHECK(NodeItem::kSize == 40);
}

TEST_CASE("a bbox covering the whole extent returns every feature") {
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& info = r.header().info();
    REQUIRE(info.has_extent);

    BBox all{info.geographical_extent[0], info.geographical_extent[1],
             info.geographical_extent[3], info.geographical_extent[4]};

    FeatureIterator it = r.select_bbox(all);
    std::uint64_t seen = 0;
    while (it.next()) ++seen;
    CHECK(seen == info.features_count);
}

TEST_CASE("a degenerate bbox outside the extent returns nothing") {
    FcbReader r = FcbReader::open_file(kFixture);
    BBox none{-1e9, -1e9, -1e9 + 1.0, -1e9 + 1.0};

    FeatureIterator it = r.select_bbox(none);
    std::uint64_t seen = 0;
    while (it.next()) ++seen;
    CHECK(seen == 0);
}

TEST_CASE("a quarter bbox returns a strict non-empty subset") {
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& info = r.header().info();
    const double mid_x = (info.geographical_extent[0] + info.geographical_extent[3]) / 2.0;
    const double mid_y = (info.geographical_extent[1] + info.geographical_extent[4]) / 2.0;

    BBox quarter{info.geographical_extent[0], info.geographical_extent[1], mid_x, mid_y};
    FeatureIterator it = r.select_bbox(quarter);
    std::uint64_t seen = 0;
    while (it.next()) ++seen;

    CHECK(seen > 0);
    CHECK(seen < info.features_count);
}

TEST_CASE("the last feature in the file is reachable by bbox query") {
    // Guards the RangeFrom / total_size() edge case: the last leaf has no
    // successor offset, so its length comes from its own 4-byte prefix.
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& info = r.header().info();
    BBox all{info.geographical_extent[0], info.geographical_extent[1],
             info.geographical_extent[3], info.geographical_extent[4]};

    FeatureIterator it = r.select_bbox(all);
    std::string last_id;
    while (it.next()) last_id = it.current().id();
    CHECK_FALSE(last_id.empty());
}
```

- [ ] **Step 6: Run and verify it fails**

- [ ] **Step 7: Implement the R-tree search**

Port `packed_rtree/mod.rs:690-760` (`stream_search`) and `:920-1040` (the HTTP streaming variant, whose range-coalescing you want for both transports). Key points, each already cited in the Format Reference:

- Level bounds from `generate_level_bounds` (`:342-375`); `level_bounds[0]` is the leaf level and last in storage.
- Leaf test: `node_index >= num_nodes - num_items`.
- Internal `offset` is a **child node index**; leaf `offset` is a **byte offset relative to `feature_begin`**.
- When descending into level 0, extend the node range by **one extra node**, clamped to `level_bounds[0].end`, so `next.offset` is available for the length calculation.
- Feature ranges: `[feature_begin + n.offset, feature_begin + next.offset)`; for the last item, `[feature_begin + n.offset, total_size())`.
- Coalesce node ranges when `wasted_bytes = (children.start - tail.end) * 40 <= 256*1024`.
- Results must be returned sorted by `offset` so the feature iterator reads forward.

`select_bbox` then drives a `FeatureIterator` over the resulting offset list rather than a linear cursor. Refactor `FeatureIterator` to take a `std::vector<SearchResultItem>` (empty meaning "sequential scan") rather than duplicating the class.

- [ ] **Step 8: Run and verify it passes**

- [ ] **Step 9: Cross-check the result set against Rust**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust
cargo run -p fcb_cli -- ser -i ../../examples/data/delft.fcb --bbox <minx> <miny> <maxx> <maxy> -o /tmp/rust_bbox.jsonl
```

Compare the **set of feature ids** returned by Rust and C++ for the same bbox. They must be identical. Any difference is an inclusive/exclusive boundary bug in the intersection predicate — check `NodeItem::intersects` in the Rust source and mirror the comparison operators exactly (`>=` vs `>`).

- [ ] **Step 10: Commit (milestone)**

```bash
git add src/cpp/include/fcb src/cpp/src src/cpp/tests src/cpp/CMakeLists.txt
git commit -m "feat(cpp): implement packed R-tree bbox query"
```

---

## Task 9: Attribute key encoding and comparison

The B+tree is split across two tasks because the key layer is independently testable and is where the subtle correctness traps live.

**Files:**
- Create: `src/cpp/include/fcb/key.hpp`, `src/cpp/src/key.cpp`
- Create: `src/cpp/tests/test_keys.cpp`
- Modify: `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `ColumnInfo`, generated `ColumnType`.
- Produces:
  - `enum class fcb::KeyKind { Int8, UInt8, Int16, UInt16, Int32, UInt32, Int64, UInt64, Float32, Float64, Bool, DateTime, String20, String50, String100 };`
  - `struct fcb::KeyValue` — tagged union holding the decoded value.
  - `std::size_t fcb::key_serialized_size(KeyKind);`
  - `KeyValue fcb::decode_key(KeyKind, bytes_view);`
  - `std::vector<std::uint8_t> fcb::encode_key(const KeyValue&);`
  - `int fcb::compare_keys(const KeyValue& a, const KeyValue& b);` — `<0`, `0`, `>0`
  - `KeyKind fcb::key_kind_for_column(::ColumnType);`
  - `KeyValue fcb::key_min(KeyKind); KeyValue fcb::key_max(KeyKind);`

- [ ] **Step 1: Write the failing test**

Create `src/cpp/tests/test_keys.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/key.hpp>

#include <cmath>
#include <limits>

using namespace fcb;

TEST_CASE("serialized sizes match the Rust key encoders") {
    CHECK(key_serialized_size(KeyKind::Int8)      == 1);
    CHECK(key_serialized_size(KeyKind::UInt8)     == 1);
    CHECK(key_serialized_size(KeyKind::Int16)     == 2);
    CHECK(key_serialized_size(KeyKind::UInt16)    == 2);
    CHECK(key_serialized_size(KeyKind::Int32)     == 4);
    CHECK(key_serialized_size(KeyKind::UInt32)    == 4);
    CHECK(key_serialized_size(KeyKind::Int64)     == 8);
    CHECK(key_serialized_size(KeyKind::UInt64)    == 8);
    CHECK(key_serialized_size(KeyKind::Float32)   == 4);
    CHECK(key_serialized_size(KeyKind::Float64)   == 8);
    CHECK(key_serialized_size(KeyKind::Bool)      == 1);
    CHECK(key_serialized_size(KeyKind::DateTime)  == 12);  // i64 secs + u32 nanos
    CHECK(key_serialized_size(KeyKind::String20)  == 20);
    CHECK(key_serialized_size(KeyKind::String50)  == 50);
    CHECK(key_serialized_size(KeyKind::String100) == 100);
}

TEST_CASE("integers round-trip as little-endian two's complement") {
    KeyValue v = KeyValue::from_i32(-2);
    auto bytes = encode_key(v);
    REQUIRE(bytes.size() == 4);
    CHECK(bytes[0] == 0xFE);
    CHECK(bytes[1] == 0xFF);
    CHECK(bytes[2] == 0xFF);
    CHECK(bytes[3] == 0xFF);
    CHECK(compare_keys(decode_key(KeyKind::Int32, bytes_view(bytes)), v) == 0);
}

TEST_CASE("floats are stored as raw IEEE-754 LE bits with NO order transform") {
    KeyValue v = KeyValue::from_f64(1.0);
    auto bytes = encode_key(v);
    REQUIRE(bytes.size() == 8);
    // 1.0 == 0x3FF0000000000000, little-endian on disk.
    CHECK(bytes[7] == 0x3F);
    CHECK(bytes[6] == 0xF0);
    CHECK(bytes[0] == 0x00);
}

TEST_CASE("float comparison uses ordered_float total order") {
    const double nan = std::numeric_limits<double>::quiet_NaN();
    const double inf = std::numeric_limits<double>::infinity();

    // NaN sorts greatest and equals itself.
    CHECK(compare_keys(KeyValue::from_f64(nan), KeyValue::from_f64(nan)) == 0);
    CHECK(compare_keys(KeyValue::from_f64(nan), KeyValue::from_f64(inf)) > 0);
    CHECK(compare_keys(KeyValue::from_f64(inf), KeyValue::from_f64(nan)) < 0);

    // -0.0 == +0.0
    CHECK(compare_keys(KeyValue::from_f64(-0.0), KeyValue::from_f64(0.0)) == 0);

    CHECK(compare_keys(KeyValue::from_f64(-1.0), KeyValue::from_f64(1.0)) < 0);
    CHECK(compare_keys(KeyValue::from_f64(-inf), KeyValue::from_f64(-1.0)) < 0);
}

TEST_CASE("fixed strings zero-pad, truncate silently, and compare bytewise") {
    KeyValue short_s = KeyValue::from_string(KeyKind::String50, "abc");
    auto bytes = encode_key(short_s);
    REQUIRE(bytes.size() == 50);
    CHECK(bytes[0] == 'a');
    CHECK(bytes[3] == 0x00);
    CHECK(bytes[49] == 0x00);

    // Truncation is at the BYTE level and can split a UTF-8 sequence.
    std::string long_s(60, 'x');
    KeyValue truncated = KeyValue::from_string(KeyKind::String50, long_s);
    auto tb = encode_key(truncated);
    REQUIRE(tb.size() == 50);
    CHECK(tb[49] == 'x');

    // Two distinct strings sharing a 50-byte prefix COLLIDE. This is by design;
    // callers must post-verify long-string matches against the real attribute.
    std::string a = std::string(50, 'y') + "AAA";
    std::string b = std::string(50, 'y') + "BBB";
    CHECK(compare_keys(KeyValue::from_string(KeyKind::String50, a),
                       KeyValue::from_string(KeyKind::String50, b)) == 0);
}

TEST_CASE("string sentinels are all-0xFF and all-0x00") {
    auto mx = encode_key(key_max(KeyKind::String50));
    auto mn = encode_key(key_min(KeyKind::String50));
    CHECK(mx[0] == 0xFF);
    CHECK(mx[49] == 0xFF);
    CHECK(mn[0] == 0x00);
    CHECK(mn[49] == 0x00);
}

TEST_CASE("DateTime is i64 seconds followed by u32 nanos, both LE") {
    KeyValue v = KeyValue::from_datetime(/*secs=*/1, /*nanos=*/2);
    auto bytes = encode_key(v);
    REQUIRE(bytes.size() == 12);
    CHECK(bytes[0] == 1);
    CHECK(bytes[8] == 2);
    CHECK(compare_keys(decode_key(KeyKind::DateTime, bytes_view(bytes)), v) == 0);
}

TEST_CASE("column type maps to the key kind the writer produces") {
    using CT = ::ColumnType;
    CHECK(key_kind_for_column(CT::String) == KeyKind::String50);
    CHECK(key_kind_for_column(CT::Json)   == KeyKind::String100);
    CHECK(key_kind_for_column(CT::Binary) == KeyKind::String100);
    CHECK(key_kind_for_column(CT::Bool)   == KeyKind::Bool);
    CHECK(key_kind_for_column(CT::Double) == KeyKind::Float64);
    // StringKey20 exists in the format but the writer never emits it.
}
```

- [ ] **Step 2: Run and verify it fails**

- [ ] **Step 3: Write the implementation**

The two things that must not be got wrong:

1. **No total-order bit transform for floats.** Encode with `memcpy` of the IEEE-754 bits (byteswapped on big-endian hosts), and implement `compare_keys` for floats as an explicit `ordered_float` port:

```cpp
static int cmp_ordered_double(double a, double b) {
    const bool na = std::isnan(a), nb = std::isnan(b);
    if (na && nb) return 0;   // NaN == NaN
    if (na) return 1;         // NaN sorts greatest
    if (nb) return -1;
    if (a < b) return -1;
    if (a > b) return 1;
    return 0;                 // covers -0.0 == +0.0
}
```

2. **Fixed strings truncate silently at the byte level.** No error, no UTF-8 boundary respect, zero padding.

- [ ] **Step 4: Run and verify it passes**

- [ ] **Step 5: Commit (milestone)**

```bash
git add src/cpp/include/fcb/key.hpp src/cpp/src/key.cpp src/cpp/tests src/cpp/CMakeLists.txt
git commit -m "feat(cpp): implement attribute key encoding and ordered comparison"
```

---

## Task 10: Static B+tree attribute query

**Files:**
- Create: `src/cpp/include/fcb/stree.hpp`, `src/cpp/src/stree.cpp`, `src/cpp/src/payload.cpp`
- Create: `src/cpp/tests/test_stree.cpp`
- Modify: `src/cpp/include/fcb/query.hpp`, `src/cpp/include/fcb/reader.hpp`, `src/cpp/src/reader.cpp`, `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `KeyValue`, `KeyKind`, `compare_keys`, `AttrIndexInfo`, `RangeReader`, `SearchResultItem`.
- Produces:
  - `enum class fcb::Operator { Eq, Ne, Gt, Ge, Lt, Le };`
  - `struct fcb::AttrCondition { std::string field; Operator op; KeyValue value; };`
  - `using fcb::AttrQuery = std::vector<AttrCondition>;`
  - `std::uint64_t fcb::stree_num_nodes(std::uint64_t num_items, std::uint16_t branching_factor);`
  - `std::vector<SearchResultItem> fcb::stree_find_exact(...)`, `..._find_range(...)`
  - `std::vector<SearchResultItem> fcb::stree_query(RangeReader&, const AttrIndexInfo&, KeyKind, Operator, const KeyValue&);`
  - `FeatureIterator fcb::FcbReader::select_attr(const AttrQuery&);`

- [ ] **Step 1: Write the failing test**

Create `src/cpp/tests/test_stree.cpp`. Start with the pure arithmetic, which needs no file:

```cpp
#include <doctest/doctest.h>
#include <fcb/stree.hpp>
#include <fcb/reader.hpp>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("stree node count uses branching_factor and breaks at n < bf") {
    // NOTE: unlike the R-tree (which breaks at n == 1), the B+tree loop
    // breaks when n < branching_factor. This asymmetry is intentional --
    // see stree.rs:462-497. Do not "fix" it.
    // n=100, bf=16: 100 -> 7 (107) -> break since 7 < 16.
    CHECK(stree_num_nodes(100, 16) == 107);
    // n=16, bf=16: 16 -> 1 (17) -> break since 1 < 16.
    CHECK(stree_num_nodes(16, 16) == 17);
    // n=10, bf=16: 10 -> 1 (11) -> break.
    CHECK(stree_num_nodes(10, 16) == 11);
    // n=1000, bf=16: 1000 -> 63 (1063) -> 4 (1067) -> break since 4 < 16.
    CHECK(stree_num_nodes(1000, 16) == 1067);
}

TEST_CASE("payload tag is the MSB and the mask is the low 63 bits") {
    CHECK(kPayloadTag  == 0x8000000000000000ull);
    CHECK(kPayloadMask == 0x7FFFFFFFFFFFFFFFull);
    CHECK(is_payload_ref(kPayloadTag | 1234ull));
    CHECK_FALSE(is_payload_ref(1234ull));
    CHECK(payload_offset(kPayloadTag | 1234ull) == 1234ull);
}

TEST_CASE("payload entries decode as u32 count then count x u64, all LE") {
    std::vector<std::uint8_t> raw = {
        0x02, 0x00, 0x00, 0x00,                          // count = 2
        0x0A, 0,0,0,0,0,0,0,                             // offset 10
        0x14, 0,0,0,0,0,0,0,                             // offset 20
    };
    auto offsets = decode_payload_entry(bytes_view(raw));
    REQUIRE(offsets.size() == 2);
    CHECK(offsets[0] == 10u);
    CHECK(offsets[1] == 20u);
}
```

Then the end-to-end cases. **Before writing them**, discover which columns the fixture actually indexes:

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust
cargo run -p fcb_cli -- info ../../examples/data/delft.fcb
```

Use a real indexed column name and a real value from that output. Then add:

```cpp
TEST_CASE("Eq query returns the same feature id set as the Rust reader") {
    FcbReader r = FcbReader::open_file(kFixture);
    // REPLACE with a real indexed column and value from `fcb info`.
    AttrQuery q = {{"<indexed_column>", Operator::Eq, KeyValue::from_string(KeyKind::String50, "<value>")}};

    FeatureIterator it = r.select_attr(q);
    std::set<std::string> ids;
    while (it.next()) ids.insert(it.current().id());

    CHECK_FALSE(ids.empty());
    // The exact expected set is asserted in the conformance suite (Task 12).
}

TEST_CASE("Ge and Gt differ by exactly the equal-keyed features") {
    FcbReader r = FcbReader::open_file(kFixture);
    // REPLACE with a real numeric indexed column and a value present in the data.
    KeyValue v = KeyValue::from_f64(/* real value */ 0.0);

    auto collect = [&](Operator op) {
        AttrQuery q = {{"<numeric_column>", op, v}};
        FeatureIterator it = r.select_attr(q);
        std::set<std::string> ids;
        while (it.next()) ids.insert(it.current().id());
        return ids;
    };

    auto ge = collect(Operator::Ge);
    auto gt = collect(Operator::Gt);
    auto eq = collect(Operator::Eq);

    CHECK(gt.size() + eq.size() == ge.size());
    for (const auto& id : gt) CHECK(ge.count(id) == 1);
    for (const auto& id : eq) CHECK(ge.count(id) == 1);
}

TEST_CASE("duplicate keys resolve through the payload section") {
    // A key with multiple features must return all of them, sharing one leaf entry.
    FcbReader r = FcbReader::open_file(kFixture);
    AttrQuery q = {{"<column_with_duplicates>", Operator::Eq, /* value with >1 feature */ KeyValue::from_i32(0)}};
    FeatureIterator it = r.select_attr(q);
    std::uint64_t n = 0;
    while (it.next()) ++n;
    CHECK(n > 1);
}

TEST_CASE("multiple conditions are ANDed, and the second condition really applies") {
    FcbReader r = FcbReader::open_file(kFixture);

    auto ids = [&](const AttrQuery& q) {
        FeatureIterator it = r.select_attr(q);
        std::set<std::string> s;
        while (it.next()) s.insert(it.current().id());
        return s;
    };

    // Pick <col_b> and the threshold so that the second condition is known to
    // exclude a NON-EMPTY, PROPER subset of the first condition's results.
    // `count(two) <= count(one)` would pass even if condition 2 were ignored
    // entirely -- assert a strict reduction instead.
    AttrQuery one = {{"<col_a>", Operator::Ge, KeyValue::from_f64(0.0)}};
    AttrQuery two = {{"<col_a>", Operator::Ge, KeyValue::from_f64(0.0)},
                     {"<col_b>", Operator::Le, KeyValue::from_f64(/* discriminating threshold */ 0.0)}};

    auto a = ids(one), b = ids(two);
    CHECK_FALSE(a.empty());
    CHECK_FALSE(b.empty());
    CHECK(b.size() < a.size());                       // strictly fewer
    for (const auto& id : b) CHECK(a.count(id) == 1); // and a proper subset
}

TEST_CASE("long-string equality post-filters away index collisions") {
    // Two features whose <string_column> values share the first 50 bytes but
    // differ after. The B+tree cannot tell them apart; select_attr must.
    // Requires the `long_strings` conformance fixture (Task 12).
    FcbReader r = FcbReader::open_file(FCB_CONFORMANCE_DIR "/long_strings.fcb");

    const std::string a = std::string(50, 'y') + "AAA";
    AttrQuery q = {{"<string_column>", Operator::Eq,
                    KeyValue::from_string(KeyKind::String50, a)}};

    FeatureIterator it = r.select_attr(q);
    std::uint64_t n = 0;
    while (it.next()) ++n;

    // Exactly one feature actually has this value; the other collides in the
    // index only. Without post-filtering this returns 2.
    CHECK(n == 1);
}

TEST_CASE("a feature with the attribute on several CityObjects is returned once") {
    FcbReader r = FcbReader::open_file(FCB_CONFORMANCE_DIR "/duplicate_keys.fcb");
    AttrQuery q = {{"<column_with_duplicates>", Operator::Eq, KeyValue::from_i32(0)}};

    FeatureIterator it = r.select_attr(q);
    std::vector<std::string> ids;
    while (it.next()) ids.push_back(it.current().id());

    std::set<std::string> uniq(ids.begin(), ids.end());
    CHECK(ids.size() == uniq.size());  // no duplicate features
}

TEST_CASE("querying an unindexed column throws AttributeIndexNotFound") {
    FcbReader r = FcbReader::open_file(kFixture);
    AttrQuery q = {{"definitely_not_a_column", Operator::Eq, KeyValue::from_i32(1)}};
    CHECK_THROWS_AS(r.select_attr(q), Error);
}
```

- [ ] **Step 2: Run and verify it fails**

- [ ] **Step 3: Write the implementation**

Port `static_btree/stree.rs`. The traps, all already cited in the Format Reference:

- `node_size = branching_factor - 1` for **search**, but `generate_level_bounds` divides by `branching_factor` and breaks at `n < branching_factor`.
- `find_exact` descent (`stree.rs:763-782`): on `Ok(i)` → child `node_items[i].offset + node_size`; `Err(0)` → `node_items[0].offset`; `Err(len)` → `node_items[len-1].offset + node_size`; else → `node_items[i].offset`.
- `find_partition` (`stree.rs:1086-1128`) uses the same descent **except** `Ok(i)` → `node_items[i].offset` with **no** `+ node_size`. This difference is real.
- `find_range` (`stree.rs:923-991`): `lower > upper` → empty; `lower == upper` → `find_exact`; `start = max(find_partition(lower), leaf.start)`; `end = min(find_partition(upper) + node_size, leaf.end)`; walk leaves in `node_size` chunks emitting `lower <= key <= upper` **inclusive on both ends**.
- Operator lowering exactly as in the Format Reference table — note `Gt`/`Lt`/`Ne` are `find_range` **minus** `find_exact`.
- Payload: if `offset & kPayloadTag`, seek `payload_data_start + (offset & kPayloadMask)` and emit one result per contained offset.
- Payload prefetch: before the walk, read `clamp(ceil(num_unique_items*0.1)*64, 16*1024, 4*1024*1024)` bytes from `payload_data_start` into a cache so payload lookups are usually free.
- Multi-condition: run each condition, intersect result sets sequentially on `offset`, early-exit when empty.
- **Deduplicate by feature offset.** One feature can contain several CityObjects carrying the same indexed attribute, which produces repeated offsets. `select_attr` returns each *feature* at most once; dedupe on `offset` before building the iterator.
- If the requested field has no `AttributeIndex` entry, throw `Error{AttributeIndexNotFound}`.
- Mirror the Rust asymmetry: `Json` and `Binary` columns throw `UnsupportedColumnType` on **read** even though the writer indexes them.

**Mandatory post-filtering for truncated string keys.** `FixedStringKey<N>` stores only the first N bytes, so two distinct strings sharing an N-byte prefix are indistinguishable in the tree (`static_btree/key.rs:483-489`). The B+tree therefore yields **candidates**, not answers, whenever the queried column is `String` (N=50) or `Json`/`Binary` (N=100) **and** the query value is ≥ N bytes.

It is not acceptable for a function named `select_attr` to return rows that do not match. This library owns the feature attribute decoder (Task 7), so it must verify:

1. Run the tree query to get candidate offsets.
2. If the column's key kind is **any** `FixedStringKey<N>`, then for each candidate: read the feature, decode the real attribute value for that column, and re-apply the operator against the **full, untruncated** value. Drop candidates that fail.
3. For all other key kinds the index is exact — no post-filter, no extra reads.

**Post-filter every fixed-string query, not just long ones.** A `query_value.size() >= N` gate looks safe but is not: keys are zero-padded, so `"a"` and `"a\0"` both encode to `61 00 00 ...` and collide even though the query is one byte long. CityJSON strings are JSON strings and may legally contain U+0000. The gate would therefore admit false positives on short values too, for both equality and ordering. Filtering unconditionally is simpler and always correct; the cost is bounded by the candidate count, which the index has already narrowed.

**Matching is existential.** A feature matches if *any* of its CityObjects carries an attribute satisfying the condition — this mirrors how the writer indexes each occurrence. Stop scanning a feature's objects at the first match.

Add a query-options struct so this is expressible:

```cpp
struct AttrQueryOptions {
    /// Return raw index candidates without verifying them against the
    /// decoded attribute. Faster, but may include non-matching features
    /// for fixed-string columns. Default: false (verify).
    bool exact_index_only = false;
};
```

`FcbReader::select_attr(const AttrQuery&, AttrQueryOptions = {})`.

Note this makes C++ **stricter than Rust**, which returns the false positives. Record it under "Known divergences" — it is a divergence in the safe direction, and the conformance suite must special-case it (see Task 12).

- [ ] **Step 4: Run and verify it passes**

- [ ] **Step 5: Commit (milestone)**

```bash
git add src/cpp/include/fcb src/cpp/src src/cpp/tests src/cpp/CMakeLists.txt
git commit -m "feat(cpp): implement static B+tree attribute query with payload resolution"
```

---

## Task 11: HTTP range reader

**Files:**
- Create: `src/cpp/include/fcb/http/curl_range_reader.hpp`, `src/cpp/src/http/curl_range_reader.cpp`
- Create: `src/cpp/tests/test_http.cpp`
- Modify: `src/cpp/CMakeLists.txt`, `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: `RangeReader`, `RangeRequest`.
- Produces (only when `FCB_WITH_CURL`):
  - `class fcb::CurlRangeReader : public RangeReader` — ctor `CurlRangeReader(const std::string& url)`; overrides `total_size()`, `read()`, `read_batch()`.
  - `struct fcb::CurlOptions { long timeout_ms = 30000; bool follow_redirects = true; std::string user_agent; }` — optional second ctor arg.

- [ ] **Step 1: Write the failing test**

The HTTP tests must not depend on the network. Serve the fixture over loopback with a **purpose-written** range server.

**`python3 -m http.server` will not work** — `SimpleHTTPRequestHandler` has no `Range`/`Content-Range` handling in any current Python, so it answers every request with 200 and the whole file. Testing against it would validate nothing and would mask exactly the bugs this task can introduce. Write `src/cpp/tests/range_server.py` instead, implementing:

- `HEAD` returning `Content-Length` and `Accept-Ranges: bytes`
- `GET` with `Range: bytes=a-b` → `206` plus a correct `Content-Range: bytes a-b/total`
- `GET` with no `Range` → `200` and the full body
- a `?ignore_range=1` mode that returns `200` with the **whole body** despite a `Range` header, to exercise the client's fallback path
- a `?bad_range=1` mode returning a malformed `Content-Range`
- `416` with `Content-Range: bytes */total` for an unsatisfiable range
- deterministic behaviour, no keep-alive surprises (`protocol_version = "HTTP/1.1"` with correct `Content-Length` on every response)

It binds port 0 and prints the chosen port on stdout so the CMake wrapper can pass the URL through `FCB_TEST_HTTP_URL`.

Create `src/cpp/tests/test_http.cpp`:

```cpp
#ifdef FCB_WITH_CURL
#include <doctest/doctest.h>
#include <fcb/http/curl_range_reader.hpp>
#include <fcb/reader.hpp>

#include <cstdlib>
#include <string>

using namespace fcb;

// FCB_TEST_HTTP_URL is set by CTest to a loopback URL serving examples/data/delft.fcb.
static std::string fixture_url() {
    const char* u = std::getenv("FCB_TEST_HTTP_URL");
    return u ? std::string(u) : std::string();
}

TEST_CASE("CurlRangeReader reports total size via a HEAD request") {
    const std::string url = fixture_url();
    if (url.empty()) { MESSAGE("FCB_TEST_HTTP_URL not set; skipping"); return; }

    CurlRangeReader r(url);
    CHECK(r.total_size() > 0);
}

TEST_CASE("CurlRangeReader returns the same bytes as the local file") {
    const std::string url = fixture_url();
    if (url.empty()) return;

    FileRangeReader local(FCB_TEST_DATA_DIR "/delft.fcb");
    CurlRangeReader remote(url);

    CHECK(remote.total_size() == local.total_size());
    CHECK(remote.read(0, 64) == local.read(0, 64));
    CHECK(remote.read(1000, 256) == local.read(1000, 256));
    // The last byte: exercises the open-ended-range path.
    const std::uint64_t n = local.total_size();
    CHECK(remote.read(n - 16, 16) == local.read(n - 16, 16));
}

TEST_CASE("a bbox query over HTTP returns the same ids as over the local file") {
    const std::string url = fixture_url();
    if (url.empty()) return;

    auto ids_from = [](std::shared_ptr<RangeReader> rr) {
        FcbReader r = FcbReader::open(std::move(rr));
        const auto& info = r.header().info();
        BBox all{info.geographical_extent[0], info.geographical_extent[1],
                 info.geographical_extent[3], info.geographical_extent[4]};
        FeatureIterator it = r.select_bbox(all);
        std::set<std::string> ids;
        while (it.next()) ids.insert(it.current().id());
        return ids;
    };

    auto local  = ids_from(std::make_shared<FileRangeReader>(FCB_TEST_DATA_DIR "/delft.fcb"));
    auto remote = ids_from(std::make_shared<CurlRangeReader>(url));
    CHECK(local == remote);
}

TEST_CASE("opening a remote file costs a bounded number of requests") {
    const std::string url = fixture_url();
    if (url.empty()) return;

    CurlRangeReader r(url);
    r.reset_request_count();
    FcbReader reader = FcbReader::open(std::shared_ptr<RangeReader>(&r, [](RangeReader*){}));
    CHECK(reader.header().info().features_count > 0);
    // Rust's open path prefetches 12944 bytes and needs 1 range request
    // (plus 1 HEAD for total_size). Allow a small margin.
    CHECK(r.request_count() <= 4);
}
```

Add a `reset_request_count()` / `request_count()` pair to `CurlRangeReader` — it is the only way to test the prefetch behaviour that justifies the whole design.

Wire the server into CTest in `src/cpp/tests/CMakeLists.txt`:

```cmake
if(FCB_WITH_CURL)
    find_package(Python3 COMPONENTS Interpreter REQUIRED)
    add_test(NAME fcb_http_tests
             COMMAND ${CMAKE_COMMAND}
                     -DPYTHON=${Python3_EXECUTABLE}
                     -DTEST_EXE=$<TARGET_FILE:fcb_tests>
                     -DDATA_DIR=${CMAKE_CURRENT_SOURCE_DIR}/../../../examples/data
                     -P ${CMAKE_CURRENT_SOURCE_DIR}/run_http_tests.cmake)
endif()
```

And `src/cpp/tests/run_http_tests.cmake` starts **`range_server.py`** (specified above — NOT `python3 -m http.server`, which cannot serve Range requests) in `DATA_DIR`. The server binds port 0 and prints the chosen port on stdout; the wrapper reads it, exports `FCB_TEST_HTTP_URL=http://127.0.0.1:<port>/delft.fcb`, runs `TEST_EXE`, and kills the server in a way that also fires if the test crashes.

- [ ] **Step 2: Run and verify it fails**

```bash
cd src/cpp && cmake -B build -S . -DFCB_WITH_CURL=ON \
  -DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake \
  -DVCPKG_MANIFEST_FEATURES="tests;curl" && cmake --build build
```

Expected: `fatal error: 'fcb/http/curl_range_reader.hpp' file not found`.

- [ ] **Step 3: Write the implementation**

`CurlRangeReader`:
- `total_size()`: `CURLOPT_NOBODY` HEAD, read `CURLINFO_CONTENT_LENGTH_DOWNLOAD_T`. Cache it. If the server does not report a length, fall back to a `Range: bytes=0-0` request and parse `Content-Range: bytes 0-0/<total>`.
- `read(offset, length)`: set `CURLOPT_RANGE` to `"<offset>-<offset+length-1>"`, write into a `std::vector` via a write callback. Response handling must be explicit:
  - **206** — validate the `Content-Range` header actually describes the range that was asked for. A server may legally return a *different* range than requested; if `Content-Range`'s start ≠ `offset`, or its length ≠ the body length, throw `Error{HttpError}`. Never assume the body corresponds to the request.
  - **200** — the server ignored `Range` and sent the whole representation. Do **not** "truncate": resizing the body to `length` yields bytes `[0, length)`, not `[offset, offset+length)`, silently returning wrong data. Slice `[offset, offset+length)` out of the full body, and throw unless the body is long enough for the **entire requested slice** (`body.size() >= offset + length`, checked without overflow) — a body merely longer than `offset` is not sufficient. (Simplest correct alternative, also acceptable: reject 200 outright for any `offset != 0`.)
  - **416** — treat as an empty read if `offset >= total_size()`, else `Error{HttpError}`.
  - Any other status → `Error{HttpError}` including the code.
  - A body shorter than requested that is *not* explained by EOF is a truncated transfer → `Error{HttpError}`, per the `RangeReader` contract.
  - `length == 0` returns empty without issuing a request.
  - Guard `offset + length` against overflow before formatting the header.
- **Representation stability.** The core issues many ranges against one logical file and assumes they are the same bytes. Set `Accept-Encoding: identity` (a compressed representation makes byte ranges meaningless), capture the `ETag` from the first response, and send `If-Match` on subsequent requests so a mutated object fails loudly instead of silently mixing versions. If the server sends no `ETag`, fall back to `Last-Modified`/`If-Unmodified-Since`; if neither exists, document that mutable URLs are unsupported.
- `read_batch()`: use `curl_multi` so ranges are issued concurrently over one connection (HTTP/2 multiplexing where available). Reuse a single `CURLM` handle across calls.
- Set `CURLOPT_FOLLOWLOCATION`, a timeout, and a user agent. **Do not touch any TLS option** — libcurl's platform default is what keeps this dependency-free.

Reuse one `CURL` easy handle across `read()` calls so connections are kept alive; that is where most of the latency win lives.

Buffering is **per-query**, never global. `read_header()` constructs its own `BufferedRangeReader` with `min_req_size = 12944` (matching `http_reader/mod.rs:80-98`, which over-fetches the header plus the top ~3 R-tree levels in one request) and discards it when the header is parsed. Each `select_*` then constructs its own with `min_req_size = 1048576` for the feature phase. `FcbReader` itself holds only the bare transport. This keeps concurrent iterators from altering each other's buffering policy and avoids a decorator wrapping a decorator.

- [ ] **Step 4: Run and verify it passes**

```bash
cd src/cpp && cmake --build build && ctest --test-dir build --output-on-failure
```

- [ ] **Step 5: Confirm the default build has no curl and no TLS**

```bash
cd src/cpp
cmake -B build-default -S . \
  -DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake \
  -DVCPKG_MANIFEST_FEATURES=tests
cmake --build build-default
nm -u build-default/libfcb_core_cpp.a 2>/dev/null | grep -iE 'curl|ssl|crypto' || echo "CLEAN: no curl/TLS symbols"
```

Expected: `CLEAN: no curl/TLS symbols`. This is the check that keeps the vcpkg port acceptable — if it fails, the curl adapter is leaking into the default build.

- [ ] **Step 6: Commit (milestone)**

```bash
git add src/cpp/include/fcb/http src/cpp/src/http src/cpp/tests src/cpp/CMakeLists.txt
git commit -m "feat(cpp): add optional libcurl HTTP range reader"
```

---

## Task 12: Conformance suite and spec correction

**The corpus itself was generated in Task 2b.** This task specifies its contents (below), wires up the C++ tests that consume it, adds the differential harness, and corrects the specification. If a fixture named here does not exist, go back and add it to the Task 2b generators.

**Files:**
- Create: `scripts/gen_conformance.sh`
- Create: `src/cpp/tests/conformance/` (fixtures: `.fcb` + `.expected.jsonl` pairs)
- Create: `src/cpp/tests/test_conformance.cpp`
- Modify: `.llm/docs/specification.md`
- Modify: `src/cpp/tests/CMakeLists.txt`

**Interfaces:**
- Consumes: everything above.
- Produces: `scripts/gen_conformance.sh`; a `fcb_conformance` CTest target.

- [ ] **Step 1: Write the corpus generator**

Create `scripts/gen_conformance.sh`. It uses the Rust CLI as the oracle: for each input `.city.jsonl`, write a `.fcb`, then read it back with the Rust reader and save that output as `.expected.jsonl`. Comparing C++ against **the Rust reader's output of the same file** (not the original input) cancels shared normalization and isolates C++ bugs.

```bash
#!/usr/bin/env bash
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${REPO_ROOT}/src/cpp/tests/conformance"
RUST="${REPO_ROOT}/src/rust"
mkdir -p "${OUT}"

# Inputs: existing fixtures plus synthetic edge cases.
INPUTS=(
  "${RUST}/fcb_core/tests/data/small.city.jsonl"
  "${RUST}/fcb_core/tests/data/delft.city.jsonl"
  "${RUST}/fcb_core/tests/data/geom_temp.city.jsonl"
  "${RUST}/fcb_core/tests/data/noise_extension.city.jsonl"
)

for src in "${INPUTS[@]}"; do
  name="$(basename "${src}" .city.jsonl)"
  echo "==> ${name}"
  (cd "${RUST}" && cargo run --release -p fcb_cli -- ser -i "${src}" -o "${OUT}/${name}.fcb")
  (cd "${RUST}" && cargo run --release -p fcb_cli -- deser -i "${OUT}/${name}.fcb" -o "${OUT}/${name}.expected.jsonl")
done

echo "Conformance corpus written to ${OUT}"
```

Check the actual CLI subcommand names in `src/rust/cli/` and the `justfile` before running — `ser`/`deser` are placeholders here and must be corrected to the real ones.

- [ ] **Step 2: Add synthetic edge-case inputs**

**Two classes of fixture, because CityJSON-in / CLI-out cannot express every case.**

The writer infers its column schema from JSON values (`writer/attribute.rs:34`), and that inference only ever produces `Bool`, `Double`, `Long`/`ULong`, `String`, `DateTime` and `Json`. It can never emit `Byte`, `UByte`, `Short`, `UShort`, `Int`, `UInt`, `Float` or `Binary`. Separately, `serde_json` will not serialize or parse `NaN`/`Infinity` literals, so no `.city.jsonl` file can carry them. And the Rust CityJSON decoder has no `Byte`/`UByte`/`Binary` branch at all — it ends in `unreachable!()` (`reader/deserializer.rs:372`), so it cannot even produce expected output for those.

**Class A — JSON-authored, Rust CLI generates both `.fcb` and `.expected.jsonl`.** Only cases the writer can actually express:

| File | Targets |
|---|---|
| `single_feature.city.jsonl` | `num_items == 1`; the last-feature size path |
| `long_strings.city.jsonl` | Two features whose `String` attributes share a 50-byte prefix but differ after — the truncation-collision path, and the post-filter that must remove it |
| `duplicate_keys.city.jsonl` | ≥3 features sharing one attribute value — forces payload entries; plus one feature carrying the attribute on several CityObjects, to pin the dedupe rule |
| `degenerate_extent.city.jsonl` | All features at one point, so the geographical extent has zero width/height — bbox queries against a zero-area extent and division-by-zero guards |
| `inferable_types.city.jsonl` | One attribute each of `Bool`, `Double`, `Long`, `ULong`, `String`, `DateTime`, `Json` — every type the writer can infer |

**Class B — binary fixtures built by a dedicated Rust generator.** Add `src/rust/fcb_conformance/` (a dev-only workspace member, `publish = false`) that constructs `Header` column schemas and attribute indexes **directly**, bypassing JSON inference, and writes `.fcb` files plus a hand-authored `.expected.json` describing what a correct reader must return:

| File | Targets |
|---|---|
| `all_column_types.fcb` | An indexed column of every `ColumnType` including `Byte`, `UByte`, `Short`, `UShort`, `Int`, `UInt`, `Float`, `Binary` — the only way to exercise every key encoder |
| `byte_high_values.fcb` | A `Byte` column holding values > 127 — pins the `u8` decision from "Known divergences" and would catch a silent regression to `i8` |
| `float_edges.fcb` | `Float64` keys of `NaN`, `+inf`, `-inf`, `-0.0` written straight into the index — unreachable via JSON |
| `no_spatial_index.fcb` | `index_node_size == 0` — the "no R-tree" layout branch |
| `branching_factor_2.fcb` | Minimum legal branching factor; deepest tree, most level-bound edge cases |
| `partial_final_node.fcb` | `num_unique_items` chosen so the last B+tree node is partially filled |

**Class C — malformed inputs, for the verifier and bounds checks.** Generated by mutating a valid `.fcb` (a small script is enough): bad magic; version byte 2; `header_size` of 0, 7, and 2^31; `features_count` of `UINT64_MAX`; `index_node_size` of 1; an `AttributeIndex.length` that overruns the file; a feature size prefix of `0xFFFFFFFF`; a payload count that runs past the payload region; a file truncated mid-header and mid-feature. Each must produce a `fcb::Error` — **never** a crash, a hang, or an allocation over `kMaxFeatureSize`. Run this whole class under ASan/UBSan; it is the only part of the suite that tests the security posture.

Add Class A to the `INPUTS` array in the generator; Class B and C get their own scripts.

- [ ] **Step 3: Write the failing conformance test**

Create `src/cpp/tests/test_conformance.cpp`:

```cpp
#include <doctest/doctest.h>
#include <fcb/cityjson.hpp>
#include <fcb/reader.hpp>

#include <fstream>
#include <nlohmann/json.hpp>
#include <string>
#include <vector>

using namespace fcb;
using nlohmann::json;

static const char* kCorpus = FCB_CONFORMANCE_DIR;

static std::vector<json> read_jsonl(const std::string& path) {
    std::vector<json> out;
    std::ifstream f(path);
    REQUIRE_MESSAGE(f.good(), "cannot open " << path);
    std::string line;
    while (std::getline(f, line)) {
        if (!line.empty()) out.push_back(json::parse(line));
    }
    return out;
}

static void check_case(const std::string& name) {
    CAPTURE(name);
    const std::string fcb_path = std::string(kCorpus) + "/" + name + ".fcb";
    const std::string exp_path = std::string(kCorpus) + "/" + name + ".expected.jsonl";

    std::vector<json> expected = read_jsonl(exp_path);
    REQUIRE_FALSE(expected.empty());

    FcbReader r = FcbReader::open_file(fcb_path);

    std::vector<json> actual;
    actual.push_back(to_cityjson_metadata(r.header()));
    FeatureIterator it = r.select_all();
    while (it.next()) actual.push_back(to_cityjson_feature(it.current(), r.header()));

    REQUIRE(actual.size() == expected.size());
    for (std::size_t i = 0; i < actual.size(); ++i) {
        CAPTURE(i);
        // Compare parsed trees, never strings: key order and float
        // formatting legitimately differ between implementations.
        CHECK(actual[i] == expected[i]);
    }
}

// Class A: Rust CLI generates both the .fcb and the expected output.
TEST_CASE("conformance: small")             { check_case("small"); }
TEST_CASE("conformance: delft")             { check_case("delft"); }
TEST_CASE("conformance: geom_temp")         { check_case("geom_temp"); }
TEST_CASE("conformance: noise_extension")   { check_case("noise_extension"); }
TEST_CASE("conformance: single_feature")    { check_case("single_feature"); }
TEST_CASE("conformance: long_strings")      { check_case("long_strings"); }
TEST_CASE("conformance: duplicate_keys")    { check_case("duplicate_keys"); }
TEST_CASE("conformance: degenerate_extent") { check_case("degenerate_extent"); }
TEST_CASE("conformance: inferable_types")   { check_case("inferable_types"); }

// Class B fixtures are checked in test_conformance_binary.cpp against
// hand-authored expectations -- the Rust reader cannot decode several of
// them (deserializer.rs:372 is `unreachable!()` for Byte/UByte/Binary),
// so it is not a usable oracle there.
```

Add `FCB_CONFORMANCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}/conformance"` to the test target's compile definitions.

- [ ] **Step 4: Generate the corpus and run**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
./scripts/gen_conformance.sh
cd src/cpp && cmake --build build && ctest --test-dir build --output-on-failure
```

Expect failures on the first run — that is the point. Fix the C++ side for each, one at a time. Do **not** modify `.expected.jsonl`; it is the oracle.

If a case reveals that Rust itself is wrong (e.g. the `Json`/`Binary` read asymmetry), document it in the spec (Step 6) and mirror the Rust behaviour — bug-compatibility beats divergence for a second implementation.

- [ ] **Step 5: Add the differential query harness**

Add `src/cpp/tests/test_differential.cpp`, covering **both** query paths. Use a fixed seed so failures reproduce.

**Spatial.** For each Class A corpus file, run N=200 pseudo-random bboxes — drawn from the file's own extent, plus deliberate degenerate cases (zero-area, exactly-on-boundary, fully-enclosing, fully-outside) — and compare returned feature-id sets against the Rust CLI. This is where inclusive/exclusive boundary bugs surface.

**Attribute.** Do the same for attribute queries, and weight it *more* heavily: the B+tree is the larger, less documented, less trustworthy component, and the plan's unit tests cover it thinnest. For each indexed column, generate queries across all six operators using values sampled from the actual data — including the minimum, the maximum, values known absent, and values adjacent to real ones — and compare id sets.

Two expected, legitimate divergences that the harness must special-case rather than report as failures:
- **Long-string post-filtering**: where the query value is ≥ the key width, C++ returns a subset of Rust's results. Assert `cpp ⊆ rust`, and assert that every id in `rust \ cpp` genuinely does *not* match when checked against its decoded attribute.
- **`Byte` columns with values > 127**: C++ decodes `u8`, Rust `i8` (see "Known divergences"). Skip these columns in the differential comparison and cover them in the Class B binary fixtures instead.

Gate the whole file behind a CMake option `FCB_DIFFERENTIAL_TESTS` (default `OFF`) so ordinary CI does not require a Rust toolchain.

- [ ] **Step 6: Correct the specification**

The port is the best spec audit available. Fix `.llm/docs/specification.md` with what was learned. At minimum:

1. §file-storage-overview — the "Header Size (4 bytes)" box is the **FlatBuffers size prefix**, not a custom field. State that `GetSizePrefixedRoot` must be used and that the prefix excludes itself.
2. §file-storage-overview — delete "each section is aligned to facilitate efficient http range requests." Sections are concatenated with **no padding**. Replace with the exact offset formulas from this plan's Format Reference.
3. §attribute-indexing — replace the four vague paragraphs with the real layout: bare concatenation of per-column blobs in `Column.index` order; no per-index header; `Entry<K> = key || u64 LE offset`; `node_size = branching_factor - 1` for search while level bounds divide by `branching_factor` and break at `n < branching_factor`; **no leaf sibling pointers** (range scans are index arithmetic over the contiguous leaf array).
4. §serialization-by-type — correct "floating point: wrapped in `orderedfloat` to handle nan values properly" to state explicitly that the **on-disk bytes are the plain IEEE-754 LE bit pattern with no order-preserving transform**, and that ordering is applied after decode.
5. §serialization-by-type — correct "strings: fixed-width prefix with utf-8 encoding and overflow handling" to: fixed width N ∈ {20, 50, 100}, zero-padded, **silently truncated at the byte level** (can split a UTF-8 sequence), no length prefix, no terminator. Add the consequence: strings sharing an N-byte prefix collide, so long-string matches are candidates requiring post-verification.
6. §serialization-by-type — replace "datetimes: normalized representation" with: 12 bytes, `i64 LE` UNIX seconds then `u32 LE` subsecond nanos; parse failures at write time silently become epoch 0.
7. §payload-section — state that the tagged offset's low 63 bits are **relative to the start of the payload section**, that `AttributeIndex.length` **includes** the payload region, and that the payload entry is `u32 count` + `count × u64`, all LE.
8. §rtree-indexing — add that internal-node `offset` is a **child node index** while leaf `offset` is a byte offset relative to the features section, and add the `index_size` formula.
9. New §feature-framing — features are size-prefixed FlatBuffers (4-byte LE prefix excluding itself), stored in **Hilbert order**, with no padding between them.
10. New §implementation-notes — record the two known Rust quirks so a third implementer is not surprised: `select_query` on the seekable reader hardcodes node size 16 instead of reading `header.index_node_size()` (`reader/mod.rs:220`), and `Json`/`Binary` columns are indexed by the writer but rejected by the reader (`reader/attr_query.rs:273`).

- [ ] **Step 7: Run everything and verify green**

```bash
cd src/cpp && ctest --test-dir build --output-on-failure
cd src/cpp && ctest --test-dir build-asan --output-on-failure
```

- [ ] **Step 8: Commit (milestone)**

```bash
git add scripts/gen_conformance.sh src/cpp/tests .llm/docs/specification.md
git commit -m "test(cpp): add conformance corpus and correct the format specification"
```

---

## Task 13: Retire the CXX bridge and ship

Gated on Task 12 being fully green. This is the task that delivers the "replace `src/cpp/` in place" end state.

**Files:**
- Delete: `src/cpp/CMakeLists.bridge.txt`, `src/cpp/include/fcb_bridge.h`, `src/cpp/examples/*`, `src/cpp/tests/roundtrip_test.cpp`, `src/cpp/example_output.fcb`, `src/cpp/build/`
- Delete: `src/rust/fcb_cpp/` (entire crate)
- Modify: `src/rust/Cargo.toml` (drop `fcb_cpp` from workspace members)
- Create: `src/cpp/examples/read_local.cpp`, `src/cpp/examples/read_http.cpp`
- Create: `src/cpp/cmake/flatcitybufConfig.cmake.in`
- Modify: `src/cpp/CMakeLists.txt` (install/export rules)
- Modify: `justfile`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- Modify: `src/cpp/README.md`, `src/cpp/INSTALL.md`

**Interfaces:**
- Consumes: everything above.
- Produces: an installed `flatcitybuf::flatcitybuf` CMake target with headers under `include/fcb/`.

- [ ] **Step 1: Verify the gate before deleting anything**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp
ctest --test-dir build --output-on-failure
ctest --test-dir build-asan --output-on-failure
```

Both must be fully green, including every conformance case. If any test fails, **stop** — do not delete the bridge.

- [ ] **Step 2: Write the failing test (the install surface)**

Create `src/cpp/tests/install_test/CMakeLists.txt` — a standalone consumer project that only does `find_package(flatcitybuf CONFIG REQUIRED)` and builds a one-file program using the public API:

```cmake
cmake_minimum_required(VERSION 3.16)
project(fcb_install_test LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
find_package(flatcitybuf CONFIG REQUIRED)
add_executable(consumer consumer.cpp)
target_link_libraries(consumer PRIVATE flatcitybuf::flatcitybuf)
```

`src/cpp/tests/install_test/consumer.cpp`:

```cpp
#include <fcb/reader.hpp>
#include <cstdio>

int main(int argc, char** argv) {
    if (argc < 2) return 2;
    fcb::FcbReader r = fcb::FcbReader::open_file(argv[1]);
    std::printf("%llu\n",
                static_cast<unsigned long long>(r.header().info().features_count));
    auto it = r.select_all();
    unsigned long long n = 0;
    while (it.next()) ++n;
    return (n == r.header().info().features_count) ? 0 : 1;
}
```

- [ ] **Step 3: Run and verify it fails**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp
cmake --install build --prefix /tmp/fcb-install
cmake -B /tmp/fcb-consumer -S tests/install_test -DCMAKE_PREFIX_PATH=/tmp/fcb-install
```

Expected: `Could not find a package configuration file provided by "flatcitybuf"`.

- [ ] **Step 4: Add install and export rules**

Create `src/cpp/cmake/flatcitybufConfig.cmake.in`:

```cmake
@PACKAGE_INIT@
include(CMakeFindDependencyMacro)
find_dependency(flatbuffers CONFIG)
if(@FCB_WITH_JSON@)
    find_dependency(nlohmann_json 3.2.0 CONFIG)
endif()
if(@FCB_WITH_CURL@)
    find_dependency(CURL)
endif()
include("${CMAKE_CURRENT_LIST_DIR}/flatcitybufTargets.cmake")
check_required_components(flatcitybuf)
```

Append to `src/cpp/CMakeLists.txt`:

```cmake
include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

add_library(flatcitybuf::flatcitybuf ALIAS fcb_core_cpp)
set_target_properties(fcb_core_cpp PROPERTIES EXPORT_NAME flatcitybuf)

install(TARGETS fcb_core_cpp EXPORT flatcitybufTargets
        ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR})
install(DIRECTORY include/   DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
install(EXPORT flatcitybufTargets
        NAMESPACE flatcitybuf::
        DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/flatcitybuf)

configure_package_config_file(
    cmake/flatcitybufConfig.cmake.in
    "${CMAKE_CURRENT_BINARY_DIR}/flatcitybufConfig.cmake"
    INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/flatcitybuf)
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/flatcitybufConfigVersion.cmake"
    VERSION ${PROJECT_VERSION} COMPATIBILITY SameMajorVersion)
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/flatcitybufConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/flatcitybufConfigVersion.cmake"
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/flatcitybuf)
```

Note the `generated/` include path changes on install (`include/fcb/generated`), so public headers must include the generated headers via a path that works both in-tree and installed. Simplest fix: move `generated/` under `include/fcb/generated/` in-tree too, and always include as `<fcb/generated/header_generated.h>`. Do that now rather than maintaining two include paths.

- [ ] **Step 5: Run and verify it passes**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp
rm -rf /tmp/fcb-install /tmp/fcb-consumer
cmake --build build && cmake --install build --prefix /tmp/fcb-install
cmake -B /tmp/fcb-consumer -S tests/install_test -DCMAKE_PREFIX_PATH=/tmp/fcb-install
cmake --build /tmp/fcb-consumer
/tmp/fcb-consumer/consumer ../../examples/data/delft.fcb && echo "INSTALL OK"
```

Expected: prints the feature count, then `INSTALL OK`.

- [ ] **Step 6: Delete the bridge**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
git rm -r src/rust/fcb_cpp
git rm src/cpp/CMakeLists.bridge.txt src/cpp/include/fcb_bridge.h
git rm -r src/cpp/examples
git rm src/cpp/tests/roundtrip_test.cpp src/cpp/example_output.fcb
rm -rf src/cpp/build src/cpp/build-bridge
```

Edit `src/rust/Cargo.toml` and remove `"fcb_cpp"` from `[workspace] members`, leaving `["cli", "fcb_core", "wasm", "fcb_api", "fcb_py"]`.

Verify the Rust workspace still builds:

```bash
cd src/rust && cargo check --workspace --exclude fcb_wasm --exclude fcb_py
```

- [ ] **Step 7: Write the new examples**

Create `src/cpp/examples/read_local.cpp` (open a file, print metadata, iterate, emit CityJSONSeq to stdout) and `src/cpp/examples/read_http.cpp` (same over a URL, guarded by `#ifdef FCB_WITH_CURL`). Register both in `src/cpp/CMakeLists.txt` behind an `FCB_BUILD_EXAMPLES` option (default `ON`).

- [ ] **Step 8: Update the justfile**

Replace the `pre-commit-cpp` recipe and add native recipes:

```make
# Run C++ checks (native implementation)
pre-commit-cpp: check-cpp

check-cpp:
    cd src/cpp && cmake -B build -S . -DFCB_BUILD_TESTS=ON
    cd src/cpp && cmake --build build
    cd src/cpp && ctest --test-dir build --output-on-failure

check-cpp-http:
    cd src/cpp && cmake -B build-http -S . -DFCB_WITH_CURL=ON -DFCB_BUILD_TESTS=ON
    cd src/cpp && cmake --build build-http
    cd src/cpp && ctest --test-dir build-http --output-on-failure

gen-conformance:
    ./scripts/gen_conformance.sh

gen-cpp-fbs:
    ./scripts/gen_cpp_flatbuffers.sh
```

Also remove `build-cpp` / `test-cpp` recipes that reference the Rust staticlib, and drop `build-cpp` from `build-all`.

- [ ] **Step 9: Update CI**

In `.github/workflows/ci.yml`, replace the `check-cpp-roundtrip` job with a native one. It no longer needs Rust, cxxbridge, or OpenSSL:

```yaml
  check-cpp:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - name: Install dependencies (Linux)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y cmake libflatbuffers-dev nlohmann-json3-dev doctest-dev libcurl4-openssl-dev
      - name: Install dependencies (macOS)
        if: runner.os == 'macOS'
        run: brew install flatbuffers nlohmann-json doctest
      - name: Install dependencies (Windows)
        if: runner.os == 'Windows'
        run: |
          vcpkg install flatbuffers nlohmann-json doctest curl --triplet x64-windows
        shell: bash
      - name: Windows toolchain args
        if: runner.os == 'Windows'
        run: echo "CMAKE_ARGS=-DCMAKE_TOOLCHAIN_FILE=$VCPKG_INSTALLATION_ROOT/scripts/buildsystems/vcpkg.cmake -DVCPKG_TARGET_TRIPLET=x64-windows" >> $GITHUB_ENV
        shell: bash
      - name: Configure
        working-directory: src/cpp
        run: cmake -B build -S . -DFCB_BUILD_TESTS=ON -DFCB_WITH_CURL=ON ${CMAKE_ARGS}
        shell: bash
      - name: Build
        working-directory: src/cpp
        run: cmake --build build --config Release
      - name: Test
        working-directory: src/cpp
        run: ctest --test-dir build --output-on-failure -C Release
```

Add a `no-tls-in-default-build` step that runs the `nm -u | grep -iE 'curl|ssl|crypto'` check from Task 11 Step 5 and fails the job if it matches.

In `.github/workflows/release.yml`, delete the `cxxbridge-cmd` install and the `cargo build -p fcb_cpp` step from `build-cpp-bindings`; the matrix now just configures and builds CMake, and packages `libfcb_core_cpp.a` plus `include/`.

- [ ] **Step 10: Update the docs**

Rewrite `src/cpp/README.md` and `src/cpp/INSTALL.md` for the native library. Specifically:
- Remove every mention of `cxxbridge`, `lib.rs.h`, `lib.rs.cc`, `FLATCITYBUF_CXX_BRIDGE_SOURCE`, and "the CXX bridge source that must be compiled alongside your code" — none of that exists any more, and `INSTALL.md` currently instructs consumers to do exactly that.
- Remove the "Linux note: OpenSSL is installed as a vcpkg dependency automatically" line. It is no longer true and was the reason the vcpkg port was rejected.
- Document `FCB_WITH_JSON`, `FCB_WITH_CURL`, `FCB_BUILD_TESTS`, `FCB_BUILD_EXAMPLES`.
- Document the `RangeReader` extension point with a short custom-adapter example, since that is the library's main architectural affordance.
- Note that the writer is not implemented natively; producing `.fcb` files still requires the Rust CLI.

- [ ] **Step 11: Run the full suite one final time**

```bash
cd /Users/hbbaba/tudelft/cityjson/flatcitybuf
just check-cpp
just check-cpp-http
cd src/rust && cargo check --workspace --exclude fcb_wasm --exclude fcb_py
```

All three must pass.

- [ ] **Step 12: Commit (milestone)**

```bash
git add -A
git commit -m "feat(cpp)!: replace CXX bridge with native C++ reader

BREAKING CHANGE: the C++ API is now fcb/*.hpp instead of the generated
lib.rs.h CXX bridge. Consumers no longer compile lib.rs.cc and no longer
need a Rust toolchain or OpenSSL. The writer is not yet implemented
natively; use the Rust CLI to produce .fcb files."
```

- [ ] **Step 13: Open the pull request**

```bash
git push -u origin develop
gh pr create --base main --head develop \
  --title "feat(cpp): native C++ FlatCityBuf reader replacing the CXX bridge" \
  --body "$(cat <<'EOF'
## Summary

Replaces the Rust-FFI C++ bindings with a from-scratch native C++17 reader.
No Rust toolchain, no OpenSSL, no CXX bridge.

- Sans-IO core over a synchronous, user-implementable `RangeReader` with batched
  multi-range reads. Local files and HTTP are two adapters behind one traversal path.
- Header parsing, sequential scan, CityJSON emission, packed R-tree bbox query,
  static B+tree attribute query with payload resolution.
- Optional libcurl HTTP adapter (`FCB_WITH_CURL`, default OFF). The default build
  links neither curl nor any TLS library — verified in CI.
- Conformance corpus generated by the Rust CLI; C++ output compared as parsed JSON
  trees against the Rust reader's output of the same file.
- `.llm/docs/specification.md` corrected in ten places where it diverged from the
  reference implementation.

## Not included

The writer. Producing `.fcb` files still requires the Rust CLI.

## Breaking

The C++ API is now `fcb/*.hpp`. Consumers no longer compile `lib.rs.cc`.
EOF
)"
```

---

## Task 14: Report the upstream defects found during the port

Two genuine defects in the Rust implementation surfaced while porting. Filing them is part of the work, not optional — C++ deliberately diverges in both cases, and each divergence needs a tracked reason.

**Defect 2 — `Transform` is written at a misaligned offset.** Found in Task 5. The writer places the `Transform` struct at body+68 in the header FlatBuffer, an odd multiple of 4, even though it is six doubles and requires 8-byte alignment. `GeographicalExtent` is affected similarly depending on buffer placement. Consequences: the C++ FlatBuffers verifier's `check_alignment` rejects every Rust-written header at every possible buffer placement (the offset is internal, so no allocation strategy fixes it), and accessing the field through the generated accessor is undefined behaviour — UBSan reports "member call on misaligned address ... for type 'Transform'". The C++ reader works around it by reading struct doubles via `memcpy` and disabling only `check_alignment`. Rust's own verifier does not catch this, which is why it went unnoticed. Reproduce with the probe in Task 5's notes, or simply run the C++ suite under UBSan before the memcpy fix.

**Defect 1 — `Byte` attribute index: writer stores `u8`, reader decodes `i8`.**

The port surfaced a genuine defect in the Rust implementation. Filing it is part of the work, not an optional extra — C++ now deliberately diverges, and that divergence needs a tracked reason.

- [ ] **Step 1: Confirm the discrepancy with a Rust test**

Add a `#[test]` to `src/rust/fcb_core/src/reader/attr_query.rs` that writes a `Byte` column containing `200`, reads it back, and asserts the returned value. It should fail, returning `-56` (`200` reinterpreted as `i8`).

- [ ] **Step 2: File the issue**

```bash
gh issue create --repo cityjson/flatcitybuf \
  --title "Byte attribute index: writer stores u8, reader decodes i8" \
  --body "$(cat <<'EOF'
The writer stores `Byte` attribute values as `u8` (`writer/attribute.rs:209`) and
builds the index as `MemoryIndex<u8>` (`writer/attr_index.rs:240`), but the reader
decodes that index as `i8` (`reader/attr_query.rs:118`).

For stored values > 127 the reader therefore returns a negative number that was
never written: a stored `200` reads back as `-56`.

Found while implementing the native C++ reader, which decodes `u8` to match the
writer. The two implementations will disagree on `Byte` queries until this is
resolved.

Note that `writer/attribute.rs:327` currently routes `Byte`/`UByte`/`Short`/`UShort`
into "not supported" during normal attribute extraction, so the path is rarely hit
via the CLI - but it is reachable for hand-built and third-party files.
EOF
)"
```

- [ ] **Step 3: Link the issue from the plan and the code**

Replace the placeholder in the "Known divergences from the Rust reader" section with the issue URL, and add the same URL as a comment above the `Byte` case in `src/cpp/src/key.cpp`.

- [ ] **Step 4: Commit (milestone)**

```bash
git add docs/superpowers/plans/2026-07-19-native-cpp-core.md src/cpp/src/key.cpp
git commit -m "docs(cpp): link upstream issue for the Byte u8/i8 discrepancy"
```

---

## Self-Review

This plan was reviewed twice: by an architectural advisor before drafting, and by an independent reviewer (codex/gpt-5.6-sol) reading the actual Rust source afterwards. The second pass found **nine blocking errors**. All are fixed above; they are recorded here so the corrections are not silently re-introduced.

**Errors found and fixed:**

1. `AttributeIndex` was stated as 12 bytes. It is **16** — field order forces padding after each `ushort`. Verified in both generated backends. The original text also told the executor *not* to fix the test if it disagreed, which would have sent someone defending a wrong number.
2. `Byte → i8` was stated as the writer's behaviour. The writer emits **`u8`**; only the *reader* uses `i8`. C++ follows the writer; the discrepancy is filed as Task 14.
3. The generated C++ namespace was assumed to be `FlatCityBuf`. Every `namespace` declaration in the schemas is **commented out**, so types are global. Verified by running flatc.
4. The public API exposed `HeaderView::raw()` and `Feature::raw()` returning generated pointers — directly contradicting this plan's own ownership rule. Now private, behind an internal detail header.
5. `select_attr` could return false positives from truncated string keys, with the plan telling *callers* to post-verify. The library owns the attribute decoder and now post-filters itself.
6. The HTTP test harness relied on `python3 -m http.server` serving Range requests. It does not. Replaced with a purpose-written range server covering 206/200/416 and malformed `Content-Range`.
7. HTTP 200 fallback said to "truncate", which returns bytes `[0, length)` instead of `[offset, offset+length)` — silently wrong data. Now specified as slice-or-reject.
8. Several conformance fixtures were impossible to generate: the writer's schema inference reaches only 6 column types, `serde_json` cannot express NaN/Inf, and the Rust decoder hits `unreachable!()` for `Byte`/`UByte`/`Binary`. Fixtures are now split into JSON-authored (Class A), Rust-generated binary (Class B), and malformed (Class C).
9. Size arithmetic on untrusted input was unchecked, allowing overflow and a ~4 GiB allocation from a crafted feature prefix. Now checked throughout, with `kMaxFeatureSize` and explicit bounds validation.

**Also corrected:** `BufferedRangeReader::read_batch` bypassed its own cache, defeating the decorator exactly when traversal batches. Buffering was ambiguously wrapped in both `open()` and `select_all()`, letting concurrent iterators mutate each other's policy — now strictly per-query. Malformed `node_size`/`branching_factor` were clamped where Rust asserts, so they are now rejected. The `total_size()` rationale was wrong (features carry their own size prefix); it is required as a bounds contract instead. Task 7 was too large for one commit and is split into 7a/7b/7c. Task 8 no longer ports the Hilbert curve at all — it is writer-only, and dropping it removes a verbatim-transcription risk and a real UB trap. Several tests were tautological or too weak to fail: the `|| true` assertion, the AND test that passed if the second condition were ignored, the "same as Rust" test that only checked non-empty, and a UTF-8-splitting test that used only ASCII.

**Spec coverage.** Every section of `.llm/docs/specification.md` maps to a task: header schema (Task 5), attributes (7a), geometry/semantics/templates (7b), extensions and appearances (7c), R-tree (8), attribute B+tree with payload and both prefetch optimizations (9–10), HTTP range mechanism (4, 11). Task 12 corrects the spec in ten places where it diverges from the reference implementation. Out of scope by decision: the **writer**.

**Remaining known assumptions.** Two, both now narrow and both with a stated resolution procedure: the Rust CLI subcommand names (`ser`/`deser`/`info` are placeholders — confirm against `src/rust/cli/` before Tasks 7c, 10 and 12 depend on them), and the specific indexed column names in the `delft` fixture (obtain from `fcb info`; the plan gives the command). The previous Hilbert-probe assumption is gone with the Hilbert code.

**Verification status.** Format Reference constants confirmed correct by the independent review: section offset arithmetic and the no-padding claim; the R-tree `n == 1` versus S-tree `n < branching_factor` asymmetry; `Entry<K>` layout; payload MSB tagging and its payload-relative base; raw IEEE-754 float keys with no order transform; 12-byte DateTime; byte-level string truncation; and the absence of leaf sibling pointers.

---

## Execution Handoff

Plan saved to `docs/superpowers/plans/2026-07-19-native-cpp-core.md`.
