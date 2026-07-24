# Pure-TypeScript FlatCityBuf Reader — Hazard Analysis

Advisor analysis for the planned fourth implementation (TS, ESM, Vite 8, Vitest 5,
`flatc --ts` + the `flatbuffers` npm runtime). Evidence basis:

- Rust reference: `src/rust/fcb_core/src/packed_rtree/mod.rs`, `src/rust/fcb_core/src/http_reader/mod.rs`, `src/rust/fcb_core/src/static_btree/{stree.rs,key.rs,query/http.rs}`, `src/rust/fcb_core/src/reader/attr_query.rs`
- WASM binding being replaced: `src/rust/wasm/src/lib.rs`, `src/rust/wasm/src/gloo_client.rs`
- Python port (worktree `.claude/worktrees/native-py`): `src/py/flatcitybuf/*.py`
- Generated TS: produced fresh in this session with the locally installed `flatc 25.9.23`
  from `src/fbs/{header,feature,geometry}.fbs` (scratchpad `ts-gen*/`)
- Runtime: `flatbuffers@25.9.23` installed from npm in this session and inspected
  (`node_modules/flatbuffers/mjs/{byte-buffer.js,constants.js,utils.js}`)
- Numeric/encoding claims verified empirically in Node (V8) in this session

All file:line citations are to the paths above. Where a claim rests on something I
could not run or inspect, it is marked UNVERIFIED.

---

## SECTION 1 — JS/TS-specific gotchas that will bite

These have no C++ or Python analogue; nothing in the existing ports warns about them.
Style follows the same "gotchas that will bite" call-out used in the (now retired)
native Python port plan.

1. **`flatc --ts` silently deletes `class Header`, and includes are not generated.
   The required invocation is `flatc --ts --ts-omit-entrypoint --gen-all`.**
   Verified with flatc 25.9.23: because the schema file is `header.fbs` and the root
   table is `Header`, both the per-namespace entry-point re-export file and the table's
   class file map to `header.ts`; the entry point wins and the output contains **zero**
   occurrences of `class Header` (it contains `export { Header } from './header.js';`
   — a circular self-import). Separately, without `--gen-all` the emitted `header.ts`
   contains `import { Extension } from './extension.js';` but no `extension.ts` is ever
   generated (`extension.fbs` is only an `include`), so the package does not compile.
   With `--ts --ts-omit-entrypoint --gen-all` both problems disappear and
   `class Header` exists. Neither failure is an error at generation time — pin a test
   that imports `Header` and `CityFeature` and type-checks the generated tree, and put
   the exact flag set in the generation script header (mirroring
   `scripts/gen_python_fbs.sh` from the Python plan, Task 3).

2. **There is no FlatBuffers verifier in JavaScript — the "always run the Verifier"
   rule (a C++-port global constraint: always run the FlatBuffers Verifier before
   accessing any root) is
   impossible to follow.** `grep -ri verif` over `flatbuffers@25.9.23`'s entire `mjs/`
   tree returns nothing; the runtime ships only `ByteBuffer`, `Builder`, and
   flexbuffers. Rust calls `size_prefixed_root_as_header` (which verifies); Python's
   runtime at least bounds-checks vector reads; the JS runtime does neither — reading
   past a `Uint8Array` returns `undefined`, which propagates as `NaN` through the
   offset arithmetic in `byte-buffer.js` (`readInt32` at mjs/byte-buffer.js:57-62 is
   plain element access and `|`), so a hostile or truncated buffer produces silent
   garbage or bizarre secondary exceptions, never a clean error. Consequences: (a) all
   section-bound and length-prefix validation from the C++ plan's "all input is
   untrusted, all size arithmetic is checked" constraint becomes the *only* line
   of defense and must be done before any generated accessor runs; (b) enforce
   `kMaxFeatureSize` before allocating; (c) validate that the 4-byte feature prefix and
   the header size land inside `total_size` before constructing a `ByteBuffer`.

3. **u64: `Number` is provably safe for every offset that is a file position;
   BigInt is mandatory for B+tree entry offsets *until the payload tag is stripped*,
   and for Long/ULong key comparison.** Where each field lands:
   - *flatc-generated accessors:* u64/i64 table fields return `bigint`. Verified:
     `featuresCount():bigint` with default `BigInt('0')` (generated header.ts:66-69);
     runtime `readUint64`/`readInt64` assemble a `BigInt` from two u32 reads
     (mjs/byte-buffer.js:67-74). Our schemas contain exactly one such field:
     `Header.features_count: ulong` (`src/fbs/header.fbs:136`). Convert once with a
     `<= Number.MAX_SAFE_INTEGER` guard and use `number` internally and publicly.
   - *R-tree `NodeItem.offset` (u64 LE at byte 32 of the 40-byte node,
     `packed_rtree/mod.rs:23-33`):* read with `DataView.getBigUint64(o, true)`, guard,
     convert to `number`. Leaf offsets are byte offsets into the feature section and
     internal offsets are child node indices (`packed_rtree/mod.rs:31`, `:385`, `:531`)
     — both bounded by the file size, and 2^53 bytes is 9 PB. Safe as `number` after a
     single guard; keeping them BigInt just poisons all downstream arithmetic.
   - *B+tree `Entry.offset` (u64 after the key, `static_btree/entry.rs:25-52`):*
     **BigInt is mandatory here.** `PAYLOAD_TAG = 1u64 << 63` (`stree.rs:15`) is a real
     bit in the wire data: a tagged offset is ≥ 2^63, and `Number()` of it rounds —
     verified in Node: `Number((1n<<63n)|12345n) === Number(1n<<63n)` is `true` (the
     12345 is destroyed). Test and strip the tag in BigInt
     (`(v & (1n<<63n)) !== 0n`, then `v & ((1n<<63n)-1n)`, cf. `PAYLOAD_MASK` at
     `stree.rs:17`), *then* convert the low 63 bits to `number` (payload-relative or
     feature-relative offset, both file-bounded). The classic trap: `1 << 63` in JS is
     `1 << (63 & 31)` = `-2147483648` — verified. Write the tag as a `0x8000000000000000n`
     literal, never as a shift of a Number.
   - *Payload entries (`u32 count` + `count × u64`, `static_btree/payload.rs:36-61`):*
     untagged feature offsets; BigInt read, guard, convert.
   - *Long/ULong B+tree keys and their range sentinels:* `i64::MAX/MIN` and `u64::MAX`
     (`key.rs:127-134`, `:213-220`) are not representable as `number`. Since the
     operator lowering builds ranges against exactly these sentinels
     (`query/stream.rs:161-191`, cited in the C++ format reference), Long/ULong key
     decode and comparison must stay in BigInt end-to-end. DateTime's i64 seconds
     also arrive as BigInt, but its sentinels are epoch 0 and 253402300799 (year 9999,
     `key.rs:159-165`, `:242-244`) — both convert to `number` exactly, so DateTime may
     use `number` seconds + `number` nanos after a guard.

4. **BigInt leaks into the public API and JSON emission unless you decide policy up
   front.** `JSON.stringify({a: 1n})` throws `TypeError: Do not know how to serialize
   a BigInt` — verified. Affected surfaces: (a) `Long`/`ULong` *attribute values* in
   the per-object attribute blob (8-byte LE records; the Python port decodes them as
   exact ints, `attribute.py:22-23`) — in TS these come off `getBigInt64/getBigUint64`;
   (b) `featuresCount`; (c) Long/ULong query values accepted from the caller.
   Recommended policy, to be fixed in the API design task and documented: decode
   integer attribute values to `number` when `Number.isSafeInteger` holds (the
   overwhelmingly common case), else keep `bigint` and document that
   `JSON.stringify` of such a feature requires a replacer. Note the conformance
   consequence: `JSON.parse` of the expected `.jsonl` corpus loses the same >2^53
   precision on the *expected* side, so a JS-side conformance test structurally
   cannot detect a divergence above 2^53 — if a corpus case with a >2^53 Long value is
   ever added, its TS assertion must compare against the raw token text, not parsed
   values.

5. **Optional scalars: the Python plan's gotcha #1 does NOT apply — but truthiness
   does.** Verified against generated code from flatc 25.9.23: every `= null` scalar
   field produces a `|null` accessor that probes the vtable itself —
   `MaterialMapping.value(): number|null` (generated material-mapping.ts:
   `const offset = this.bb!.__offset(this.bb_pos, 12); return offset ? …readUint32(…) : null;`),
   `SemanticObject.parent(): number|null` (semantic-object.ts:63-66),
   `Texture.wrapMode(): WrapMode|null`, `Texture.textureType(): TextureType|null`
   (texture.ts:42-50), `Material.ambientIntensity/shininess/transparency(): number|null`,
   `Material.isSmooth(): boolean|null` (material.ts:32-95). No manual vtable probing is
   needed, unlike Python. The residual JS hazard is *falsiness*: `value()` returning
   `0` ("shared material 0", the exact case the Python plan calls out) is falsy, so
   `if (mapping.value())` silently drops it, as does `mapping.value() ?? undefined`
   *not* — but `mapping.value() || fallback` *does*. Same for `parent() === 0` and
   `wrapMode() === WrapMode.None` (enum value 0). Every presence check must be
   `!== null`, and a lint rule banning truthiness tests on these accessors is cheap
   insurance. One more trap in the same family: `borderColor(index)` and all *vector
   element* accessors return `0`, not `null`, when the *vector* is absent
   (texture.ts:52-55) — presence of a vector is only `…Length() > 0` or the
   `…Array() !== null` check, never the element read.

6. **`u32::MAX` sentinels vs 32-bit signed bitwise operators.** The `4294967295`
   null sentinel (semantics values, appearance index arrays — Python gotcha #2 covers
   the *emission* side) has a JS-specific corruption mode: every bitwise operator
   coerces to *signed* 32-bit. Verified: `4294967295 | 0 === -1`, `~4294967295 === 0`.
   The runtime itself is careful (`readUint32` is `readInt32(...) >>> 0`,
   mjs/byte-buffer.js:64-65), so generated accessors return `4294967295` correctly —
   the hazard is entirely in *our* hand-written code: sentinel checks must be
   `x === 0xFFFFFFFF` on the accessor's return value; never "normalize" with `|0`,
   never test with `~x`, never use `<<`/`>>` on offsets that can reach 2 GB (a byte
   offset above 2^31 goes negative under `|0`; files that size are in scope for HTTP
   reading). The one place upstream that is full of u32 bit-twiddling — `hilbert()`
   (`packed_rtree/mod.rs:236-289`) — is writer-only and must not be ported (format
   reference, "Hilbert curve: Writer-only").

7. **String keys: compare `Uint8Array`s byte-wise; JS string `<` is UTF-16 code-unit
   order, which disagrees with the on-disk UTF-8 byte order.** The byte-level
   truncation itself is Python gotcha #5; the JS-only part is the comparison trap.
   Verified: for U+FF61 vs U+10000, JS `"｡" < "\u{10000}"` is `false` (surrogates
   0xD800.. sort below 0xFF61 in UTF-16) while the UTF-8 byte comparison — the order
   the B+tree is built in (`FixedStringKey` is raw bytes, `key.rs:434-464`) — says the
   opposite. A comparator written on decoded strings returns wrong partitions for
   non-BMP content and *passes every ASCII test*. Posture: encode query values with
   `TextEncoder` (always UTF-8), truncate/zero-pad at the byte level to 50/100, compare
   `Uint8Array`s lexicographically; `TextDecoder` (default non-fatal, U+FFFD
   replacement — matching Python's `errors="replace"`) is for display only, never for
   logic. Do not use `Buffer.compare` — it does not exist in browsers.

8. **Float total order: neither `===` nor `Object.is` implements it; DataView NaN
   payload normalization is a non-issue for us.** Required order (format reference:
   `ordered_float` semantics): NaN sorts greatest, NaN == NaN, −0.0 == +0.0. Verified
   JS facts: `NaN === NaN` → false (wrong for equality), `Object.is(-0, 0)` → false
   (wrong: ordered_float says equal), `-0 === 0` → true (right for zeros, wrong for
   NaN). So the comparator must special-case NaN explicitly (both NaN → 0; one NaN →
   that side greater) and then use plain `<`/`>` (which already treat −0 == +0).
   On payload normalization: ECMA-262 leaves NaN bit patterns implementation-defined
   through `getFloat64`; empirically V8 round-trips a signalling-NaN payload intact
   (`0x7ff0000000000001n` survived get→set). Either way it cannot matter here: keys are
   compared *after* decode with all-NaNs-equal semantics, and this reader never
   re-encodes a decoded float — the only rule to keep is exactly that: never implement
   `find_exact` for floats by re-encoding the query key and comparing bytes.

9. **Typed-array views have alignment requirements and aliasing semantics that
   DataView does not; both will bite through the *generated* accessors.** Verified:
   `new Uint32Array(buffer, 2, …)` throws `RangeError: start offset of Uint32Array
   should be a multiple of 4`; `new DataView(buffer, 2)` is fine. Two concrete
   consequences:
   - *Hand-parsed sections:* the R-tree begins at `8 + 4 + header_size`
     (format reference "File layout"; no padding, `writer/mod.rs:266-271`), i.e. at an
     arbitrary alignment, and B+tree entries are `K + 8` bytes (e.g. 58 for
     StringKey50). A `Float64Array` bulk view over node bytes only works if the
     backing slice starts 8-aligned — which is luck, not design. Use `DataView` for
     all hand-parsed decoding; only construct typed-array views over buffers you
     allocated yourself at offset 0.
   - *Generated accessors:* `solidsArray():Uint32Array`, `borderColorArray():Float64Array`
     etc. construct views directly over the feature's backing `ArrayBuffer`
     (material-mapping.ts: `new Uint32Array(this.bb!.bytes().buffer, this.bb!.bytes().byteOffset + …)`).
     If a feature is handed to the ByteBuffer as a `subarray` of a larger batch fetch
     (arbitrary `byteOffset`), these accessors throw `RangeError` for some features and
     not others, depending on where in the batch each feature landed. And when they
     don't throw, they *alias* the batch buffer — `subarray` shares memory, so
     retaining one small geometry array pins the entire multi-MB batch against GC.
     `slice()` copies — applied at the wrong altitude that is an accidental copy of a
     100 MB in-memory file. Rule, matching what the WASM binding already does
     deliberately (`wasm/src/lib.rs:594-595`, "Not zero-copy", `buffer.to_vec()`):
     copy each feature's size-prefixed bytes into a fresh `ArrayBuffer` (offset 0 ⇒
     FlatBuffers' internal alignment holds ⇒ every `*Array()` accessor is safe), and
     let batch buffers die. Copy features; never copy sections.

10. **`DataView` getters default to BIG-endian when the flag is omitted.** Verified:
    `dv.getUint32(0)` returned `0x04030201` where `dv.getUint32(0, true)` returned
    `0x01020304`. Every hand-serialized structure in this format is LE (format
    reference `.llm/docs/specification.md:108`), so a single
    forgotten `, true` yields plausible-looking garbage (a byteswapped f64 bbox is
    still a finite f64). This is the inverse of the Python situation (`struct` needed
    an explicit `<` too, gotcha in plan §Global Constraints) but far quieter than
    C++ (which is natively LE on all supported targets and needed nothing). Mitigate
    structurally: one tiny `le.ts` module wrapping `getF64/getU64/getU32/getU16` with
    the flag baked in; forbid raw `DataView` method calls elsewhere by convention or
    lint. The `flatbuffers` runtime needs no help — it reads byte-wise LE and gates
    its float scratch-buffer trick on `isLittleEndian` (mjs/utils.js:4).

11. **The WASM binding you are replacing has JS-boundary bugs; do not port them as
    "reference behaviour".** Three found while reading it:
    - *Every JS number becomes a `Float64` key* (`wasm/src/lib.rs:1110-1112`:
      `value_js.as_f64()` → `KeyType::Float64`). `build_query` is a pass-through
      (`reader/attr_query.rs:289-302`), and the typed HTTP index rejects mismatched
      key variants with "key type mismatch" (`static_btree/query/http.rs:193-202`).
      Net effect today: attribute queries against `Int`, `Float`, `Long`, … indexed
      columns *fail* from WASM; only `Double`, `Bool`, `String`, `DateTime` columns
      work. The TS reader must look up `Column.type` first and encode the query value
      as that column's key type (format reference "Column type → key type").
    - *String query values are routed by `s.len() > 50` into a `StringKey100`*
      (`wasm/src/lib.rs:1114-1118`) — but the writer only ever produces
      `FixedStringKey<50>` for String columns (`writer/attr_index.rs:272`, cited in
      format reference), so a >50-byte query value errors instead of truncating to 50
      bytes like Rust's own native reader does. TS: always encode String-column keys
      as 50-byte truncated keys.
    - *`index_node_size` from the header is ignored on the HTTP path*: both
      `wasm/src/lib.rs:275` and `fcb_core/src/http_reader/mod.rs:220` pass
      `PackedRTree::DEFAULT_NODE_SIZE` (16) to `http_stream_search` instead of
      `header.index_node_size()`. Any file written with a non-default node size is
      silently mis-traversed over HTTP today. TS should use the header value and this
      should be filed as an upstream finding (`docs/upstream-findings.md`), same
      pattern as C++ Task 14.

---

## SECTION 2 — Async all the way down

The Rust local reader, C++, and Python are synchronous; Rust's HTTP reader is async
but hides buffering inside `AsyncBufferedHttpRangeClient`. In the browser every read
— HTTP `fetch`, `Blob.slice().arrayBuffer()`, even `File` — is async. Concretely:

### 2.1 Which traversals become async, and how the loops restructure

The good news: **no traversal in this codebase is recursive, and every one already
reads at the top of a queue-driven loop.** That shape ports to `async`/`await`
mechanically:

- *R-tree bbox / pointIntersects* (streaming form, `packed_rtree/mod.rs:690-770`;
  HTTP form `:934-1139`): `while queue.pop() { node_items = read(...); … }` becomes
  `while (queue.length) { const bytes = await reader.read(...); … }`. The Python port
  has the same shape (`packed_rtree.py:126-…`, `reader.read(byte_offset, byte_len)`
  mid-loop at `packed_rtree.py:191`) — the "read in the middle of a descent" is
  already hoisted to once-per-queue-item, which is the right granularity for async.
- *R-tree pointNearest*: same, but the queue is a priority queue — see Section 3.
- *B+tree find_exact / find_partition / find_range*
  (`stree.rs:1246-1426`, `:1430-…`, `:1593-…`): one node-range read per level
  (`read_http_node_items` at `stree.rs:1310`), binary search within the fetched
  entries — pure CPU between reads. Payload resolution is already batched to the end
  (`batch_resolve_payloads`, `stree.rs:1416-1423`) with an up-front prefetch
  (`prefetch_payload`, `stree.rs:1305`). Ports as: `await` per level + one payload
  prefetch + one batched payload resolve.
- *Sequential scan*: two awaits per feature (4-byte prefix, then body) —
  `http_reader/mod.rs:568-572`, `wasm/src/lib.rs:662-671` — served from a 1 MB
  buffered fetch so only ~1 physical request per MB.
- *Everything after bytes arrive is synchronous*: FlatBuffers accessors, attribute
  decode, CityJSON emission. The async boundary is exactly the `RangeReader` seam,
  which is the same sans-IO cut the C++ plan made for batching (a sans-IO core where
  parsing and traversal operate only on buffers handed in by a synchronous,
  user-implementable range-read interface).

Public iteration should be an async iterator (`for await (const f of reader.selectAll())`).
One re-entrancy hazard with no sync analogue: two overlapping `next()` calls on the
same iterator interleave their `pos += …` state updates (`SelectAll.pos`,
`http_reader/mod.rs:553`, has a direct TS equivalent). Either serialize `next()`
internally with a held promise, or document single-consumer semantics. Rust never had
this problem (`&mut self` makes it unrepresentable).

### 2.2 What the Rust HTTP readers actually do (constants and batching rules to reproduce)

| Behaviour | Value / rule | Citation |
|---|---|---|
| Max speculative fetch | `DEFAULT_HTTP_FETCH_SIZE = 1_048_576` (1 MB) | `http_reader/mod.rs:42`, `wasm/src/lib.rs:45` |
| Open prefetch | `assumed_header_size + Σ_{i<3} 16^i * 40` = 2024 + 10920 = **12944** bytes (core); the WASM binding assumes 4096 ⇒ **15016** | `http_reader/mod.rs:80-98`; `wasm/src/lib.rs:95-113` |
| What the prefetch buys | magic + header + the top 3 R-tree levels (root, 16, 256 nodes) — the whole internal tree for files ≤ 4096 features | comment at `http_reader/mod.rs:76-92` |
| R-tree node reads | exact ranges, `min_req_size(0)` — "we've already determined precisely which nodes to fetch" | `packed_rtree/mod.rs:203-206` |
| R-tree child-range merge rule | children of a popped node extend the queue *tail* iff same level and `wasted_bytes = (gap_in_nodes) * 40 ≤ combine_request_threshold`, else new queue entry | `packed_rtree/mod.rs:997-1039` |
| Combine threshold (spatial) | `256 * 1024` | `http_reader/mod.rs:213`, `wasm/src/lib.rs:264` |
| Leaf +1 rule | descending into level 0 extends the range by one node (clamped) so `next.offset` gives this feature's length | `packed_rtree/mod.rs:979-990` |
| Feature batching | results (sorted by construction) are greedily grouped: same batch iff `wasted = next.start − prev_end < threshold` | `http_reader/mod.rs:612-650`, `wasm/src/lib.rs:713-750` |
| Batch request size | `first.start .. last.start + (last.length ?? 4)`, capped at 1 MB; set as `min_req_size` so the buffered client absorbs the whole batch in one request | `http_reader/mod.rs:660-681`, `wasm/src/lib.rs:756-780` |
| B+tree node reads | `min_req_size(1024*1024)` per node-range read | `stree.rs:87-91` |
| Combine threshold (attr) | `1024 * 1024` per index | `http_reader/mod.rs:363` etc., `wasm/src/lib.rs:341` |
| Payload prefetch | `clamp(ceil(num_items * 0.1) * 64, 16 KiB, 4 MiB)` fetched once at query start | `stree.rs:402-444`, used at `stree.rs:1298-1305` |
| Attr feature reads | **not batched** — two `get_range` per feature, acknowledged TODO | `http_reader/mod.rs:703-722`, `wasm/src/lib.rs:808-837` |
| Last feature | `RangeFrom(start..)` — read its own 4-byte prefix to size it | `packed_rtree/mod.rs:969-974` |

The TS design must reproduce the *two-layer* structure this implies: a thin
`BufferedRangeReader` (over-fetch to `minRequestSize`, serve subsequent hits from the
buffer — this is what `AsyncBufferedHttpRangeClient` does; exact internals of that
crate are UNVERIFIED, it is a crates.io dependency at `http-range-client 0.9.0`,
`src/rust/Cargo.toml:19`, but every call site's contract is visible above) plus the
explicit range/batch planning in the traversals. The batching *rules* live in our
code, not in the buffer — porting the buffer alone does not reproduce the request
pattern.

### 2.3 One async interface, or a sync fast path?

Recommended interface (matching the C++ sans-IO shape — a synchronous,
user-implementable range-read interface with no async runtime in the core —
and the Python protocol, plan Task 5):

- `read(offset: number, length: number, opts?: {signal?: AbortSignal}): Promise<Uint8Array>`
- `size(): number` — **resolved once at `open()`, then synchronous.** Rust requires
  total size only for the last feature (format reference "features", note at
  `.llm/docs/specification.md:117`); HTTP learns it from `Content-Range` on the
  first 206, files from `stat`, Blobs have `.size` synchronously. Making `size()` a
  per-call promise buys nothing and infects every bounds check.
- Optionally `readBatch(ranges): Promise<Uint8Array[]>` with a default one-by-one
  implementation, so a future multipart-range or parallel-fetch adapter can slot in —
  this is exactly C++'s "batching, not asynchrony" primitive (the concurrency
  primitive is batching, not asynchrony) reinterpreted for a world that has both.

**Recommendation: no sync fast path.** Reasoning: (a) `Blob`/`File` reads are
irreducibly async (`blob.arrayBuffer()` returns a promise), so a sync path covers only
the fully-in-memory case; (b) `await` on an already-resolved promise is a microtask,
not I/O — for a full `delft.fcb`-scale scan that is hundreds of thousands of resolved
awaits, which V8 handles at tens of millions per second; decode and JSON-building
dominate by orders of magnitude (worth one benchmark to confirm, mirroring Python
Task 12 — UNVERIFIED until measured); (c) a dual sync/async API doubles every
traversal or forces the Zalgo-style "sometimes promise, sometimes value" return that
is a well-known JS API smell. If profiling later shows the memory case matters, add an
internal `tryReadSync(): Uint8Array | null` that only `BufferedRangeReader` consults
inside hot loops — an optimization detail, not API.

### 2.4 Concurrency hazards

- **Overlapping range requests / coalescing.** Two in-flight traversals (or an
  impatient UI issuing a second query) against one shared `BufferedRangeReader`
  duplicate fetches and can interleave buffer replacement. Rust sidesteps this by
  ownership: `select_*` *consumes* the reader and hands the client to the iterator
  (`http_reader/mod.rs:174-189` takes `self`). TS cannot enforce that; either (a) copy
  the ownership model — a query returns an iterator that captures the reader, and
  starting a new query while one is live is documented as sequential on the same
  reader — or (b) make the buffer an in-flight map: key requests by exact
  `offset:length` and share the promise for identical requests. Note dedupe by exact
  key is ineffective for *overlapping-but-unequal* ranges; if (b) is chosen, align
  physical fetches to a fixed block grid so keys collide. (a) is simpler and matches
  the reference; recommend (a) for v1.
- **Cancellation.** Every `read` takes an optional `AbortSignal`; the reader plumbs
  one `AbortController` per query so dropping an async iterator (its `return()`
  method) aborts in-flight fetches. Without this, a pan/zoom-driven map UI — the
  actual consumer of the WASM module today — leaks a queue of 1 MB fetches per
  gesture. Rust/WASM has no cancellation at all (nothing in `wasm/src/lib.rs` aborts);
  this is a required improvement, not parity.
- **Server ignores `Range` and returns 200 with the whole body.** The current WASM
  client is *silently wrong* here: `gloo_client.rs:29-44` accepts any `response.ok()`
  and returns the full body as if it were the requested slice — every downstream
  offset then reads garbage. The Python port instead raises by design and documents
  why (`http_reader.py:88-94`); C++'s curl adapter slices the 200 body. For TS:
  require `status === 206` when a `Range` header was sent; on 200, immediately
  `controller.abort()` (do NOT await `response.arrayBuffer()` of a possibly-10 GB
  body) and throw a descriptive error suggesting the fallback — `openBuffer(await
  (await fetch(url)).arrayBuffer())` for files the caller knows are small.
- **Browser-only failure mode: CORS header visibility.** Cross-origin, `Content-Range`
  and `Content-Length` are only readable if the server sends
  `Access-Control-Expose-Headers` naming them; a 206 with an invisible `Content-Range`
  means `size()` cannot be learned at open. Detect (206 but `headers.get('content-range')
  === null`) and fail with an error that names the missing server header — do not
  guess. Also send `Accept-Encoding: identity`-friendly requests (do not set custom
  encodings): ranges over transparently-compressed responses do not mean what byte
  offsets mean. (Exact proxy behaviours: UNVERIFIED, environment-dependent; the
  status/header checks above are the defense either way.)
- **Node fetch vs browser fetch** differ in redirect/cache subtleties but both
  support `Range` and `AbortSignal`; keeping `node:fs` behind a separate subpath
  export (as planned) avoids bundler-visible `node:` imports in the browser build.

---

## SECTION 3 — pointNearest

No Python analogue exists (verified: `packed_rtree.py` implements bbox intersection
only — `intersects` at `packed_rtree.py:56`, no distance functions, no heap). The
Rust implementation exists in three forms: in-memory (`packed_rtree/mod.rs:571-668`),
seekable-stream (`:771-873`), and HTTP (`:1140-1256`). All three share the algorithm;
the HTTP form differs only in fetching node *ranges* and emitting byte ranges.

### The algorithm, precisely

- **Distance metrics — there are two, mixed deliberately.**
  `min_distance_squared(x, y)`: squared Euclidean distance from the query point to the
  nearest point of a node's bbox, 0 if inside (`packed_rtree/mod.rs:154-167`;
  `clamp`-based). `centroid_distance_squared(x, y)`: squared distance to the bbox
  centroid (`:144-150`). Internal nodes are *ordered and pruned* by min-distance;
  a leaf's *final score* is its centroid distance (`:646`, `:844`, `:1207`). Both are
  squared — no `sqrt` anywhere; port as-is.
- **Priority queue contents:** a min-heap (Rust `BinaryHeap<Reverse<…>>`) of
  `{distance, level, node_index}` (in-memory/stream) or `{distance, level, nodes: Range}`
  (HTTP), seeded with the root range at distance 0.0
  (`:598-606`, `:796-804`, `:1167-1175`).
- **Traversal order:** best-first. Pop the smallest-distance entry; if
  `next.distance > best_dist` (strictly greater), **terminate** (`:611-615`, `:809-813`,
  `:1180-1184`). Fetch that entry's nodes. For each node: compute its min-distance;
  skip if `dist >= best_dist` (`:632-636`). If leaf: compute centroid distance and
  replace the best iff *strictly* smaller (`:648-654`) — so on exact centroid-distance
  ties, the first-encountered leaf (lowest storage index among those reached first)
  wins. If internal: push its child range keyed by the *parent node's* min-distance
  (`:655-662`).
- **Heap tie-breaking is unspecified:** ordering is by `distance` alone
  (`PartialOrd` on `distance`, `:586-596`); equal-distance entries pop in arbitrary
  heap order, and `partial_cmp(...).unwrap_or(Equal)` makes any NaN distance compare
  Equal rather than panic. A JS port using a custom binary heap will have a
  *different but equally valid* arbitrary order; the final answer can differ between
  implementations only when two leaf centroids are exactly equidistant. Flag this in
  the conformance test (assert distance, not necessarily identity, on constructed
  ties) rather than trying to replicate Rust's heap internals.
- **Correctness note (why mixing metrics is sound, and why you must not "fix" it):**
  a child's bbox is contained in its parent's, and a leaf's centroid lies inside its
  own bbox, so `min_distance(parent) ≤ centroid_distance(any descendant leaf)` — the
  internal-node key is an admissible lower bound for the leaf scoring metric, making
  the best-first search exact *for the nearest-centroid problem*. It is **not**
  nearest-feature-geometry; do not substitute leaf min-distance for centroid distance
  or results diverge from all three Rust forms.
- **Result:** exactly one item (or none): `Vec` of length ≤ 1 (`:667`, `:870`,
  `:1255`). HTTP form emits `HttpRange::Range(start..end)` using the +1 leaf rule's
  next node, or `RangeFrom(start..)` for the last feature (`:1209-1222`).
- **HTTP form detail:** children ranges get the same leaf `+1` extension and
  level-bound clamp as bbox (`:1233-1243`), but — unlike the bbox/pointIntersects
  branches — **no range merging happens** (there is no queue-tail to extend in a
  heap; compare `:1245-1249` with the bbox merge logic at `:997-1039`).

### Round-trip cost of a naive async port

Every heap pop performs one `read_http_node_items`, i.e. one range request when the
buffer misses — and for R-tree reads the buffer is told *not* to over-fetch
(`min_req_size(0)`, `packed_rtree/mod.rs:203-206`). Pops until termination =
1 (root) + one per enqueued child-range with lower bound < final best. Depth for
branching 16: `delft.fcb` (1115 features → levels 1115/70/5/1) is 4 levels; 1M
features is 6. Typical well-separated data: pops ≈ depth + a handful of competing
sibling ranges — call it 5-15 sequential round trips, each latency-bound (nothing can
be pipelined: the next pop depends on the previous fetch). Degenerate cases (query
point far outside the extent, heavily overlapping bboxes, all centroids
near-equidistant) can pop a large fraction of all internal ranges — hundreds of
serial requests. That is the pathology to design away.

### Recommended structure

1. **Whole-index fast path.** `rtree_size = rtree_index_size(n, ns)` is known before
   any traversal (format reference "File layout"). For `delft.fcb` it is
   (1115+70+5+1)×40 = 47,640 bytes — *one* range request smaller than the 256 KB
   spatial combine threshold, after which the search runs the in-memory algorithm
   (`:571-668`) with zero further index I/O. Rule: if `rtree_size ≤ combine_request_threshold`
   (256 KB ⇒ files up to ~6,100 features; consider 1 MB ⇒ ~24,000), fetch the whole
   R-tree once. This makes pointNearest *cheaper* than bbox for most real files and
   trivially correct — the in-memory port gets exercised by the same unit tests as the
   streaming one.
2. **Above the threshold: keep best-first, batch by wave.** Instead of fetching one
   heap entry per await, drain *every* heap entry whose `distance` is below the
   current `best_dist` (on the first iteration: the root), coalesce their node ranges
   with the existing wasted-bytes rule (reuse the bbox merge logic,
   `packed_rtree/mod.rs:997-1039`, which the Rust nearest branch simply never got),
   issue the resulting requests concurrently via `readBatch`, then process all fetched
   nodes and re-drain. Admissibility is unaffected — processing a superset of the
   minimal node set can only tighten `best_dist` sooner. Round trips collapse to
   O(depth) waves (≈ 3-6), each possibly containing several parallel requests instead
   of serial ones.
3. **Let the open-time prefetch work.** The 12944-byte open prefetch already contains
   the top 3 levels (`http_reader/mod.rs:80-98`); route nearest's node reads through
   the same buffered reader so levels ≥ 1 are usually free, and don't reproduce the
   `min_req_size(0)` pessimization for the upper levels — exact ranges are right for
   *leaf* reads, but for internal levels (≤ 2.5 KB per full level up to 4096 features)
   rounding up to the whole level is cheaper than a second visit.

---

## SECTION 4 — Task decomposition risks

Ranked by slip probability, with the ordering that keeps a shippable reader at every
cut point.

**Most likely to slip:**

1. **B+tree attribute queries** — the (now retired) Python port plan already flagged
   this as the hardest task with a designed fallback; TS adds the
   BigInt tag handling (Section 1 #3), BigInt key comparators for Long/ULong, and
   byte-wise string comparison (Section 1 #7) on top. Same mitigation applies
   unchanged: it must be the last *core* feature so its slip cuts nothing else.
2. **HTTP batching parity** — the batching rules live in traversal code, not in the
   buffer (Section 2.2), and the only way to test them is a counting/mocking
   `RangeReader` asserting request logs (the C++ port's
   `src/cpp/tests/fake_range_reader.hpp`
   pattern). Writing those
   assertions is fiddly and tends to get skipped under pressure — then the reader is
   *correct but 50× chattier* and nobody notices until it is on a real CDN. Make the
   request-log assertions part of the R-tree/B+tree HTTP tasks' definition of done,
   not a later "optimization" task.
3. **pointNearest** — no prior port anywhere (Section 3), two mixed metrics that look
   like a bug and invite "fixing", unspecified tie order. Contain it: it is one
   `Query` variant, additive, behind the same search entry point.
4. **Vitest browser mode + range test server** — browser-mode is the newest tool in
   the stack (Vitest 5 beta) and needs a range-capable server with CORS headers
   (extend `src/cpp/tests/range_server.py`, which the (now retired) Python port plan
   already reuses — it needs `Access-Control-Expose-Headers:
   Content-Range` added for browser use; UNVERIFIED whether it sets CORS headers
   today). Do not let any *core* task depend on browser mode: everything must pass in
   Node first; browser runs are an additive CI job.

**Decisions that must be made early because everything downstream hardens them:**

- The BigInt policy (Section 1 #3/#4) — public types for `featuresCount`, Long/ULong
  attribute values, and query values. Changing this after Task ~6 is a breaking
  rewrite of decoder signatures and tests.
- The generation flag set (`--ts --ts-omit-entrypoint --gen-all`, Section 1 #1) with a
  compile-and-import test — a wrong flag set fails at the *first consumer*, which
  should be Task 2, not Task 8.
- No-verifier defensive-decoding posture (Section 1 #2) — bounds checks belong in
  layout/feature-framing code from the start; retrofitting them under every generated
  accessor later is a diff across the whole codebase.

**Ordering (12-15 tasks) so the hardest slips cost the least:**

1. Scaffolding: package, ESM/Vite/Vitest config, error taxonomy (mirrors Python Task 1).
2. Generated bindings + the flag-set/compile/`getSizePrefixedRootAs*` pin test
   (settles Section 1 #1, #5-analogue; mirrors Python Task 3 which settles its
   gotcha #6).
3. Layout: magic, header size, section offsets, checked arithmetic, LE helper module
   (Section 1 #10).
4. Async `RangeReader` + `BufferedRangeReader` + memory adapter + counting fake
   (Section 2.3 interface; request-log test infrastructure born here).
5. Header parse → `FileInfo` (16-byte `AttributeIndex` stride is confirmed correct in
   generated TS — `index * 16`, accessors at struct offsets 0/4/8/12).
6. File + Blob adapters (Node subpath export; Blob is trivially async-only).
7. Sequential scan + per-object attribute decode (per-object schema rule; BigInt
   policy lands here) — **first shippable artifact: a local-file/Blob scanner.**
8. CityJSON + CityJSONFeature emission (port the finding-#8-fixed C++ semantics per
   Python Task 8's test list; optional-scalar `!== null` discipline, Section 1 #5).
9. Conformance harness against the shared corpus (whole-line compare, Python Task 12
   style) — run it from here on, not at the end. **Shippable: conformant local reader.**
10. HTTP adapter: 206/Content-Range/CORS/abort handling (Section 2.4) + open
    prefetch. **Shippable: remote sequential reader — already replaces the WASM
    module's `select_all` use case.**
11. Packed R-tree: bbox + pointIntersects, in-memory *and* streaming paths, HTTP
    batching rules with request-log assertions. **Shippable: spatial reader.**
12. Pagination (limit/offset over sorted result lists — trivial, mirrors
    `http_reader/mod.rs:233-243`; do it with Task 11 or immediately after).
13. Keys module: encodings, BigInt comparators, sentinels, byte-wise string compare
    (pure functions, fully unit-testable without I/O).
14. B+tree: find_exact/partition/range, payload tag+prefetch+batch resolve, operator
    lowering, the four deliberate divergences. **Slip here loses only attribute
    queries — everything above still ships.**
15. pointNearest (whole-index fast path first, wave-batched best-first second) +
    upstream-findings write-up (WASM defects from Section 1 #11). **Slip here loses
    one query type.**

The load-bearing property: after tasks 9, 10, 11, and 14 there is a releasable
artifact each time, and the two highest-risk tasks (14, 15) are last and severable —
the same shape that let the (now retired) Python port plan absorb its B+tree risk.
