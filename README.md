# FlatCityBuf 🏙️

<div align="center">

![FlatCityBuf Logo](./docs/logo.png)

**A cloud-optimized binary format for storing and retrieving 3D city models**

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/HideBa/flatcitybuf)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![WebAssembly](https://img.shields.io/badge/WebAssembly-654FF0?style=flat&logo=webassembly&logoColor=white)](https://webassembly.org/)

_Bringing the semantic richness of CityJSON with the performance of FlatBuffers_

[🚀 Getting Started](#-getting-started) • [📊 Benchmarks](#-performance--benchmarks) • [📖 Documentation](#-documentation) • [📚 API Reference](https://cityjson.github.io/flatcitybuf/) • [🤝 Contributing](#-contributing)

</div>

---

## ✨ Overview

FlatCityBuf revolutionizes 3D city model storage and retrieval by combining the semantic richness of [CityJSON](https://github.com/cityjson/cityjson-spec) with the performance benefits of [FlatBuffers](https://github.com/google/flatbuffers) binary serialization and advanced spatial indexing techniques.

## Demo

Try the browser viewer live at
**[flatcitybuf-prototype.hideba.me](https://flatcitybuf-prototype.hideba.me)** —
open a `.fcb` over HTTP range requests (the full 3DBAG dataset by default) or a
local file, run spatial and attribute queries, and render the result with
deck.gl. No server component; reading is pure TypeScript, with export to
CityJSON/OBJ using a lazy-loaded WASM helper. Source in
[`examples/web`](examples/web). Supersedes the earlier WASM-based prototype.


https://github.com/user-attachments/assets/0f2df60a-4270-4b1a-9890-7ca37875801a



## Example FlatCityBuf File

- [3DBAG all (70GB)](https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb): serialised whole 3DBAG dataset with spatial and attribute indexing
- [3DBAG small (3.4GB)](https://flatcitybuf.open3d.city/data/3dbag_subset_all_index.fcb)
- [Delft (6MB)](examples/data/delft.fcb)
- [Every hosted file](docs/data.md): the full `.fcb` and CityJSONSeq inventory, with sizes and URLs

### 🎯 Why FlatCityBuf?

Traditional CityJSON formats face significant challenges in large-scale urban applications:

- **Slow parsing**: Entire files must be loaded and parsed
- **Memory intensive**: High memory consumption for large datasets
- **No spatial queries**: Lack of efficient spatial indexing
- **Limited cloud support**: Poor performance with remote data access

### 🚀 Key Features

| Feature                   | Benefit                                                   |
| ------------------------- | --------------------------------------------------------- |
| **⚡ Zero-copy Access**   | Access specific city objects without parsing entire files |
| **☁️ Cloud Optimized**    | HTTP range requests for partial data retrieval            |
| **🗺️ Spatial Indexing**   | Packed R-tree for lightning-fast spatial queries          |
| **🔍 Attribute Indexing** | Static B+Tree for instant attribute-based filtering       |
| **🌐 Multi-platform**     | Rust core plus a pure TypeScript reader for the browser and Node.js |

---

## 🚄 Performance & Benchmarks

FlatCityBuf delivers **10-20× faster** data retrieval compared to CityJSONTextSequence formats:

### Speed Comparison Results

| Dataset  | CityJSON | FlatCityBuf | **Speed Improvement** | Memory Reduction |
| -------- | -------- | ----------- | --------------------- | ---------------- |
| 3DBAG    | 56 ms    | 6 ms        | **8.6×**              | 4.7× less memory |
| 3DBV     | 3.8 s    | 122ms       | **32.6×**             | 4.5× less memory |
| Helsinki | 4.0 s    | 132ms       | **30.6×**             | 2.9× less memory |
| NYC      | 887 ms   | 43 ms       | **20.7×**             | 4.1× less memory |

> 📈 **Performance**: 8.6-256× faster queries with 2.1-6.4× less memory usage

---

## 🏗️ Project Structure

```
flatcitybuf/
├── 🦀 src/rust/         # Rust reader + writer (fcb_core, cli, fcb_api)
├── ⚙️ src/cpp/          # Native C++ reader + writer
├── 🐍 src/py/           # Pure-Python reader (no compiled dependency)
├── 🌐 src/ts/           # Pure TypeScript reader (browser + Node.js)
├── 📚 docs/             # Format specification and per-language guides
├── ✅ conformance/      # Shared oracle corpus every implementation validates against
└── 🧪 examples/         # Usage examples, tutorials and the web demo
```

### Technology Stack

- **Core**: Rust with zero-copy deserialization
- **Serialization**: FlatBuffers schema with custom optimizations
- **Spatial Index**: Packed R-tree for efficient range queries
- **Attribute Index**: Static B+Tree for attribute indexing
- **Web Support**: Pure TypeScript reader (`@cityjson/flatcitybuf`), no WebAssembly
- **CLI**: Comprehensive command-line tools

### Language Implementations

FlatCityBuf has four independent, from-scratch implementations of the same
format — no FFI between them; each parses (and, for Rust and C++, produces) the
bytes directly. Rust is the authoritative reference:

- **[Rust](docs/rust.md)** – Reader and writer, the reference implementation (`cargo install fcb_cli`)
- **[C++](docs/cpp.md)** – Native reader and writer, conformant; no CXX bridge or Rust dependency
- **[Python](docs/py.md)** – Pure-Python native reader, conformant, no compiled dependency (`pip install flatcitybuf`)
- **[TypeScript](docs/ts.md)** – Native reader for the browser or Node.js, conformant (`@cityjson/flatcitybuf`)

---

## 🚀 Getting Started

### Prerequisites

- **Rust toolchain** (recent stable)
- **Node.js** ≥ 22.12 (for the TypeScript reader in `src/ts`)

### 📦 Installation

#### Package Manager Installation (Recommended)

**Rust CLI**: Install from crates.io

```bash
cargo install fcb_cli --locked
```

This installs the `fcb` binary to your Cargo bin directory (usually `~/.cargo/bin/`).

**C++**: Install via vcpkg

The `flatcitybuf` port is served from a [custom vcpkg registry](https://github.com/HideBa/vcpkg): add the registry to your project's `vcpkg-configuration.json`, depend on `flatcitybuf` (feature `curl` for the HTTP reader), and link `flatcitybuf::flatcitybuf`. Registry configuration and current baselines: [`src/cpp/INSTALL.md`](src/cpp/INSTALL.md#install-via-vcpkg)

**Python**: Install from PyPI

```bash
pip install flatcitybuf
```

For more details, see [PyPI documentation](https://pypi.org/project/flatcitybuf/)

**JavaScript/TypeScript**: Install from npm

```bash
npm install @cityjson/flatcitybuf
```

For more details, see [npm documentation](https://www.npmjs.com/package/@cityjson/flatcitybuf)

#### Build from Source

```bash
# Clone the repository
git clone https://github.com/HideBa/flatcitybuf.git
cd flatcitybuf/src/rust

# Build the core library and CLI
cargo build --workspace --all-features --exclude fcb_py --release
```

The browser/Node.js reader is a separate pure TypeScript package in `src/ts`
(published as `@cityjson/flatcitybuf`); build it with `npm ci && npm run build`
from `src/ts`. See the [TypeScript guide](docs/ts.md).

### 🛠️ CLI Usage

#### Convert CityJSON/CityJSONSeq to FlatCityBuf

Replace `cargo run -p fcb_cli --` with `fcb` in the following commands if you want to use the installed binary directly.

Input and output are positional: the input comes first, the output second.

```bash
# Basic conversion from CityJSONSeq
fcb ser input.city.jsonl output.fcb

# Convert standard CityJSON file
fcb ser city.city.json output.fcb

# Multiple input files -- the last positional is the output
fcb ser file1.city.jsonl file2.city.jsonl merged.fcb

# Glob patterns to process all matching files
fcb ser 'data/*.city.jsonl' output.fcb
fcb ser 'cities/**/*.city.json' all_cities.fcb

# With spatial index and attribute index
fcb ser data.city.jsonl data.fcb --attr-index attribute_name,attribute_name2

# Back to CityJSONSeq
fcb deser data.fcb output.city.jsonl

# Show information about the file (static text report)
fcb inspect data.fcb --static

# Browse a dataset in an interactive terminal UI (local path or http(s):// URL,
# which reads only the header over range requests)
fcb inspect data.fcb
```

### 🧪 Run Benchmarks

```bash
# Core reading benchmarks
cargo bench -p fcb_core --bench read -- --release
```

---

## 📚 Documentation

| Document                                             | What it is for                                                                        |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------- |
| **[Format specification](docs/specification.md)**    | The binary format, from schema level down to byte offsets, constants and formulas      |
| **[Rust guide](docs/rust.md)**                       | Building, testing and using the Rust reader, writer and `fcb` CLI                      |
| **[C++ guide](docs/cpp.md)**                         | Building, testing and using the native C++ reader and writer                           |
| **[Python guide](docs/py.md)**                       | Installing and using the pure-Python reader                                            |
| **[TypeScript guide](docs/ts.md)**                   | Installing and using the TypeScript reader in the browser or Node.js                   |
| **[Datasets](docs/data.md)**                         | The public `.fcb` and CityJSONSeq files, what is hosted and where                       |
| **[Testing](docs/TESTING.md)**                       | The full manual verification procedure, local and remote                               |
| **[Upstream findings](docs/upstream-findings.md)**   | Permanent record of defects found across the implementations, each cited and reproduced |
| **[Contributing](CONTRIBUTING.md)**                  | How to report bugs, request features and submit pull requests                           |

- **[API reference, all languages](https://cityjson.github.io/flatcitybuf/)** — [Rust](https://cityjson.github.io/flatcitybuf/rust/), [C++](https://cityjson.github.io/flatcitybuf/cpp/), [Python](https://cityjson.github.io/flatcitybuf/python/) and [TypeScript](https://cityjson.github.io/flatcitybuf/typescript/), rebuilt on every push to `main`
- **[docs.rs/fcb_core](https://docs.rs/fcb_core)** - the Rust crate's reference on docs.rs
- **[MSc thesis at TU Delft](https://resolver.tudelft.nl/uuid:6727c979-5e46-4fe0-9349-a7803e825d02)** - FlatCityBuf was developed by @hideba for his MSc thesis in Geomatics, read all the details!

---

## 🤝 Contributing

We welcome contributions from the community! Please see our [Contributing Guidelines](CONTRIBUTING.md) for details on:

- 🐛 Reporting bugs
- 💡 Requesting features
- 🔧 Submitting pull requests
- 📝 Improving documentation

---

## 🙏 Acknowledgements & Special Thanks

### Core Contributors

This project builds upon the excellent work of the geospatial and 3D GIS community:

### Technical Foundations

- **[FlatGeobuf](https://github.com/flatgeobuf/flatgeobuf)** - FlatGeobuf team
  _Licensed under BSD 2-Clause License. Provided the foundational spatial indexing algorithms and FlatBuffers integration patterns._

- **[CityBuf](https://github.com/3DBAG/CityBuf)** - 3DBAG organisation
  _Original FlatBuffers schema for CityJSON features, authored by Ravi Peters (3DGI) and Balázs Dukai (3DGI)._

### Standards & Specifications

- **[CityJSON](https://www.cityjson.org/specs/2.0.1/)** - For the semantic foundation of 3D city models
- **[FlatBuffers](https://github.com/google/flatbuffers)** - Google's cross-platform serialization library
- **[OGC CityGML](https://www.ogc.org/standards/citygml)** - International standard for 3D city models

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 📖 Citation

The reserach paper has been published on 20th 3D GeoInfo conference in 2025. The paper is publicly availabe on [ISPRS achives](https://isprs-archives.copernicus.org/articles/XLVIII-4-W15-2025/17/2025/) and its DOI is `10.5194/isprs-archives-XLVIII-4-W15-2025-17-2025`

If you use FlatCityBuf in your research, please cite:

```bibtex
@inproceedings{25_3dgeoinfo_fcb,
 author = {Baba, Hidemichi and Ledoux, Hugo and Peters, Ravi},
 title = {{FlatCityBuf}: {A} new cloud-optimised {CityJSON} format},
 booktitle = {Proceedings 20th 3D GeoInfo Conference},
 year = {2025},
 volume = {XLVIII-4/W15-2025},
 pages = {17--24},
 address = {Tokyo, Japan},
 publisher = {ISPRS},
 doi = {10.5194/isprs-archives-XLVIII-4-W15-2025-17-2025}
}
```

---

<div align="center">

**[⭐ Star us on GitHub](https://github.com/HideBa/flatcitybuf)** • **[🐛 Report Issues](https://github.com/HideBa/flatcitybuf/issues)** • **[💬 Discussions](https://github.com/HideBa/flatcitybuf/discussions)**

</div>
