# FlatCityBuf — native C++ reader

A from-scratch C++17 implementation of the FlatCityBuf reader. It replaces the
previous CXX-bridge bindings over the Rust core: there is no Rust toolchain,
no generated bridge source to compile, and no TLS dependency.

See [INSTALL.md](INSTALL.md) for building and using it.

## Why native

The FFI bindings were awkward precisely where it mattered: the Rust side owned
a tokio runtime, and bridging that to C++ callers leaked complexity in both
directions. This implementation has no async runtime at all. All IO goes
through one synchronous, user-implementable `RangeReader` interface with a
batched read, so local files and HTTP share a single traversal path and host
applications keep their own threading model.

## Layout

| Path | Contents |
|---|---|
| `include/fcb/` | public headers |
| `include/fcb/generated/` | committed flatc output (consumers never need flatc) |
| `src/` | implementation; `src/detail/` is internal |
| `tests/` | doctest suite, range-capable HTTP test server (conformance corpus lives at `/conformance` in the repo root) |
| `examples/` | `read_local.cpp`, `read_http.cpp` |

## Verification

Output is checked against the Rust reader on the full Delft fixture — all 1115
features, compared as parsed JSON trees rather than text, since key order and
float formatting legitimately differ. A conformance corpus covers edge cases
the main fixture does not reach: single-feature files, prefix-colliding
strings, duplicate keys forcing payload entries, zero-area extents, and
geometry templates.

The suite runs clean under ASan and UBSan.

## Correctness notes

Porting surfaced several defects in the Rust implementation, most now fixed
upstream — see [`docs/upstream-findings.md`](../../docs/upstream-findings.md).
Two behaviours here are deliberately stricter than the reference:

- `select_attr` post-filters fixed-width string candidates against the full,
  untruncated value. Keys are truncated to 50 or 100 bytes and zero-padded, so
  the index yields candidates rather than answers.
- Range operators are evaluated as strict-or-inclusive bounds at the leaf
  rather than as "range minus equal", which drops genuine matches when one
  feature carries several values of an indexed attribute.
