# fcb_cpp

Rust crate providing C++ bindings for the FlatCityBuf core library via [CXX](https://cxx.rs/).

This crate is not intended to be used directly from Rust. It exists to generate the static library (`libfcb_cpp.a`) and CXX bridge headers consumed by the C++ integration layer.

## Documentation

For C++ usage instructions, pre-built binary installation, API reference, and examples, see the **[C++ Bindings README](../../cpp/README.md)**.

## Crate Structure

- `src/lib.rs` — CXX bridge definitions (shared types and function signatures)
- `src/reader.rs` — Local file reader wrapper
- `src/writer.rs` — FCB file writer wrapper

## Building

```bash
# Build the static library
cd src/rust
cargo build --release -p fcb_cpp --no-default-features
```

Output: `target/release/libfcb_cpp.a` (Unix) or `target/release/fcb_cpp.lib` (Windows)
