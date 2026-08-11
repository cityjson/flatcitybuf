# FlatCityBuf — native C++

A from-scratch C++17 implementation of FlatCityBuf: a **reader and a writer**,
parsing and producing `.fcb` bytes directly. It replaces the previous
CXX-bridge bindings over the Rust core — there is no Rust toolchain, no
generated bridge source to compile, and no TLS dependency. All IO goes through
one synchronous `fcb::RangeReader`, so local files, HTTP range requests and
your own transport share a single traversal path.

## Quick start

```bash
cd src/cpp
cmake -B build -S .
cmake --build build
./build/fcb_read_local ../../examples/data/delft.fcb > delft.jsonl
```

That reads a `.fcb` out as CityJSONSeq. Writing goes the other way, through
`fcb::FcbWriter` — `./build/fcb_write_cityjson <input.jsonl> <output.fcb>`, from
[`examples/write_cityjson.cpp`](examples/write_cityjson.cpp).

The library API (open, `select_bbox` / `select_attr`, CityJSON conversion) is
shown in [docs/cpp.md](../../docs/cpp.md) and [INSTALL.md](INSTALL.md).

## Layout

| Path | Contents |
|---|---|
| `include/fcb/` | public headers (`writer/` for the writer API) |
| `include/fcb/generated/` | committed flatc output (consumers never need flatc) |
| `src/` | implementation; `src/detail/` is internal |
| `tests/` | doctest suite and the range-capable HTTP test server |
| `examples/` | one runnable program per capability — see [examples/README.md](examples/README.md) |

## Documentation

- **[docs/cpp.md](../../docs/cpp.md)** — the C++ guide: status, build options,
  reading and writing, testing, and where this implementation deliberately
  differs from the Rust reference
- [INSTALL.md](INSTALL.md) — dependencies, install, CMake integration, HTTP,
  custom transports, writing a file
- [examples/README.md](examples/README.md) — every example program, with its
  real output
- [../../docs/specification.md](../../docs/specification.md) — the format itself
