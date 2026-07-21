The design is not implementation-ready. The binary-layout material is largely sound, but several query and API choices would bake in correctness bugs or force redesign mid-implementation.

## Blocking correctness issues

1. **The design adopts a known-bad strict-operator lowering.**

   The design says the cited reference covers “operator lowering” and that TypeScript should make the same choices ([design:34](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:34)). The Rust implementation computes `Gt`, `Lt`, and `Ne` using range results minus `find_exact` results ([stream.rs:161](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/static_btree/query/stream.rs:161)).

   That is incorrect at feature granularity. A feature may contain the queried attribute on multiple CityObjects; if it contains both `k` and `k' > k`, subtracting the feature found at `k` removes a legitimate `Gt(k)` match. This is already documented as an unfixed upstream defect ([upstream-findings.md:128](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/upstream-findings.md:128), [upstream-findings.md:137](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/upstream-findings.md:137)).

   The TS design should explicitly use direct strict/inclusive leaf bounds, as the C++ port does, rather than porting the Rust lowering.

2. **Fixed-width string indexes require mandatory feature-level post-filtering, which the design omits.**

   The design correctly specifies UTF-8 byte comparison and 50/100-byte truncation/padding ([design:330](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:330)), but treats the resulting B+tree answer as exact.

   It is only a candidate set. Different full strings can have the same truncated key; even short values can collide because `"a"` and `"a\0"` have identical zero-padded index representations. The C++ reader therefore decodes each candidate’s full attribute and re-evaluates the predicate ([reader.cpp:394](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp/src/reader.cpp:394), [reader.cpp:412](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp/src/reader.cpp:412)).

   Add an explicit post-filter stage for every `String`/`JsonString` indexed predicate, not merely overlength queries. Candidate spatial intersection may happen first to reduce reads, but post-filtering must precede `featuresCount`, pagination, and yielded results. Full-string ordering should remain UTF-8 byte ordering, not JavaScript string ordering.

3. **The synchronous `select()` example cannot provide the promised count.**

   The API shows:

   > `const cursor = reader.select(...)`  
   > `cursor.featuresCount // total matches`

   ([design:190](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:190), [design:197](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:197))

   Remote R-tree/B+tree traversal, fixed-string verification, and spatial/attribute intersection are asynchronous. The Rust HTTP API resolves the hit list and total before returning the iterator ([http_reader/mod.rs:198](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/http_reader/mod.rs:198), [http_reader/mod.rs:231](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/http_reader/mod.rs:231)).

   Make this either:

   - `const cursor = await reader.select(options)`, with a stable `featuresCount`, or
   - a streaming cursor with an explicit `count: Promise<number | undefined>`.

   A property that starts undefined and mutates after iteration would be timing-dependent and should be rejected. Also, an exact empty result must report `0`; Rust’s current `count > 0 ? Some(count) : None` behavior ([http_reader/mod.rs:495](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/http_reader/mod.rs:495)) is a bug, not useful precedent.

4. **Length checks are not a substitute for FlatBuffers verification.**

   The design says generated accessors will be used without a verifier and that explicit section/length checks are the “only defense.” Nested tables, vectors, vtables, and relative offsets can still point outside a section despite a valid outer length.

   The Rust reader verifies both header and feature buffers ([http_reader/mod.rs:109](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/http_reader/mod.rs:109), [http_reader/mod.rs:503](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/http_reader/mod.rs:503)); the C++ reader does full feature verification too ([reader.cpp:204](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp/src/reader.cpp:204)).

   Before implementation, choose one of:

   - implement schema-aware structural validation;
   - fully decode into checked owned structures; or
   - explicitly declare malformed/untrusted FCB files unsupported.

   Catching accessor exceptions is insufficient because invalid accessors can also return plausible defaults or garbage.

5. **The public attribute type excludes valid wire values.**

   The proposed type is:

   > `number | bigint | string | boolean | Date | null`

   ([design:225](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:225))

   But the reader supports:

   - `Json` as objects/arrays/scalars ([deserializer.rs:380](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/reader/deserializer.rs:380));
   - `Binary` as bytes ([deserializer.rs:406](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/reader/deserializer.rs:406));
   - `DateTime` as a UTF-8 string, not a `Date` ([deserializer.rs:372](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/reader/deserializer.rs:372)).

   `AttrValue` therefore needs a JSON value type and `Uint8Array`. DateTime should remain an ISO string for decoded attributes. Query keys need a separate exact representation because JavaScript `Date` loses the B+tree key’s nanoseconds.

## Public API problems

6. **The lazy `Feature` lifetime rule contradicts its ownership model.**

   The design says the handle has “a private copy of the feature’s bytes” but is “valid only until the next iteration step” ([design:215](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:215)).

   Those statements are incompatible. If each handle owns its bytes, advancing the cursor cannot invalidate it. The C++ port’s `Feature` owns a shared buffer ([reader.cpp:23](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp/src/reader.cpp:23)), and each iteration creates a new copied buffer ([reader.cpp:201](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp/src/reader.cpp:201)).

   Make `Feature` durable and immutable. Runtime generation checks would add machinery while surprising users, especially because already-decoded attributes would necessarily outlive the nominal handle anyway.

7. **`Feature.attributes()` has no defined shape.**

   Attributes belong to individual CityObjects, not the containing feature: `CityFeature` contains `objects`, and each `CityObject` has its own `attributes` and optional `columns` ([feature.fbs:61](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/fbs/feature.fbs:61), [feature.fbs:68](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/fbs/feature.fbs:68)).

   A singular `.attributes()` must specify one of:

   - `Record<objectId, Record<string, AttrValue>>`;
   - `attributes(objectId)`;
   - lazy `cityObjects()` handles with per-object attributes.

   Do not let implementation invent this shape midway through Stage B.

8. **The mixed `number | bigint` policy gives one column an unstable runtime type.**

   “Number when safe, bigint otherwise” ([design:225](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:225)) makes ordinary code data-dependent: sorting, arithmetic, aggregation, schema inference, and serialization change behavior when one large value appears.

   Prefer:

   - raw `Long`/`ULong` attributes always decode as `bigint`;
   - query values accept `bigint`, or safe integer `number` only;
   - unsafe numeric inputs are rejected before they have already been rounded;
   - `toCityJSON()` has an explicit 64-bit policy such as `"lossy-number"`, `"decimal-string"`, or `"error"`.

   Returning bigint inside `toCityJSON()` is not merely inconvenient for `JSON.stringify`; it means the result is not a JSON-compatible value.

9. **The combined selection object lacks necessary semantic constraints.**

   “Spatial and attribute predicates combine” ([design:208](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:208)) leaves several decisions unresolved:

   - Are `bbox`, `point`, and `nearest` mutually exclusive at runtime and in TypeScript?
   - Does `nearest + where` mean “absolute nearest, if it satisfies `where`” or “nearest feature satisfying `where`”?
   - What happens when an attribute has no index: error or sequential scan?
   - What is the stable result order after set intersection?
   - Are duplicate offsets removed?
   - Are `limit` and `offset` restricted to nonnegative safe integers?
   - How are non-finite or inverted bboxes rejected?

   The nearest distinction is especially load-bearing: filtered-nearest requires attribute eligibility during nearest traversal; intersecting two independently computed result sets implements different behavior.

   A discriminated spatial union is safer:

   ```ts
   spatial?: { kind: 'bbox'; value: BBox }
          | { kind: 'point'; value: Point }
          | { kind: 'nearest'; value: Point }
   ```

   Either define filtered-nearest precisely or reject `nearest + where` in v1.

10. **The single-live-query restriction is unnecessary and leak-prone.**

   The design prohibits a second query while a cursor is live ([design:459](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:459)). A cursor created but never iterated or explicitly closed can then retain the lock indefinitely.

   The C++ port instead creates per-query buffered readers over the shared immutable source ([reader.cpp:245](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp/src/reader.cpp:245), [reader.cpp:439](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp/src/reader.cpp:439)). That maps cleanly to JS. Keep iterator-level protection against overlapping `next()` calls if desired, but allow independent cursors.

## Other omitted decode and failure paths

11. Add explicit tasks and tests for:

   - Semantic-object attributes, which use `Header.semantic_columns` and their own attribute blobs—not just CityObject attributes.
   - `features_count == 0`, which means unknown rather than empty ([header.fbs:136](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/fbs/header.fbs:136)); sequential scans must continue to EOF, as the C++ port does ([reader.cpp:129](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/cpp/src/reader.cpp:129)).
   - Duplicate attribute-index declarations, invalid branching factors, index sections shorter than their calculated layout, misaligned child groups, feature offsets outside the feature section, and checked arithmetic.
   - `fromBytes` ownership: copy the supplied bytes or explicitly document that later mutation/detachment is unsafe.
   - Exact `RangeReader.read`: validation of integer/nonnegative offset and length, exact-length reads, EOF behavior, and short-response errors.
   - Passing `AbortSignal` through `readBatch`; the current interface omits it ([design:417](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:417)).
   - Full `Content-Range` validation: requested start/end, total size, and returned body length—not just `206` and header presence.

12. Two smaller claims are wrong:

   - The design says pre-1970 DateTime keys are invisible to `Le` and `Ne` ([design:43](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:43)). `Lt` is affected as well because it uses the same epoch-zero lower sentinel.
   - “All four implementations agree” on the deliberate divergences ([design:44](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:44)) is false today: Rust decodes `Byte` using `as i8` ([deserializer.rs:392](/Users/hbbaba/tudelft/cityjson/flatcitybuf/src/rust/fcb_core/src/reader/deserializer.rs:392)). Likewise, the C++ and Python error-code sets are not identical, so the promise that every implementation reports errors identically ([design:221](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:221)) should be replaced with a deliberately designed TS taxonomy.

## Staging

Stage E is the dependency problem. It combines spatial traversal, pagination, key codecs, B+tree traversal, attribute predicates, and intersection, while later claiming that a B+tree slip “cuts nothing else.” That is not true if combined querying, exact counts, and pagination are part of the same stage.

Split it into:

1. spatial traversal and pagination;
2. attribute key codecs and B+tree candidate retrieval;
3. full-value post-filtering and spatial/attribute composition.

Feature and complete attribute decoding must precede step 3. Semantic attribute decoding belongs with geometry/semantic decoding, not final polish. Nearest should remain isolated.

The retirement stage also needs a repository-wide acceptance check. Removing `src/rust/wasm` affects the Rust workspace, wasm dependencies, `justfile`, build scripts, package exports/files, publishing workflows, examples, and contributor documentation—not just the directory and README.

## Defer or remove

- Defer wave-batched nearest traversal. First port the exact serial Rust algorithm plus the whole-index fast path; only add wave batching after request-log benchmarks show it is needed.
- Keep `readBatch` internal unless a current traversal uses it. Designing today for a future multipart-range adapter is unnecessary.
- Drop custom lint rules for truthiness and raw `DataView` access from the design. Encapsulation plus focused tests is enough unless the project already has corresponding lint infrastructure.
- Remove process prescriptions about particular models and commit frequency from the technical design.
- Drop the claim that `cjseqToCj` is “a few lines” ([design:25](/Users/hbbaba/tudelft/cityjson/flatcitybuf/docs/superpowers/specs/2026-07-21-native-ts-reader-design.md:25)). The existing implementation also merges features, removes duplicate vertices, and updates transforms. Either port it as a tested utility or leave it out of the demo.

The low-level layout, little-endian handling, R-tree offset interpretation, B+tree payload-tag use of `bigint`, UTF-8 key comparison, `ordered_float` comparison, per-feature byte copies, per-object column schemas, and nearest-centroid metric are otherwise correctly characterized.