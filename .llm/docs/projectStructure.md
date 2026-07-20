# FlatCityBuf Project Structure

```
flatcitybuf/
│
├── 📄 README.md                    # Project overview and getting started guide
├── 📄 CLAUDE.md                     # AI/LLM coding guidelines for this repository
├── 📄 CONTRIBUTING.md              # Contribution guidelines
├── 📄 LICENSE                       # MIT License
│
├── 📁 .llm/                        # AI/LLM context and documentation
│   └── docs/
│       ├── productContext.md       # Project purpose, problem, and goals
│       ├── specification.md        # FlatCityBuf encoding specification
│       └── projectStructure.md     # This file - folder structure overview
│
├── 📁 docs/                         # Public documentation and assets
│   ├── logo.png                    # Project logo
│   └── ...
│
├── 📁 examples/                     # Usage examples and tutorials
│   └── data/                       # Example data files
│       └── delft.fcb               # Sample FlatCityBuf file
│
├── 📁 scripts/                      # Build and utility scripts
│
├── 📁 src/                          # Source code root
│   │
│   ├── 📁 fbs/                      # FlatBuffers schema definitions
│   │   └── cityjson.fbs            # CityJSON FlatBuffers schema
│   │
│   ├── 📁 rust/                     # Rust workspace (core implementation)
│   │   ├── 📄 Cargo.toml           # Workspace configuration
│   │   ├── 📄 Cargo.lock           # Dependency lock file
│   │   ├── 📄 CLAUDE.md            # Rust-specific coding guidelines
│   │   ├── 📄 Dockerfile           # Docker image for Rust builds
│   │   ├── 📄 makefile             # Build automation
│   │   │
│   │   ├── 📁 cli/                 # fcb_cli - Command-line interface
│   │   │   ├── src/
│   │   │   │   └── main.rs        # CLI entry point
│   │   │   └── Cargo.toml
│   │   │
│   │   ├── 📁 fcb_core/            # Core library (crate)
│   │   │   ├── src/
│   │   │   │   ├── lib.rs         # Library entry point
│   │   │   │   ├── http/          # HTTP range request client
│   │   │   │   ├── index/         # Spatial (Packed R-tree) & Attribute (B+Tree) indexing
│   │   │   │   ├── reader/        # FlatBuffers reading & deserialization
│   │   │   │   └── writer/        # FlatBuffers writing & serialization
│   │   │   └── Cargo.toml
│   │   │
│   │   ├── 📁 fcb_py/              # Python bindings (PyO3)
│   │   │   ├── src/
│   │   │   │   └── lib.rs         # FFI bridge to Python
│   │   │   └── Cargo.toml
│   │   │
│   │   ├── 📁 fcb_cpp/             # C++ bindings (cxx bridge)
│   │   │   ├── src/
│   │   │   │   └── lib.rs         # FFI bridge to C++
│   │   │   └── Cargo.toml
│   │   │
│   │   ├── 📁 wasm/                # WebAssembly bindings (wasm-bindgen)
│   │   │   ├── src/
│   │   │   │   └── lib.rs         # FFI bridge to JS/TS
│   │   │   └── Cargo.toml
│   │   │
│   │   ├── 📁 fcb_api/             # REST API server (axum)
│   │   │   ├── src/
│   │   │   │   └── main.rs        # API server entry point
│   │   │   └── Cargo.toml
│   │   │
│   │   └── 📁 data/                # Test data and fixtures
│   │
│   ├── 📁 cpp/                      # C++ library bindings
│   │   ├── 📁 include/             # Public C++ headers
│   │   │   └── flatcitybuf/
│   │   │       └── flatcitybuf.hpp
│   │   ├── 📁 examples/            # C++ usage examples
│   │   │   └── example.cpp
│   │   ├── 📁 build/               # C++ build artifacts (generated)
│   │   ├── 📄 CMakeLists.txt       # CMake build configuration
│   │   └── 📄 Doxyfile             # Doxygen documentation config
│   │
│   └── 📁 ts/                       # npm package dir -- ONE tracked file
│       └── 📄 package.json         # npm name, version, metadata
│                                    # fcb_wasm.{js,d.ts}, fcb_wasm_bg.wasm
│                                    # and snippets/ land here from
│                                    # `just build-wasm` and are gitignored;
│                                    # the demo page lives in examples/wasm/
│
├── 📁 data/                         # Development data files
│   └── out/                        # Output files from conversions
│
├── 📁 .agent/                       # Agent configuration (for AI tools)
│   └── rules/
│
├── 📁 .cursor/                      # Cursor IDE configuration
│   └── rules/
│
├── 📁 .serena/                      # Serena AI cache
│   ├── cache/
│   └── memories/
│
└── 📁 .vscode/                      # VS Code configuration
    └── settings.json
```

## Component Overview

### Rust Workspace (`src/rust/`)

The Rust workspace is organized as a multi-crate project with the following members:

| Crate | Purpose | Language Bindings |
|-------|---------|-------------------|
| **fcb_core** | Core library with read/write/indexing capabilities | Pure Rust |
| **cli** | Command-line interface (`fcb` command) | - |
| **fcb_py** | Python bindings via PyO3 | Python |
| **fcb_cpp** | C++ bindings via cxx bridge | C++ |
| **wasm** | WebAssembly bindings via wasm-bindgen | JavaScript/TypeScript |
| **fcb_api** | REST API server using axum | HTTP API |

### Language Bindings

FlatCityBuf provides native bindings for multiple languages:

1. **Python** (`src/rust/fcb_py/`) → Published to PyPI as `flatcitybuf`
2. **C++** (`src/cpp/`) → Standalone C++ library with CMake build
3. **JavaScript/TypeScript** (`src/ts/`) → Published to npm as `@cityjson/flatcitybuf`

### Build Artifacts (Not Tracked)

- `src/rust/target/` - Rust build artifacts (in `.gitignore`)
- `src/cpp/build/` - C++ build artifacts (in `.gitignore`)

## Key Patterns

### Workspace Dependency Management

All dependencies are managed at the workspace level in `src/rust/Cargo.toml`. Individual crates use workspace dependencies:

```toml
[dependencies]
fcb_core = { workspace = true }
```

### Code Organization

Each crate follows standard Rust conventions:
- `src/lib.rs` - Library entry point
- `src/main.rs` - Binary entry point
- Feature flags in `Cargo.toml` for conditional compilation

### Cross-Language Bridge

Language bindings use appropriate FFI technologies:
- **Python**: PyO3 with async runtime support
- **C++**: cxx bridge for automatic Rust/C++ interoperability
- **WASM**: wasm-bindgen for browser compatibility
