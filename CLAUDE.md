# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

FlatCityBuf is a cloud-optimized binary format for storing and retrieving 3D city models. It combines the semantic richness of CityJSON with the performance benefits of FlatBuffers binary serialization and advanced spatial indexing techniques. The project focuses on enabling efficient partial data retrieval from cloud storage using HTTP range requests.

## Common Development Commands

### Python Development

```bash
# Build and install Python bindings in development mode
cd src/rust
make py-develop

# Build Python wheel for distribution
cd src/rust
make py-build

# Run Python tests
cd src/rust
make py-test

# Clean Python build artifacts
cd src/rust
make py-clean
```

### Building and Testing

```bash
# Build all Rust crates (excluding WASM)
cd src/rust
cargo build --workspace --all-features --exclude fcb_wasm --release

# Run pre-commit checks (formatting, linting, tests) - runs check-common, check-wasm, check-py
cd src/rust
make pre-commit

# Run tests with nextest (faster test runner)
cd src/rust
cargo nextest run --all-features --workspace --exclude fcb_wasm --exclude fcb_py

# Run specific integration test
cd src/rust
cargo test -p fcb_core --test e2e

# Run Python tests (uv required)
cd src/rust
make py-test

# Run benchmarks
cd src/rust
cargo bench -p fcb_core --bench read -- --release
```

### Code Generation

```bash
# Generate Rust code from FlatBuffers schemas
make gen-all  # Runs all generation scripts
# OR specifically:
./scripts/gen_rust.sh  # Generates Rust bindings from .fbs files
```

### WebAssembly Build

```bash
# Build WASM module for release
cd src/rust/wasm && wasm-pack build --target web --release --out-dir ../../ts

# Build WASM module for debug (via makefile)
cd src/rust
make wasm-build
```

### Linting and Formatting

```bash
cd src/rust

# Format code
cargo fmt --all

# Run clippy with auto-fix
cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --exclude fcb_wasm

# Run clippy for WASM target
cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown

# Check for security vulnerabilities
cargo audit
```

## High-Level Architecture

### Core Components

1. **FlatBuffers Schemas** (`/src/fbs/`)
   - `header.fbs` - File metadata, transformations, and index information
   - `geometry.fbs` - 3D geometry structures and semantic surfaces
   - `feature.fbs` - City objects and their attributes
   - `extension.fbs` - Support for CityJSON extensions

2. **Rust Workspace** (`/src/rust/`)
   - `fcb_core` - Core library for reading/writing FlatCityBuf format
   - `cli` - Command-line interface for file conversion and analysis
   - `fcb_wasm` - WebAssembly bindings for browser usage
   - `fcb_py` - Python bindings using PyO3 and maturin
   - `fcb_api` - HTTP API server for FlatCityBuf operations

3. **Indexing Structures**
   - **Packed R-tree**: 2D spatial indexing using Hilbert space-filling curves
   - **Static B+Tree**: Attribute-based indexing with efficient range queries
   - Both indices store byte offsets for direct feature access

### File Format Structure

```
┌─────────────────┐
│  Magic Bytes    │  8 bytes - File identifier
├─────────────────┤
│  Header Size    │  4 bytes - Size of header
├─────────────────┤
│     Header      │  Variable - Metadata & schema
├─────────────────┤
│   R-tree Index  │  Variable - Spatial index
├─────────────────┤
│ Attribute Index │  Variable - B+Tree indices
├─────────────────┤
│    Features     │  Variable - City objects
└─────────────────┘
```

### Key Design Principles

1. **Zero-Copy Access**: FlatBuffers enables direct memory access without parsing
2. **HTTP Range Optimization**: File structure aligned for efficient partial downloads
3. **Hilbert Ordering**: Features sorted by Hilbert curve for spatial locality
4. **Hierarchical Geometry**: Efficient encoding of 3D boundaries using dimensional arrays
5. **Extension Support**: Full compatibility with CityJSON extension mechanism

### Query Execution Flow

1. **Spatial Queries**:
   - Traverse R-tree using HTTP range requests
   - Retrieve feature offsets from leaf nodes
   - Batch nearby features to minimize requests

2. **Attribute Queries**:
   - Traverse B+Tree index to find matching keys
   - Handle duplicates via payload section
   - Use prefetching and batch resolution for efficiency

3. **Feature Retrieval**:
   - Use offsets to fetch specific byte ranges
   - Decode features using FlatBuffers zero-copy access
   - Process geometry and attributes on demand

## Development Guidelines

### Error Handling

- Use `thiserror` for custom error types (not `anyhow` in library code)
- Return `Result<T, E>` instead of panicking
- Provide descriptive error messages

### Performance Considerations

- Minimize HTTP requests through batching
- Use buffered readers with caching
- Prefer iterators over collecting into vectors
- Profile with `criterion` benchmarks

### Testing Strategy

- Unit tests with `#[cfg(test)]` modules
- Integration tests in `/tests/` directories
- Use `cargo nextest` for faster test execution
- Mock HTTP clients for remote data tests

### Code Style

- Follow Rust naming conventions (snake_case, PascalCase)
- Keep functions focused and modular
- Document public APIs with rustdoc
- Use clippy with strict warnings enabled

## Important Notes

- The project uses a Rust workspace structure - always build from `/src/rust/`
- WASM builds require `wasm-pack` and target `wasm32-unknown-unknown`
- Python bindings require `uv` package manager and use maturin for building
- FlatBuffers schemas must be regenerated after changes using `make gen-all`
- HTTP optimization is critical - always consider range request efficiency
- Geometry templates are supported for efficient repeated geometry encoding
- Use `thiserror` for custom error types in library code, avoid `anyhow` except when explicitly approved
