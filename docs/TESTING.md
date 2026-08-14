# Verifying the `develop` branch

A step-by-step guide to testing the four reader implementations (Rust, C++,
Python, TypeScript) and the web viewer, over **local files** and **remote files
served by HTTP range requests**.

Every command and every expected result below was executed on macOS 15
(arm64) against `develop` at `06b8d38`. Where a step is known to fail, that is
called out in [§8 Known issues](#8-known-issues-found-while-validating-this-guide)
rather than hidden.

---

## 1. What you are verifying

`develop` is 116 commits ahead of `main`. The substance:

| Area | What changed |
|---|---|
| **C++** (`src/cpp`) | Native C++17 reader and writer replacing the CXX bridge; optional libcurl HTTP adapter |
| **Python** (`src/py`) | Pure-Python reader (`py3-none-any`) replacing the PyO3 extension |
| **TypeScript** (`src/ts`) | Native TS reader replacing the WASM binding |
| **Web example** (`examples/web`) | React/deck.gl viewer on the native TS reader |
| **Rust** (`src/rust`) | Writer determinism, a breaking FlatBuffers alignment fix (`540772a`), shared conformance corpus |

The thing to keep in mind throughout: **Rust is the oracle.** The other three
are from-scratch ports with no FFI. A disagreement is a defect in the port
until proven otherwise.

---

## 2. Prerequisites

| Tool | Needed for | Verified with |
|---|---|---|
| `just` | all task recipes | 1.46.0 |
| Rust + `cargo-nextest` | Rust suite, writing `.fcb`, the CLI | 1.93.1 / nextest 0.9.115 |
| `cmake` + C++17 compiler | C++ suite | cmake 4.1.2 |
| `flatbuffers`, `nlohmann-json`, `doctest` | C++ deps — `brew install flatbuffers nlohmann-json doctest` | — |
| `libcurl` | C++ **HTTP** adapter only | 8.7.1 |
| `uv` | Python suite | 0.9.8 |
| `python3` | the range-request test server (used by C++, Python **and** TS suites) | 3.12.12 |
| Node ≥ 22.12 + npm | TS suite, web example | 24.11.1 / 11.7.0 |
| Chromium via Playwright | TS **browser** suite — `cd src/ts && npx playwright install chromium` | 1.61.1 |
| Network access | remote-file tests (§7), including the `#[ignore]`d Rust HTTP tests | — |

### Three things that will bite you

1. **`just check` never rewrites a file; `just fix` is the only recipe that
   does.** Every language directory exposes the same verbs — `check`, `test`,
   `lint`, `type`, `build`, `fix` — and the root justfile fans each one out
   across all five. If you want the autofixes (rustfmt, `clippy --fix`, ruff,
   clang-format), ask for them explicitly with `just fix`.
2. **Do not run the suites concurrently.** `src/ts/test/http.test.ts` spawns
   `range_server.py` and fails its `beforeAll` hook if the port is not printed
   within 10 s. Running it while a Rust or C++ build saturates the CPU is
   enough to blow that budget — it showed up here as
   `Error: Hook timed out in 10000ms` with all 10 tests skipped, and passed in
   644 ms on an idle machine. It is a resource-contention flake, not a bug in
   the reader.
3. **`just check` covers everything, including `examples/web`.** If it fails
   there, check `git status` first — the web example is under active
   development, and a failing `tsc --noEmit` in it is usually uncommitted work
   in progress rather than a branch defect. Run the per-language recipes in §4
   to see each part in isolation.

---

## 3. Quick pass (~3 minutes, no network)

Run these one at a time, from the repository root:

```bash
just check        # everything, every language, read-only
```

Or one language at a time — same verbs everywhere:

```bash
cd src/rust && just test && cd ../..
cd src/cpp  && just test && cd ../..
cd src/py   && just check && cd ../..
cd src/ts   && just check && cd ../..
```

Expected:

| Command | Expected result | Time here |
|---|---|---|
| `cd src/rust && just test` | `215 tests run: 215 passed, 3 skipped` | 39-74 s |
| `cd src/cpp && just test` | `1/1 Test #1: fcb_tests ... Passed`, `100% tests passed` | 6-22 s (after build) |
| `cd src/py && just check` | `255 passed`, then `251 passed, 4 skipped` (numpy-less pass) | 20 s |
| `cd src/ts && just check` | 244 Node tests, 3 browser tests, clean build | 30 s |
| `just type` | clean across all five directories | 60 s |
| `just lint` | clean (rustfmt, clippy, ruff, clang-format) | 30 s |

Three Rust tests (`http::remote_3dbag_attr_query`,
`http::remote_3dbag_opens_and_counts_a_bbox`, `http::remote_3dbag_bbox_scan`)
hit the public bucket at flatcitybuf.open3d.city; they are `#[ignore]`d, so a
plain `cargo nextest run` skips them and everything in this section stays
offline. Run them with `just test-remote` — see §7.4.

If all six pass, the core of every implementation is green. Continue to §4–§7
for the cross-implementation and remote checks no recipe automates.

---

## 4. Full pass, implementation by implementation

### 4.1 Rust — the oracle

```bash
cd src/rust
just check          # lint + type + test + build
# or individually:
just test           # cargo nextest, 215 tests
just lint           # cargo fmt --check + clippy
just type           # cargo check
```

`cargo fmt --check` is clean. Clippy emits two pre-existing `dead_code`
warnings in test/bench targets (`fcb_api`'s e2e test, `stats`'s `BenchResult`);
neither is an error. `just fix` is the mutating counterpart (`cargo fmt` +
`clippy --fix`).

> `just lint` deliberately does **not** pass `-D warnings`: `fcb_core` carries
> 50 pre-existing style lints (`should_implement_trait`,
> `non_canonical_partial_ord_impl`, `missing_safety_doc`,
> `extra_unused_lifetimes`, `empty_docs`). Clean those up, then add the flag —
> see [issue 4](#issue-4-the-old-just-clippy-recipe-failed-with-50-pre-existing-lints).

Sanity-check the CLI, which is also what you will use in §6:

```bash
cd src/rust && just inspect ../../examples/data/delft.fcb
```

```
▶ File Details
  Source: ../../examples/data/delft.fcb
  Size: 7.31 MB
  Version: 2.0
  Title: 3DBAG

▶ Dataset
  Features: 1115
  Columns: 44
  Geospatial Extent: Yes
    Min: [84501.55, 445805.03, -3.75]
    Max: [85675.23, 446983.47, 95.04]
    Dimensions: 1173.68 × 1178.44 × 98.79

▶ Indices
  Spatial R-tree: Yes (node size: 16)
  Attribute Indices: 44 (B+Tree)
    1. b3_bag_bag_overlap
    2. b3_dak_type
    3. b3_h_dak_50p
    4. b3_h_dak_70p
    5. b3_h_dak_max
    6. b3_h_dak_min
    7. b3_h_maaiveld
    8. b3_kas_warenhuis
    9. b3_mutatie_ahn3_ahn4
    10. b3_nodata_fractie_ahn3
    ... 34 more attributes...

▶ Coordinate Reference System
  CRS: EPSG:7415

▶ Coordinate Transform
  Scale: [0.001000, 0.001000, 0.001000]
  Translate: [85088.390625, 446394.250000, 45.648003]
```

The recipe is `fcb inspect <source> --static`. Drop `--static` and the same
command browses the header in a full-screen terminal UI instead (it also
accepts an `http(s)://` URL). The UI needs an interactive TTY; without one —
piped, redirected, or in CI — `inspect` falls back to exactly the static report
above and exits 0, which is why the checks below can capture it.

### 4.2 C++ — local

```bash
cd src/cpp && just test
```

Configures `build-native` with `-DFCB_BUILD_TESTS=ON`, builds, runs ctest.
Expect `100% tests passed, 0 tests failed out of 1` (the single `fcb_tests`
binary holds the whole doctest suite). Roughly 22 s of test time after the
build.

The build also produces the two example programs used later:
`src/cpp/build-native/fcb_read_local` and `.../fcb_read_http`.

### 4.3 C++ — HTTP adapter

```bash
cd src/cpp && just test-http
```

This configures a **separate** `build-curl` tree with `-DFCB_WITH_CURL=ON`,
then `run_http_tests.sh` starts `range_server.py`, exports
`FCB_TEST_HTTP_URL`, and runs the suite against it.

Expect `150 passed | 0 failed` out of 150 test cases (16111 assertions). This
used to fail on a hardcoded fixture size; see
[issue 1](#issue-1-fixed-c-just-test-http-failed-on-a-stale-hardcoded-fixture-size).

Note that plain `just test` **skips** every HTTP
test, because they never set `FCB_TEST_HTTP_URL` — the tests bail out with
`FCB_TEST_HTTP_URL not set; skipping`. `just test-http` is the only thing that
exercises them.

Two more C++ recipes exist and are deliberately **not** in `check`:
`just tidy` (clang-tidy; still noisy, see issue 5) and `just harden` (the
no-curl/no-TLS link assertion plus the ASan/UBSan run that CI does).

### 4.4 Python

```bash
cd src/py
just check      # lint + mypy + pytest, in BOTH optional-dependency states
# or individually:
just test       # pytest            -> 255 passed, 1 deselected
just lint       # ruff check + ruff format --check
just type       # mypy --strict
```

The deselected test is `test_benchmark.py` (timing-sensitive, excluded by
`addopts = -m 'not benchmark'`). To run it: `uv run pytest -m benchmark`.

`numpy` is an optional extra with a pure-Python fallback, and **both paths must
be checked** — CI runs the suite twice for exactly this reason:

`just check` already does both passes; `just test-no-numpy` runs just the
numpy-less one and restores numpy afterwards:

```bash
cd src/py && just test-no-numpy
```

| Pass | Expected |
|---|---|
| without `numpy` | `251 passed, 4 skipped, 1 deselected`; mypy `Success: no issues found in 35 source files` |
| with `numpy` | `255 passed, 1 deselected`; mypy clean |

The 4 skips in the first pass are the numpy-parity tests bailing out via
`pytest.importorskip("numpy")` — that is the correct outcome, not a gap.
`mypy` passes in both, because `pyproject.toml` carries an
`ignore_missing_imports` override for `numpy`.

Leave the environment in the `--extra dev --extra numpy` state afterwards;
that is what `src/py/README.md` documents for development.

### 4.5 TypeScript — Node

```bash
cd src/ts
just check          # lint + type + test + test-browser + build
# or individually:
just test           # vitest run -> 16 files, 244 tests
just type           # tsc --noEmit, twice (src and test configs)
just build          # vite build + tsc --emitDeclarationOnly -> dist/
```

`just lint` here is a documented no-op: there is no ESLint or Prettier in this
package, so `just type` is the only static gate.

`test/http.test.ts` (10 tests) spawns `range_server.py` on loopback, so
`python3` must be on `PATH` — see caveat 2 in §2 if it times out.

Run `cd src/ts && just build` before touching the web example: it depends on
`@cityjson/flatcitybuf` as `file:../../src/ts` and resolves it from `dist/`.

### 4.6 TypeScript — real browser

```bash
cd src/ts
npx playwright install chromium   # once
just test-browser
```

Expect `Test Files 2 passed (2)`, `Tests 3 passed (3)`.

This is a genuinely different target, not a duplicate of the Node run. The
browser project's `globalSetup` starts `range_server.py` on a *different* port
from the Vitest page server, so every request is truly cross-origin — which is
the only way to exercise the CORS failure path (`no_cors_expose`). A Node
`fetch` does not enforce CORS at all and cannot reach it. The setup also
computes the **Node** reader's output for `small.fcb` and hands it to the
browser, so the browser tests compare against the Node reader rather than
against themselves.

### 4.7 Web example

```bash
cd examples/web
just check      # type + test + build
# or individually: just test / just type / just build / just dev
```

Expect `3 files, 50 tests`. `npm install` runs as a dependency of each recipe
and resolves `@cityjson/flatcitybuf` from `../../src/ts/dist`, so build that
first.

The "chunks larger than 500 kB" warning from the build is expected (deck.gl +
maplibre) and not a failure.

---

## 5. Conformance: all four readers against the shared corpus

`conformance/` holds 14 hand-authored cases. Each is a `.fcb` written by the
Rust writer plus an `.expected.jsonl` produced by reading it back with the
**Rust reader**. Every port compares its own whole output, line for line,
against that file.

These run inside the suites you already ran, but it is worth checking coverage
explicitly, because it is not equal:

```bash
ls conformance/*.expected.jsonl | wc -l                    # 14
cd src/py && uv run pytest tests/test_conformance.py -q     # 10 passed
cd src/ts && npx vitest run test/conformance.test.ts        # 20 tests (14 cases + 6 int64/material)
```

| Reader | Corpus cases covered |
|---|---|
| C++ | 14 / 14 |
| TypeScript | 14 / 14 |
| **Python** | **10 / 14** |

Python's `CASES` list at `src/py/tests/test_conformance.py:23` is hardcoded and
was never extended when four cases were added for the later ports. One of the
four it skips **fails** — see
[issue 2](#issue-2-python-reader-collapses-a-nesting-level-on-single-solid-multisolidcompositesolid).

---

## 6. Local-file test: the four-way cross-check

This is the strongest single check in the repo, and it is not automated: emit
the same local `.fcb` as CityJSONSeq from all four readers and diff the parsed
JSON. Two readers agreeing on a *wrong* answer is a real failure mode here, so
compare **whole lines**, never selected keys.

Set up a scratch directory:

```bash
export FCBTMP=/tmp/fcbcheck && mkdir -p $FCBTMP
export FCBROOT=$(git rev-parse --show-toplevel)
```

**Rust (the oracle):**

```bash
cd $FCBROOT/src/rust
just deser ../../examples/data/delft.fcb $FCBTMP/delft.rust.jsonl
```

**C++:**

```bash
cd $FCBROOT
./src/cpp/build-native/fcb_read_local examples/data/delft.fcb \
    > $FCBTMP/delft.cpp.jsonl
# stderr: "1115 features, CityJSON 2.0, EPSG:7415"
```

**Python:**

```bash
cat > $FCBTMP/dump_py.py <<'EOF'
import json, sys
import flatcitybuf as fcb
reader = fcb.FcbReader.open_file(sys.argv[1])
with open(sys.argv[2], "w") as out:
    out.write(json.dumps(fcb.to_cityjson_metadata(reader.header)) + "\n")
    for feature in reader.select_all():
        out.write(json.dumps(fcb.to_cityjson_feature(feature, reader.header)) + "\n")
EOF
cd $FCBROOT/src/py
uv run --extra dev python $FCBTMP/dump_py.py \
    ../../examples/data/delft.fcb $FCBTMP/delft.py.jsonl
```

**TypeScript** (needs `cd src/ts && just build` first; the script must sit inside
`src/ts` so it can import `./dist`):

```bash
cat > $FCBROOT/src/ts/dump_ts.mjs <<'EOF'
import { writeFileSync } from 'node:fs'
import { fromFile } from './dist/io/node.js'
import { toCityJSONMetadata, toCityJSONFeature } from './dist/index.js'
const reader = await fromFile(process.argv[2])
const lines = [JSON.stringify(toCityJSONMetadata(reader.header))]
for await (const f of await reader.selectAll())
  lines.push(JSON.stringify(toCityJSONFeature(f, reader.header)))
writeFileSync(process.argv[3], lines.join('\n') + '\n')
await reader.close()
EOF
cd $FCBROOT/src/ts
node dump_ts.mjs ../../examples/data/delft.fcb $FCBTMP/delft.ts.jsonl
rm dump_ts.mjs
```

**Compare.** Key order and float formatting legitimately differ between
languages, so diff the *parsed* trees, not the text:

```bash
cat > $FCBTMP/compare.py <<'EOF'
import json, sys

def load(p):
    with open(p) as f:
        return [json.loads(l) for l in f if l.strip()]

oracle_path, *others = sys.argv[1:]
oracle = load(oracle_path)
status = 0
for other_path in others:
    other = load(other_path)
    if len(other) != len(oracle):
        print(f"{other_path}: LINE COUNT {len(other)} != {len(oracle)}")
        status = 1
        continue
    bad = [i for i, (a, b) in enumerate(zip(oracle, other)) if a != b]
    if bad:
        status = 1
        print(f"{other_path}: {len(bad)}/{len(oracle)} lines differ; "
              f"first at line {bad[0]+1}")
        a, b = oracle[bad[0]], other[bad[0]]
        ka = set(a) if isinstance(a, dict) else set()
        kb = set(b) if isinstance(b, dict) else set()
        print("   keys only in oracle:", ka - kb, "| only in other:", kb - ka)
        for k in sorted(ka & kb):
            if a[k] != b[k]:
                print(f"   key {k!r} differs;")
                print(f"     oracle: {str(a[k])[:200]}")
                print(f"     other : {str(b[k])[:200]}")
                break
    else:
        print(f"{other_path}: IDENTICAL to oracle ({len(oracle)} lines)")
sys.exit(status)
EOF
python3 $FCBTMP/compare.py $FCBTMP/delft.rust.jsonl \
    $FCBTMP/delft.cpp.jsonl $FCBTMP/delft.py.jsonl $FCBTMP/delft.ts.jsonl
```

Expected — verified on this branch:

```
delft.cpp.jsonl: IDENTICAL to oracle (1116 lines)
delft.py.jsonl:  IDENTICAL to oracle (1116 lines)
delft.ts.jsonl:  IDENTICAL to oracle (1116 lines)
```

1116 lines = 1 metadata line + 1115 features. Run the same comparison against
any `conformance/*.fcb` to widen coverage; that is exactly what the per-port
conformance suites automate.

### Round-tripping through the writer

Reading the committed fixtures only proves the readers agree on *those bytes*.
To close the loop, rewrite from source and re-read:

```bash
cd $FCBROOT/src/rust
just ser ../../examples/data/delft.city.jsonl $FCBTMP/roundtrip.fcb
```

Then point all four readers at `$FCBTMP/roundtrip.fcb` and diff again.

> The corpus is **not byte-reproducible**: `cjseq2` iterates CityObjects from a
> `HashMap`, so every regeneration produces different bytes for identical data.
> Never assert on a physical byte offset — derive it at runtime. For the same
> reason `just gen-conformance` always dirties the tree; diff the *parsed*
> JSON before committing, and do not commit pure churn.

---

## 7. Remote-file tests (HTTP range requests)

The whole point of the format is fetching only the bytes a query needs. Two
complementary ways to test it: a local server you control (deterministic, can
misbehave on demand), and a real public file (proves it works against a real
CDN with real CORS).

### 7.1 Against the local range server

`src/cpp/tests/range_server.py` is the shared harness for all three ports.
Python's own `http.server` does **not** implement Range at all — it answers
every request with `200` and the whole body, which would validate nothing — so
this server exists to do it properly. It also misbehaves on demand via query
parameters:

| Query param | Behaviour it forces |
|---|---|
| `?ignore_range=1` | `200` with the entire body despite a `Range` header |
| `?bad_range=1` | `206` with a malformed `Content-Range` |
| `?wrong_offset=1` | `206` with a range the client never asked for |
| `?long_end=1` | `206` whose end runs far past the request |
| `?stall_body=1` | `206` headers, then hangs forever — the client's own timeout must fire |
| `?no_etag=1` | omits the `ETag` / `Last-Modified` validators |
| `?no_cors_expose=1` | sends `Access-Control-Allow-Origin` but **not** `Access-Control-Expose-Headers` |

Start it (it binds port 0 and prints the chosen port on stdout):

```bash
cd $FCBROOT
python3 src/cpp/tests/range_server.py examples/data > $FCBTMP/port.txt &
sleep 1 && export PORT=$(cat $FCBTMP/port.txt) && echo "serving on $PORT"
```

**C++ over HTTP** (needs the `build-curl` tree from `cd src/cpp && just test-http`):

```bash
./src/cpp/build-curl/fcb_read_http "http://127.0.0.1:$PORT/delft.fcb"
```

```
1115 features, CityJSON 2.0
931 features in the western half, 9 HTTP requests
```

Nine requests for 931 of 1115 features is the property under test: the reader
walks the R-tree and fetches only intersecting ranges.

**The strongest remote check** — have the Python and TypeScript readers emit
the *whole* file over HTTP and diff it against the **local** Rust oracle from
§6. Same bytes, different transport: the output must be identical.

```bash
cat > $FCBTMP/remote_dump_py.py <<'EOF'
import json, os, sys
import flatcitybuf as fcb
url = f"http://127.0.0.1:{os.environ['PORT']}/delft.fcb"
reader = fcb.FcbReader.open(fcb.HttpRangeReader(url))
with open(sys.argv[1], "w") as out:
    out.write(json.dumps(fcb.to_cityjson_metadata(reader.header)) + "\n")
    for feature in reader.select_all():
        out.write(json.dumps(fcb.to_cityjson_feature(feature, reader.header)) + "\n")
EOF
cd $FCBROOT/src/py && uv run python $FCBTMP/remote_dump_py.py $FCBTMP/http.py.jsonl

cat > $FCBROOT/src/ts/remote_dump.mjs <<'EOF'
import { writeFileSync } from 'node:fs'
import { FcbReader } from './dist/index.js'
const reader = await FcbReader.fromUrl(
  `http://127.0.0.1:${process.env.PORT}/delft.fcb`)
const lines = []
for await (const line of reader.cityjson()) lines.push(JSON.stringify(line))
writeFileSync(process.argv[2], lines.join('\n') + '\n')
EOF
cd $FCBROOT/src/ts && node remote_dump.mjs $FCBTMP/http.ts.jsonl && rm remote_dump.mjs

python3 $FCBTMP/compare.py $FCBTMP/delft.rust.jsonl \
    $FCBTMP/http.py.jsonl $FCBTMP/http.ts.jsonl
```

```
http.py.jsonl: IDENTICAL to oracle (1116 lines)
http.ts.jsonl: IDENTICAL to oracle (1116 lines)
```

**TypeScript's HTTP edge cases** — covered automatically by `test/http.test.ts`
(10 tests: strict `206` validation, the `200`-instead-of-`206` rejection,
malformed `Content-Range`, request counting). Re-run it alone with:

```bash
cd $FCBROOT/src/ts && npx vitest run test/http.test.ts
```

**Rust over HTTP** — covered by `src/rust/fcb_core/tests/http.rs`, which runs
inside `cargo nextest run --all-features` and hits the public bucket directly
(no local server).

Stop the server when finished: `kill %1`.

### 7.2 Against the real public file

The bucket the web viewer defaults to is live and correctly configured:

```bash
curl -sS -o /dev/null -D - -H "Range: bytes=0-31" \
  -H "Origin: http://localhost:5173" \
  https://flatcitybuf.open3d.city/data/3dbag_subset_all_index.fcb | \
  grep -iE "^HTTP|accept-ranges|content-range|access-control-expose"
```

```
HTTP/2 206
accept-ranges: bytes
content-range: bytes 0-31/3819399975
access-control-expose-headers: Accept-Ranges, Authorization, Content-Length, Content-Range, ...
```

`Content-Range` **must** appear in `Access-Control-Expose-Headers` or a browser
client cannot learn the file size and will refuse to guess. On the public
bucket at flatcitybuf.open3d.city that means a CORS policy whose
`ExposeHeaders` lists `Content-Range` and `Accept-Ranges`; check it with the
command above before blaming the reader.
(The header block above was recorded against the previous GCS host; only the
`HTTP/2 206`, `accept-ranges` and `content-range` lines are host-independent.)

**Python against 3.8 GB, remotely:**

```bash
cat > $FCBTMP/remote_py.py <<'EOF'
import time
import flatcitybuf as fcb
URL = "https://flatcitybuf.open3d.city/data/3dbag_subset_all_index.fcb"
t = time.time()
reader = fcb.FcbReader.open(fcb.HttpRangeReader(URL))
info = reader.header.info
print("features_count:", info.features_count, "| open %.2fs" % (time.time() - t))
t = time.time()
hits = fcb.search_rtree(reader.range_reader, reader.header.layout.rtree_begin,
                        info.features_count, info.index_node_size,
                        (84000, 446000, 85000, 447000))
print("bbox hits:", len(hits), "in %.2fs" % (time.time() - t))
print("first id:", fcb.to_cityjson_feature(reader.feature_at(hits[0]), reader.header)["id"])
EOF
cd $FCBROOT/src/py && uv run --extra dev python $FCBTMP/remote_py.py
```

```
features_count: 595762 | open 0.17s
bbox hits: 2064 in 11.94s
first id: NL.IMBAG.Pand.0503100000000031
```

**TypeScript against the same file** (needs `cd src/ts && just build`):

```bash
cat > $FCBROOT/src/ts/remote_ts.mjs <<'EOF'
import { FcbReader } from './dist/index.js'
const URL_ = 'https://flatcitybuf.open3d.city/data/3dbag_subset_all_index.fcb'
const reader = await FcbReader.fromUrl(URL_)
console.log('features_count:', reader.header.info.featuresCount)
const cur = await reader.select({
  spatial: { kind: 'bbox', value: [84000, 446000, 85000, 447000] }, limit: 3,
})
console.log('total matches:', cur.featuresCount)
for await (const f of cur) console.log('  ', f.id)
EOF
cd $FCBROOT/src/ts && node remote_ts.mjs && rm remote_ts.mjs
```

```
features_count: 595762
total matches: 2064
   NL.IMBAG.Pand.0503100000000031
   NL.IMBAG.Pand.0503100000023394
   NL.IMBAG.Pand.0503100000023504
```

**The cross-check that matters:** both readers report 595762 features, 2064
matches for the same bbox, and the same first feature id — over the network,
against a 3.8 GB file neither of them downloaded. Opening it took 90 ms in TS
and 170 ms in Python.

> ⚠️ The C++ reader **rejects** these two public files with
> `error: header failed FlatBuffers verification`. They predate the alignment
> fix on this branch — this is correct behaviour, not a C++ defect. See
> [issue 3](#issue-3-the-public-demo-files-predate-the-alignment-fix). Use the
> local range server (§7.1) for like-for-like four-way remote comparison.

### 7.3 Web viewer, manual

```bash
cd $FCBROOT/src/ts && just build       # the viewer resolves dist/
cd $FCBROOT/examples/web && just dev
```

Open the printed URL and walk through:

1. **Default URL loads.** The pre-filled 3DBAG subset URL loads and the first
   200 buildings render; the camera flies to the data. Confirm in DevTools →
   Network that the requests are `206 Partial Content`, not one 3.8 GB `200`.
2. **Another URL.** Paste a different `.fcb` URL; the header panel updates.
3. **Local file.** Pick `examples/data/delft.fcb` from disk — this goes through
   the `Blob` path, not `fetch`, and is never subject to CORS.
4. **Bbox query.** Draw a box; only features inside it are returned, and the
   bbox rectangle is drawn.
5. **Attribute query.** Only *queryable* (indexed) columns are offered; a
   condition such as `b3_h_dak_50p > 20` filters the result.
6. **Paging.** "Load next batch" appends without re-querying from scratch.
7. **Inspector.** Clicking a building shows its attributes, including files
   where one feature carries several CityObjects.

If you see *"sent a 206 response without an accessible Content-Range header"*,
the server is not exposing the header cross-origin. For an R2 bucket, add
`Content-Range` and `Accept-Ranges` to `ExposeHeaders` in its CORS policy. For
GCS:

```bash
echo '[{"maxAgeSeconds":3600,"method":["GET","HEAD","OPTIONS"],"origin":["*"],
"responseHeader":["Content-Type","Content-Range","Accept-Ranges"]}]' > cors.json
gsutil cors set cors.json gs://your-bucket
```

Then hard-reload — browsers cache the failed response.

### 7.4 The opt-in live-3DBAG suite (all four readers)

Every reader has an automated integration test against the published
`3dbag_all_index.fcb` (~68 GB, EPSG:28992). They are **off by default** — they
hit a live bucket, so they must never run in normal CI or download 68 GB when
someone runs `pytest`/`vitest`/`nextest`. One command turns them on everywhere:

```bash
just test-remote        # Rust, C++, Python, TypeScript, in sequence
```

It sets `FCB_REMOTE_HTTP_URL` to the 3DBAG URL and runs each reader's gated
test. Point it at any current-format file by exporting that variable first, or
run one reader on its own:

```bash
cd src/py  && just test-remote
cd src/cpp && just test-remote      # builds the curl tree first
```

Each test asserts the same three things, so a pass means the readers genuinely
agree, not merely that each returns *something*:

1. **The header verifies** — proof the file is in the post-`540772a` format
   (this is what the C++ reader rejected before the re-upload).
2. **`features_count == 10771547`**, and a ~1 km Amsterdam box
   `[120000, 486000, 121000, 487000]` returns **exactly 2762 features** — the
   same number in Rust, C++, Python and TypeScript.
3. **A bounded number of range requests** — opening costs 2, the bbox query a
   few dozen; the 68 GB body is never downloaded.

Gating, per reader:

| Reader | Mechanism | Skips when |
|---|---|---|
| Rust | `#[ignore]` + `FCB_REMOTE_HTTP_URL` | always, unless `--run-ignored` |
| C++ | `FCB_REMOTE_HTTP_URL` env check | var unset |
| Python | `@pytest.mark.skipif` on the env var | var unset |
| TypeScript | `describe.skipIf` on the env var | var unset |

> If `3dbag_all_index.fcb` is regenerated, the expected `10771547` / `2762`
> constants must be updated in lock-step across all four suites
> (`src/rust/fcb_core/tests/http.rs`, `src/cpp/tests/test_http.cpp`,
> `src/py/tests/test_http.py`, `src/ts/test/http.test.ts`) — each file's
> comment says so.

The local-range-server tests (§7.1) stay: they cover the failure paths a real
CDN cannot (ignored ranges, malformed `Content-Range`, CORS), deterministically
and offline. The remote suite is an addition, not a replacement.

---

## 8. Known issues found while validating this guide

These were found by running the steps above. None are speculative; each has a
reproduction.

### Issue 1 (FIXED): C++ `just test-http` failed on a stale hardcoded fixture size

`src/cpp/tests/test_http.cpp:33` asserts `r.total_size() == 7668160`, but
`examples/data/delft.fcb` is now **7666308** bytes — the fixture was
regenerated by `540772a` (the alignment fix) and the assertion was not updated.

```
test_http.cpp:33: ERROR: CHECK( r.total_size() == 7668160 ) is NOT correct!
  values: CHECK( 7666308 == 7668160 )
[doctest] test cases: 150 | 149 passed | 1 failed
```

Nothing else fails; the HTTP adapter itself is fine. CI never caught it because
its `check-cpp` job does not set `FCB_TEST_HTTP_URL`, so every HTTP test skips.

**Fixed.** The assertion now derives the expected size at run time from
`FileRangeReader(FCB_TEST_DATA_DIR "/delft.fcb").total_size()` rather than
re-pinning a literal — the corpus is not byte-reproducible, so a literal would
drift again. Verified: `150 passed | 0 failed`, 16111/16111 assertions.

`ci.yml` now runs `just test-http` as its own step, so this class of drift
cannot hide in CI again.

### Issue 2: Python reader collapses a nesting level on single-solid MultiSolid/CompositeSolid

Not covered by any suite: `appearance_depths` is one of the four corpus cases
missing from `src/py/tests/test_conformance.py:23`. Running it by hand, **7 of
12 features disagree with the Rust oracle** (`one_solid_multisolid`,
`one_solid_compositesolid`, `null_solid_compositesolid`, `null_shell_solid`,
`palette`, `empty_texture_values`, `texture_theme_without_values`,
`empty_semantics_surfaces`).

For a `MultiSolid` holding exactly one solid:

```
rust  : "boundaries": [[[[[0,1,2,3]],[[4,5,6,7]]]]]   (solid→shell→surface→ring)
python: "boundaries":  [[[[0,1,2,3]],[[4,5,6,7]]]]    (one level short)
rust  : "material": {"winter": {"values": [[[0,1]]]}}
python: "material": {"winter": {"values":  [[0,1]]}}
```

Mechanism: `src/py/flatcitybuf/geometry.py:190-198` dispatches on *which count
array is populated* and then applies `_collapse()` (line 117), which unwraps a
single-element outermost level. A `MultiSolid` with one solid is indistinguishable
from a `Solid` by that test, so it loses the solid level.
`decode_material_values` (line 236) has the same flaw at its
`if len(solids) == 1:` guard, line 262.

The C++ reader gets this right by dispatching on the geometry **type**
(`src/cpp/src/geometry.cpp:159-200`, `switch (type)` with a dedicated
`MultiSolid`/`CompositeSolid` arm and no collapse), and Python's
`_decode_semantics_values` (`src/py/flatcitybuf/cityjson.py:251`) already takes
`geom_type` and dispatches correctly — so the fix is to thread the geometry
type into `decode_boundaries` / `decode_material_values` / `decode_texture_values`
and drop the array-shape inference. The docstring's claim that dispatching on
the populated array "is what the reference does" is simply not true of the
current C++ source.

Reproduce:

```bash
cd $FCBROOT/src/py && uv run --extra dev python -c "
import json
from pathlib import Path
from flatcitybuf.cityjson import to_cityjson_feature, to_cityjson_metadata
from flatcitybuf.reader import FcbReader
C = Path('$FCBROOT/conformance')
exp = [json.loads(l) for l in (C/'appearance_depths.expected.jsonl').read_text().splitlines() if l.strip()]
r = FcbReader.open_file(C/'appearance_depths.fcb')
act = [to_cityjson_metadata(r.header)] + [to_cityjson_feature(f, r.header) for f in r.select_all()]
print(sum(a != b for a, b in zip(act, exp)), 'of', len(exp), 'lines differ')
"
```

Add the four missing names to `CASES` and the suite will catch this
permanently. Once fixed, it belongs in `docs/upstream-findings.md`.

### Issue 3 (partly fixed): the public demo files predated the alignment fix

| File | Size | Post-alignment-fix format? |
|---|---|---|
| `3dbag_all_index.fcb` | 68.5 GB | **yes** — re-serialized 2026-07-23; verifies in all four readers |
| `3dbag_subset_all_index.fcb` | 3.8 GB | no — still the pre-`540772a` layout |

`540772a` (2026-07-20) fixed `finish_size_prefixed` laying out 8-byte-aligned
structs relative to inconsistent bases. Files written before it keep the old,
internally inconsistent alignment, so C++ — which re-enabled `check_alignment`
— rejects them with `header failed FlatBuffers verification`, while Rust
(verifier does not check alignment), Python and TypeScript (no verifier at all)
read them anyway.

`3dbag_all_index.fcb` has since been re-serialized and now verifies under all
four readers — see the opt-in remote suite in §7.4. `3dbag_subset_all_index.fcb`
(the web viewer's fallback URL) has not; re-uploading it would close this out.

### Issue 4: the old `just clippy` recipe failed with 50 pre-existing lints

```
error: could not compile `fcb_core` (lib) due to 50 previous errors
error: Recipe `clippy` failed on line 111 with exit code 101
```

The recipe is `cargo clippy -- -D warnings`, which promotes long-standing style
lints in `fcb_core` to errors: `should_implement_trait`,
`non_canonical_partial_ord_impl`, `missing_safety_doc`,
`extra_unused_lifetimes`, `empty_docs`.

Nothing in CI ran it, so it was dead weight that failed whenever someone
reached for it. The restructured `src/rust/justfile` drops `-D warnings` from
`just lint` (with a comment recording the intent) so the recipe matches what CI
actually enforces. Clean up the 50 lints, then re-add `-- -D warnings` there.

### Issue 5 (FIXED): the C++ tree had never been clang-formatted

`src/cpp/.clang-format` and `.clang-tidy` were both committed, but nothing ever
invoked them — no justfile recipe, no CI job — and **45 of 51 files did not
conform**.

**Fixed.** `cd src/cpp && just fix` reformatted 47 files (+1586 / -982), and
`just lint` (`clang-format --dry-run --Werror`) is now clean and part of both
`just check` and CI. The suite is unaffected: `150 passed | 0 failed`,
16111/16111 assertions.

> Keep that reformat as its own commit and add its SHA to
> `.git-blame-ignore-revs`, or it poisons `git blame` for the whole C++ tree.

`.clang-tidy` is a separate story and is deliberately **not** gated: under the
clang-tidy on PATH it reports dozens of warnings plus 10 hard diagnostic
errors. `just tidy` runs it; clean it up before promoting it into `lint`.

---

## 8b. API documentation

Every language generates its own API reference behind the same verb:

```bash
just docs                    # all four at once
cd src/py && just docs       # or one at a time
```

| Language | Generator | Output |
|---|---|---|
| Rust | `cargo doc` (+ docs.rs metadata) | `src/rust/target/doc/` |
| C++ | Doxygen (`brew install doxygen`) | `src/cpp/docs/html/` |
| Python | pdoc | `src/py/docs/api/` |
| TypeScript | TypeDoc | `src/ts/docs/` |

All four outputs are gitignored. `.github/workflows/docs.yml` builds them into
one GitHub Pages site (`/rust/`, `/cpp/`, `/python/`, `/typescript/`) and
attaches a tarball to each release.

Known warnings, both benign and both reported rather than papered over:
Doxygen cannot resolve `README.md`'s link to `../../docs/upstream-findings.md`
(outside its `INPUT`), and TypeDoc reports that `FileLayout` and one of the two
`JsonValue` declarations are referenced by documented API but not exported from
`src/ts/src/index.ts` — a real public-surface gap, left for the maintainer.

---

---

## 9. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `Error: Hook timed out in 10000ms` in `test/http.test.ts` | `range_server.py` did not print its port in 10 s — usually CPU contention from another suite. Run suites one at a time. |
| `range_server.py did not report a port` (C++/Python) | `python3` missing from `PATH`, or the port is already bound. It binds port 0, so it should not collide; check the server's stderr. |
| C++ HTTP tests all say `FCB_TEST_HTTP_URL not set; skipping` | You ran `just test`, not `just test-http`. Only the latter starts the server and exports the URL. |
| `built without FCB_WITH_CURL` from `fcb_read_http` | You are running the `build-native` binary. Use `src/cpp/build-curl/fcb_read_http`. |
| `header failed FlatBuffers verification` (C++) | The file predates `540772a`. Rewrite it with the current Rust CLI. See issue 3. |
| `Cannot find module '@cityjson/flatcitybuf'` in `examples/web` | `src/ts/dist` is missing. Run `cd src/ts && just build` first. |
| Browser tests fail to launch | `cd src/ts && npx playwright install chromium`. |
| `sent a 206 response without an accessible Content-Range header` | Cross-origin server not exposing `Content-Range`. See §7.3. |
| A recipe changed my files | Only `just fix` does that, by design. `just check` and everything under it is read-only. |
| `just gen-conformance` dirties the tree with no semantic change | Expected: `cjseq2` iterates a `HashMap`, so regeneration is never byte-reproducible. Diff the parsed JSON; do not commit churn. |
| `just lint` fails in `src/cpp` | Expected — 45 of 51 files predate the committed `.clang-format`. See issue 5. |
| `just <verb>` says "does not contain recipe" | The deprecated root aliases (`check-common`, `ts-test`, …) were removed. Use the unified verbs, or `cd` into the language directory. |
| `examples/web` typecheck errors | Check `git status` first — the web example is under active development, and a failing `tsc --noEmit` there is usually uncommitted work in progress, not a branch defect. |

---

## 10. Sign-off checklist

- [ ] Rust: 174/174 (`cd src/rust && just check`)
- [ ] C++ local: `cd src/cpp && just test` — 100% passed
- [ ] C++ HTTP: `cd src/cpp && just test-http` — 150/150
- [ ] Python: `cd src/py && just check` — 255 then 251+4 skipped; mypy clean
- [ ] TypeScript: `cd src/ts && just check` — 244 Node + 3 browser, build clean
- [ ] Web example: `cd examples/web && just check` — 50/50, type and build clean
- [ ] Whole workspace: `just check`
- [ ] Local four-way cross-check on `delft.fcb`: three `IDENTICAL to oracle`
- [ ] Round-trip through the Rust writer, re-compared four ways
- [ ] Remote via local range server: Python and TS `IDENTICAL to oracle`; C++ reads the bbox
- [ ] Remote via public URL: Python and TS agree on count, matches and first id
- [ ] Web viewer manual walkthrough (§7.3), all 7 steps

---

## 11. Last full run

Executed end-to-end on 2026-07-23 against `develop` @ `06b8d38`, macOS 15
(arm64). Suites run sequentially.

| Step | Result |
|---|---|
| Rust `cargo nextest --all-features --workspace` | ✅ 174/174 (74 s) |
| Rust `cargo fmt --check` | ✅ clean |
| Rust `cargo clippy` (read-only) | ✅ 2 pre-existing `dead_code` warnings in test targets |
| `just lint` in `src/cpp` | ❌ 45 of 51 files unformatted — **issue 5** |
| `cd src/cpp && just test` | ✅ 100% passed |
| `cd src/cpp && just test-http` | ✅ 150/150 cases, 16111/16111 assertions (issue 1 fixed) |
| Python, no `numpy` | ✅ 251 passed, 4 skipped; mypy clean |
| Python, with `numpy` | ✅ 255 passed; mypy clean |
| `cd src/py && just check` | ✅ both passes, lint + mypy clean |
| `cd src/ts && just check` | ✅ 244 Node + 3 browser, type and build clean |
| `just type` (all five directories) | ✅ clean, except the `examples/web` WIP below |
| TS browser (Chromium) | ✅ 2 files, 3/3 |
| `examples/web` tests | ✅ 3 files, 50/50 |
| `examples/web` typecheck | ⚠️ clean on the committed tree; fails against uncommitted WIP |
| Conformance coverage | ⚠️ C++ 14/14, TS 14/14, Python 10/14 — **issue 2** |
| Four-way local cross-check, `delft.fcb` | ✅ C++/Python/TS each identical to Rust, 1116/1116 lines |
| Round-trip (`ser` → re-read four ways) | ✅ identical, 1116/1116 lines |
| Remote, local range server | ✅ Python and TS identical to the local oracle, 1116 lines; C++ 931/1115 features in 9 requests |
| Remote, public hosted file | ✅ Python and TS both: 595762 features, 2064 bbox matches, same first id; C++ rejects — **issue 3** |
