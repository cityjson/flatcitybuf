# FlatCityBuf — C++

A from-scratch C++17 implementation of FlatCityBuf: a **reader and a writer**,
parsing and producing the bytes directly. It replaces the previous CXX-bridge
bindings over the Rust core, so there is **no Rust toolchain, no generated
bridge source to compile, and no TLS dependency**. Output is validated against
the shared conformance corpus, and the writer's output is compared byte-for-byte
against real Rust-written files.

Source: [`src/cpp/`](../src/cpp).

## Status

| | |
|---|---|
| Reading | conformant against the corpus in [`conformance/`](../conformance) |
| Writing | `fcb::FcbWriter`, checked byte-for-byte against Rust-written files |
| Standard | C++17; no Rust toolchain, no CXX bridge, no TLS dependency |
| Transports | local file, libcurl HTTP range requests, or your own `fcb::RangeReader` |

### Why native

The FFI bindings were awkward precisely where it mattered: the Rust side owned a
tokio runtime, and bridging that to C++ callers leaked complexity in both
directions. This implementation has no async runtime at all. All IO goes through
one synchronous, user-implementable `fcb::RangeReader` interface with a batched
read, so local files and HTTP share a single traversal path and host
applications keep their own threading model. Batching, not asynchrony, is the
concurrency primitive: a blocking interface is trivially wrapped by whatever
threading model an application already has, whereas an imposed async runtime is
not.

## Build and install

```bash
cd src/cpp
cmake -B build -S .
cmake --build build
cmake --install build --prefix /your/prefix
```

CMake options ([`CMakeLists.txt:8-11`](../src/cpp/CMakeLists.txt)):

| Option | Default | Effect |
|---|---|---|
| `FCB_WITH_JSON` | `ON` | CityJSON conversion, and the writer |
| `FCB_WITH_CURL` | `OFF` | the libcurl HTTP range reader |
| `FCB_BUILD_TESTS` | `ON` | the doctest suite |
| `FCB_BUILD_EXAMPLES` | `ON` | the example programs |

Dependencies, package names per platform, `find_package` integration, HTTP
usage, and implementing your own transport: **[`src/cpp/INSTALL.md`](../src/cpp/INSTALL.md)**.

## Reading

```cpp
#include <fcb/reader.hpp>
#include <fcb/cityjson.hpp>

fcb::FcbReader r = fcb::FcbReader::open_file("city.fcb");
auto it = r.select_bbox({84500, 445800, 85000, 446500});
while (it.next()) {
    std::cout << fcb::to_cityjson_feature(it.current(), r.header()).dump() << "\n";
}
```

`select_all()` iterates everything in stored (Hilbert) order and `select_attr()`
queries the static B+tree; all three return a `FeatureIterator`
([`include/fcb/reader.hpp:67-99`](../src/cpp/include/fcb/reader.hpp)). Reading
over HTTP is the same code against a different transport — build with
`FCB_WITH_CURL=ON` and see [INSTALL.md](../src/cpp/INSTALL.md).

Field access is `nlohmann::json`, not a bespoke API, and vertices stay quantized
integers — the runnable programs in
[`src/cpp/examples/README.md`](../src/cpp/examples/README.md) walk through
header inspection, CityJSON conversion, attribute queries, raw feature access,
custom transports, HTTP, and writing, each with its real output.

## Writing

`fcb::FcbWriter` produces `.fcb` natively; it needs `FCB_WITH_JSON` (on by
default). Building a file is two passes over the CityJSONSeq input: first
`fcb::add_attributes` accumulates an `fcb::AttributeSchema`
([`include/fcb/writer/attribute.hpp:42,50`](../src/cpp/include/fcb/writer/attribute.hpp)),
then each feature goes through `FcbWriter::add_feature`, and
`FcbWriter::write(std::ostream&)` streams the result out
([`include/fcb/writer/fcb_writer.hpp:80,92,105`](../src/cpp/include/fcb/writer/fcb_writer.hpp)).
`add_feature` spools each encoded feature to a temp file and `write` reads it
back in chunks, so peak memory does not grow with the number of features — the
`std::vector`-returning `write()` overload
([`fcb_writer.hpp:111`](../src/cpp/include/fcb/writer/fcb_writer.hpp)) is a
convenience for small files and does not have that property.

Two traps are worth knowing before you start: input lines must be parsed as
`nlohmann::ordered_json` (plain `nlohmann::json` sorts object members
alphabetically and silently renumbers the columns), and column numbering follows
the order `add_attributes` first *sees* a name — document order, never
alphabetical.

The full procedure, including index options, is in
[INSTALL.md § Writing a file](../src/cpp/INSTALL.md#writing-a-file); the
complete runnable program is
[`examples/write_cityjson.cpp`](../src/cpp/examples/write_cityjson.cpp),
described in [examples/README.md](../src/cpp/examples/README.md).

There is no CLI here — this is a library. The Rust CLI covers conversion; see
[rust.md](./rust.md).

## Testing

```bash
cd src/cpp
just check        # lint + build + test + test-http, read-only
just test         # native build + the doctest suite (no HTTP adapter)
just test-http    # the libcurl adapter, in its own build tree
```

`test-http` configures a **separate** build tree so the default build stays
curl-free and TLS-free, and it starts the range-capable test server that exports
`FCB_TEST_HTTP_URL` — without that variable every HTTP test silently skips,
which is why `just test` does not cover them.

Extras beyond the five standard verbs (`just --list` in `src/cpp`):

| Recipe | What it does |
|---|---|
| `test-remote` | opt-in live 3DBAG HTTP test against the published ~68 GB file |
| `harden` | the two gates CI enforces and no other recipe does: the default build must link neither curl nor a TLS stack, and the suite must be clean under ASan/UBSan |
| `tidy` | clang-tidy static analysis; not part of `check` yet |
| `gen-fbs` | regenerate the committed FlatBuffers headers |
| `docs` | doxygen HTML API docs (needs doxygen on PATH) |

### What the suite proves

- **Conformance** — `tests/test_conformance.cpp` replays the shared corpus at
  repo-root [`conformance/`](../conformance), whose `.expected.jsonl` files hold
  the Rust reader's own output. Python (conformant, [`src/py`](../src/py)) and
  TypeScript (conformant, [`src/ts`](../src/ts)) validate against the same
  corpus. It covers edge cases the Delft fixture never reaches: single-feature
  files, prefix-colliding strings, duplicate keys forcing payload entries,
  zero-area extents, and geometry templates.
- **The writer oracle** — `tests/test_writer_oracle.cpp` compares this writer's
  bytes against real Rust-written `.fcb` files, not merely checking that the
  result decodes. Decoding correctly is a weaker claim: two implementations can
  agree on a wrong answer. Offsets are derived from the header's own computed
  layout, never hardcoded, because the corpus is not byte-reproducible.
- **The full fixture** — beyond the automated suite, the manual procedure in
  [TESTING.md](./TESTING.md) dumps the whole Delft fixture through this reader
  and diffs it line by line against the Rust reader's own dump, comparing
  parsed JSON trees rather than text since key order and float formatting
  legitimately differ between languages.

Remote (HTTP range) verification is in [TESTING.md](./TESTING.md) too.

## Deliberate divergences from the Rust reader

Two behaviours here are stricter than the reference, on purpose:

- `select_attr` post-filters fixed-width string candidates against the full,
  untruncated value. Keys are truncated to 50 bytes (100 for Json/Binary) and
  zero-padded, so the index yields candidates rather than answers. Pass
  `AttrQueryOptions{true}` to skip verification — faster, and wrong for long
  strings.
- Range operators are evaluated as strict-or-inclusive bounds at the leaf rather
  than as "range minus equal", which drops genuine matches when one feature
  carries several values of an indexed attribute.

Porting surfaced several defects in the Rust implementation, most now fixed
upstream — each is recorded in
[upstream-findings.md](./upstream-findings.md).

## See also

- [`src/cpp/INSTALL.md`](../src/cpp/INSTALL.md) — dependencies, install, CMake
  integration, HTTP, custom transports, writing
- [`src/cpp/examples/README.md`](../src/cpp/examples/README.md) — every example
  program, with its real output
- [specification.md](./specification.md) — the format itself, down to byte
  offsets
- [TESTING.md](./TESTING.md) — manual verification, local and remote
- [rust.md](./rust.md) · [py.md](./py.md) · [ts.md](./ts.md) — the other
  implementations
