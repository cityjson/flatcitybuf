# Native TypeScript FlatCityBuf Reader — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `wasm-bindgen` binding at `src/rust/wasm` with a from-scratch pure-TypeScript FlatCityBuf **reader**, so a browser reads `.fcb` files with no WebAssembly, no 3.6 MB binary payload and no `init()` step.

**Architecture:** A layered reader mirroring `src/py` and `src/cpp`. FlatBuffers tables are read through `flatc --ts` generated bindings plus the `flatbuffers` npm runtime; the R-tree and B+tree sections are raw little-endian structs read with `DataView`, never through FlatBuffers. All I/O sits behind one async `RangeReader` interface so `fetch`, `Blob`, `node:fs` and test doubles are interchangeable. Everything after bytes arrive is synchronous.

**Tech Stack:** TypeScript (strict), ESM only, Vite 8 library mode, Vitest 5 (incl. browser mode via Playwright), `flatbuffers` npm as the sole runtime dependency.

**Design document:** `docs/superpowers/specs/2026-07-21-native-ts-reader-design.md`. Read it first — this plan implements it and does not restate its reasoning.

---

## READ THIS FIRST

### Three reference implementations already exist. Use them.

This is the **fourth** implementation of this reader. Rust (`src/rust/fcb_core`) is the origin; C++ (`src/cpp`) was ported from it in July 2026 and is fully conformant; the pure-Python port (`src/py`, branch `native-py`) is through its Task 11.

1. **The format is documented to ground truth.** `.llm/docs/specification.md` holds a byte-level "format reference" (merged in from the retired native C++ plan) with every constant, formula and byte offset **cited to the Rust source line that proves it**. **Read it before writing any code, and cite it from your tests instead of re-deriving anything.** This plan does not duplicate it.
2. **One row of that Format Reference is a trap.** Its "operator lowering" row faithfully documents Rust's lowering — and Rust's lowering is a known-live defect. See Task 14.
3. **The deliberate divergences are already decided** (same document, "Known divergences from the Rust reader"): `Byte` decodes as `u8`, `Json`/`Binary` index queries are rejected, float `max_value()` is `+inf`, `DateTime` `min_value()` is epoch 0. TypeScript makes the same four choices.
4. **Eight upstream defects are written up** in `docs/upstream-findings.md`. #5 (operator lowering), #7 and #8 (appearance) matter most here.

### The single most important lesson from the C++ port

Both round-trip bugs in finding #8 survived because every test compared the new reader against the reference reader's *output*, and both agreed on the wrong answer. **Comparing against the corpus is necessary but not sufficient.** Where TypeScript decodes something the corpus does not exercise, write a round-trip test that goes through the Rust *writer*.

### The oracle technique (use it; do not hand-derive expected values)

When you need to know what the reference does for some input, make the reference tell you: temporarily add a test to the Rust source that prints the actual output for each case, run it, pin those values in the TS tests, revert the injection. This caught a wrong hand-derivation during the C++ port. `src/rust/fcb_core/src/reader/geom_decoder.rs` is where the appearance decoders live.

### JS/TS gotchas that will bite

Full detail with evidence in the design doc's "JS/TS hazards" section and in `docs/superpowers/specs/2026-07-21-native-ts-reader-hazards-analysis.md`. The short list, because every task touches at least one:

1. `flatc --ts` with default flags **silently omits `class Header`**. Required: `flatc --ts --ts-omit-entrypoint --gen-all`. (Task 3)
2. **There is no FlatBuffers verifier in JavaScript.** Framing checks are the only defense and are *not* verification. Inputs are trusted; say so. (Task 4)
3. **BigInt is mandatory** for B+tree entry offsets until the payload tag is stripped, and for `Long`/`ULong` keys. `1 << 63` in JS is `-2147483648`. Write the tag as `0x8000000000000000n`. (Tasks 13, 14)
4. **`Long`/`ULong` attributes always decode to `bigint`**; `toCityJSON()` applies an explicit `int64` policy. (Tasks 8, 10)
5. **Optional scalars** return `T | null` from the generated accessors — but `0` is falsy. Every presence check is `!== null`. (Task 9)
6. **`u32::MAX` is a null sentinel** and JS bitwise operators are 32-bit *signed*: `4294967295 | 0 === -1`. Compare with `=== 0xFFFFFFFF`; never `|0`, never `~x`. (Task 9)
7. **String keys compare as UTF-8 bytes**, never as JS strings — JS `<` is UTF-16 code-unit order and disagrees for non-BMP text while passing every ASCII test. (Task 13)
8. **Float order is `ordered_float`:** NaN greatest, NaN == NaN, −0.0 == +0.0. Neither `===` nor `Object.is` gives this. (Task 13)
9. **Copy each feature's bytes into a fresh `ArrayBuffer` at offset 0.** Generated `*Array()` accessors throw `RangeError` on misaligned `subarray` views and otherwise alias the whole batch buffer. Copy features; never copy sections. (Task 8)
10. **`DataView` getters default to BIG-endian.** Everything on the wire is LE. Use `le.ts`; never call a raw `DataView` getter. (Task 4)
11. **The wasm binding has JS-boundary bugs** (all-numbers-become-Float64 keys, `StringKey100` for >50-byte queries, `index_node_size` ignored on the HTTP path, 200-accepted-as-206). Do not port them as reference behaviour. (Task 18)

### Conventions

- **TDD, strictly.** Write the failing test, run it, confirm it fails *for the expected reason*, implement, confirm it passes, commit. Steps that say "verify it fails" are not optional.
- **Fable for hard analytical passes.** `Agent` tool with `model: "fable"`. One narrow question, forbid behaviour changes, demand evidence (printed byte arrays, side-by-side JSON) rather than conclusions.
- **codex review before closing each stage:** `codex exec --model gpt-5.6-sol --sandbox read-only "<focused prompt>"`. Three real defects and zero false positives during the C++ port; two blocking defects in this plan's own design document.
- Commit after every task. Never leave a red suite.

---

## Global Constraints

- **TypeScript strict mode**, `"strict": true` plus `noUncheckedIndexedAccess`. ESM only — no CommonJS build.
- **Node ≥ 22.12.** Not 20: `vitest@5.0.0-beta.6` declares `engines.node = "^22.12.0 || ^24.0.0 || >=26.0.0"` (verified with `npm view`), so a Node 20 job cannot install it. CI runs 22 and 24; 24.11.1 is what is installed locally.
- **`flatbuffers` npm is the ONLY runtime dependency.** Everything else is `devDependencies`. If a task seems to need another runtime dep, stop and re-scope.
- **Pinned versions:** `vite@^8.1.5`, `vitest@5.0.0-beta.6`, `@vitest/browser@5.0.0-beta.6`, `@vitest/browser-playwright` (a **separate** package in Vitest 5 — `@vitest/browser` alone is not enough), `playwright`, `@types/node`, `flatbuffers@^25.9.23` (matching the locally installed `flatc 25.9.23`).
- **All integers on the wire are little-endian.** Every read goes through `le.ts`; a raw `DataView` getter outside that module is a bug.
- **kebab-case** for directories and files. Each `index.ts` opens with a one-line comment naming the Rust module it ports from.
- **No browser-mode dependency in core tasks.** Everything passes under Node first; browser runs are an additive CI job.
- **Breaking API changes relative to the wasm package are explicitly allowed.**
- **Inputs are trusted.** Framing is bounds-checked; a malformed file may still throw or return garbage. Documented in the README, never papered over.
- **`u32::MAX` (4294967295) means null**; `Number.MAX_SAFE_INTEGER` guards every u64→number conversion.

---

## File Structure

```
src/ts/
  package.json            # @cityjson/flatcitybuf — REWRITTEN in Task 1
  vite.config.ts          # library build + vitest config          <- Task 1
  tsconfig.json                                                     <- Task 1
  README.md               # incl. migration from the wasm API      <- Task 18
  src/
    index.ts              # public API re-exports only
    errors.ts             # FcbError + ErrorCode                    <- Task 1
    le.ts                 # little-endian DataView wrappers         <- Task 4
    layout.ts             # magic, header size, section offsets     <- Task 4
    io/
      range-reader.ts     # RangeReader, BufferedRangeReader,
                          #   BytesRangeReader (in-memory)          <- Task 5
      blob.ts             # browser File / Blob                     <- Task 7
      node.ts             # node:fs, exported via "./node"          <- Task 7
      fetch.ts            # HTTP Range                              <- Task 11
    header/
      index.ts            # readHeader -> HeaderView                <- Task 6
      file-info.ts        # FileInfo, ColumnInfo, AttrIndexInfo     <- Task 6
      attribute-index.ts  # the 16-byte AttributeIndex struct       <- Task 6
    feature/
      index.ts            # framing, sequential scan, Feature       <- Task 8
      attribute.ts        # attribute blob decode                   <- Task 8
    geometry/
      index.ts, boundaries.ts, semantics.ts, appearance.ts          <- Task 9
    cityjson/
      index.ts, types.ts                                            <- Task 10
    packed-rtree/
      index.ts, node-item.ts, search.ts                             <- Task 12
      nearest.ts                                                    <- Task 16
    static-btree/
      index.ts, key.ts                                              <- Task 13
      entry.ts, payload.ts, stree.ts, query.ts                      <- Task 14
    post-filter.ts        # string candidate verification           <- Task 15
    reader.ts             # FcbReader facade, select(), cursors     <- Tasks 8/12/15
    generated/            # flatc --ts output, committed            <- Task 3
  test/
    fixtures/ counting-reader.ts                                    <- Task 5
    *.test.ts
conformance/              # shared corpus, moved here               <- Task 2
examples/web/             # Vite demo app                           <- Task 17
scripts/gen_ts_fbs.sh                                               <- Task 3
```

---

## Task 1: Package skeleton, tooling, error taxonomy

**Files:**
- Create: `src/ts/tsconfig.json`, `src/ts/vite.config.ts`, `src/ts/src/errors.ts`, `src/ts/src/index.ts`, `src/ts/test/errors.test.ts`
- Modify: `src/ts/package.json` (replace the wasm package definition), `src/ts/.gitignore`, `justfile`
- Create: `.github/workflows/ci-ts.yml`

**Interfaces:**
- Produces: `ErrorCode` (string enum), `FcbError extends Error` with `code: ErrorCode`.

The `ErrorCode` set starts from C++'s (`src/cpp/include/fcb/error.hpp:11-24`) and adds the four failure modes only this port has. Do not invent others; later tasks reference these names.

- [ ] **Step 1: Write the failing test**

```ts
// src/ts/test/errors.test.ts
import { describe, expect, it } from 'vitest'
import { ErrorCode, FcbError } from '../src/errors.js'

describe('FcbError', () => {
  it('carries its code and message', () => {
    const err = new FcbError(ErrorCode.MissingMagicBytes, 'bad magic')
    expect(err.code).toBe(ErrorCode.MissingMagicBytes)
    expect(err.message).toContain('bad magic')
  })

  it('is an Error, catchable and instanceof-checkable', () => {
    // Subclassing Error breaks instanceof unless the prototype is restored;
    // TS targeting ES5 silently loses it. This test pins that it works.
    try {
      throw new FcbError(ErrorCode.IoError, 'boom')
    } catch (e) {
      expect(e).toBeInstanceOf(FcbError)
      expect(e).toBeInstanceOf(Error)
      expect((e as FcbError).code).toBe(ErrorCode.IoError)
    }
  })

  it('has a name that identifies it in stack traces', () => {
    expect(new FcbError(ErrorCode.IoError, 'x').name).toBe('FcbError')
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd src/ts && npm install && npx vitest run test/errors.test.ts
```
Expected: FAIL — cannot resolve `../src/errors.js`.

- [ ] **Step 3: Write the package files**

```jsonc
// src/ts/package.json  — replaces the wasm package definition entirely
{
  "name": "@cityjson/flatcitybuf",
  "version": "0.3.0",
  "type": "module",
  "description": "Pure TypeScript reader for FlatCityBuf, a cloud-optimized binary format for 3D city models",
  "author": { "name": "Hidemichi Baba", "email": "baba.papa1120.ba@gmail.com" },
  "license": "MIT",
  "repository": { "type": "git", "url": "https://github.com/cityjson/flatcitybuf" },
  "homepage": "https://github.com/cityjson/flatcitybuf",
  "bugs": { "url": "https://github.com/cityjson/flatcitybuf/issues" },
  "keywords": ["cityjson", "flatbuffers", "geospatial", "3d-city-models"],
  "sideEffects": false,
  "files": ["dist"],
  "exports": {
    ".": { "types": "./dist/index.d.ts", "import": "./dist/index.js" },
    "./node": { "types": "./dist/io/node.d.ts", "import": "./dist/io/node.js" }
  },
  "scripts": {
    "build": "vite build && tsc --emitDeclarationOnly",
    "test": "vitest run",
    "typecheck": "tsc --noEmit"
  },
  "dependencies": { "flatbuffers": "^25.9.23" },
  "devDependencies": {
    "@types/node": "^22.0.0",
    "vite": "^8.1.5",
    "vitest": "5.0.0-beta.6",
    "typescript": "^5.6.0"
  },
  "engines": { "node": ">=22.12.0" }
}
```

`@types/node` is not optional: Task 7 adds `src/io/node.ts`, which imports `node:fs`, and `tsc --emitDeclarationOnly` fails to resolve it without the types.

```jsonc
// src/ts/tsconfig.json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022", "DOM"],
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "declaration": true,
    "outDir": "dist",
    "rootDir": "src",
    "skipLibCheck": true,
    "verbatimModuleSyntax": true
  },
  "include": ["src"]
}
```

```ts
// src/ts/vite.config.ts
import { defineConfig } from 'vite'

export default defineConfig({
  build: {
    lib: {
      entry: { index: 'src/index.ts', 'io/node': 'src/io/node.ts' },
      formats: ['es'],
    },
    rollupOptions: { external: ['flatbuffers', /^node:/] },
    target: 'es2022',
  },
  test: { include: ['test/**/*.test.ts'] },
})
```

```ts
// src/ts/src/errors.ts
/** Error categories. The first thirteen mirror fcb::ErrorCode
 *  (src/cpp/include/fcb/error.hpp) so the implementations report the same
 *  failures under the same names; the rest exist only in this port. */
export enum ErrorCode {
  MissingMagicBytes = 'missing magic bytes',
  IllegalHeaderSize = 'illegal header size',
  InvalidFlatbuffer = 'invalid flatbuffer',
  NoIndex = 'no index',
  AttributeIndexNotFound = 'attribute index not found',
  NoColumnsInHeader = 'no columns in header',
  MissingRequiredField = 'missing required field',
  UnsupportedColumnType = 'unsupported column type',
  InvalidAttributeValue = 'invalid attribute value',
  QueryExecutionError = 'query execution error',
  IoError = 'io error',
  HttpError = 'http error',
  JsonError = 'json error',
  /** The server answered a Range request with 200 and a whole body. */
  RangeNotSupported = 'range not supported',
  /** A cross-origin 206 whose Content-Range is not exposed by CORS. */
  RangeHeadersNotExposed = 'range headers not exposed',
  /** e.g. `nearest` combined with `where`. */
  UnsupportedQueryCombination = 'unsupported query combination',
  /** Two overlapping next() calls on one cursor. */
  ReentrantIteration = 'reentrant iteration',
  /** A caller argument failed validation before any I/O. */
  InvalidArgument = 'invalid argument',
}

/** Every error this package raises. */
export class FcbError extends Error {
  readonly code: ErrorCode

  constructor(code: ErrorCode, message: string) {
    super(`${code}: ${message}`)
    // Restores the prototype chain, which subclassing Error otherwise loses
    // under some downlevel targets. Without it `instanceof FcbError` is false.
    Object.setPrototypeOf(this, new.target.prototype)
    this.name = 'FcbError'
    this.code = code
  }
}
```

```ts
// src/ts/src/index.ts
export { ErrorCode, FcbError } from './errors.js'
```

Replace `src/ts/.gitignore` with:

```gitignore
node_modules/
dist/
```

- [ ] **Step 4: Run the tests and the typechecker**

```bash
cd src/ts && npx vitest run && npx tsc --noEmit
```
Expected: 3 passed, tsc silent.

- [ ] **Step 5: Add the justfile recipes and CI**

Add to `justfile`:

```make
# TypeScript reader
ts-test:
    cd src/ts && npm ci && npx vitest run

ts-lint:
    cd src/ts && npx tsc --noEmit

ts-build:
    cd src/ts && npm run build
```

Create `.github/workflows/ci-ts.yml` running those three on `ubuntu-latest` with **Node 22 and 24** (not 20 — see Global Constraints), with **no Rust toolchain**. That is only possible because Task 2 commits the `.fcb` corpus; without it every test from Task 3 onward opens a file that does not exist in a fresh checkout. Leave the wasm jobs alone until Task 18.

- [ ] **Step 6: Commit**

```bash
git add src/ts justfile .github/workflows/ci-ts.yml
git commit -m "feat(ts): pure-TypeScript package skeleton and error taxonomy"
```

---

## Task 2: Move the conformance corpus somewhere all four readers share

**Files:**
- Move: `src/cpp/tests/conformance/` → `conformance/` (repo root)
- Modify: `scripts/gen_conformance.sh` (line 13, `OUT=`), `src/cpp/tests/CMakeLists.txt` (`FCB_CONFORMANCE_DIR`), `src/cpp/tests/test_conformance.cpp` if it hardcodes anything

The corpus is the shared oracle for Rust, C++, Python and now TypeScript. Leaving it under `src/cpp` makes TypeScript's dependency on it look accidental. **The Python plan's Task 2 specifies the identical move to the identical destination**, so whichever lands first makes the other a no-op — do not choose a different path.

**This task also starts committing the `.fcb` binaries**, which `.gitignore:65` currently excludes. Two reasons: the TypeScript CI job has no Rust toolchain and therefore cannot regenerate them, and committing pins the oracle bytes instead of leaving them to drift with the writer. The whole corpus is 124 KB, and `.gitignore` already has the precedent `!examples/**/*.fcb`.

- [ ] **Step 1: Move and repoint — without destroying anything**

An untracked `conformance/` may already exist locally from a worktree run. **Do not `rm -rf` it**; move it aside so a mistake is recoverable.

```bash
if [ -e conformance ]; then mv conformance conformance.local.bak; fi
git mv src/cpp/tests/conformance conformance
# then update OUT= in scripts/gen_conformance.sh:13 and
# FCB_CONFORMANCE_DIR in src/cpp/tests/CMakeLists.txt
```

Add to `.gitignore`, directly under the existing `!examples/**/*.fcb`:

```gitignore
# The conformance corpus IS committed: it is the shared oracle for four
# readers, the TypeScript CI has no Rust toolchain to regenerate it, and
# pinning the bytes is the point. 124 KB total.
!conformance/*.fcb
```

- [ ] **Step 2: Regenerate and prove nothing changed**

```bash
./scripts/gen_conformance.sh
python3 - <<'EOF'
import json, glob, subprocess, os
for p in sorted(glob.glob("conformance/*.expected.jsonl")):
    old = subprocess.run(["git","show",f"HEAD:src/cpp/tests/conformance/{os.path.basename(p)}"],
                         capture_output=True, text=True)
    if old.returncode: continue
    a=[json.loads(l) for l in old.stdout.splitlines() if l.strip()]
    b=[json.loads(l) for l in open(p) if l.strip()]
    print(os.path.basename(p), "SAME" if a==b else "CHANGED")
EOF
```
Expected: every file `SAME`. Regeneration reorders JSON keys without changing meaning — compare parsed, never bytes, and revert any file whose JSON is unchanged.

- [ ] **Step 3: Rebuild C++ and confirm the corpus still resolves**

```bash
cd src/cpp && cmake -B build-native -S . && cmake --build build-native -j8
./build-native/tests/fcb_tests
```
Expected: the full C++ suite passes.

- [ ] **Step 4: Add the two fixtures the existing corpus cannot provide**

Two later tasks need cases no current fixture exercises. Both must be **generated by the Rust CLI**, like every other case, so the `.expected.jsonl` is a real oracle rather than something hand-written.

`conformance/inputs/multi_object_attrs.city.jsonl` — a feature whose **CityObjects carry different values of one indexed attribute**, including a feature holding both `k` and some `k' > k`. This is the shape upstream finding #5 breaks, and no existing fixture has it: every `duplicate_keys` feature has a single CityObject with a unique value. Build it as a `Building` parent plus two `BuildingPart` children with, say, `h = 1` and `h = 9`, alongside control features with a single value each.

`conformance/inputs/colliding_strings.city.jsonl` — at least two features whose `String` attribute **agrees in its first 50 bytes and differs after**, plus one whose value is short. `long_strings` does not work for this: its values are 53-byte `yyyy…AAA` / `yyyy…BBB`, which do collide — but the plan's original tests queried `"a"`, which the raw index already answers with nothing, so they would have passed with no post-filter at all. Include a short value so the zero-padding collision (`"a"` vs `"a\0"`) is reachable too.

Two more, both re-encodings of `small`'s input with different writer flags, needed because otherwise the corresponding code paths are untestable:

- `small_node8.fcb` — written with a **non-default `index_node_size` of 8**. Without it, a reader that hardcodes 16 (which both the wasm binding and `fcb_core`'s HTTP reader do — Task 18) passes the entire suite.
- `no_count.fcb` — written with `features_count` left at **0**, which means *unknown*. Without it, "0 means scan to EOF" cannot be asserted on anything.

If the Rust CLI has no flag for either, add one, or write the two files with a small Rust test harness that drives the writer directly. Extend `scripts/gen_conformance.sh` so both are reproducible.

```bash
./scripts/gen_conformance.sh
git add conformance/inputs/multi_object_attrs.city.jsonl \
        conformance/inputs/colliding_strings.city.jsonl \
        conformance/multi_object_attrs.expected.jsonl \
        conformance/colliding_strings.expected.jsonl \
        conformance/small_node8.fcb conformance/no_count.fcb
```

Then add both names to the C++ conformance case list so the existing readers gain the same coverage, rebuild, and confirm they pass there too. **If C++ fails one of them, that is a real defect in C++ and it is a finding** — do not adjust the fixture to make it pass.

- [ ] **Step 5: Commit**

```bash
git add -A conformance src/cpp scripts/gen_conformance.sh .gitignore
git commit -m "refactor: move the conformance corpus to a shared top-level directory

Also commits the .fcb binaries -- the corpus is the shared oracle for four
readers and the TypeScript CI has no Rust toolchain to regenerate them --
and adds two cases no existing fixture covered: a feature whose CityObjects
carry several values of one indexed attribute, and two string values that
collide in their first 50 bytes."
```

---

## Task 3: Generate the TypeScript FlatBuffers bindings

**Files:**
- Create: `scripts/gen_ts_fbs.sh`, `src/ts/src/generated/**` (committed)
- Create: `src/ts/test/generated.test.ts`

**Interfaces:**
- Produces: generated classes for `Header`, `CityFeature`, `CityObject`, `Geometry`, `GeometryInstance`, `SemanticObject`, `MaterialMapping`, `TextureMapping`, `Appearance`, `Material`, `Texture`, `Column`, `AttributeIndex`, `Transform`, `Vertex`, `DoubleVertex`, `Vec2` and the enums.

**This task settles the flag set, and it is the one place a wrong answer is silent.** With default flags the entry-point re-export file and the `Header` table both map to `header.ts`, the re-export wins, and the output contains **zero** occurrences of `class Header` — plus a circular self-import. Without `--gen-all` the output imports a never-generated `./extension.js`. Neither is an error at generation time.

- [ ] **Step 1: Write the generation script**

```bash
#!/usr/bin/env bash
# Regenerate the committed TypeScript FlatBuffers bindings from src/fbs/*.fbs.
#
# The flag set is load-bearing and was verified empirically:
#   --ts-omit-entrypoint  without it, the per-namespace re-export file
#                         collides with header.ts and `class Header` is
#                         silently NOT emitted (a circular self-export wins).
#   --gen-all             without it, header.ts imports ./extension.js,
#                         which is never generated, and nothing compiles.
# Neither failure is reported by flatc. See test/generated.test.ts.
#
# Generated with: flatc 25.9.23 / flatbuffers npm ^25.9.23
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${REPO_ROOT}/src/ts/src/generated"
rm -rf "${OUT}" && mkdir -p "${OUT}"
flatc --ts --ts-omit-entrypoint --gen-all -o "${OUT}" \
  "${REPO_ROOT}/src/fbs/header.fbs" \
  "${REPO_ROOT}/src/fbs/feature.fbs"
echo "TypeScript bindings written to ${OUT}"
```

```bash
chmod +x scripts/gen_ts_fbs.sh
```

- [ ] **Step 2: Write the failing test — this is where gotcha #1 is settled**

```ts
// src/ts/test/generated.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import * as flatbuffers from 'flatbuffers'
import { describe, expect, it } from 'vitest'
import { Header } from '../src/generated/header.js'
import { CityFeature } from '../src/generated/city-feature.js'

const CORPUS = resolve(__dirname, '../../../conformance')

describe('generated bindings', () => {
  it('actually exports the Header CLASS, not a re-export of itself', () => {
    // With default flatc flags this is a circular re-export and Header is
    // undefined at runtime while still type-checking. See gen_ts_fbs.sh.
    expect(typeof Header).toBe('function')
    expect(typeof Header.getRootAsHeader).toBe('function')
    expect(typeof CityFeature).toBe('function')
  })

  it('reads a real header as a size-prefixed root', () => {
    // Pins HOW this runtime exposes size-prefixed roots. Do not assume.
    const raw = readFileSync(resolve(CORPUS, 'small.fcb'))
    const headerSize = raw.readUInt32LE(8)
    // The prefix is INCLUDED in the slice, per the Format Reference.
    const slice = raw.subarray(8, 12 + headerSize)
    const bb = new flatbuffers.ByteBuffer(new Uint8Array(slice))
    const header = Header.getSizePrefixedRootAsHeader(bb)
    expect(header.version()).toBe('2.0')
    expect(header.featuresCount()).toBeGreaterThan(0n)
  })
})
```

- [ ] **Step 3: Run it and watch it fail**

```bash
cd src/ts && npx vitest run test/generated.test.ts
```
Expected: FAIL — cannot resolve `../src/generated/header.js`.

- [ ] **Step 4: Generate and re-run**

```bash
./scripts/gen_ts_fbs.sh
cd src/ts && npx vitest run test/generated.test.ts && npx tsc --noEmit
```
Expected: PASS. If `getSizePrefixedRootAsHeader` is not the right API for this runtime version, find the one that is and **record the answer in a comment in the test** — every later task depends on it. If `typeof Header` is `'undefined'`, the flag set regressed; fix the script, do not work around it in application code.

- [ ] **Step 5: Commit**

```bash
git add scripts/gen_ts_fbs.sh src/ts/src/generated src/ts/test/generated.test.ts
git commit -m "feat(ts): generate and commit the TypeScript FlatBuffers bindings"
```

---

## Task 4: Little-endian helpers and file layout

**Files:**
- Create: `src/ts/src/le.ts`, `src/ts/src/layout.ts`, `src/ts/test/layout.test.ts`

**Interfaces:**
- Produces from `le.ts`: `readU16(dv, o)`, `readU32(dv, o)`, `readI32(dv, o)`, `readU64(dv, o): bigint`, `readI64(dv, o): bigint`, `readF32(dv, o)`, `readF64(dv, o)`, `toSafeNumber(v: bigint, what: string): number`.
- Produces from `layout.ts`: `MAGIC_SIZE = 8`, `NODE_ITEM_SIZE = 40`, `DEFAULT_NODE_SIZE = 16`, `MAX_HEADER_SIZE = 536870912`, `MAX_FEATURE_SIZE = 268435456`; `checkMagicBytes(b: Uint8Array): boolean`; `rtreeIndexSize(numItems: number, nodeSize: number): number`; `FileLayout { headerLen, rtreeBegin, rtreeSize, attrIndexBegin, attrIndexSize, featureBegin }`; `computeLayout(opts): FileLayout`; `validateLayoutAgainstSize(layout: FileLayout, totalSize: number): void`.

All formulas: Format Reference → "File layout". Port `src/cpp/src/layout.cpp` directly.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/layout.test.ts
import { describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import { readF64, readU32, readU64, toSafeNumber } from '../src/le.js'
import {
  checkMagicBytes, computeLayout, rtreeIndexSize, validateLayoutAgainstSize,
} from '../src/layout.js'

describe('le', () => {
  it('reads little-endian, which is the OPPOSITE of DataView defaults', () => {
    const dv = new DataView(new Uint8Array([1, 2, 3, 4]).buffer)
    expect(readU32(dv, 0)).toBe(0x04030201)
    expect(dv.getUint32(0)).toBe(0x01020304) // the trap, pinned
  })

  it('reads u64 as bigint and converts only within safe range', () => {
    const buf = new Uint8Array(8)
    new DataView(buf.buffer).setBigUint64(0, 2n ** 53n, true)
    const dv = new DataView(buf.buffer)
    expect(readU64(dv, 0)).toBe(2n ** 53n)
    expect(() => toSafeNumber(2n ** 60n, 'offset')).toThrow(FcbError)
    expect(toSafeNumber(12345n, 'offset')).toBe(12345)
  })

  it('reads f64 little-endian', () => {
    const buf = new Uint8Array(8)
    new DataView(buf.buffer).setFloat64(0, -1.5, true)
    expect(readF64(new DataView(buf.buffer), 0)).toBe(-1.5)
  })
})

describe('magic bytes', () => {
  it('ignores byte seven, which is written but never validated', () => {
    // lib.rs:56-58 validates b[0..3], b[4..7] and b[3] <= 1 only.
    expect(checkMagicBytes(new TextEncoder().encode('fcb\x01fcb\x00'))).toBe(true)
    expect(checkMagicBytes(new TextEncoder().encode('fcb\x01fcb\xff'))).toBe(true)
    expect(checkMagicBytes(new TextEncoder().encode('xcb\x01fcb\x00'))).toBe(false)
  })

  it('rejects a future version', () => {
    expect(checkMagicBytes(new TextEncoder().encode('fcb\x02fcb\x00'))).toBe(false)
  })

  it('rejects a buffer shorter than the magic', () => {
    expect(checkMagicBytes(new Uint8Array(4))).toBe(false)
  })
})

describe('rtreeIndexSize', () => {
  it('counts a root node even for a single item', () => {
    // The loop DIVIDES FIRST and only then tests n === 1, so a one-feature
    // file stores a leaf AND a root: 2 nodes, 80 bytes. Asserting 40 here
    // misplaces every section of single_feature.fcb.
    // (packed_rtree/mod.rs:888, src/cpp/src/layout.cpp:36-44)
    expect(rtreeIndexSize(1, 16)).toBe(80)
    expect(rtreeIndexSize(16, 16)).toBe((16 + 1) * 40)
    expect(rtreeIndexSize(17, 16)).toBe((17 + 2 + 1) * 40)
  })

  it('REJECTS a node size below 2 rather than clamping it', () => {
    // layout.cpp:25-29: "reject rather than clamp, so we never invent a
    // layout." A clamping reader silently reads a corrupt file as if it
    // were well formed. 0 means "no index" only at computeLayout.
    expect(() => rtreeIndexSize(4, 0)).toThrow(FcbError)
    expect(() => rtreeIndexSize(4, 1)).toThrow(FcbError)
  })

  it('rejects a zero item count, which would never terminate', () => {
    expect(() => rtreeIndexSize(0, 16)).toThrow(FcbError)
  })
})

describe('computeLayout', () => {
  it('places sections back to back with no padding', () => {
    const l = computeLayout({
      headerSize: 64, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 0,
    })
    expect(l.headerLen).toBe(8 + 4 + 64)
    expect(l.rtreeBegin).toBe(l.headerLen)
    expect(l.rtreeSize).toBe(80)          // leaf + root, see above
    expect(l.attrIndexBegin).toBe(l.headerLen + 80)
    expect(l.featureBegin).toBe(l.headerLen + 80)
  })

  it('places the feature section after the attribute index', () => {
    const l = computeLayout({
      headerSize: 64, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 256,
    })
    expect(l.attrIndexBegin).toBe(l.headerLen + 80)
    expect(l.featureBegin).toBe(l.headerLen + 80 + 256)
  })

  it('has no rtree when the node size or the feature count is zero', () => {
    expect(computeLayout({
      headerSize: 64, featuresCount: 0, indexNodeSize: 16, attrIndexSize: 0,
    }).rtreeSize).toBe(0)
    expect(computeLayout({
      headerSize: 64, featuresCount: 5, indexNodeSize: 0, attrIndexSize: 0,
    }).rtreeSize).toBe(0)
  })

  it('rejects a header larger than the file', () => {
    const l = computeLayout({
      headerSize: 64, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 0,
    })
    expect(() => validateLayoutAgainstSize(l, 10)).toThrow(FcbError)
  })

  it('rejects a header size outside the legal window', () => {
    expect(() => computeLayout({
      headerSize: 4, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 0,
    })).toThrow(FcbError)
    expect(() => computeLayout({
      headerSize: 536870913, featuresCount: 1, indexNodeSize: 16, attrIndexSize: 0,
    })).toThrow(FcbError)
  })
})
```

- [ ] **Step 2: Run, watch fail**

```bash
cd src/ts && npx vitest run test/layout.test.ts
```
Expected: FAIL — cannot resolve `../src/le.js`.

- [ ] **Step 3: Implement `le.ts` then `layout.ts`**

`le.ts` is a thin module; the only judgement in it is `toSafeNumber`:

```ts
// src/ts/src/le.ts
/** Every wire read goes through here. DataView getters default to BIG-endian
 *  when the flag is omitted, so a single forgotten `true` yields plausible
 *  garbage -- a byteswapped f64 bbox is still a finite f64. Nothing outside
 *  this module may call a raw DataView getter. */
import { ErrorCode, FcbError } from './errors.js'

export const readU16 = (dv: DataView, o: number): number => dv.getUint16(o, true)
export const readU32 = (dv: DataView, o: number): number => dv.getUint32(o, true)
export const readI32 = (dv: DataView, o: number): number => dv.getInt32(o, true)
export const readU64 = (dv: DataView, o: number): bigint => dv.getBigUint64(o, true)
export const readI64 = (dv: DataView, o: number): bigint => dv.getBigInt64(o, true)
export const readF32 = (dv: DataView, o: number): number => dv.getFloat32(o, true)
export const readF64 = (dv: DataView, o: number): number => dv.getFloat64(o, true)

/** Converts a wire u64 that is known to be a file position. Throws rather
 *  than silently rounding: a 2^53+ offset read as a Number indexes nowhere. */
export function toSafeNumber(v: bigint, what: string): number {
  if (v > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new FcbError(ErrorCode.InvalidFlatbuffer,
      `${what} ${v} exceeds Number.MAX_SAFE_INTEGER`)
  }
  return Number(v)
}
```

`layout.ts` implements the constants and the three functions above. `rtreeIndexSize` clamps the node size to `[2, 65535]`, then accumulates `n = ceil(n / nodeSize)` until `n === 1`. `computeLayout` enforces `8 <= headerSize <= MAX_HEADER_SIZE` and computes the offsets exactly as the table in the Format Reference does.

- [ ] **Step 4: Run, expect PASS**

```bash
cd src/ts && npx vitest run && npx tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git add src/ts/src/le.ts src/ts/src/layout.ts src/ts/test/layout.test.ts
git commit -m "feat(ts): little-endian helpers and file layout arithmetic"
```

---

## Task 5: RangeReader, buffered decorator, in-memory source

**Files:**
- Create: `src/ts/src/io/range-reader.ts`, `src/ts/test/fixtures/counting-reader.ts`, `src/ts/test/range-reader.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export interface ReadOpts { signal?: AbortSignal }
  export interface RangeReader {
    read(offset: number, length: number, opts?: ReadOpts): Promise<Uint8Array>
    size(): number
  }
  export class BufferedRangeReader implements RangeReader {
    constructor(inner: RangeReader, minRequestSize?: number)  // default 1048576
    setMinRequestSize(bytes: number): void
  }
  export class BytesRangeReader implements RangeReader {
    constructor(bytes: Uint8Array)   // COPIES
  }
  ```
- Produces from the fixture: `class CountingReader implements RangeReader` with `reads: Array<{offset: number, length: number}>`.

`size()` is synchronous by design (see the design doc). `read` returns **exactly** `length` bytes or throws. `BytesRangeReader` copies, so later mutation or `ArrayBuffer` detachment cannot corrupt an open reader.

`setMinRequestSize` exists because the traversals change the over-fetch policy per phase — exact ranges for R-tree leaves, 1 MB for feature batches (Format Reference → "HTTP constants").

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/fixtures/counting-reader.ts
import type { RangeReader, ReadOpts } from '../../src/io/range-reader.js'

/** Records every underlying read so tests can assert the REQUEST PATTERN,
 *  not just the bytes. Without these assertions a reader can be correct and
 *  50x chattier than the reference, and nothing notices until it is on a CDN. */
export class CountingReader implements RangeReader {
  readonly reads: Array<{ offset: number; length: number }> = []

  constructor(private readonly data: Uint8Array) {}

  async read(offset: number, length: number, _opts?: ReadOpts): Promise<Uint8Array> {
    this.reads.push({ offset, length })
    return this.data.subarray(offset, offset + length)
  }

  size(): number {
    return this.data.length
  }
}
```

```ts
// src/ts/test/range-reader.test.ts
import { describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import { BufferedRangeReader, BytesRangeReader } from '../src/io/range-reader.js'
import { CountingReader } from './fixtures/counting-reader.js'

const ramp = (n: number) => Uint8Array.from({ length: n }, (_, i) => i & 0xff)

describe('BytesRangeReader', () => {
  it('serves exact ranges and reports its size', async () => {
    const r = new BytesRangeReader(ramp(256))
    expect(r.size()).toBe(256)
    expect(Array.from(await r.read(4, 3))).toEqual([4, 5, 6])
  })

  it('copies its input, so later mutation cannot corrupt it', async () => {
    const src = ramp(16)
    const r = new BytesRangeReader(src)
    src.fill(0xff)
    expect(Array.from(await r.read(0, 2))).toEqual([0, 1])
  })

  it('rejects a read past the end rather than returning a short buffer', async () => {
    const r = new BytesRangeReader(ramp(16))
    await expect(r.read(12, 8)).rejects.toThrow(FcbError)
  })

  it('rejects non-integer and negative arguments', async () => {
    const r = new BytesRangeReader(ramp(16))
    await expect(r.read(-1, 4)).rejects.toThrow(FcbError)
    await expect(r.read(0.5, 4)).rejects.toThrow(FcbError)
    await expect(r.read(0, -4)).rejects.toThrow(FcbError)
  })
})

describe('BufferedRangeReader', () => {
  it('serves sequential reads from one underlying fetch', async () => {
    const inner = new CountingReader(ramp(2048))
    const r = new BufferedRangeReader(inner, 512)
    expect(Array.from(await r.read(0, 4))).toEqual([0, 1, 2, 3])
    expect(Array.from(await r.read(4, 4))).toEqual([4, 5, 6, 7])
    expect(inner.reads).toHaveLength(1)
    expect(inner.reads[0]).toEqual({ offset: 0, length: 512 })
  })

  it('refetches when the request leaves the buffered window', async () => {
    const inner = new CountingReader(ramp(2048))
    const r = new BufferedRangeReader(inner, 512)
    await r.read(0, 4)
    await r.read(1024, 4)
    expect(inner.reads).toHaveLength(2)
  })

  it('never over-fetches past the end of the file', async () => {
    const inner = new CountingReader(ramp(100))
    const r = new BufferedRangeReader(inner, 512)
    await r.read(90, 10)
    expect(inner.reads[0]!.offset + inner.reads[0]!.length).toBeLessThanOrEqual(100)
  })

  it('satisfies a request larger than minRequestSize in one read', async () => {
    const inner = new CountingReader(ramp(2048))
    const r = new BufferedRangeReader(inner, 16)
    await r.read(0, 1000)
    expect(inner.reads).toHaveLength(1)
    expect(inner.reads[0]!.length).toBeGreaterThanOrEqual(1000)
  })
})
```

- [ ] **Step 2: Run, watch fail.**

```bash
cd src/ts && npx vitest run test/range-reader.test.ts
```
Expected: FAIL — cannot resolve `../src/io/range-reader.js`.

- [ ] **Step 3: Implement.** `BufferedRangeReader` holds `{start, bytes}`; on a miss it reads `max(length, minRequestSize)` from `offset`, clamped to `size()`, and serves the slice. It always returns a `subarray` of its own buffer — callers that keep bytes beyond the next read must copy, which Task 8 does for features.

- [ ] **Step 4: Run, expect PASS.**

- [ ] **Step 5: Commit**

```bash
git add src/ts/src/io src/ts/test/range-reader.test.ts src/ts/test/fixtures
git commit -m "feat(ts): range reader interface, buffered decorator, in-memory source"
```

---

## Task 6: Header parsing and `FileInfo`

**Files:**
- Create: `src/ts/src/header/index.ts`, `src/ts/src/header/file-info.ts`, `src/ts/src/header/attribute-index.ts`, `src/ts/test/header.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export interface ColumnInfo { index: number; name: string; type: ColumnType; nullable: boolean }
  export interface AttrIndexInfo { columnIndex: number; length: number; branchingFactor: number; numUniqueItems: number; begin: number }
  export interface FileInfo {
    featuresCount: number          // 0 means UNKNOWN, not empty
    indexNodeSize: number
    columns: ColumnInfo[]
    semanticColumns: ColumnInfo[]
    geographicalExtent?: [number, number, number, number, number, number]
    /** `transform` is NOT required by the schema. Absent must stay
     *  distinguishable from a real zero transform. */
    hasTransform: boolean
    scale?: [number, number, number]
    translate?: [number, number, number]
    referenceSystem?: string
    version: string
    identifier?: string
    title?: string
    attributeIndices: AttrIndexInfo[]
  }
  export interface HeaderView { info: FileInfo; raw: Header; layout: FileLayout }
  export function readHeader(reader: RangeReader): Promise<HeaderView>
  ```

**`AttributeIndex` is 16 bytes, not 12** — field order forces padding after each `ushort`. Wire layout: `0:u16 index, 2:pad, 4:u32 length, 8:u16 branching_factor, 10:pad, 12:u32 num_unique_items` (Format Reference → "Attribute B+tree"). Decode it with `le.ts` against the struct's base offset, not by trusting a stride you guessed.

`begin` is cumulative: `attrIndexBegin + Σ length of preceding entries`, entries sorted by `columnIndex`.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/header.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { BytesRangeReader } from '../src/io/range-reader.js'
import { readHeader } from '../src/header/index.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const open = (name: string) =>
  new BytesRangeReader(new Uint8Array(readFileSync(resolve(CORPUS, name))))

// Pinned from conformance/small.expected.jsonl line 0 -- the Rust writer's
// own output. "Six finite numbers" and "length 3" pass for a reader that
// returns the WRONG six numbers, which is the whole failure mode here.
const SMALL = {
  featuresCount: 0,            // <- paste real values from the expected JSONL
  scale: [0, 0, 0] as [number, number, number],
  translate: [0, 0, 0] as [number, number, number],
  extent: [0, 0, 0, 0, 0, 0] as [number, number, number, number, number, number],
}

describe('readHeader', () => {
  it('reads the EXACT version, count and transform of small.fcb', async () => {
    const { info } = await readHeader(open('small.fcb'))
    expect(info.version).toBe('2.0')
    expect(info.featuresCount).toBe(SMALL.featuresCount)
    expect(info.scale).toEqual(SMALL.scale)
    expect(info.translate).toEqual(SMALL.translate)
  })

  it('reads the EXACT geographical extent', async () => {
    const { info } = await readHeader(open('small.fcb'))
    expect(info.geographicalExtent).toEqual(SMALL.extent)
  })

  it('distinguishes an ABSENT transform from a zero transform', async () => {
    // `transform` is not required by the schema (src/fbs/header.fbs:131) and
    // C++ tracks its presence separately (include/fcb/header.hpp:54). A
    // reader that defaults it to zeros makes a missing transform look like a
    // real one that collapses every coordinate to the origin.
    const { info } = await readHeader(open('degenerate_extent.fcb'))
    expect(info.hasTransform).toBeTypeOf('boolean')
    if (!info.hasTransform) expect(info.scale).toBeUndefined()
  })

  it('computes section offsets that fit inside the file', async () => {
    const reader = open('small.fcb')
    const { layout } = await readHeader(reader)
    expect(layout.featureBegin).toBeLessThan(reader.size())
    expect(layout.rtreeBegin).toBe(layout.headerLen)
  })

  it('rejects a file whose magic bytes are wrong', async () => {
    const bad = new Uint8Array(64)
    await expect(readHeader(new BytesRangeReader(bad))).rejects.toThrow(/magic/i)
  })

  it('treats featuresCount 0 as UNKNOWN: the scan runs to EOF', async () => {
    // Asserting `typeof === 'number'` on a nonzero-count file pins nothing.
    // Task 2 generates no_count.fcb -- the same input as small, written with
    // features_count left at 0 -- so this is testable at all.
    const reader = open('no_count.fcb')
    const { info } = await readHeader(reader)
    expect(info.featuresCount).toBe(0)
    const r = await FcbReader.fromReader(reader)
    let n = 0
    for await (const _ of await r.selectAll()) n++
    expect(n).toBe(SMALL.featuresCount)     // every feature, despite the 0
  })
})

describe('AttributeIndex struct', () => {
  // Pinned from Step 0: run the C++ reader over duplicate_keys.fcb and print
  // each index's column, length and branching factor. Iterating "whatever
  // entries came back" passes for a reader that returns NONE.
  const EXPECTED_INDICES = [
    { columnIndex: 0, length: 0, branchingFactor: 0 },   // <- real values
    { columnIndex: 1, length: 0, branchingFactor: 0 },
  ]

  it('decodes every declared index with its exact fields', async () => {
    // Field order in header.fbs forces 2 bytes of padding after each ushort,
    // making the struct 16 bytes; reading it as 12 walks into the next entry
    // and yields plausible-looking nonsense rather than an error.
    const { info } = await readHeader(open('duplicate_keys.fcb'))
    expect(info.attributeIndices).toHaveLength(EXPECTED_INDICES.length)
    info.attributeIndices.forEach((ai, i) => {
      expect(ai.columnIndex).toBe(EXPECTED_INDICES[i]!.columnIndex)
      expect(ai.length).toBe(EXPECTED_INDICES[i]!.length)
      expect(ai.branchingFactor).toBe(EXPECTED_INDICES[i]!.branchingFactor)
    })
  })

  it('gives each index a begin offset that follows the previous one', async () => {
    const { info, layout } = await readHeader(open('duplicate_keys.fcb'))
    const sorted = [...info.attributeIndices].sort((a, b) => a.columnIndex - b.columnIndex)
    let expected = layout.attrIndexBegin
    for (const ai of sorted) {
      expect(ai.begin).toBe(expected)
      expected += ai.length
    }
    // The cumulative sum must land EXACTLY on the feature section.
    expect(expected).toBe(layout.featureBegin)
  })
})
```

- [ ] **Step 2: Run, watch fail.** Expected: cannot resolve `../src/header/index.js`.

- [ ] **Step 3: Implement.** `readHeader` reads the first 8 bytes, validates the magic, reads the 4-byte size prefix, validates it against `MAX_HEADER_SIZE` and `reader.size()`, reads `8 .. 12 + headerSize` and hands that slice (prefix **included**) to `Header.getSizePrefixedRootAsHeader`. It then builds `FileInfo`, computes the layout with `computeLayout`, and calls `validateLayoutAgainstSize`.

- [ ] **Step 4: Run, expect PASS.**

- [ ] **Step 5: Commit**

```bash
git add src/ts/src/header src/ts/test/header.test.ts
git commit -m "feat(ts): header parsing, file info, and the 16-byte attribute index struct"
```

---

## Task 7: Blob and Node file sources

**Files:**
- Create: `src/ts/src/io/blob.ts`, `src/ts/src/io/node.ts`, `src/ts/test/sources.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export class BlobRangeReader implements RangeReader { constructor(blob: Blob) }
  // src/io/node.ts — reachable only via the "./node" subpath export
  export class FileRangeReader implements RangeReader {
    static open(path: string): Promise<FileRangeReader>
    close(): Promise<void>
  }
  export function fromFile(path: string): Promise<FcbReader>   // added in Task 8
  ```

`BlobRangeReader` uses `blob.slice(o, o + n).arrayBuffer()`; `blob.size` gives `size()` synchronously. `FileRangeReader` uses `fs.promises.open` and `filehandle.read` into a fresh buffer; `open()` is async because it `stat`s for the size.

**`node.ts` must be the only file importing `node:*`.** A browser bundle that resolves `node:fs` is a build failure for consumers.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/sources.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import { BlobRangeReader } from '../src/io/blob.js'
import { FileRangeReader } from '../src/io/node.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const PATH = resolve(CORPUS, 'small.fcb')
const BYTES = new Uint8Array(readFileSync(PATH))

describe('BlobRangeReader', () => {
  it('reports size synchronously and serves exact ranges', async () => {
    const r = new BlobRangeReader(new Blob([BYTES]))
    expect(r.size()).toBe(BYTES.length)
    expect(Array.from(await r.read(8, 4))).toEqual(Array.from(BYTES.subarray(8, 12)))
  })

  it('rejects a read past the end', async () => {
    const r = new BlobRangeReader(new Blob([BYTES]))
    await expect(r.read(BYTES.length - 2, 8)).rejects.toThrow(FcbError)
  })
})

describe('FileRangeReader', () => {
  it('serves the same bytes as reading the whole file', async () => {
    const r = await FileRangeReader.open(PATH)
    try {
      expect(r.size()).toBe(BYTES.length)
      expect(Array.from(await r.read(8, 4))).toEqual(Array.from(BYTES.subarray(8, 12)))
    } finally {
      await r.close()
    }
  })

  it('reports a missing file as an FcbError, not a raw ENOENT', async () => {
    await expect(FileRangeReader.open(resolve(CORPUS, 'nope.fcb'))).rejects.toThrow(FcbError)
  })
})
```

- [ ] **Step 2: Run, watch fail.**
- [ ] **Step 3: Implement both sources.**
- [ ] **Step 4: Run, expect PASS,** and confirm the isolation holds:

```bash
cd src/ts && grep -rn "node:" src/ --include=*.ts | grep -v "src/io/node.ts"
```
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add src/ts/src/io/blob.ts src/ts/src/io/node.ts src/ts/test/sources.test.ts
git commit -m "feat(ts): Blob and node:fs range readers"
```

---

## Task 8: Feature framing, sequential scan, per-object attributes

**Files:**
- Create: `src/ts/src/feature/index.ts`, `src/ts/src/feature/attribute.ts`, `src/ts/src/reader.ts`, `src/ts/test/features.test.ts`, `src/ts/test/attributes.test.ts`
- Modify: `src/ts/src/index.ts`, `src/ts/src/io/node.ts` (add `fromFile`)

**Interfaces:**
- Produces:
  ```ts
  export type JsonValue = null | boolean | number | string | JsonValue[] | { [k: string]: JsonValue }
  export type AttrValue = number | bigint | string | boolean | Uint8Array | JsonValue | null

  export class CityObjectView {
    readonly id: string
    readonly type: string
    hasAttributes(): boolean          // DECLARES an attributes vector
    hasColumns(): boolean             // overrides the header schema
    attributes(): Record<string, AttrValue>
  }
  export class Feature {
    readonly id: string
    cityObjects(): CityObjectView[]
    attributes(objectIndex: number): Record<string, AttrValue>
    vertices(): Int32Array
  }
  export function decodeAttributes(blob: Uint8Array, schema: readonly ColumnInfo[]): Record<string, AttrValue>

  export class FcbReader {
    static fromReader(reader: RangeReader): Promise<FcbReader>   // the primitive
    static fromBytes(bytes: Uint8Array): Promise<FcbReader>
    static fromBlob(blob: Blob): Promise<FcbReader>
    get header(): HeaderView
    selectAll(): Promise<FeatureCursor>
    close(): Promise<void>        // releases the underlying reader, if it holds one
  }
  export interface FeatureCursor extends AsyncIterable<Feature> {
    readonly featuresCount: number | undefined
  }
  ```

`fromReader` is the primitive every other constructor and every later task builds on — Task 7's `fromFile`, Task 11's `fromUrl`, and the request-log tests all call it directly.

`close()` exists because `fromFile` opens a `node:fs` handle that must stay open for later queries and therefore cannot be closed inside `fromFile`. For readers with nothing to release it resolves immediately. Also implement `Symbol.asyncDispose` so `await using` works.

**Attribute schema resolution is PER OBJECT.** `CityObject.columns` overrides `Header.columns` whenever set, and this is the normal case: in `examples/data/delft.fcb` all 1115 objects with attributes declare their own columns and the header's 44 are never used. Attribute blobs are **not self-delimiting** — each value's width comes from its column type — so a wrong schema desynchronises the rest of the blob and yields plausible garbage rather than an error.

**Emit `attributes` iff the object DECLARES an attributes vector.** Present-but-empty becomes `{}`; absent is omitted entirely. The corpus distinguishes these.

**Each feature's bytes are copied into a fresh `ArrayBuffer` at offset 0** (hazard 9), which is what makes the `Feature` handle durable and every generated `*Array()` accessor safe.

**`Long`/`ULong` decode to `bigint`, always.** `Date`/`DateTime` columns decode to an **ISO-8601 string**, not a `Date` — matching Rust, and because `Date` cannot hold the key's nanoseconds.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/features.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbReader } from '../src/reader.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const DATA = resolve(__dirname, '../../../examples/data')
const open = async (p: string) =>
  FcbReader.fromBytes(new Uint8Array(readFileSync(p)))

describe('sequential scan', () => {
  it('iterates a single-feature file exactly once', async () => {
    const r = await open(resolve(CORPUS, 'single_feature.fcb'))
    const cursor = await r.selectAll()
    const seen = []
    for await (const f of cursor) seen.push(f)
    expect(seen).toHaveLength(1)
  })

  it('yields exactly featuresCount features for small.fcb', async () => {
    const r = await open(resolve(CORPUS, 'small.fcb'))
    const cursor = await r.selectAll()
    let n = 0
    for await (const _ of cursor) n++
    expect(n).toBe(r.header.info.featuresCount)
  })

  it('yields durable handles that survive a reader which REUSES its buffer', async () => {
    // fromBytes owns an immutable whole-file copy, so subarray-backed
    // features would stay valid there and this test would pass even without
    // per-feature copying. Go through a reader whose buffer is replaced on
    // every read -- that is the case the copy exists for.
    const raw = new Uint8Array(readFileSync(resolve(CORPUS, 'small.fcb')))
    const churning = {
      size: () => raw.length,
      async read(offset: number, length: number) {
        // A fresh buffer each time, then scribbled over on the NEXT read.
        const b = new Uint8Array(length)
        b.set(raw.subarray(offset, offset + length))
        churning.last?.fill(0xdd)
        churning.last = b
        return b
      },
      last: undefined as Uint8Array | undefined,
    }
    const r = await FcbReader.fromReader(churning)
    const held = []
    for await (const f of await r.selectAll()) held.push(f)
    expect(held.length).toBeGreaterThan(1)
    // Touch the FIRST feature after the cursor has moved far past it, and
    // touch a generated array accessor, which is what aliasing breaks.
    expect(held[0]!.id).not.toBe(held[held.length - 1]!.id)
    expect(held[0]!.vertices().length).toBeGreaterThan(0)
    expect(held[0]!.cityObjects().length).toBeGreaterThan(0)
  })

  it('serializes overlapping next() calls instead of interleaving position', async () => {
    // A native async generator gives this for free. Both must resolve to
    // DIFFERENT features -- interleaved position updates would return the
    // same one twice or skip one.
    const r = await open(resolve(CORPUS, 'small.fcb'))
    const it = (await r.selectAll())[Symbol.asyncIterator]()
    const [a, b] = await Promise.all([it.next(), it.next()])
    expect(a.value.id).not.toBe(b.value.id)
  })
})

describe('attribute schema resolution', () => {
  it('uses each object OWN columns when it declares them', async () => {
    const r = await open(resolve(DATA, 'delft.fcb'))
    let checked = 0
    for await (const f of await r.selectAll()) {
      f.cityObjects().forEach((o, i) => {
        if (!o.hasAttributes() || !o.hasColumns()) return
        // A wrong schema shows up as a nonsense key, not an exception:
        // during the C++ port it surfaced as column index 28777, which is
        // ASCII "ip" from the middle of a string value.
        for (const key of Object.keys(f.attributes(i))) {
          expect(key).toMatch(/^[\x20-\x7e]+$/)
          checked++
        }
      })
    }
    expect(checked).toBeGreaterThan(0)
  })

  it('distinguishes an absent attributes vector from an empty one', async () => {
    const r = await open(resolve(CORPUS, 'small.fcb'))
    for await (const f of await r.selectAll()) {
      f.cityObjects().forEach((o, i) => {
        if (!o.hasAttributes()) return
        expect(f.attributes(i)).toBeTypeOf('object')
      })
    }
  })
})
```

```ts
// src/ts/test/attributes.test.ts
import { describe, expect, it } from 'vitest'
import { ColumnType } from '../src/generated/column-type.js'
import { decodeAttributes } from '../src/feature/attribute.js'

const col = (index: number, name: string, type: ColumnType) =>
  ({ index, name, type, nullable: true })

/** Attribute records are `u16 column_index` then the value, back to back. */
const rec = (index: number, body: number[]) => {
  const out = new Uint8Array(2 + body.length)
  new DataView(out.buffer).setUint16(0, index, true)
  out.set(body, 2)
  return out
}
const concat = (...parts: Uint8Array[]) => {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
  let o = 0
  for (const p of parts) { out.set(p, o); o += p.length }
  return out
}

describe('decodeAttributes', () => {
  it('decodes an int and a bool against their schema', () => {
    const schema = [col(0, 'n', ColumnType.Int), col(1, 'b', ColumnType.Bool)]
    const blob = concat(rec(0, [0x2a, 0, 0, 0]), rec(1, [1]))
    expect(decodeAttributes(blob, schema)).toEqual({ n: 42, b: true })
  })

  it('decodes Long as bigint ALWAYS, never as a number', () => {
    // Data-dependent types make sorting and serialization behave differently
    // the day one large value appears. See the design doc, 64-bit policy.
    const schema = [col(0, 'big', ColumnType.Long)]
    const body = new Uint8Array(8)
    new DataView(body.buffer).setBigInt64(0, 3n, true)
    expect(decodeAttributes(concat(rec(0, Array.from(body))), schema))
      .toEqual({ big: 3n })
  })

  it('decodes Byte as u8, matching the WRITER (Rust reader disagrees)', () => {
    // Deliberate divergence #1: the writer stores Byte as u8, the Rust
    // reader decodes i8, so stored values > 127 come back negative there.
    const schema = [col(0, 'b', ColumnType.Byte)]
    expect(decodeAttributes(concat(rec(0, [200])), schema)).toEqual({ b: 200 })
  })

  it('returns an empty object for an empty blob', () => {
    expect(decodeAttributes(new Uint8Array(0), [])).toEqual({})
  })

  it('throws on a column index that is not in the schema', () => {
    // Cannot be skipped: the record is not self-delimiting, so the rest of
    // the blob is unreadable once alignment is lost.
    expect(() => decodeAttributes(concat(rec(99, [1])), [col(0, 'n', ColumnType.Bool)]))
      .toThrow()
  })

  it('throws on a truncated value rather than reading past the blob', () => {
    const schema = [col(0, 'n', ColumnType.Int)]
    expect(() => decodeAttributes(concat(rec(0, [1, 2])), schema)).toThrow()
  })
})
```

- [ ] **Step 2: Run, watch fail.**

- [ ] **Step 3: Implement.** Order: `attribute.ts` (pure, no I/O), then `feature/index.ts` (framing + `Feature`), then `reader.ts` (`FcbReader.fromBytes`/`fromBlob`/`fromReader`, `selectAll`, the cursor).

Framing, precisely: read the 4-byte LE prefix; validate `0 < len <= MAX_FEATURE_SIZE` and `offset + 4 + len <= size()`; then copy **all `4 + len` bytes — prefix included** — into a fresh `Uint8Array` and hand that to `CityFeature.getSizePrefixedRootAsCityFeature`. A size-prefixed accessor reads the prefix itself; handing it a body-only buffer misparses. The reference does exactly this (`src/cpp/src/reader.cpp:182`, `:196`) and the format reference says the prefix is included (`.llm/docs/specification.md:112`).

The copy is what makes the handle durable and every generated `*Array()` accessor safe (hazard 9): a fresh buffer starts at offset 0, so FlatBuffers' internal alignment holds.

Scan to EOF rather than to `featuresCount` — `0` means unknown.

**Use a native async generator for the cursor.** It serializes overlapping `next()` calls itself — a second `next()` before the first settles is queued by the language, not interleaved — so there is no hand-rolled in-flight flag and no `ReentrantIteration` error path to maintain. Keep `featuresCount` on the returned object that wraps the generator.

Add to `src/io/node.ts`:

```ts
export async function fromFile(path: string): Promise<FcbReader> {
  return FcbReader.fromReader(await FileRangeReader.open(path))
}
```

- [ ] **Step 4: Run, expect PASS.**

- [ ] **Step 5: Commit**

```bash
git add src/ts/src/feature src/ts/src/reader.ts src/ts/src/index.ts src/ts/src/io/node.ts src/ts/test
git commit -m "feat(ts): feature framing, sequential scan, per-object attribute decoding"
```

---

## Task 9: Geometry — boundaries, semantics, appearance

**Files:**
- Create: `src/ts/src/geometry/index.ts`, `boundaries.ts`, `semantics.ts`, `appearance.ts`, `src/ts/test/geometry.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export function decodeBoundaries(solids: Uint32Array, shells: Uint32Array, surfaces: Uint32Array, strings: Uint32Array, indices: Uint32Array): unknown[]
  export function decodeMaterialValues(solids: readonly number[], shells: readonly number[], values: readonly number[]): unknown
  export function decodeTextureValues(solids: readonly number[], shells: readonly number[], surfaces: readonly number[], strings: readonly number[], values: readonly number[]): unknown
  export function decodeSemantics(...): { surfaces: unknown[]; values: unknown }
  /** Reads the OPTIONAL shared-material scalar. Returns undefined when the
   *  field is absent and 0 when it is present and zero -- a distinction
   *  `if (m.value())` destroys. */
  export function sharedMaterialValue(m: { value(): number | null }): number | undefined
  ```

Port from `src/cpp/src/geometry.cpp` — the C++ version is current, including finding #8's fixes. Rules that are easy to get subtly wrong:

- **Collapse applies only at the OUTERMOST level.** Inner levels always wrap. Getting this backwards produces output one level off that still looks structurally plausible.
- **`u32::MAX` → `null`** in semantics values and in both appearance index arrays. Compare `=== 0xFFFFFFFF`; never `|0` or `~x` (hazard 6).
- **A single Solid drops the solid level; so does `solids === [1]`.** That second half is finding #8 — do not reintroduce a `solids[0] > 1` guard.
- **A single-string MultiLineString keeps its depth** — no `strings.length > 1` guard.
- **An empty (as opposed to absent) material/texture mapping vector omits the key entirely**; only a vector whose mappings were all skipped yields `{}`. The `empty_appearance` fixture covers this.
- **Optional scalars use `!== null`** (hazard 5). `MaterialMapping.value()` returning `0` is a real shared-material 0, and `if (value())` silently drops it.

- [ ] **Step 1: Write the failing unit tests.** Reuse the expected values from `src/cpp/tests/test_geometry.cpp` — they were taken from the Rust functions via the oracle technique and are known good.

```ts
// src/ts/test/geometry.test.ts
import { describe, expect, it } from 'vitest'
import { decodeMaterialValues, decodeTextureValues } from '../src/geometry/appearance.js'

describe('material values', () => {
  it('drops the solid level for a solid of one shell (finding #8)', () => {
    // Do NOT reintroduce a `solids[0] > 1` guard: solids === [1] collapses too.
    expect(decodeMaterialValues([1], [2], [7, 8])).toEqual([[7, 8]])
  })

  it('turns u32::MAX into null', () => {
    expect(decodeMaterialValues([], [], [0xffffffff, 1, 0])).toEqual([null, 1, 0])
  })

  it('keeps a material index of 0, which is falsy in JS', () => {
    expect(decodeMaterialValues([], [], [0])).toEqual([0])
  })
})

describe('the OPTIONAL MaterialMapping.value scalar', () => {
  // This is a DIFFERENT field from the values vector above -- a nullable
  // uint on the mapping itself (src/fbs/geometry.fbs:51), handled outside
  // decodeMaterialValues (cf. src/cpp/src/cityjson.cpp:215). Testing the
  // vector does not exercise it, so `if (mapping.value())` can stay broken
  // while every test above passes.
  it('distinguishes an absent shared value from a shared value of 0', () => {
    expect(sharedMaterialValue({ value: () => null })).toBeUndefined()
    expect(sharedMaterialValue({ value: () => 0 })).toBe(0)   // falsy but PRESENT
    expect(sharedMaterialValue({ value: () => 3 })).toBe(3)
  })
})

describe('texture values', () => {
  it('keeps the depth of a single-string MultiLineString', () => {
    expect(decodeTextureValues([], [], [1], [4], [0, 10, 11, 12]))
      .toEqual([[0, 10, 11, 12]])
    // The MultiSurface look-alike is distinguished by its shells entry.
    expect(decodeTextureValues([], [1], [1], [4], [0, 10, 11, 12]))
      .toEqual([[[0, 10, 11, 12]]])
  })
})
```

Add equivalent cases for `decodeBoundaries` and `decodeSemantics`, ported from `src/cpp/tests/test_geometry.cpp`. If a needed expected value is not in the C++ tests, obtain it with the oracle technique — do not hand-derive it.

- [ ] **Step 2: Run, watch fail.**
- [ ] **Step 3: Implement**, resolving hazard 5 for every optional scalar.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** — `feat(ts): decode geometry boundaries, semantics and appearance`

---

## Task 10: CityJSON emission and the conformance suite

**Files:**
- Create: `src/ts/src/cityjson/index.ts`, `src/ts/src/cityjson/types.ts`, `src/ts/test/conformance.test.ts`
- Modify: `src/ts/src/feature/index.ts` (add `Feature.toCityJSON`), `src/ts/src/reader.ts` (add `cityjson()`)

**Interfaces:**
- Produces:
  ```ts
  export interface Int64Policy { int64?: 'lossy-number' | 'decimal-string' | 'error' }
  export function toCityJSONMetadata(header: HeaderView): CityJSON
  export function toCityJSONFeature(feature: Feature, header: HeaderView, opts?: Int64Policy): CityJSONFeature
  ```

`int64` defaults to `'lossy-number'` so the emitted object is always JSON-serializable and conformance comparison works. `'error'` throws on a value outside the safe range; `'decimal-string'` emits it as a string.

**Per-feature `appearance`** (materials, textures, `vertices-texture`) must be emitted. C++ forgot it initially and the conformance test did not catch it **because it compared only selected keys** — which is exactly why the test below compares whole lines.

- [ ] **Step 1: Write the conformance test — compare WHOLE lines**

```ts
// src/ts/test/conformance.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbReader } from '../src/reader.js'
import { toCityJSONFeature, toCityJSONMetadata } from '../src/cityjson/index.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const CASES = ['small', 'geom_temp', 'noise_extension', 'single_feature',
  'long_strings', 'duplicate_keys', 'degenerate_extent',
  'inferable_types', 'empty_appearance']

describe.each(CASES)('conformance: %s', (name) => {
  it('matches the Rust reader line for line', async () => {
    const expected = readFileSync(resolve(CORPUS, `${name}.expected.jsonl`), 'utf8')
      .split('\n').filter((l) => l.trim()).map((l) => JSON.parse(l))

    const r = await FcbReader.fromBytes(
      new Uint8Array(readFileSync(resolve(CORPUS, `${name}.fcb`))))
    const actual: unknown[] = [toCityJSONMetadata(r.header)]
    for await (const f of await r.selectAll()) {
      actual.push(toCityJSONFeature(f, r.header))
    }

    expect(actual).toHaveLength(expected.length)

    // Compare the WHOLE line, metadata included. Comparing selected keys is
    // what hid the missing per-feature `appearance` object through the whole
    // C++ port -- and a selected-key metadata check lets an implementation
    // omit the extent, the CRS, the identifier and the title and still pass.
    for (let i = 0; i < actual.length; i++) {
      expect(actual[i], `${name} line ${i}`).toEqual(expected[i])
    }
  })
})
```

If line 0 does not match because the Rust writer round-trips an optional metadata field this reader does not reproduce, that is a decision to make **explicitly and once**: either emit the field, or add it to a named `KNOWN_METADATA_GAPS` set with a comment saying why. Do not weaken the comparison to make a specific case pass.

```ts
```

The int64 policy needs its own test, and it needs a value that actually exercises it. `inferable_types` holds only `-42` and `42`, both safe integers, so all three policies emit the same thing and any test over it is vacuous. **Test the emitter directly** against a synthetic attribute record rather than hunting for a fixture:

```ts
import { emitInt64 } from '../src/cityjson/index.js'

describe('int64 policy', () => {
  const BIG = 9007199254740993n     // 2^53 + 1: NOT representable as a number

  it('defaults to a lossy number, keeping the output JSON-serializable', () => {
    expect(emitInt64(BIG, 'lossy-number')).toBe(9007199254740992)  // rounded
    expect(() => JSON.stringify({ v: emitInt64(BIG, 'lossy-number') })).not.toThrow()
  })

  it('emits an exact decimal string when asked', () => {
    expect(emitInt64(BIG, 'decimal-string')).toBe('9007199254740993')
  })

  it('throws on an unsafe value under the error policy', () => {
    expect(() => emitInt64(BIG, 'error')).toThrow(FcbError)
    expect(emitInt64(42n, 'error')).toBe(42)      // safe values pass through
  })

  it('never leaks a bigint into the emitted object under ANY policy', () => {
    for (const p of ['lossy-number', 'decimal-string', 'error'] as const) {
      if (p === 'error') continue
      expect(typeof emitInt64(42n, p)).not.toBe('bigint')
    }
  })
})
```

Add `emitInt64(value: bigint, policy: 'lossy-number' | 'decimal-string' | 'error'): number | string` to the produced interface above.

- [ ] **Step 2: Run it.** Expect failures; each one is a real defect. Fix until green.

```bash
cd src/ts && npx vitest run test/conformance.test.ts
```

- [ ] **Step 3: Ask codex to review stage C**

```bash
codex exec --model gpt-5.6-sol --sandbox read-only "Review the pure-TypeScript FlatCityBuf reader in src/ts/src against the Rust reference in src/rust/fcb_core and the C++ port in src/cpp/src. Focus on: (1) any place the TypeScript decodes a different value than the C++ for the same bytes, especially optional FlatBuffers scalars, u32::MAX null sentinels, signed-vs-unsigned reads, and BigInt boundaries; (2) the collapse rules in geometry/boundaries.ts and geometry/appearance.ts against findings #7 and #8 in docs/upstream-findings.md; (3) tests that would pass even if the code were wrong. Cite file:line and give a concrete input for each finding."
```

- [ ] **Step 4: Act on findings.**

- [ ] **Step 5: Commit** — `feat(ts): CityJSON emission, conformant against the shared corpus`

---

## Task 11: HTTP range reader

**Files:**
- Create: `src/ts/src/io/fetch.ts`, `src/ts/test/http.test.ts`
- Modify: `src/cpp/tests/range_server.py` (add CORS headers), `src/ts/src/reader.ts` (add `FcbReader.fromUrl`)

**Interfaces:**
- Produces:
  ```ts
  export interface FetchRangeReaderOpts { fetchSize?: number; signal?: AbortSignal; fetch?: typeof globalThis.fetch }
  export class FetchRangeReader implements RangeReader {
    static open(url: string, opts?: FetchRangeReaderOpts): Promise<FetchRangeReader>
  }
  // reader.ts
  static fromUrl(url: string, opts?: FetchRangeReaderOpts): Promise<FcbReader>
  ```

Constants: Format Reference → "HTTP constants". 1 MB default fetch; the open prefetch is `2024 + (1 + 16 + 256) * 40 = 12944` bytes, which buys magic + header + the top three R-tree levels in one request.

`src/cpp/tests/range_server.py` already implements `?ignore_range=1` (200 with the whole body), `?bad_range=1` (malformed `Content-Range`) and `?wrong_offset=1` (a range the client did not ask for). **It sends no CORS headers** — add `Access-Control-Allow-Origin: *` and `Access-Control-Expose-Headers: Content-Range, Content-Length`, and a `?no_cors_expose=1` mode that omits the second one so the browser failure path can be tested.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/http.test.ts
import { spawn, type ChildProcess } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { FcbError } from '../src/errors.js'
import { FetchRangeReader } from '../src/io/fetch.js'
import { FcbReader } from '../src/reader.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const SERVER = resolve(__dirname, '../../cpp/tests/range_server.py')
let proc: ChildProcess
let base: string

beforeAll(async () => {
  proc = spawn('python3', [SERVER, CORPUS])
  base = await new Promise<string>((ok) => {
    proc.stdout!.on('data', (d: Buffer) => {
      const m = /(\d+)/.exec(d.toString())
      if (m) ok(`http://127.0.0.1:${m[1]}`)
    })
  })
})
afterAll(() => { proc.kill() })

describe('FetchRangeReader', () => {
  it('learns its size from Content-Range at open', async () => {
    const r = await FetchRangeReader.open(`${base}/small.fcb`)
    expect(r.size()).toBe(readFileSync(resolve(CORPUS, 'small.fcb')).length)
  })

  it('serves the same bytes as the local reader', async () => {
    const local = new Uint8Array(readFileSync(resolve(CORPUS, 'small.fcb')))
    const r = await FetchRangeReader.open(`${base}/small.fcb`)
    expect(Array.from(await r.read(8, 16))).toEqual(Array.from(local.subarray(8, 24)))
  })

  it('THROWS when the server ignores Range and returns 200', async () => {
    // The wasm client accepts this and every later offset reads garbage.
    await expect(FetchRangeReader.open(`${base}/small.fcb?ignore_range=1`))
      .rejects.toThrow(FcbError)
  })

  it('throws on a malformed Content-Range', async () => {
    await expect(FetchRangeReader.open(`${base}/small.fcb?bad_range=1`))
      .rejects.toThrow(FcbError)
  })

  it('throws when the server returns a DIFFERENT range than requested', async () => {
    // Indistinguishable from success unless the start/end are checked.
    await expect(FetchRangeReader.open(`${base}/small.fcb?wrong_offset=1`))
      .rejects.toThrow(FcbError)
  })

  it('aborts in-flight requests when the signal fires', async () => {
    const ac = new AbortController()
    const r = await FetchRangeReader.open(`${base}/small.fcb`)
    ac.abort()
    await expect(r.read(0, 16, { signal: ac.signal })).rejects.toThrow()
  })
})

describe('FcbReader.fromUrl', () => {
  it('scans a remote file to the same CityJSON as the local one', async () => {
    const remote = await FcbReader.fromUrl(`${base}/small.fcb`)
    const local = await FcbReader.fromBytes(
      new Uint8Array(readFileSync(resolve(CORPUS, 'small.fcb'))))
    const ids = async (r: FcbReader) => {
      const out: string[] = []
      for await (const f of await r.selectAll()) out.push(f.id)
      return out
    }
    expect(await ids(remote)).toEqual(await ids(local))
  })

  it('opens with ONE request, not one per section', async () => {
    // The 12944-byte prefetch buys magic + header + the top 3 rtree levels.
    let calls = 0
    const counting: typeof fetch = (...args) => { calls++; return fetch(...args) }
    await FcbReader.fromUrl(`${base}/small.fcb`, { fetch: counting })
    expect(calls).toBe(1)
  })
})
```

- [ ] **Step 2: Run, watch fail.**
- [ ] **Step 3: Add the CORS headers to `range_server.py`, then implement `fetch.ts`.** On a `200` response to a Range request, call `controller.abort()` **before** awaiting the body — it may be gigabytes — and throw `ErrorCode.RangeNotSupported` with a message suggesting `fromBytes`. Validate the full `Content-Range`: start, end, total, and that the body length matches.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** — `feat(ts): HTTP range reader over fetch, with strict 206 validation`

---

## Task 12: Packed R-tree — bbox, pointIntersects, pagination

**Files:**
- Create: `src/ts/src/packed-rtree/index.ts`, `node-item.ts`, `search.ts`, `src/ts/test/packed-rtree.test.ts`
- Modify: `src/ts/src/reader.ts` (add `select()`)

**Interfaces:**
- Produces:
  ```ts
  export interface NodeItem { minX: number; minY: number; maxX: number; maxY: number; offset: number }
  export interface SearchResultItem { offset: number; index: number }   // offset is feature-section-relative
  export type SpatialQuery =
    | { kind: 'bbox'; value: [number, number, number, number] }
    | { kind: 'point'; value: [number, number] }
    | { kind: 'nearest'; value: [number, number] }      // implemented in Task 16
  export function searchRtree(reader: RangeReader, rtreeBegin: number, numItems: number, nodeSize: number, query: SpatialQuery, opts?: ReadOpts): Promise<SearchResultItem[]>
  // reader.ts
  export type Operator = 'Eq' | 'Ne' | 'Gt' | 'Ge' | 'Lt' | 'Le'
  export interface AttrCondition { field: string; operator: Operator; value: unknown }
  export interface SelectOptions { spatial?: SpatialQuery; where?: AttrCondition[]; limit?: number; offset?: number; signal?: AbortSignal }
  select(opts?: SelectOptions): Promise<FeatureCursor>
  ```

**`Operator` and `AttrCondition` are declared HERE, in this task**, even though nothing consumes them until Task 14 — `SelectOptions.where` references them, so Task 12 would not type-check on its own otherwise. Task 14 imports them rather than redeclaring them. Until Task 14 lands, `select({ where })` throws `ErrorCode.QueryExecutionError` with "attribute queries not implemented yet", and a test pins that.

**Every search function takes `ReadOpts`** and threads the `AbortSignal` down to each `read`. A signal that only lives on the facade cancels nothing: the traversal is where the in-flight fetches are. Task 16's `searchNearest` and Task 14's `searchStree` take it too, and each has a test that aborts mid-descent.

Add a test helper `featureBounds(feature, header): {minX, minY, maxX, maxY}` in `test/fixtures/` that computes a feature's extent from its own vertices and the header transform. It is the brute-force oracle for every spatial assertion.

Read nodes with `le.ts` at `i * 40` — never through FlatBuffers. Format Reference → "Packed R-tree" for every rule, especially:
- `levelBounds[0]` is the **leaf** level and is **last** in storage order.
- Internal `offset` is a child *node index*; leaf `offset` is a *byte offset* relative to `featureBegin`.
- The **+1 leaf fetch rule**: when descending into level 0, extend the node range by one extra node, clamped to `levelBounds[0].end`, so the next offset is available to compute a feature's length.
- **Use `header.info.indexNodeSize`, never a hardcoded 16.** Both the wasm binding and `fcb_core`'s HTTP reader hardcode the default and silently mis-traverse files written with another node size — see Task 18.

`limit`/`offset` apply after the search, over the sorted result list; `featuresCount` reports the total match count regardless.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/packed-rtree.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbReader } from '../src/reader.js'
import { BufferedRangeReader, BytesRangeReader } from '../src/io/range-reader.js'
import { CountingReader } from './fixtures/counting-reader.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const bytes = (n: string) => new Uint8Array(readFileSync(resolve(CORPUS, n)))
const ids = async (c: AsyncIterable<{ id: string }>) => {
  const out: string[] = []
  for await (const f of c) out.push(f.id)
  return out
}

describe('bbox search', () => {
  it('returns every feature for a bbox covering the whole extent', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const all = await ids(await r.selectAll())
    const hit = await ids(await r.select({
      spatial: { kind: 'bbox', value: [e[0], e[1], e[3], e[4]] },
    }))
    expect(hit.sort()).toEqual(all.sort())
  })

  it('returns nothing for a bbox outside the extent', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const cursor = await r.select({
      spatial: { kind: 'bbox', value: [1e9, 1e9, 1e9 + 1, 1e9 + 1] },
    })
    expect(cursor.featuresCount).toBe(0)   // 0, never undefined
    expect(await ids(cursor)).toEqual([])
  })

  it('returns a PROPER SUBSET for a bbox covering part of the extent', async () => {
    // A whole-extent bbox proves nothing: a search that ignores the bbox
    // entirely passes it. The real assertion is that a partial box excludes
    // somebody, and that the survivors are the ones whose own bbox overlaps.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const half: [number, number, number, number] =
      [e[0], e[1], (e[0] + e[3]) / 2, (e[1] + e[4]) / 2]
    const all = await ids(await r.selectAll())
    const hit = await ids(await r.select({ spatial: { kind: 'bbox', value: half } }))
    expect(hit.length).toBeGreaterThan(0)
    expect(hit.length).toBeLessThan(all.length)   // something was excluded
    expect(hit.every((id) => all.includes(id))).toBe(true)
  })

  it('agrees with a brute-force scan over every feature bbox', async () => {
    // The oracle that cannot be tautological: compute the answer without the
    // R-tree at all, by scanning every feature and testing its own extent.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const box: [number, number, number, number] =
      [e[0], e[1], (e[0] + e[3]) / 2, (e[1] + e[4]) / 2]
    const brute: string[] = []
    for await (const f of await r.selectAll()) {
      const b = featureBounds(f, r.header)      // min/max over its vertices
      if (b.maxX >= box[0] && b.minX <= box[2] &&
          b.maxY >= box[1] && b.minY <= box[3]) brute.push(f.id)
    }
    const hit = await ids(await r.select({ spatial: { kind: 'bbox', value: box } }))
    expect(hit.sort()).toEqual(brute.sort())
  })

  it('treats pointIntersects as a degenerate bbox, against the brute oracle', async () => {
    // Comparing point search to bbox search lets BOTH be identically wrong.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const cx = (e[0] + e[3]) / 2, cy = (e[1] + e[4]) / 2
    const brute: string[] = []
    for await (const f of await r.selectAll()) {
      const b = featureBounds(f, r.header)
      if (b.minX <= cx && cx <= b.maxX && b.minY <= cy && cy <= b.maxY) brute.push(f.id)
    }
    const p = await ids(await r.select({ spatial: { kind: 'point', value: [cx, cy] } }))
    expect(p.sort()).toEqual(brute.sort())
  })

  it('honours a NON-DEFAULT index_node_size from the header', async () => {
    // Both the wasm binding and fcb_core's HTTP reader hardcode 16 here and
    // silently mis-traverse such files -- upstream finding, Task 18. Without
    // this fixture a hardcoded 16 passes the entire suite.
    // Generate it in Task 2: the same input as small, written with --node-size 8.
    const r = await FcbReader.fromBytes(bytes('small_node8.fcb'))
    expect(r.header.info.indexNodeSize).toBe(8)
    const e = r.header.info.geographicalExtent!
    const all = await ids(await r.selectAll())
    const hit = await ids(await r.select({
      spatial: { kind: 'bbox', value: [e[0], e[1], e[3], e[4]] },
    }))
    expect(hit.sort()).toEqual(all.sort())
  })

  it('rejects an inverted or non-finite bbox before doing any I/O', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({ spatial: { kind: 'bbox', value: [10, 10, 0, 0] } }))
      .rejects.toThrow(/invalid/i)
    await expect(r.select({ spatial: { kind: 'bbox', value: [NaN, 0, 1, 1] } }))
      .rejects.toThrow(/invalid/i)
  })
})

describe('pagination', () => {
  it('pages results while featuresCount still reports the total', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const all = await ids(await r.selectAll())
    const cursor = await r.select({ limit: 2, offset: 1 })
    expect(cursor.featuresCount).toBe(all.length)
    expect(await ids(cursor)).toEqual(all.slice(1, 3))
  })

  it('rejects a negative or fractional limit/offset', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({ limit: -1 })).rejects.toThrow(/invalid/i)
    await expect(r.select({ offset: 1.5 })).rejects.toThrow(/invalid/i)
  })
})

describe('request pattern', () => {
  it('reads the rtree in far fewer requests than it has nodes', async () => {
    // Correct-but-chatty is the failure mode nobody notices until it is on a
    // CDN. Assert the request LOG, not just the bytes.
    //
    // CRITICAL: a default 1 MB BufferedRangeReader swallows all of
    // small.fcb (20 KB) on the first read, so clearing inner.reads after
    // open would measure an already-warm cache and ZERO subsequent reads
    // would pass regardless of how bad the traversal planning is. Use a
    // buffer far smaller than the file so misses are real.
    const data = bytes('small.fcb')
    const inner = new CountingReader(data)
    const r = await FcbReader.fromReader(new BufferedRangeReader(inner, 512))
    const e = r.header.info.geographicalExtent!
    expect(inner.reads.reduce((n, x) => n + x.length, 0)).toBeLessThan(data.length)
    inner.reads.length = 0
    await r.select({ spatial: { kind: 'bbox', value: [e[0], e[1], e[3], e[4]] } })
    expect(inner.reads.length).toBeGreaterThan(0)              // it really read
    expect(inner.reads.length).toBeLessThan(r.header.info.featuresCount)
  })
})
```

- [ ] **Step 2: Run, watch fail.**
- [ ] **Step 3: Implement** `node-item.ts`, then `search.ts` (queue-driven descent, `await` at the top of the loop), then `select()` in `reader.ts`.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** — `feat(ts): packed R-tree bbox and point search, with pagination`

---

## Task 13: Attribute keys

**Files:**
- Create: `src/ts/src/static-btree/key.ts`, `src/ts/test/keys.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export type KeyKind = 'u8' | 'i16' | 'u16' | 'i32' | 'u32' | 'i64' | 'u64' | 'f32' | 'f64' | 'bool' | 'datetime' | 'str50' | 'str100'
  export function keyKindForColumn(t: ColumnType): KeyKind
  export function keySize(kind: KeyKind): number
  export function encodeKey(kind: KeyKind, value: unknown): Uint8Array
  /** Fixed-width string kinds decode to the raw padded Uint8Array, NOT to a
   *  string: a truncated key can end mid-UTF-8, and decoding then re-encoding
   *  would change the bytes the tree is ordered by. */
  export function decodeKey(kind: KeyKind, dv: DataView, offset: number): number | bigint | boolean | Uint8Array | { seconds: bigint; nanos: number }
  export function compareKeys(kind: KeyKind, a: unknown, b: unknown): number
  export function keyMin(kind: KeyKind): unknown
  export function keyMax(kind: KeyKind): unknown
  export function needsPostFilter(kind: KeyKind): boolean   // true for str50/str100
  ```

All seven encodings: Format Reference → "Attribute B+tree". `DateTime` is **12 bytes** (`i64` LE seconds then `u32` LE nanos). `FixedStringKey` is raw N bytes, zero-padded, truncated at the **byte** level.

**Column type → key kind, as the writer actually emits:** `Bool→bool, Byte→u8, UByte→u8, Short→i16, UShort→u16, Int→i32, UInt→u32, Long→i64, ULong→u64, Float→f32, Double→f64, String→str50, DateTime→datetime, Json→str100, Binary→str100`. `str20` is defined upstream but never produced, and `i8` is never produced either — the writer maps `Byte` to `u8` — so neither appears in `KeyKind`. `str100` does appear, because `keyKindForColumn` must classify `Json`/`Binary` columns in order for the query layer to reject them (Task 14).

**Reproduce all four deliberate divergences** and document them in the public docstring of the query API, as C++ and Python do.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/keys.test.ts
import { describe, expect, it } from 'vitest'
import { compareKeys, decodeKey, encodeKey, keyMax, keyMin, keySize } from '../src/static-btree/key.js'

const roundTrip = (kind: Parameters<typeof encodeKey>[0], v: unknown) => {
  const b = encodeKey(kind, v)
  return decodeKey(kind, new DataView(b.buffer, b.byteOffset, b.byteLength), 0)
}

describe('sizes', () => {
  it('gives DateTime twelve bytes: i64 seconds then u32 nanos', () => {
    expect(keySize('datetime')).toBe(12)
    expect(keySize('str50')).toBe(50)
    expect(keySize('str100')).toBe(100)
    expect(keySize('f64')).toBe(8)
  })
})

describe('round trips', () => {
  it('round-trips every numeric kind', () => {
    expect(roundTrip('i32', -5)).toBe(-5)
    expect(roundTrip('u32', 4294967295)).toBe(4294967295)
    expect(roundTrip('i64', -5n)).toBe(-5n)
    expect(roundTrip('u64', 18446744073709551615n)).toBe(18446744073709551615n)
    expect(roundTrip('f64', -1.5)).toBe(-1.5)
    expect(roundTrip('bool', true)).toBe(true)
  })

  it('stores floats as PLAIN IEEE-754 bits, with no total-order transform', () => {
    const b = encodeKey('f64', 1.5)
    const dv = new DataView(b.buffer, b.byteOffset, b.byteLength)
    expect(dv.getFloat64(0, true)).toBe(1.5)
  })
})

describe('float ordering is ordered_float, not JavaScript', () => {
  it('sorts NaN greatest and equal to itself', () => {
    const nan = Number.NaN
    expect(compareKeys('f64', nan, nan)).toBe(0)          // JS says NaN !== NaN
    expect(compareKeys('f64', nan, Number.POSITIVE_INFINITY)).toBeGreaterThan(0)
  })

  it('treats -0.0 and +0.0 as equal, unlike Object.is', () => {
    expect(compareKeys('f64', -0, 0)).toBe(0)             // Object.is says false
  })
})

describe('string keys', () => {
  it('truncates at the BYTE level, SPLITTING a UTF-8 sequence', () => {
    // 'é' is 2 bytes and 50 is even, so 'é'.repeat(40) truncates on a clean
    // boundary and demonstrates nothing. Use a 3-byte character: 16 of them
    // is 48 bytes, so the 17th is cut after its FIRST byte.
    const s = '☃'.repeat(20)                               // 60 bytes, 3 each
    const k = encodeKey('str50', s)
    expect(k).toHaveLength(50)
    expect(Array.from(k.subarray(48, 50)))
      .toEqual([0xe2, 0x98])                               // half a snowman
    // And it must still be usable: decoding is display-only and lossy.
    expect(() => new TextDecoder().decode(k)).not.toThrow()
  })

  it('decodes fixed-width keys as BYTES, never as a JS string', () => {
    // A truncated key can end mid-sequence; TextDecoder would replace those
    // bytes with U+FFFD and re-encoding would produce different bytes, so
    // the tree order would no longer be reproducible.
    const s = '☃'.repeat(20)
    const k = encodeKey('str50', s)
    const back = decodeKey('str50', new DataView(k.buffer, k.byteOffset, 50), 0)
    expect(back).toBeInstanceOf(Uint8Array)
    expect(Array.from(back as Uint8Array)).toEqual(Array.from(k))
  })

  it('zero-pads, so "a" and "a\\0" have the SAME key -- hence post-filtering', () => {
    expect(Array.from(encodeKey('str50', 'a')))
      .toEqual(Array.from(encodeKey('str50', 'a\0')))
  })

  it('compares as UTF-8 bytes, which disagrees with JS string order', () => {
    // JS: "｡" < "\u{10000}" is false (UTF-16 surrogates sort below U+FF61).
    // UTF-8 byte order says the opposite, and that is what the tree used.
    expect('｡' < '\u{10000}').toBe(false)
    expect(compareKeys('str50', '｡', '\u{10000}')).toBeLessThan(0)
  })
})

describe('column type mapping', () => {
  it('maps every ColumnType exactly as the WRITER emits it', () => {
    // Format Reference, "Column type -> key type". Enum values from
    // src/fbs/header.fbs:9-26 -- Byte=0 ... String=11, Json=12, DateTime=13,
    // Binary=14. Getting these off by one silently indexes the wrong column.
    expect(keyKindForColumn(ColumnType.Bool)).toBe('bool')
    expect(keyKindForColumn(ColumnType.Byte)).toBe('u8')     // u8, not i8
    expect(keyKindForColumn(ColumnType.UByte)).toBe('u8')
    expect(keyKindForColumn(ColumnType.Short)).toBe('i16')
    expect(keyKindForColumn(ColumnType.UShort)).toBe('u16')
    expect(keyKindForColumn(ColumnType.Int)).toBe('i32')
    expect(keyKindForColumn(ColumnType.UInt)).toBe('u32')
    expect(keyKindForColumn(ColumnType.Long)).toBe('i64')
    expect(keyKindForColumn(ColumnType.ULong)).toBe('u64')
    expect(keyKindForColumn(ColumnType.Float)).toBe('f32')
    expect(keyKindForColumn(ColumnType.Double)).toBe('f64')
    expect(keyKindForColumn(ColumnType.String)).toBe('str50')
    expect(keyKindForColumn(ColumnType.DateTime)).toBe('datetime')
    expect(keyKindForColumn(ColumnType.Json)).toBe('str100')
    expect(keyKindForColumn(ColumnType.Binary)).toBe('str100')
  })

  it('flags exactly the fixed-width string kinds for post-filtering', () => {
    expect(needsPostFilter('str50')).toBe(true)
    expect(needsPostFilter('str100')).toBe(true)
    expect(needsPostFilter('i32')).toBe(false)
    expect(needsPostFilter('f64')).toBe(false)
    expect(needsPostFilter('datetime')).toBe(false)
  })
})

describe('DateTime keys', () => {
  it('round-trips seconds and nanos independently', () => {
    const v = { seconds: 1700000000n, nanos: 123456789 }
    expect(roundTrip('datetime', v)).toEqual(v)
  })

  it('orders by seconds first, then by nanos', () => {
    const a = { seconds: 5n, nanos: 0 }
    const b = { seconds: 5n, nanos: 1 }
    const c = { seconds: 6n, nanos: 0 }
    expect(compareKeys('datetime', a, b)).toBeLessThan(0)
    expect(compareKeys('datetime', b, c)).toBeLessThan(0)
    expect(compareKeys('datetime', a, a)).toBe(0)
  })

  it('round-trips a NEGATIVE (pre-1970) timestamp, even though ranges hide it', () => {
    // The wire format is a signed i64; only the min_value sentinel is epoch 0.
    const v = { seconds: -86400n, nanos: 0 }
    expect(roundTrip('datetime', v)).toEqual(v)
  })
})

describe('narrow integer kinds', () => {
  it('round-trips the extremes of every width', () => {
    expect(roundTrip('u8', 255)).toBe(255)
    expect(roundTrip('i16', -32768)).toBe(-32768)
    expect(roundTrip('u16', 65535)).toBe(65535)
    expect(roundTrip('i32', -2147483648)).toBe(-2147483648)
    expect(roundTrip('f32', 0.5)).toBe(0.5)              // exact in binary32
  })

  it('orders u8 as UNSIGNED, which is the writer semantics for Byte', () => {
    // Deliberate divergence #1: Rust's reader decodes Byte as i8, so it
    // orders 200 below 100. The writer stores u8; we match the writer.
    expect(compareKeys('u8', 200, 100)).toBeGreaterThan(0)
  })
})

describe('sentinels reproduce the deliberate divergences', () => {
  it('uses +inf as the float maximum, so NaN keys are invisible to ranges', () => {
    expect(keyMax('f64')).toBe(Number.POSITIVE_INFINITY)
  })

  it('uses epoch 0 as the DateTime minimum, hiding pre-1970 timestamps', () => {
    expect(keyMin('datetime')).toEqual({ seconds: 0n, nanos: 0 })
  })

  it('uses the full i64/u64 range for 64-bit keys, as bigint', () => {
    expect(keyMax('u64')).toBe(18446744073709551615n)
    expect(keyMin('i64')).toBe(-(2n ** 63n))
  })
})
```

- [ ] **Step 2: Run, watch fail.**
- [ ] **Step 3: Implement.** `compareKeys` for floats special-cases NaN then uses `<`/`>`; for strings it compares the encoded `Uint8Array`s bytewise; for 64-bit kinds it compares `bigint`s directly.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** — `feat(ts): attribute key encodings, ordered_float comparison, sentinels`

---

## Task 14: Static B+tree traversal

**Files:**
- Create: `src/ts/src/static-btree/entry.ts`, `payload.ts`, `stree.ts`, `query.ts`, `src/ts/src/static-btree/index.ts`, `src/ts/test/stree.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export type Operator = 'Eq' | 'Ne' | 'Gt' | 'Ge' | 'Lt' | 'Le'
  export interface AttrCondition { field: string; operator: Operator; value: unknown }
  export const PAYLOAD_TAG = 0x8000000000000000n
  export function searchStree(reader: RangeReader, info: AttrIndexInfo, kind: KeyKind, op: Operator, value: unknown): Promise<SearchResultItem[]>
  export function searchAttributes(reader: RangeReader, header: HeaderView, conditions: readonly AttrCondition[]): Promise<SearchResultItem[]>
  ```

Format Reference → "Attribute B+tree" for the layout. Note especially: node size for **search** is `branchingFactor - 1` entries; the level-bounds loop breaks when **`n < branchingFactor`** (unlike the R-tree's `n === 1`); payload offsets are relative to the payload section start; there are **no leaf sibling pointers** — range scans walk the contiguous leaf array by index.

**Four things this task must get right that no Format Reference row will tell you.** All four are already solved in `src/cpp/src/stree.cpp`, with comments explaining why — read it before writing code.

1. **Do NOT port Rust's operator lowering.** The Format Reference documents it faithfully and it is a known-live defect: `Gt`/`Lt`/`Ne` are computed as a range *minus* `find_exact`, and the subtraction operates on **feature offsets**, so a feature whose CityObjects carry both `k` and `k' > k` is deleted from its own `Gt(k)` result (`docs/upstream-findings.md:130-145`, "NOT FIXED upstream"). **Follow C++: evaluate strict-or-inclusive bounds at the leaf** (`stree.cpp:380-400`). One traversal, no subtraction, no false negatives. For `Ne` on a non-string column that is **two half-open scans**, not a full scan minus the equal set.

2. **Strictness is INVERTED for string kinds, and this is not a bug.** Fixed-width keys are truncated, so ordering *after* the truncation point is invisible to the index: two values sharing a 50-byte prefix compare equal in the tree but may order either way in full. So for `str50`/`str100`:
   - `Gt` and `Lt` use **non-strict** bounds, widening the scan to keep the equal-prefix band alive for Task 15's post-filter to judge on the untruncated value. Using strict bounds here discards candidates *before* they can be verified — a false negative that no post-filter can recover.
   - `Ne` is a **full scan** of the leaf level, for the same reason: excluding the prefix matches would drop features whose value merely shares a prefix.
   - `Eq` returns candidates, not answers.

   Cited: `stree.cpp:371-400`, whose comment says exactly this.

3. **`Eq` on a type maximum must clamp its child descent.** Separator entries with no right sibling carry `K::max_value()` as a sentinel, and that sentinel's offset **already points at the last child group** — adding `node_size` walks off the end of the level. `Eq(true)` on a bool column is enough to trigger it. Clamp back when `child >= levels[childLevel].end` (`stree.cpp:205-222`).

4. **Range search must widen its upper scan by one node.** `find_partition` descends **left** on an exact hit, so when the upper bound is itself a separator key its matching leaf entry sits at exactly `upperIdx + nodeSize` — one past the un-widened end, and silently dropped. Use `min(upperIdx + 2 * nodeSize, leafEnd())`; the per-key filter rejects anything out of range, so widening costs at most one node read (`stree.cpp:282-292`, and `docs/upstream-findings.md:101`).

5. **`PAYLOAD_TAG` is a real bit in the wire data.** Test and strip it in BigInt, then convert. Never write the tag as `1 << 63`, which in JS is `-2147483648`.

**`Json` and `Binary` queries are REJECTED** with `ErrorCode.UnsupportedColumnType`, matching deliberate divergence #2 in Rust and C++. They are `FixedStringKey<100>` over an opaque blob, so index hits are near-meaningless. Task 15's post-filter therefore covers `String` columns only.

Multi-condition queries are AND-intersected sequentially with early exit on empty.

- [ ] **Step 0: Get the expected result SETS from the oracle before writing the test**

This task's tests need pinned feature-id lists, not shape assertions. `duplicate_keys.fcb` cannot serve: every one of its features has a single CityObject with a unique value, so no feature carries both `k` and a larger `k'` — the exact shape finding #5 breaks. That is why Task 2 adds `multi_object_attrs`.

Run the query through the C++ reader and record the **exact id list** for each operator:

```bash
# Add a temporary case to src/cpp/tests that opens
# conformance/multi_object_attrs.fcb and prints, for the indexed column,
# the sorted feature ids returned by Eq/Ne/Gt/Ge/Lt/Le against a chosen
# value. Run it, paste the lists below, then revert the injection.
cd src/cpp && cmake --build build-native -j8 && ./build-native/tests/fcb_tests
```

Choose the value so that **at least one feature holds both it and something larger** — that feature must appear in `Gt`'s list. If C++ omits it, C++ has the bug too and that is a finding, not a reason to weaken the test.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/stree.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { PAYLOAD_TAG } from '../src/static-btree/index.js'
import { FcbReader } from '../src/reader.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const bytes = (n: string) => new Uint8Array(readFileSync(resolve(CORPUS, n)))
const ids = async (c: AsyncIterable<{ id: string }>) => {
  const out: string[] = []
  for await (const f of c) out.push(f.id)
  return out
}

describe('payload tag', () => {
  it('is a bigint literal, because 1 << 63 in JS is -2147483648', () => {
    expect(PAYLOAD_TAG).toBe(0x8000000000000000n)
    expect(1 << 63).toBe(-2147483648)                 // the trap, pinned
  })

  it('survives Number() only when stripped in bigint first', () => {
    const tagged = PAYLOAD_TAG | 12345n
    // Number() ROUNDS the low bits; it does not necessarily erase them.
    // (Node 24 lands 12288 away from the tag, not 0 away -- so asserting
    // equality with Number(PAYLOAD_TAG) here would simply be false.)
    expect(Number(tagged)).not.toBe(12345 + Number(PAYLOAD_TAG))
    expect(Number(PAYLOAD_TAG | 1n)).toBe(Number(PAYLOAD_TAG))   // 1 vanishes
    // Stripping in bigint first is exact:
    expect(Number(tagged & (PAYLOAD_TAG - 1n))).toBe(12345)
  })

  it('recognises a tagged offset without losing the untagged case', () => {
    expect(isTagged(PAYLOAD_TAG | 7n)).toBe(true)
    expect(isTagged(7n)).toBe(false)
    expect(stripTag(PAYLOAD_TAG | 7n)).toBe(7n)
  })
})

// Paste the Step 0 oracle output here. Every assertion below compares
// against these PINNED lists, not against another query's result -- two
// queries can be identically wrong.
const H = 'h'                       // the indexed column
const PIVOT = 5
const ORACLE = {
  Eq: [/* ids from Step 0 */],
  Ne: [/* ... */],
  Gt: [/* MUST include the feature that holds both 5 and 9 */],
  Ge: [/* ... */],
  Lt: [/* ... */],
  Le: [/* ... */],
}

describe('attribute queries', () => {
  it.each(Object.keys(ORACLE) as Array<keyof typeof ORACLE>)(
    '%s matches the C++ reader exactly', async (op) => {
      const r = await FcbReader.fromBytes(bytes('multi_object_attrs.fcb'))
      const hit = await ids(await r.select({
        where: [{ field: H, operator: op, value: PIVOT }],
      }))
      expect(hit.sort()).toEqual([...ORACLE[op]].sort())
      expect(hit.length).toBeGreaterThan(0)     // an empty list proves nothing
    })

  it('Gt keeps a feature whose OTHER CityObject holds a smaller value', async () => {
    // Upstream finding #5, stated as the concrete regression. Rust computes
    // Gt as range-minus-exact over FEATURE offsets, so a feature carrying
    // both 5 and 9 is returned by the range (via 9), found by find_exact(5),
    // and then subtracted away -- a false negative for a genuine match.
    // multi_object_attrs.fcb exists to have exactly one such feature.
    const r = await FcbReader.fromBytes(bytes('multi_object_attrs.fcb'))
    const hit = await ids(await r.select({
      where: [{ field: H, operator: 'Gt', value: PIVOT }],
    }))
    expect(hit).toContain(BOTH_VALUES_FEATURE_ID)   // pinned in Step 0
  })

  it('Eq on the type maximum does not walk off the end of the level', async () => {
    // Separator entries with no right sibling carry K::max_value(), whose
    // offset ALREADY points at the last child group; adding node_size runs
    // past it. Eq(true) on a bool column is enough. (stree.cpp:205-222)
    const r = await FcbReader.fromBytes(bytes('inferable_types.fcb'))
    const b = r.header.info.columns.find((c) => c.type === ColumnType.Bool)
    if (b) {
      await expect(r.select({
        where: [{ field: b.name, operator: 'Eq', value: true }],
      })).resolves.toBeDefined()
    }
  })

  it('Le finds a value that is itself a separator key', async () => {
    // find_partition descends LEFT on an exact hit, so a separator-valued
    // upper bound lands one node past the un-widened scan end and used to be
    // dropped. (stree.cpp:282-292, upstream-findings.md:101)
    const r = await FcbReader.fromBytes(bytes('multi_object_attrs.fcb'))
    const le = await ids(await r.select({
      where: [{ field: H, operator: 'Le', value: SEPARATOR_VALUE }],   // Step 0
    }))
    expect(le.sort()).toEqual([...ORACLE_LE_SEPARATOR].sort())
  })

  it('AND-intersects multiple conditions', async () => {
    const r = await FcbReader.fromBytes(bytes('multi_object_attrs.fcb'))
    const both = await ids(await r.select({
      where: [
        { field: H, operator: 'Ge', value: PIVOT },
        { field: H, operator: 'Le', value: PIVOT },
      ],
    }))
    expect(both.sort()).toEqual([...ORACLE.Eq].sort())
  })

  it('rejects a query on a column with no attribute index', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({ where: [{ field: 'nope', operator: 'Eq', value: 1 }] }))
      .rejects.toThrow(/index/i)
  })

  it('rejects Json and Binary index queries, as Rust and C++ do', async () => {
    // Deliberate divergence #2. NOTE the enum values: String is 11, Json is
    // 12, DateTime is 13, Binary is 14 (src/fbs/header.fbs:9-26). Using a
    // literal 13 here would search for DateTime and silently skip the test.
    const r = await FcbReader.fromBytes(bytes('inferable_types.fcb'))
    const json = r.header.info.columns.find((c) => c.type === ColumnType.Json)
    expect(json, 'fixture must contain a Json column').toBeDefined()
    await expect(r.select({
      where: [{ field: json!.name, operator: 'Eq', value: '{}' }],
    })).rejects.toThrow(/unsupported column type/i)
  })

  it('aborts a traversal when the signal fires', async () => {
    const r = await FcbReader.fromBytes(bytes('multi_object_attrs.fcb'))
    const ac = new AbortController()
    ac.abort()
    await expect(r.select({
      where: [{ field: H, operator: 'Ge', value: PIVOT }], signal: ac.signal,
    })).rejects.toThrow()
  })
})
```

Every `/* ids from Step 0 */`, `BOTH_VALUES_FEATURE_ID`, `SEPARATOR_VALUE` and `ORACLE_LE_SEPARATOR` above is filled from the Step 0 run **before** implementing. A test whose expected value comes from the code under test proves nothing.

- [ ] **Step 2: Run, watch fail.**
- [ ] **Step 3: Implement** `entry.ts`, `payload.ts`, `stree.ts`, then `query.ts`.
- [ ] **Step 4: Cross-check against C++** by running the same queries through `fcb_tests` and comparing the result sets, not just the counts.
- [ ] **Step 5: Commit** — `feat(ts): static B+tree search with strict leaf bounds`

---

## Task 15: String post-filter and query composition

**Files:**
- Create: `src/ts/src/post-filter.ts`, `src/ts/test/post-filter.test.ts`
- Modify: `src/ts/src/reader.ts` (compose spatial ∩ attribute, count after filtering)

**Interfaces:**
- Produces:
  ```ts
  export function postFilterCandidates(feature: Feature, header: HeaderView, conditions: readonly AttrCondition[]): boolean
  ```

**A `String`/`Json`/`Binary` index returns CANDIDATES, not answers.** Keys are fixed-width, truncated and zero-padded, so distinct values collide — and not only long ones: `"a"` and `"a\0"` have identical index representations. Every such predicate needs a post-filter that decodes each candidate's full, untruncated attribute and re-evaluates the predicate **existentially over the feature's CityObjects**, each with its own column schema. See `src/cpp/src/reader.cpp:394-412`.

**The post-filter is not gated on query length**, and it must run **before** `featuresCount` and pagination — otherwise both report candidate counts rather than match counts.

Composition: when both `spatial` and `where` are given, intersect the two result sets by feature offset, then post-filter, then count, then page. **`nearest` combined with `where` throws `ErrorCode.UnsupportedQueryCombination`.**

- [ ] **Step 0: Pin the collision groups from the fixture**

Use `colliding_strings.fcb` from Task 2, not `long_strings.fcb` — the latter's values are 53-byte `yyyy…AAA` / `yyyy…BBB`, which do collide, but any test querying a short value like `"a"` gets an empty answer straight from the raw index and would pass with no post-filter whatsoever.

```bash
python3 - <<'EOF'
import json, collections
groups = collections.defaultdict(list)
for line in open('conformance/colliding_strings.expected.jsonl'):
    line = line.strip()
    if not line: continue
    obj = json.loads(line)
    fid = next(iter(obj.get('CityObjects', {})), None)
    for co in obj.get('CityObjects', {}).values():
        for k, v in (co.get('attributes') or {}).items():
            if isinstance(v, str):
                groups[(k, v.encode('utf8')[:50])].append((obj.get('id', fid), v))
for (col, prefix), rows in groups.items():
    if len({v for _, v in rows}) > 1:
        print('COLLIDING', col, rows)
EOF
```

Record, for one collision group: the column name, **each full value**, and **which feature ids hold each**. The test asserts that querying one full value returns only its own ids — while the raw index would return the whole group.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/post-filter.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbReader } from '../src/reader.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const bytes = (n: string) => new Uint8Array(readFileSync(resolve(CORPUS, n)))
const ids = async (c: AsyncIterable<{ id: string }>) => {
  const out: string[] = []
  for await (const f of c) out.push(f.id)
  return out
}

// From Step 0. AAA and BBB share their first 50 bytes and differ after.
// NOTE ColumnType.String is 11, not 12 (src/fbs/header.fbs:9-26) -- and use
// the enum, not a literal, so an off-by-one cannot silently find nothing.
const COL = 'label'
const VALUE_AAA = '...AAA'          // full value, > 50 bytes
const VALUE_BBB = '...BBB'          // same first 50 bytes
const IDS_AAA = [/* ids holding AAA */]
const IDS_BBB = [/* ids holding BBB */]

describe('string post-filtering', () => {
  it('splits a collision group by its FULL value', async () => {
    // The decisive assertion: the raw index cannot tell AAA from BBB, so a
    // reader with no post-filter returns IDS_AAA + IDS_BBB for both queries.
    const r = await FcbReader.fromBytes(bytes('colliding_strings.fcb'))
    const a = await ids(await r.select({
      where: [{ field: COL, operator: 'Eq', value: VALUE_AAA }],
    }))
    const b = await ids(await r.select({
      where: [{ field: COL, operator: 'Eq', value: VALUE_BBB }],
    }))
    expect(a.sort()).toEqual([...IDS_AAA].sort())
    expect(b.sort()).toEqual([...IDS_BBB].sort())
    expect(a.length).toBeGreaterThan(0)
    expect(b.length).toBeGreaterThan(0)
    expect(a.some((id) => b.includes(id))).toBe(false)   // disjoint
  })

  it('post-filters SHORT queries too, because zero padding also collides', async () => {
    // "a" and "a\0" have identical 50-byte zero-padded keys, so a
    // length-gated post-filter returns the wrong one of them.
    const r = await FcbReader.fromBytes(bytes('colliding_strings.fcb'))
    const hit = await ids(await r.select({
      where: [{ field: COL, operator: 'Eq', value: SHORT_VALUE }],   // Step 0
    }))
    expect(hit.sort()).toEqual([...IDS_SHORT].sort())
  })

  it('reports featuresCount AFTER post-filtering, not the candidate count', async () => {
    const r = await FcbReader.fromBytes(bytes('colliding_strings.fcb'))
    const cursor = await r.select({
      where: [{ field: COL, operator: 'Eq', value: VALUE_AAA }],
    })
    // The candidate set is strictly larger than the match set here, so a
    // count taken before filtering is detectably wrong.
    expect(cursor.featuresCount).toBe(IDS_AAA.length)
    expect(await ids(cursor)).toHaveLength(IDS_AAA.length)
  })

  it('pages the FILTERED list, not the candidate list', async () => {
    const r = await FcbReader.fromBytes(bytes('colliding_strings.fcb'))
    const cursor = await r.select({
      where: [{ field: COL, operator: 'Eq', value: VALUE_AAA }], limit: 1,
    })
    expect(cursor.featuresCount).toBe(IDS_AAA.length)
    expect(await ids(cursor)).toHaveLength(1)
  })

  it.each(['Gt', 'Ge', 'Lt', 'Le', 'Ne'] as const)(
    'applies %s to the full value, matching the C++ reader', async (op) => {
      // Index bounds for strings are deliberately NON-strict so equal-prefix
      // candidates survive to be judged here; the real operator is applied to
      // the untruncated value. Lists pinned from C++ in Step 0.
      const r = await FcbReader.fromBytes(bytes('colliding_strings.fcb'))
      const hit = await ids(await r.select({
        where: [{ field: COL, operator: op, value: VALUE_AAA }],
      }))
      expect(hit.sort()).toEqual([...ORACLE_STRING_OPS[op]].sort())
    })

  it('orders full strings by UTF-8 bytes, not by JS UTF-16 comparison', () => {
    // Same non-BMP hazard as the index keys: JS `<` disagrees with the byte
    // order the reference uses, and every ASCII test passes either way.
    expect(compareFullStrings('｡', '\u{10000}')).toBeLessThan(0)
    expect('｡' < '\u{10000}').toBe(false)
  })
})

describe('composition', () => {
  it('intersects a spatial and an attribute predicate', async () => {
    // The bbox must EXCLUDE at least one attribute hit, or a reader that
    // ignores the spatial predicate entirely passes.
    const r = await FcbReader.fromBytes(bytes('multi_object_attrs.fcb'))
    const e = r.header.info.geographicalExtent!
    const half: [number, number, number, number] =
      [e[0], e[1], (e[0] + e[3]) / 2, (e[1] + e[4]) / 2]
    const where = [{ field: COL_H, operator: 'Ge' as const, value: PIVOT }]
    const attrOnly = await ids(await r.select({ where }))
    const spatialOnly = await ids(await r.select({ spatial: { kind: 'bbox', value: half } }))
    const both = await ids(await r.select({ spatial: { kind: 'bbox', value: half }, where }))

    expect(both.sort()).toEqual(
      attrOnly.filter((id) => spatialOnly.includes(id)).sort())
    expect(both.length).toBeLessThan(attrOnly.length)   // the bbox really cut
    expect(both.length).toBeGreaterThan(0)
  })

  it('counts and pages the INTERSECTED list', async () => {
    const r = await FcbReader.fromBytes(bytes('multi_object_attrs.fcb'))
    const e = r.header.info.geographicalExtent!
    const half: [number, number, number, number] =
      [e[0], e[1], (e[0] + e[3]) / 2, (e[1] + e[4]) / 2]
    const where = [{ field: COL_H, operator: 'Ge' as const, value: PIVOT }]
    const full = await ids(await r.select({ spatial: { kind: 'bbox', value: half }, where }))
    const paged = await r.select({
      spatial: { kind: 'bbox', value: half }, where, limit: 1,
    })
    expect(paged.featuresCount).toBe(full.length)
    expect(await ids(paged)).toHaveLength(1)
  })

  it('rejects nearest combined with where', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    await expect(r.select({
      spatial: { kind: 'nearest', value: [0, 0] },
      where: [{ field: 'x', operator: 'Eq', value: 1 }],
    })).rejects.toThrow(/unsupported query combination/i)
  })
})
```

Fill the two `/* oracle */` placeholders with real values read out of `long_strings.fcb` **before** implementing.

- [ ] **Step 2: Run, watch fail.**
- [ ] **Step 3: Implement,** then verify the ordering: post-filter, then count, then page.
- [ ] **Step 4: Ask codex to review stage E**

```bash
codex exec --model gpt-5.6-sol --sandbox read-only "Review the query path of the pure-TypeScript FlatCityBuf reader: src/ts/src/packed-rtree, src/ts/src/static-btree, src/ts/src/post-filter.ts and src/ts/src/reader.ts, against src/cpp/src and src/rust/fcb_core. Focus on: (1) B+tree and R-tree traversal off-by-ones, especially level bounds, the branching_factor-1 node size, and the +1 leaf fetch rule; (2) whether Gt/Lt/Ne use strict leaf bounds rather than Rust's range-minus-exact subtraction (upstream finding #5); (3) whether string post-filtering runs before featuresCount and pagination and is not gated on query length; (4) BigInt handling of the payload tag; (5) unbounded allocations from attacker-controlled lengths; (6) tests that would pass even if the code were wrong. Cite file:line and give a concrete input for each finding."
```

- [ ] **Step 5: Act on findings, then commit** — `feat(ts): string post-filtering and spatial/attribute composition`

---

## Task 16: pointNearest

**Files:**
- Create: `src/ts/src/packed-rtree/nearest.ts`, `src/ts/test/nearest.test.ts`

**Interfaces:**
- Produces: `export function searchNearest(reader: RangeReader, rtreeBegin: number, rtreeSize: number, numItems: number, nodeSize: number, x: number, y: number): Promise<SearchResultItem[]>` — returns **at most one** item.

The one algorithm with no Python or C++ port to copy. Read `src/rust/fcb_core/src/packed_rtree/mod.rs:571-668` (in-memory), `:771-873` (stream), `:1140-1256` (HTTP).

- **Two distance metrics, mixed deliberately.** Internal nodes are ordered and pruned by **min-distance** (squared Euclidean to the nearest point of the bbox, 0 if inside); a leaf's final score is its **centroid** distance. Both squared — no `sqrt`.
- **Why the mix is sound and must not be "fixed":** a child's bbox is contained in its parent's and a leaf's centroid lies inside its own bbox, so the internal key is an admissible lower bound for the leaf metric. The search is exact *for the nearest-centroid problem*. It is **not** nearest-feature-geometry.
- **Traversal:** best-first over a min-heap seeded with the root at distance 0. Pop the smallest; if its distance is **strictly greater** than the current best, terminate. Skip nodes whose min-distance is `>= best`. Leaves replace the best only on a **strict** improvement, so on exact ties the first-reached leaf wins. Internal nodes push their child range keyed by the **parent's** min-distance.
- **Tie order is unspecified upstream** — a JS heap will have a different but equally valid order. Assert *distance*, not identity, on constructed ties.
- **v1 structure:** port the exact serial algorithm, plus a **whole-index fast path** — if `rtreeSize <= 262144` (the 256 KB spatial combine threshold), fetch the whole index in one read and run the in-memory algorithm with zero further index I/O. `delft.fcb`'s entire R-tree is 47,640 bytes, so this is one request. Wave batching above the threshold is explicitly **deferred** until a request-log benchmark justifies it.

**The threshold must be overridable** (`wholeIndexThreshold` on the select options, defaulting to 262144). Every corpus file is far below it, so without an override the streaming best-first traversal would never execute in any test — the fast path would mask every bug in it. The test above sets it to 0 and asserts both paths agree.

- [ ] **Step 1: Write the failing tests**

```ts
// src/ts/test/nearest.test.ts
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FcbReader } from '../src/reader.js'
import { BufferedRangeReader } from '../src/io/range-reader.js'
import { CountingReader } from './fixtures/counting-reader.js'

const CORPUS = resolve(__dirname, '../../../conformance')
const bytes = (n: string) => new Uint8Array(readFileSync(resolve(CORPUS, n)))
const ids = async (c: AsyncIterable<{ id: string }>) => {
  const out: string[] = []
  for await (const f of c) out.push(f.id)
  return out
}

describe('pointNearest', () => {
  it('returns exactly one feature for a point inside the extent', async () => {
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const cursor = await r.select({
      spatial: { kind: 'nearest', value: [(e[0] + e[3]) / 2, (e[1] + e[4]) / 2] },
    })
    expect(cursor.featuresCount).toBe(1)
    expect(await ids(cursor)).toHaveLength(1)
  })

  it('still returns one feature for a point far outside the extent', async () => {
    // Nothing prunes it away: min-distance ordering is a lower bound, not a
    // rejection test. An empty result here means the termination is wrong.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    expect((await ids(await r.select({
      spatial: { kind: 'nearest', value: [1e9, 1e9] },
    })))).toHaveLength(1)
  })

  it('returns the ONE feature of a single-feature file', async () => {
    const r = await FcbReader.fromBytes(bytes('single_feature.fcb'))
    const all = await ids(await r.selectAll())
    expect(all).toHaveLength(1)
    const hit = await ids(await r.select({
      spatial: { kind: 'nearest', value: [0, 0] },
    }))
    expect(hit).toEqual(all)
  })

  it('agrees with a brute-force scan over every feature CENTROID', async () => {
    // The oracle that does not depend on heap order: actually compute the
    // nearest centroid by scanning every feature, then compare DISTANCE --
    // not identity -- so an exact tie does not make the test flaky.
    //
    // Note the metric: leaves are scored by distance to the bbox CENTROID,
    // not to the nearest point of the bbox. Scoring by min-distance here
    // would make this test disagree with all three Rust forms.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    for (const [px, py] of [
      [(e[0] + e[3]) / 2, (e[1] + e[4]) / 2],   // middle
      [e[0], e[1]],                              // a corner
      [1e9, 1e9],                                // far outside
    ] as Array<[number, number]>) {
      let best = Number.POSITIVE_INFINITY
      for await (const f of await r.selectAll()) {
        const b = featureBounds(f, r.header)
        const cx = (b.minX + b.maxX) / 2, cy = (b.minY + b.maxY) / 2
        best = Math.min(best, (cx - px) ** 2 + (cy - py) ** 2)
      }
      const hit = await ids(await r.select({
        spatial: { kind: 'nearest', value: [px, py] },
      }))
      expect(hit).toHaveLength(1)
      const f = await featureById(r, hit[0]!)
      const b = featureBounds(f, r.header)
      const cx = (b.minX + b.maxX) / 2, cy = (b.minY + b.maxY) / 2
      expect((cx - px) ** 2 + (cy - py) ** 2).toBeCloseTo(best, 6)
    }
  })

  it('takes the STREAMING path for an index above the threshold', async () => {
    // The whole-index fast path would hide every bug in the best-first
    // traversal. delft.fcb's rtree is 47 KB, still under 256 KB, so force
    // the streaming path by lowering the threshold for this test.
    const r = await FcbReader.fromBytes(bytes('small.fcb'))
    const e = r.header.info.geographicalExtent!
    const px = (e[0] + e[3]) / 2, py = (e[1] + e[4]) / 2
    const fast = await ids(await r.select({ spatial: { kind: 'nearest', value: [px, py] } }))
    const streamed = await ids(await r.select({
      spatial: { kind: 'nearest', value: [px, py] }, wholeIndexThreshold: 0,
    }))
    expect(streamed).toEqual(fast)
  })

  it('reads the whole small index in ONE request', async () => {
    // The fast path: rtree_size is under the 256 KB threshold, so nearest
    // must not degenerate into one round trip per heap pop.
    const inner = new CountingReader(bytes('small.fcb'))
    const r = await FcbReader.fromReader(new BufferedRangeReader(inner))
    inner.reads.length = 0
    await r.select({ spatial: { kind: 'nearest', value: [0, 0] } })
    expect(inner.reads.length).toBeLessThanOrEqual(2)
  })
})
```

- [ ] **Step 2: Run, watch fail.**
- [ ] **Step 3: Implement** the min-heap, the two metrics, the whole-index fast path, then the streaming fallback.
- [ ] **Step 4: Cross-check against Rust** by running the same point through the Rust CLI or a temporary Rust test, comparing the chosen feature.
- [ ] **Step 5: Commit** — `feat(ts): pointNearest with a whole-index fast path`

---

## Task 17: Port the browser demo

**Files:**
- Create: `examples/web/index.html`, `examples/web/main.ts`, `examples/web/package.json`, `examples/web/vite.config.ts`
- Delete: `examples/wasm/`

The demo is how we prove the reader works in a real browser, and it is the migration example the README points at. It reads a URL **and** a dropped local file — the latter is new; the wasm binding never supported it.

The old demo imported `cjToObj` and `cjseqToCj`, both dropped. **Do not reimplement them.** Drop the OBJ-export button, and do not offer whole-file merging.

- [ ] **Step 1: Write the demo** against the public API only — no deep imports into `src/ts/src`. It should: open a URL or a dropped file; show `header.info`; run a bbox query from four inputs; run an attribute query; and list the resulting feature ids with a count.

- [ ] **Step 2: Run it and confirm it works in a real browser**

```bash
cd src/ts && npm run build && cd ../../examples/web && npm install && npm run dev
```
Open the printed URL, load `https://storage.googleapis.com/flatcitybuf/3dbag_subset_all_index.fcb`, and confirm the header renders and a bbox query returns features. **Screenshot or note the feature count** — this is the acceptance evidence for the whole port.

- [ ] **Step 3: Delete the old demo**

```bash
git rm -r examples/wasm
```

- [ ] **Step 4: Commit** — `feat(examples): port the browser demo to the native TypeScript reader`

---

## Task 18: Retire the wasm binding

**Files:**
- Delete: `src/rust/wasm/`, `scripts/build_wasm.sh`
- Modify: `src/rust/Cargo.toml` (drop the workspace member), `justfile` (drop `check-wasm`, `build-wasm`; drop `--exclude fcb_wasm` from every recipe), `.github/workflows/publish-npm.yml`, `.github/workflows/ci.yml`, `README.md`, `CONTRIBUTING.md`, `.llm/docs/projectStructure.md`, `docs/upstream-findings.md`

**`src/ts/` tracks only `.gitignore` and `package.json`** — the `.wasm`/`.js`/`.d.ts` artifacts are gitignored and built at publish time, so there are no checked-in binaries to delete. `publish-npm.yml` currently copies individual wasm-pack artifacts in; it becomes a plain `npm ci && npm run build && npm publish`.

Publishing gets much simpler: no wasm-pack, no `wasm32-unknown-unknown` target, no artifact copying.

> **Do Task 19 before this one.** Retirement removes the only thing that currently works in a browser, while every Blob and `fetch` test up to here runs under Node. Browser-mode coverage must gate the deletion, not follow it. The numbering is kept as written for stability of references; the execution order is 17 → 19 → 18.

- [ ] **Step 1: Confirm parity first.** Task 10 conformance green, Tasks 12/15/16 green, **Task 19 browser tests green**, and every behaviour the wasm `.d.ts` exposed has a TypeScript equivalent covered by a test: `HttpFcbReader` (`fromUrl`), `select_all`, `select_spatial`, `select_attr_query`, both `_paged` variants, `AsyncFeatureIter`, `meta`/`cityjson`, and the three spatial query types. `cjToObj` and `cjseqToCj` are deliberately dropped — record that in the README migration section rather than porting them.

- [ ] **Step 2: Delete the crate and repoint the workspace**

```bash
git rm -r src/rust/wasm scripts/build_wasm.sh
# remove the workspace member from src/rust/Cargo.toml
cd src/rust && cargo build --workspace
```
Expected: builds clean with `fcb_wasm` gone. Then remove every `--exclude fcb_wasm` from the `justfile`.

- [ ] **Step 3: Rewrite `publish-npm.yml`** to build the pure package from `src/ts` and publish on tag. Version lives in exactly one committed file (`src/ts/package.json`), the workflow never regenerates metadata, and it verifies the artifact (`npm pack` and inspect the file list) before publishing.

- [ ] **Step 4: Write the README**, including a **migration section** mapping every old API to the new one, and the **trust model** paragraph: input `.fcb` files are trusted; framing is bounds-checked but there is no FlatBuffers verifier in JavaScript, so a malformed or hostile file may throw or return garbage.

- [ ] **Step 5: Write up the findings in `docs/upstream-findings.md`.** Append in the existing style — what, where, cited to source lines, whether fixed, and what a consumer sees if it is not. At minimum the four wasm defects:
  1. Every JS number becomes a `Float64` key (`wasm/src/lib.rs:1110-1112`), so attribute queries against `Int`/`Float`/`Long`/… columns fail from the browser today with "key type mismatch".
  2. String query values >50 bytes are routed into a `StringKey100` (`wasm/src/lib.rs:1114-1118`) against an index the writer only ever builds as `FixedStringKey<50>`.
  3. `index_node_size` from the header is ignored on the HTTP path (`wasm/src/lib.rs:275`, `fcb_core/src/http_reader/mod.rs:220`), so files written with a non-default node size are silently mis-traversed.
  4. The gloo client accepts a `200` with the full body as if it were the requested range (`wasm/src/gloo_client.rs:29-44`) — silent corruption of every later offset.

  Plus anything TypeScript had to special-case during Tasks 4-16.

- [ ] **Step 6: Run everything**

```bash
just ts-test && just ts-lint && just ts-build
cd src/rust && cargo build --workspace && cargo test -p fcb_core
cd ../cpp && ./build-native/tests/fcb_tests
cd ../ts && npm pack --dry-run
```

- [ ] **Step 7: Commit** — `feat(ts)!: replace the wasm binding with the native TypeScript reader`

---

## Task 19: Browser-mode tests in CI

**Files:**
- Modify: `src/ts/vite.config.ts` (browser test project), `src/ts/package.json` (add `@vitest/browser`, `playwright`), `.github/workflows/ci-ts.yml`
- Create: `src/ts/test/browser/blob.browser.test.ts`, `src/ts/test/browser/fetch.browser.test.ts`

Everything above runs under Node. This task adds the browser job so the *actual shipping target* is exercised. **No core task may depend on browser mode** — Vitest 5 browser mode is the newest tool in the stack — but this task must nevertheless run **before Task 18**, because Task 18 deletes the only browser-capable reader the project currently has.

**Vitest 5 splits the browser providers into their own packages.** `@vitest/browser` alone is not enough; install `@vitest/browser-playwright` and `playwright`, and use the `playwright()` provider factory in the config. Installing only `@vitest/browser` fails at run time, not at install time.

- [ ] **Step 1: Write the browser tests** — a `File` from a `DataTransfer`-style `Blob` scanned end to end and compared against the Node result for the same file; a `fetch` range read against `range_server.py` started by the CI job; and the CORS failure path (`?no_cors_expose=1`) raising `ErrorCode.RangeHeadersNotExposed` rather than guessing a size.

- [ ] **Step 2: Configure the browser project** in `vite.config.ts` with the Playwright provider and Chromium, as a separate Vitest project so `npx vitest run` stays Node-only by default.

- [ ] **Step 3: Run locally — with the range server actually running**

The fetch browser test needs `range_server.py` up, and the browser needs the CORS headers Task 11 added to it. Start it first, or have the test's `beforeAll` spawn it as the Node HTTP test does.

```bash
cd src/ts && npx playwright install chromium
python3 ../cpp/tests/range_server.py ../../conformance &   # note the printed port
npx vitest run --project=browser
kill %1
```

- [ ] **Step 4: Add the CI job** — a separate job in `ci-ts.yml` that installs Chromium and runs the browser project. It may be `continue-on-error: false`, but it must not block the Node job.

- [ ] **Step 5: Commit** — `test(ts): browser-mode tests for the Blob and fetch paths`

---

## Self-review notes

- **Spec coverage.** Header/scan/attributes → Tasks 6, 8. CityJSON emission → Tasks 9, 10. R-tree bbox/point → Task 12; nearest → Task 16. B+tree → Tasks 13, 14; post-filter and composition → Task 15. HTTP → Task 11; Blob/node → Task 7; bytes → Task 5. Pagination → Task 12. AbortSignal → Tasks 11, 12. Trust model → Tasks 4, 18. int64 policy → Tasks 8, 10. Demo → Task 17. Retirement → Task 18. Browser mode → Task 19.
- **Risk.** Task 14 (B+tree) was the hardest part of the C++ port and TS adds BigInt tag handling on top; if it slips, Tasks 1-12 still ship a conformant reader with spatial queries. Task 15 depends on 14 and on complete attribute decoding from Task 8 — which is why 8 precedes it by a wide margin. Task 16 is severable.
- **Sequencing.** Task 2 touches the C++ build; do it early while the tree is quiet. Task 3 must precede everything that touches a FlatBuffers table. Task 10 (conformance) precedes all networking, so decode bugs and transport bugs are never debugged together.
- **Not in scope.** Writing `.fcb`; `cjToObj`; `cjseqToCj`; a FlatBuffers structural verifier; wave-batched nearest traversal; a `readBatch` primitive.
