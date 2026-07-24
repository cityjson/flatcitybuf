# Native TypeScript FlatCityBuf Reader — Design

**Date:** 2026-07-21
**Status:** Approved, ready for implementation planning
**Supersedes:** the WASM bindings at `src/rust/wasm` and the generated artifacts at `src/ts/fcb_wasm*`

---

## Goal

Replace the `wasm-bindgen` binding with a from-scratch pure-TypeScript FlatCityBuf
**reader**, so a browser can read `.fcb` files with no WebAssembly, no 3.6 MB binary
payload, and no `init()` step.

This is the **fourth** implementation of this reader. Rust (`src/rust/fcb_core`) is the
origin; C++ (`src/cpp`) was ported from it in July 2026; the pure-Python port
(`src/py`, branch `native-py`) is through its Task 11. The format is already documented
to ground truth and the conformance corpus already exists — this port inherits all of
that leverage.

## Non-goals

- **Writing `.fcb` files.** Reader only, same as the C++ and Python ports.
- **`cjToObj`** (CityJSON → Wavefront OBJ). Dropped, not ported. Not reader logic.
- **`cjseqToCj`** (CityJSONSeq → CityJSON merge). Dropped, not ported. Note it is *not*
  trivial — the Rust version also merges features, deduplicates vertices and updates the
  transform — so the demo does not reimplement it; the demo simply does not offer whole-
  file merging. Porting it later is an additive, independently testable utility.
- **Byte-identical output vs Rust for FlatBuffers sections.** Golden comparisons are on
  parsed JSON trees, never strings.

---

## Reference material — read before writing any code

1. **`.llm/docs/specification.md`** — the byte-level "format reference" (merged in
   from the retired native C++ plan): every constant, formula and byte offset, each
   cited to the Rust source line that proves it. File layout, feature framing, packed
   R-tree, attribute B+tree, all seven key encodings, operator lowering, HTTP
   constants, and the per-object attribute schema rule. **Cite it from tests instead
   of re-deriving anything.** This document does not duplicate it.
2. **Same document, "Known divergences from the Rust reader"** — four deliberate
   choices (`Byte` decodes as `u8`; `Json`/`Binary` index queries rejected; float
   `max_value()` is `+inf` so NaN-keyed features are invisible to range queries;
   `DateTime` `min_value()` is epoch 0 so pre-1970 timestamps are invisible to `Lt`,
   `Le` and `Ne`). **TypeScript makes the same four choices**, matching C++ and Python
   — Rust's *reader* still differs on the first (it decodes `Byte` as `i8`, which the
   writer never produces) — and documents them in the public docstring of the query API
   as C++ and Python do.
3. **`docs/upstream-findings.md`** — defects found during the C++ port. Three matter
   directly here:
   - **#5, `Gt`/`Lt`/`Ne` can drop genuine matches — NOT FIXED upstream.** The Format
     Reference's "operator lowering" row describes Rust's lowering: `Gt` is
     `find_range(k, MAX)` *minus* `find_exact(k)`, and the subtraction operates on
     feature offsets. A feature whose CityObjects carry both `k` and `k' > k` is
     returned by the range scan via `k'` and also by `find_exact(k)` via `k`, so the
     subtraction deletes a genuine match. **TypeScript must NOT port this lowering.** It
     follows C++ instead: evaluate strict-or-inclusive bounds at the leaf, one
     traversal, no subtraction (`docs/upstream-findings.md:130-145`). This is the one
     place where the Format Reference documents the reference implementation faithfully
     and the reference implementation is wrong.
   - **#7 and #8:** appearance indices must serialize as `1`/`null`, never `[1]`/`[]`,
     and two appearance shapes used to lose a nesting level. The corpus encodes the
     correct answers.
4. **The native Python port** — the closest prior port in spirit (plan retired after
   shipping; see git history under `docs/superpowers/plans/`). Its task-by-task
   structure is the model for this plan.

### The single most important lesson from the C++ port

Both round-trip bugs in finding #8 survived because every test compared the new reader
against the reference reader's *output*, and both agreed on the wrong answer.
**Comparing against the corpus is necessary but not sufficient.** Where TypeScript
decodes something the corpus does not exercise, write a round-trip test that goes
through the Rust *writer*.

### The oracle technique

Do not hand-derive expected values. When you need to know what the reference does for
some input, make the reference tell you: temporarily add a test to the Rust source that
prints the actual output for each case, run it, pin those values in the TS tests, revert
the injection. This caught a wrong hand-derivation during the C++ port.
`src/rust/fcb_core/src/reader/geom_decoder.rs` is where the appearance decoders live.

---

## Scope

| Capability | In | Notes |
|---|---|---|
| Header parsing → `FileInfo` | ✅ | |
| Sequential scan | ✅ | |
| Per-object attribute decoding | ✅ | Schema resolution is **per object**, not per file |
| CityJSON + CityJSONFeature emission | ✅ | Whole-line conformance against the shared corpus |
| Packed R-tree `bbox` | ✅ | |
| Packed R-tree `pointIntersects` | ✅ | Degenerate bbox |
| Packed R-tree `pointNearest` | ✅ | No prior port exists; isolated as the last task |
| Attribute B+tree queries | ✅ | Highest-risk task; severable. Includes the **mandatory string post-filter** below |
| `limit` / `offset` pagination | ✅ | `featuresCount` still reports total matches |
| HTTP range via `fetch` | ✅ | |
| Browser `File` / `Blob` | ✅ | New — the wasm binding never supported local files |
| `ArrayBuffer` / `Uint8Array` | ✅ | |
| `node:fs` | ✅ | Separate `./node` subpath export |
| `AbortSignal` cancellation | ✅ | New — the wasm binding has no cancellation at all |
| Writing `.fcb` | ❌ | |
| `cjToObj`, `cjseqToCj` | ❌ | Dropped |

Breaking API changes relative to the wasm package are explicitly allowed.

### Attribute queries return candidates, not answers

A `String` (or `Json`/`Binary`) index stores a **fixed-width truncated, zero-padded**
key, so distinct values collide — and not only long ones: `"a"` and `"a\0"` have
identical index representations. The B+tree therefore yields a **candidate set**, and
every `String`-keyed predicate requires a **post-filter** that decodes each candidate's
full, untruncated attribute and re-evaluates the predicate against it, existentially
over the feature's CityObjects. C++ does exactly this and says so
(`src/cpp/src/reader.cpp:394-412`); Python needed the same fix.

Two consequences the design must respect: the post-filter is **not gated on query
length**, and it must run **before** `featuresCount` and pagination, or both report
candidate counts rather than match counts. Full-string comparison during post-filtering
uses UTF-8 byte ordering, not JavaScript string ordering (hazard 7).

### Trust model

**Input `.fcb` files are trusted.** The framing is bounds-checked — magic, header size,
section offsets, feature length prefixes, a maximum feature size, and checked arithmetic
throughout — but there is no FlatBuffers verifier in JavaScript (hazard 2), so a
malformed or hostile file can still drive a generated accessor to read a nested table,
vtable or vector offset that points outside its section, and the result may be a throw,
a plausible default, or garbage. Length checks are **not** a substitute for
verification, and this document does not pretend otherwise.

This is a deliberate, documented limitation, stated in the README. Porting a
schema-aware structural verifier is a well-defined additive task if a consumer ever
needs to read untrusted URLs; it is out of scope for v1.

---

## Architecture

A layered reader mirroring `src/py` and `src/cpp`, with one JS-forced difference:
**every I/O path is async**, so the R-tree and B+tree traversals are `async` functions
rather than the synchronous loops the other three ports use. Everything after bytes
arrive is synchronous — the async boundary is exactly the `RangeReader` seam, which is
the same sans-IO cut the C++ port made for batching.

### Package layout

```
src/ts/
  package.json            # @cityjson/flatcitybuf, ESM, exports "." and "./node"
  vite.config.ts          # library mode build + vitest config
  tsconfig.json
  README.md               # includes a migration section from the wasm API
  src/
    index.ts              # public API surface only
    errors.ts             # FcbError + ErrorCode (mirrors src/cpp/include/fcb/error.hpp)
    le.ts                 # little-endian DataView wrappers (see hazard 10)
    layout.ts             # magic, header size, section offsets, bounds validation

    io/
      range-reader.ts     # RangeReader interface, BufferedRangeReader
      bytes.ts            # ArrayBuffer / Uint8Array
      blob.ts             # browser File / Blob
      fetch.ts            # HTTP Range: batching, coalescing, AbortSignal
      node.ts             # node:fs — reachable only via "./node"

    header/
      index.ts            # readHeader -> HeaderView
      file-info.ts        # FileInfo, ColumnInfo, AttrIndexInfo
      attribute-index.ts  # the 16-byte AttributeIndex struct decode

    feature/
      index.ts            # feature framing, sequential + offset-driven scan
      attribute.ts        # attribute blob decode against a per-object schema

    geometry/
      index.ts            # geometry -> CityJSON boundaries
      boundaries.ts       # solids/shells/surfaces/strings nesting rules
      semantics.ts        # semantic surfaces + u32::MAX null sentinel
      appearance.ts       # material / texture mapping decode

    cityjson/
      index.ts            # CityJSON + CityJSONFeature emission
      types.ts            # CityJSON TypeScript types

    packed-rtree/
      index.ts            # search entry points
      node-item.ts        # 40-byte NodeItem decode, bbox intersection
      search.ts           # bbox + pointIntersects streaming descent
      nearest.ts          # pointNearest priority-queue traversal

    static-btree/
      index.ts            # search entry points
      key.ts              # 7 encodings + ordered_float comparator + sentinels
      entry.ts            # Entry<K> layout, node/level-bounds arithmetic
      payload.ts          # PAYLOAD_TAG, payload section decode
      stree.ts            # find_exact / find_partition / find_range descent
      query.ts            # operator lowering, multi-condition AND intersect

    reader.ts             # FcbReader facade: select(), pagination, iteration
    generated/            # flatc --ts output, committed
  test/                   # vitest: unit + conformance + browser-mode
examples/web/             # Vite demo app against the new API
```

Naming is kebab-case (TS norm). Each `index.ts` is its module's public face; siblings
are internals. A one-line comment at the top of each `index.ts` names the Rust module it
ports from, so cross-referencing the four implementations stays mechanical.

`io/` is the only runtime-specific code. `node.ts` is reachable only through the
`./node` subpath export, so a browser bundle never resolves `node:fs`. Everything above
it sees one `RangeReader` interface.

`packed-rtree/nearest.ts` is its own file because it is the one algorithm with no Python
or C++ port to copy from.

---

## Public API

```ts
// @cityjson/flatcitybuf
const reader = await FcbReader.fromUrl('https://.../delft.fcb', { signal })
const reader = await FcbReader.fromBlob(fileInput.files[0])
const reader = await FcbReader.fromBytes(uint8array)
import { fromFile } from '@cityjson/flatcitybuf/node'   // node:fs

reader.header           // FileInfo: featuresCount, transform, extent, crs, columns…
reader.cityjson()       // the CityJSON metadata object (line 1 of a CityJSONSeq)

const cursor = await reader.select({
  spatial: { kind: 'bbox', value: [minX, minY, maxX, maxY] },
  where:   [['b3_h_dak_50p', 'Gt', 2.0]],   // AND-intersected
  limit: 100, offset: 200,
  signal,                                    // cancels in-flight ranges
})

cursor.featuresCount    // exact total matches, ignoring limit/offset; 0 means none

for await (const f of cursor) {
  f.id
  f.cityObjects()       // lazy per-object handles
  f.attributes(i)       // object i's attributes, decoded on demand, own schema
  f.toCityJSON()        // full CityJSONFeature, decoded on demand
}
```

### API decisions

- **`select()` is one method, not five,** and it is **`async`**. `select_all` /
  `select_spatial` / `select_attr_query` and their `_paged` twins collapse into one
  options object; omitting every field is a full scan. It must be awaited because
  resolving the hit list — index traversal, string post-filtering, spatial∩attribute
  intersection — is asynchronous, and `featuresCount` is meaningless until that is done.
  A property that starts `undefined` and mutates during iteration would be
  timing-dependent and is explicitly rejected. **An empty result reports `0`,** not
  `undefined`; Rust's `count > 0 ? Some(count) : None` is a bug, not precedent.
- **The spatial predicate is a discriminated union,** so `bbox` / `point` / `nearest`
  are mutually exclusive at the type level rather than by convention.
- **`nearest` combined with `where` is rejected** in v1 with an `FcbError`, at the type
  level and at runtime. "Nearest feature satisfying the predicate" and "the nearest
  feature, then filtered" are different algorithms with different costs; neither is
  silently assumed. Either can be added later without breaking the API.
- **The cursor is a plain `AsyncIterable`,** so `for await` works and early `break`
  cancels cleanly. No `next()` returning `undefined` as a sentinel, no `free()`, no
  `Symbol.dispose`. That entire category of wasm API disappears.
- **The cursor yields a `Feature` handle that is durable and immutable.** It owns a
  private copy of the feature's bytes (see hazard 9), so advancing the cursor does not
  invalidate it and it may be retained, stored or decoded later. Decoding is lazy —
  `.attributes(i)` and `.toCityJSON()` each decode on demand — so an app that filters
  10,000 features by attribute and renders 50 never decodes 9,950 geometries, but there
  are no lifetime rules to get wrong. (Generation-checked invalidation was considered
  and rejected: it adds machinery, surprises users, and cannot work anyway because
  already-decoded values necessarily outlive the handle.)
- **Attributes are per CityObject, not per feature.** `CityFeature` holds `objects`, and
  each `CityObject` carries its own `attributes` blob and optional `columns` override —
  which is the normal case, not an edge case. The API therefore exposes
  `feature.cityObjects()` handles with `attributes()` on each, and
  `feature.attributes(i)` as shorthand. There is no feature-level `attributes()`,
  because there is no such thing on the wire. **`Header.semantic_columns` and
  semantic-object attribute blobs are a separate decode path** and are decoded with the
  semantics they belong to.
- **`AttrValue = number | bigint | string | boolean | Uint8Array | JsonValue | null`.**
  Covering every wire type the reference decodes: `Json` columns yield parsed JSON
  values, `Binary` columns yield `Uint8Array`, and **`DateTime` yields an ISO-8601
  string, not a `Date`** — matching Rust, and because `Date` cannot represent the key's
  sub-second nanoseconds. Query values for `DateTime` conditions take a separate exact
  representation (seconds + nanos, or an ISO string) for the same reason.
- **64-bit integer policy:** `Long`/`ULong` attribute values **always** decode to
  `bigint` — never data-dependent, never lossy. `toCityJSON()` takes an explicit
  `int64` policy (`'lossy-number'` by default, so emitted CityJSON is always
  JSON-serializable and conformance comparison works; `'decimal-string'` and `'error'`
  also available). Query values accept `bigint` or a safe-integer `number`; an unsafe
  `number` is rejected rather than silently rounded. `featuresCount` is `number` after a
  guarded conversion. BigInt is used internally wherever it is load-bearing (hazard 3).
- **`features_count == 0` in the header means *unknown*, not empty.** A sequential scan
  must run to EOF rather than stopping at zero, as the C++ reader does.
- **Errors are `FcbError extends Error` with a `code` field.** The taxonomy is designed
  for TypeScript rather than copied: it starts from the C++ `ErrorCode` set and adds the
  failure modes only this port has (`RangeNotSupported`, `RangeHeadersNotExposed`,
  `UnsupportedQueryCombination`, `ReentrantIteration`). The C++ and Python sets are not
  identical to each other either, so promising cross-implementation identity would be
  false.
- **`fromUrl` is `async`** because opening prefetches magic + header + the top R-tree
  levels in one request, as the Rust HTTP reader does.
- **`fromBytes` copies the supplied bytes.** Otherwise later mutation or `ArrayBuffer`
  detachment (a `postMessage` transfer, a growing `WebAssembly.Memory`) silently
  corrupts an open reader.
- **Input validation at the boundary:** `limit` and `offset` must be non-negative safe
  integers; bbox coordinates must be finite and non-inverted; `RangeReader.read` offsets
  and lengths must be non-negative integers within `size()`. Rejected with `FcbError`,
  not coerced.

---

## Technology

| Concern | Choice |
|---|---|
| Language / module format | TypeScript, ESM only |
| Build | Vite 8.1.5, library mode |
| Test | Vitest 5.0.0-beta.6, including browser mode (Playwright provider) |
| FlatBuffers tables | `flatc --ts` generated bindings, committed; `flatbuffers` npm as the sole runtime dependency |
| Hand-serialized sections (R-tree, B+tree, payload, attribute blobs) | `DataView` directly, never FlatBuffers |
| Demo | Vite app at `examples/web/` |

The Vitest beta is test-only and cannot affect the published package.

---

## JS/TS hazards that will bite

These have no C++ or Python analogue; nothing in the existing ports warns about them.
Each was verified empirically (fresh `flatc 25.9.23` output, `flatbuffers@25.9.23`
inspected from npm, numeric claims run in Node/V8).

1. **`flatc --ts` with default flags silently omits `class Header`, and includes are
   not generated. The required invocation is `flatc --ts --ts-omit-entrypoint
   --gen-all`.** Because the schema file is `header.fbs` and the root table is `Header`,
   the per-namespace entry-point re-export file and the table's class file both map to
   `header.ts`; the entry point wins and the output contains **zero** occurrences of
   `class Header` — it contains `export { Header } from './header.js'`, a circular
   self-import. Separately, without `--gen-all` the emitted `header.ts` imports
   `./extension.js`, which is never generated (`extension.fbs` is only an `include`), so
   the package does not compile. Neither failure is an error at generation time. Pin a
   test that imports `Header` and `CityFeature` and type-checks the generated tree, and
   put the exact flag set in the generation script header.

2. **There is no FlatBuffers verifier in JavaScript.** `grep -ri verif` over
   `flatbuffers@25.9.23` returns nothing; the runtime ships only `ByteBuffer`, `Builder`
   and flexbuffers. Rust's `size_prefixed_root_as_header` verifies; Python's runtime
   bounds-checks vector reads; JS does neither — reading past a `Uint8Array` returns
   `undefined`, which propagates as `NaN` through the offset arithmetic in
   `byte-buffer.js`. A truncated or hostile buffer produces silent garbage or bizarre
   secondary exceptions, never a clean error. Consequences: all section-bound and
   length-prefix validation is the *only* line of defense and must run **before** any
   generated accessor; enforce a max feature size before allocating; validate that the
   4-byte feature prefix and the header size land inside `size()` before constructing a
   `ByteBuffer`. This posture belongs in `layout.ts` and `feature/index.ts` from the
   start — retrofitting it later is a diff across the whole codebase. **But framing
   checks are not verification** and must not be described as if they were: see "Trust
   model" above for what this design does and does not promise.

3. **64-bit values: `Number` is safe for file positions; BigInt is mandatory for B+tree
   entry offsets until the payload tag is stripped, and for `Long`/`ULong` key
   comparison.**
   - Generated accessors return `bigint` for u64/i64 table fields. Our schemas contain
     exactly one: `Header.features_count`. Convert once with a
     `<= Number.MAX_SAFE_INTEGER` guard.
   - R-tree `NodeItem.offset`: `getBigUint64(o, true)`, guard, convert to `number`.
     Both interpretations (feature byte offset, child node index) are file-bounded, and
     2^53 bytes is 9 PB. Keeping them BigInt only poisons downstream arithmetic.
   - **B+tree `Entry.offset`: BigInt is mandatory.** `PAYLOAD_TAG = 1u64 << 63` is a real
     bit in the wire data, so a tagged offset is ≥ 2^63 and `Number()` of it rounds —
     verified: `Number((1n<<63n)|12345n) === Number(1n<<63n)` is `true`, the 12345 is
     destroyed. Test and strip the tag in BigInt, *then* convert the low 63 bits.
     Write the tag as the literal `0x8000000000000000n`, never as a shift: `1 << 63` in
     JS is `1 << (63 & 31)` = `-2147483648`, verified.
   - `Long`/`ULong` B+tree keys and their `i64::MAX` / `u64::MAX` range sentinels are not
     representable as `number`, and operator lowering builds ranges against exactly those
     sentinels — so `Long`/`ULong` key decode and comparison stay BigInt end-to-end.
     `DateTime` sentinels (epoch 0, year 9999) convert exactly, so `DateTime` may use
     `number` seconds + `number` nanos after a guard.

4. **BigInt does not serialize.** `JSON.stringify({a: 1n})` throws. Handled by the
   number-when-safe policy above. Conformance consequence to record in the test file:
   `JSON.parse` of the expected `.jsonl` corpus loses the same precision on the
   *expected* side, so a JS conformance test structurally cannot detect a divergence
   above 2^53. If a corpus case with a >2^53 `Long` is ever added, its TS assertion must
   compare raw token text, not parsed values.

5. **Optional scalars: the Python port's gotcha does NOT apply here — but falsiness
   does.** Verified against generated code: every `= null` scalar field produces a
   `| null` accessor that probes the vtable itself (`MaterialMapping.value`,
   `SemanticObject.parent`, `Texture.wrapMode`, `Texture.textureType`,
   `Material.ambientIntensity` / `shininess` / `transparency` / `isSmooth`). No manual
   vtable probing is needed. The residual hazard is that `0` is falsy: `if
   (mapping.value())` silently drops shared-material 0 — the exact case the Python plan
   calls out — as does `value() || fallback`. Every presence check must be `!== null`,
   and each affected field gets a test pinning the zero-vs-absent distinction.
   Related trap: *vector element* accessors return `0`, not `null`, when the vector is
   absent; vector presence is `…Length() > 0` or `…Array() !== null`, never an element
   read.

6. **`u32::MAX` sentinels vs 32-bit signed bitwise operators.** Every JS bitwise
   operator coerces to *signed* 32-bit: `4294967295 | 0 === -1`, `~4294967295 === 0`,
   both verified. The runtime is careful (`readUint32` is `readInt32(...) >>> 0`), so
   generated accessors return `4294967295` correctly — the hazard is entirely in our
   hand-written code. Sentinel checks are `x === 0xFFFFFFFF` on the accessor's return
   value; never normalize with `|0`, never test with `~x`, never use `<<`/`>>` on
   offsets that can reach 2 GB. The one upstream function full of u32 bit-twiddling,
   `hilbert()`, is writer-only and must not be ported.

7. **String keys compare as bytes; JS string `<` is UTF-16 code-unit order and
   disagrees with the on-disk UTF-8 byte order.** Verified: for U+FF61 vs U+10000, JS
   says `"｡" < "\u{10000}"` is `false` while the UTF-8 byte comparison — the order the
   B+tree is built in — says the opposite. A comparator written on decoded strings
   returns wrong partitions for non-BMP content **and passes every ASCII test**. Posture:
   encode query values with `TextEncoder`, truncate and zero-pad at the byte level to
   50/100, compare `Uint8Array`s lexicographically. `TextDecoder` (default non-fatal,
   U+FFFD replacement, matching Python's `errors="replace"`) is for display only, never
   for logic. `Buffer.compare` does not exist in browsers.

8. **Float total order: neither `===` nor `Object.is` implements it.** Required order is
   `ordered_float`: NaN sorts greatest, NaN == NaN, −0.0 == +0.0. Verified JS facts:
   `NaN === NaN` is false (wrong for equality); `Object.is(-0, 0)` is false (wrong —
   ordered_float says equal); `-0 === 0` is true (right). The comparator special-cases
   NaN explicitly (both NaN → 0; one NaN → that side greater) then uses plain `<`/`>`.
   DataView NaN-payload normalization was investigated and refuted as a concern: keys
   are compared after decode, and this reader never re-encodes a decoded float. Keep it
   that way — never implement `find_exact` for floats by re-encoding the query key and
   comparing bytes.

9. **Typed-array views have alignment and aliasing semantics `DataView` does not, and
   both bite through the *generated* accessors.** Verified: `new Uint32Array(buffer, 2,
   …)` throws `RangeError`; `new DataView(buffer, 2)` is fine.
   - *Hand-parsed sections:* the R-tree begins at `8 + 4 + header_size` — arbitrary
     alignment, no padding anywhere in the format — and B+tree entries are `K + 8` bytes
     (58 for `StringKey50`). Use `DataView` for all hand-parsed decoding; construct
     typed-array views only over buffers we allocated ourselves at offset 0.
   - *Generated accessors:* `solidsArray()`, `borderColorArray()` etc. construct views
     directly over the feature's backing `ArrayBuffer`. If a feature is handed to the
     `ByteBuffer` as a `subarray` of a larger batch fetch, these throw `RangeError` for
     some features and not others depending on where in the batch each landed — and when
     they don't throw they *alias* the batch buffer, so retaining one small geometry
     array pins the whole multi-MB batch against GC.
   - **Rule: copy each feature's size-prefixed bytes into a fresh `ArrayBuffer` at offset
     0** (which is also what the wasm binding deliberately does today), and let batch
     buffers die. **Copy features; never copy sections.**

10. **`DataView` getters default to BIG-endian when the flag is omitted.** Verified.
    Every hand-serialized structure in this format is little-endian, so a single
    forgotten `, true` yields plausible-looking garbage — a byteswapped f64 bbox is
    still a finite f64. Mitigated structurally by `le.ts`, a tiny module wrapping
    `getF64` / `getU64` / `getU32` / `getU16` with the flag baked in; raw `DataView`
    method calls do not appear elsewhere. The `flatbuffers` runtime needs no
    help — it reads byte-wise LE.

11. **The wasm binding being replaced has JS-boundary bugs. Do not port them as
    "reference behaviour"; file them as upstream findings.**
    - *Every JS number becomes a `Float64` key* (`wasm/src/lib.rs:1110-1112`), and the
      typed index rejects mismatched key variants — so attribute queries against `Int`,
      `Float`, `Long`, … columns **fail today** from the browser; only `Double`, `Bool`,
      `String` and `DateTime` columns work. TS must look up `Column.type` first and
      encode the query value as that column's key type.
    - *String query values >50 bytes are routed into a `StringKey100`*
      (`wasm/src/lib.rs:1114-1118`), but the writer only ever produces
      `FixedStringKey<50>` for String columns, so such a query errors instead of
      truncating to 50 bytes as Rust's native reader does. TS always encodes String-column
      keys as 50-byte truncated keys.
    - *`index_node_size` from the header is ignored on the HTTP path*
      (`wasm/src/lib.rs:275`, `fcb_core/src/http_reader/mod.rs:220` both pass the default
      16), so any file written with a non-default node size is silently mis-traversed
      over HTTP. TS uses the header value.
    - *The gloo client accepts a `200` with the full body as if it were the requested
      range* (`wasm/src/gloo_client.rs:29-44`) — silent corruption of every subsequent
      offset. See the HTTP section below.

---

## Async design

### What becomes async

No traversal in this codebase is recursive, and every one already reads at the top of a
queue-driven loop — so they port to `async`/`await` mechanically:

- **R-tree bbox / pointIntersects:** `while (queue.length) { const bytes = await
  reader.read(...); … }`. The read is already hoisted to once-per-queue-item, which is
  the right granularity.
- **R-tree pointNearest:** same, with a priority queue. See below.
- **B+tree find_exact / find_partition / find_range:** one node-range read per level,
  binary search within the fetched entries (pure CPU between reads), one payload
  prefetch up front, one batched payload resolve at the end.
- **Sequential scan:** two awaits per feature (4-byte prefix, then body), served from a
  1 MB buffered fetch so there is roughly one physical request per MB.
- **Everything after bytes arrive is synchronous.**

### The `RangeReader` interface

```ts
interface RangeReader {
  read(offset: number, length: number, opts?: { signal?: AbortSignal }): Promise<Uint8Array>
  size(): number                       // resolved once at open, then synchronous
}
```

`size()` is deliberately **not** a promise: HTTP learns it from `Content-Range` on the
first 206, files from `stat`, Blobs have `.size` synchronously, and the only thing that
needs it is sizing the last feature. Making it per-call async would infect every bounds
check for nothing.

`read` returns **exactly** `length` bytes or throws — a short read is an error, never a
silently truncated buffer — and validates that offset and length are non-negative
integers inside `size()`.

A `readBatch` primitive was considered and **deferred**: no traversal in this design
issues one, and adding a method today for a hypothetical multipart-range adapter is
speculative. It can be added without breaking the interface if the request-log
benchmarks show it is needed.

**No synchronous fast path.** `Blob`/`File` reads are irreducibly async, so a sync path
would cover only the fully-in-memory case; `await` on an already-resolved promise is a
microtask, not I/O; and a dual sync/async API doubles every traversal. If the benchmark
later shows the in-memory case matters, add an internal `tryReadSync(): Uint8Array |
null` that only `BufferedRangeReader` consults inside hot loops — an optimization
detail, not API.

### Request batching

The batching *rules* live in the traversal code, not in the buffer — porting a buffered
reader alone does not reproduce the request pattern. The TS design reproduces the
two-layer structure: a thin `BufferedRangeReader` (over-fetch to a minimum request size,
serve subsequent hits from the buffer) plus explicit range planning in the traversals.
All constants come from the Format Reference's "HTTP constants" table: 1 MB default
fetch, 12,944-byte open prefetch, 256 KB spatial / 1 MB attribute combine thresholds,
the `wasted = next.start − prev_end < threshold` batching rule, and the payload prefetch
clamp.

**Request-log assertions are part of each HTTP task's definition of done**, not a later
optimization task. A counting fake `RangeReader` records every underlying read; without
those assertions the reader can be correct but 50× chattier and nobody notices until it
is on a real CDN.

### Concurrency and failure modes

- **Per-query buffered readers over one shared immutable source.** Concurrent queries on
  one `FcbReader` are supported: each `select()` creates its own `BufferedRangeReader`
  over the shared source, so no two traversals contend for one buffer. This is what the
  C++ port does (`src/cpp/src/reader.cpp:245`, `:439`) and it maps cleanly to JS. Rust's
  alternative — `select_*` *consumes* the reader — was rejected: a cursor that is created
  and never iterated would hold the lock indefinitely, which in a UI is a leak with no
  diagnostic. (Promise-sharing by exact `offset:length` key was also considered and
  rejected: it is ineffective for overlapping-but-unequal ranges unless physical fetches
  are aligned to a block grid.)
- **Single-consumer iterators.** Two overlapping `next()` calls on one cursor would
  interleave their position updates — a hazard Rust cannot represent (`&mut self`).
  The cursor holds the in-flight promise and **throws `FcbError` on a re-entrant
  `next()`** rather than serializing silently: a caller doing this has a bug, and
  queueing would hide it behind unbounded memory growth. Pinned by a test that calls
  `next()` twice without awaiting.
- **Cancellation.** One `AbortController` per query; dropping an async iterator (its
  `return()`) aborts in-flight fetches. Without this a pan/zoom-driven map UI — the
  actual consumer of the wasm module today — leaks a queue of 1 MB fetches per gesture.
  The wasm binding has no cancellation at all; this is a required improvement, not
  parity.
- **Server ignores `Range` and returns 200.** Require `status === 206` whenever a
  `Range` header was sent. On 200, immediately `controller.abort()` — do *not* await
  `arrayBuffer()` of a possibly-10 GB body — and throw a descriptive error suggesting
  `fromBytes(await (await fetch(url)).arrayBuffer())` for files the caller knows are
  small. The wasm client's silent acceptance here is a bug, not a behaviour to match.
- **Validate the whole `Content-Range`, not just its presence:** the returned start and
  end must equal what was requested, the total must be consistent across responses, and
  the body length must match the range. A proxy that serves a *different* range than
  requested is otherwise indistinguishable from a correct response, and every subsequent
  offset is silently wrong.
- **CORS header visibility.** Cross-origin, `Content-Range` and `Content-Length` are
  readable only if the server sends `Access-Control-Expose-Headers` naming them. A 206
  with an invisible `Content-Range` means `size()` cannot be learned at open — detect it
  and fail with an error that *names the missing server header*. Never guess.

---

## pointNearest

The one algorithm with no Python or C++ port to copy. Rust has it in three forms
(in-memory, seekable-stream, HTTP) sharing one algorithm.

- **Two distance metrics, mixed deliberately.** Internal nodes are ordered and pruned by
  *min-distance* (squared Euclidean from the query point to the nearest point of the
  node's bbox, 0 if inside); a leaf's final score is its *centroid* distance. Both are
  squared — no `sqrt` anywhere.
- **Why the mix is sound, and why it must not be "fixed":** a child's bbox is contained
  in its parent's and a leaf's centroid lies inside its own bbox, so the internal-node
  key is an admissible lower bound for the leaf scoring metric. The search is exact *for
  the nearest-centroid problem*. It is **not** nearest-feature-geometry. Substituting
  leaf min-distance for centroid distance diverges from all three Rust forms.
- **Traversal:** best-first over a min-heap seeded with the root at distance 0. Pop the
  smallest; if its distance is strictly greater than the current best, terminate. Skip
  nodes whose min-distance is `>= best`. Leaves replace the best only on a *strict*
  improvement, so on exact ties the first-reached leaf wins. Internal nodes push their
  child range keyed by the *parent's* min-distance. Result is at most one item.
- **Tie-breaking is unspecified upstream** — ordering is by distance alone and equal
  entries pop in arbitrary heap order. A JS binary heap will have a different but equally
  valid order. The conformance test asserts *distance*, not identity, on constructed
  ties, rather than trying to replicate Rust's heap internals.
- **Cost of a naive async port:** one serial round trip per heap pop, nothing
  pipelineable. Typically 5-15; degenerate cases (query point far outside the extent,
  heavily overlapping bboxes) can pop hundreds of ranges serially.
- **Design, v1:** port the exact serial Rust algorithm, plus a **whole-index fast path**.
  `rtree_size` is known before any traversal, and for `delft.fcb` the entire R-tree is
  47,640 bytes — one request, well under the 256 KB combine threshold. Below that
  threshold, fetch the whole index and run the in-memory algorithm with zero further
  index I/O. This makes `pointNearest` *cheaper* than bbox for most real files and lets
  the in-memory port be exercised by the same unit tests as the streaming one.
- **Deferred:** wave batching above the threshold — draining every heap entry below the
  current best, coalescing their ranges with the wasted-bytes merge rule the Rust nearest
  branch never got, and issuing them concurrently. Admissibility would be unaffected
  (processing a superset of the minimal node set can only tighten `best` sooner) and it
  would collapse round trips to O(depth) waves, but it is an optimization for files above
  ~6,100 features and should be justified by a request-log benchmark before it is built.

---

## Verification

**Oracle hierarchy, strongest first:**

1. **The shared conformance corpus** (`conformance/*.expected.jsonl`, generated by the
   Rust CLI). Nine cases: `small`, `geom_temp`, `noise_extension`, `single_feature`,
   `long_strings`, `duplicate_keys`, `degenerate_extent`, `inferable_types`,
   `empty_appearance`. Compare **whole parsed lines**, never selected keys — comparing
   selected keys is precisely what hid a missing per-feature `appearance` object through
   the entire C++ port.
2. **Cross-implementation query agreement.** Spatial and attribute queries have no
   `.expected.jsonl`, so the C++ binary and the Python reader run the same query and the
   result *sets* are compared.
3. **The oracle technique** for unit-level expected values (above).

**What the corpus cannot cover, and therefore gets dedicated tests:**

- **Browser-only paths.** Vitest browser mode runs the `fetch` range reader against
  `src/cpp/tests/range_server.py` — which already exists as a range-capable test server,
  reused rather than rewritten (it likely needs `Access-Control-Expose-Headers:
  Content-Range` added for cross-origin use) — plus the `Blob` reader against a real
  `File`. Node-only tests would leave the actual shipping target untested. **No core
  task may depend on browser mode:** everything passes in Node first, browser runs are an
  additive CI job.
- **HTTP misbehaviour:** 200-instead-of-206, a `Content-Range` that does not match the
  request, invisible CORS headers, short bodies, `AbortSignal` firing mid-descent.
- **Malformed-framing cases**, each asserting a clean `FcbError` rather than a crash or
  silent garbage: header size beyond the file, a feature length prefix that overruns the
  feature section, a feature offset outside it, an attribute-index section shorter than
  its declared `length`, duplicate or out-of-order `AttributeIndex` declarations, a
  branching factor outside `[2, 65535]`, and arithmetic that would overflow a safe
  integer. These are framing checks, not verification — see "Trust model".
- **Request counts:** the batching assertions described above.

**Process conventions**, same as the previous three ports: strict TDD (write the failing
test, run it, confirm it fails *for the expected reason*, implement, confirm green,
commit); Fable (`Agent` with `model: "fable"`) for hard analytical passes, given one
narrow question, forbidden from changing behaviour, and required to produce evidence
rather than conclusions; `codex exec --model gpt-5.6-sol --sandbox read-only` review
before closing each stage; commit after every task; never leave a red suite.

---

## Staging

Ordering principle: the hardest, least-precedented work goes late enough that a slip
still leaves a shippable reader.

| Stage | Content | Ships if everything after it slips |
|---|---|---|
| **A. Foundations** | package + Vite/Vitest + error taxonomy; generated bindings with the verified flag set; `le.ts`; layout and bounds validation; `RangeReader` + buffered + bytes/blob/node sources + counting fake | nothing user-facing |
| **B. Read a file** | header → `FileInfo`; feature framing + sequential scan; per-object attribute decode (BigInt policy lands here) | a local-file / Blob scanner |
| **C. Emit CityJSON** | boundaries, semantics, appearance; CityJSON + CityJSONFeature emission; **conformance suite green** | a conformant local reader — the real milestone |
| **D. Go remote** | `fetch` source: 206/`Content-Range`/CORS/abort handling, open prefetch, batching | a remote sequential reader — already replaces the wasm module's `select_all` use case |
| **E1. Spatial query** | packed R-tree bbox + pointIntersects (in-memory *and* streaming) with request-log assertions; pagination | a spatial reader |
| **E2. Attribute candidates** | keys module (encodings, BigInt comparators, sentinels, byte-wise string compare); B+tree traversal with **C++'s strict-bound lowering, not Rust's subtraction**; the four deliberate divergences | attribute queries over non-string columns |
| **E3. Post-filter and composition** | full-value post-filtering of string-keyed predicates; spatial ∩ attribute composition; exact `featuresCount` and pagination *after* post-filtering | full wasm parity minus nearest |
| **F. The risky one** | `pointNearest` | — |
| **G. Retire** | port the demo to `examples/web`; delete `src/rust/wasm`; repo-wide sweep (below) | the actual goal |

Notes:

- **Conformance lands at C, before any networking**, so decoding bugs and transport bugs
  are never debugged simultaneously. This is the opposite of how the wasm binding grew.
- **Stage E is split into three** because the original single stage was a dependency
  inversion: it bundled spatial traversal, key codecs, B+tree traversal, post-filtering,
  composition, exact counts and pagination while claiming a B+tree slip "cuts nothing
  else" — untrue if pagination and counting live in the same stage. As split, a slip in
  E2 costs only attribute queries, and a slip in E3 costs only string-column predicates
  and combined queries. E3 depends on complete attribute decoding from B, which is why B
  precedes it.
- **The keys module is pure functions**, fully unit-testable without I/O, so E2 front-loads
  its own risk.
- **`pointNearest` is isolated and last.** If it slips, E has already shipped everything
  the current wasm package offers except nearest-point search.
- **Semantic-object attribute decoding belongs with semantics in stage C**, not with
  final polish — it is a decode path, not a query feature.
- **Deletion is last**, mirroring Task 13 of the Python plan — the wasm build keeps
  working for consumers until the replacement passes conformance and the demo runs in a
  real browser. Retirement is a repo-wide sweep, not a directory delete: the Rust
  workspace member and its `--exclude fcb_wasm` guards throughout the `justfile`,
  `scripts/build_wasm.sh`, `package.json` `files`/`exports`/`main`/`types`, `.gitignore`,
  `publish-npm.yml`, `ci.yml`, `examples/`, `README.md`, `CONTRIBUTING.md` and
  `.llm/docs/projectStructure.md`. Note `src/ts/` tracks only `.gitignore` and
  `package.json` — the `.wasm`/`.js`/`.d.ts` artifacts are gitignored and built at
  publish time, so there are no checked-in binaries to remove. Acceptance is a clean
  `cargo build --workspace`, a clean package build and `npm pack`, and the demo running
  in a real browser.

## Dependencies and risks

- **The shared conformance corpus** must be at `conformance/` in the repo root. That
  move is Task 2 of the Python plan and currently lives on the `native-py` branch; this
  work waits on it landing on `develop` rather than duplicating the corpus.
- **`examples/wasm/index.html` imports `cjToObj` and `cjseqToCj`**, both dropped. The
  ported demo at `examples/web/` loses the OBJ-export button and owns a few inline lines
  for feature merging.
- **Vitest 5 is a beta.** Test-only; it cannot affect the published artifact. If it
  proves unstable, falling back to Vitest 4 is a `package.json` edit.
- **A third and fourth independent implementation reading the same bytes is the best
  format-bug detector available.** Anything TS must special-case to match Rust is a
  finding for `docs/upstream-findings.md`, alongside the four wasm defects already
  identified.

## Review history

- **Advisor pass (Fable):** produced `2026-07-21-native-ts-reader-hazards-analysis.md`.
  Contributed the verified `flatc` flag set, the absence of a JS FlatBuffers verifier,
  the BigInt boundary analysis, the `pointNearest` algorithm description, and four
  defects in the wasm binding being replaced.
- **Reviewer pass (codex `gpt-5.6-sol`, read-only):** found two blocking correctness
  defects in the first draft — that it adopted Rust's known-broken `Gt`/`Lt`/`Ne`
  lowering (upstream finding #5) and omitted the mandatory string post-filter — plus the
  `select()`-must-be-async, durable-`Feature`, per-CityObject-attributes, `AttrValue`
  completeness, `features_count == 0`, per-query-buffered-reader and stage-E-split
  corrections. Both blocking claims were independently verified against
  `docs/upstream-findings.md:130-145` and `src/cpp/src/reader.cpp:394-412` before being
  applied. Its recommendation to always use `bigint` for `Long`/`ULong` superseded an
  earlier decision in this design.
