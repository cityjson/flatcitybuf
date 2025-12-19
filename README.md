# FlatCityBuf 🏙️

<div align="center">

![FlatCityBuf Logo](./docs/logo.png)

**A cloud-optimized binary format for storing and retrieving 3D city models**

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/HideBa/flatcitybuf)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![WebAssembly](https://img.shields.io/badge/WebAssembly-654FF0?style=flat&logo=webassembly&logoColor=white)](https://webassembly.org/)

_Bringing the semantic richness of CityJSON with the performance of FlatBuffers_

[🚀 Getting Started](#-getting-started) • [📊 Benchmarks](#-performance--benchmarks) • [📖 Documentation](#-documentation) • [🤝 Contributing](#-contributing)

</div>

---

## ✨ Overview

FlatCityBuf revolutionizes 3D city model storage and retrieval by combining the semantic richness of [CityJSON](https://github.com/cityjson/cityjson-spec) with the performance benefits of [FlatBuffers](https://github.com/google/flatbuffers) binary serialization and advanced spatial indexing techniques.

## Demo

Web prototype can be available from **[here!](https://fcb-web-prototype.netlify.app/)**

<https://github.com/user-attachments/assets/ab49f026-1907-4a25-a5fb-8bc69e9a102b>

## Example FlatCityBuf File

- [3DBAG all (70GB)](https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb): serialised whole 3DBAG dataset with spatial and attribute indexing
- [3DBAG small (3.4GB)](https://storage.googleapis.com/flatcitybuf/3dbag_subset_all_index.fcb)
- [Delft (6MB)](examples/data/delft.fcb)

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
| **🌐 Multi-platform**     | Rust core with WASM bindings for web applications         |

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
├── 📦 fcb_core/          # Core library for reading/writing FlatCityBuf
├── 🛠️ fcb_cli/           # Command-line interface and tools
├── 🌐 fcb_wasm/         # WebAssembly bindings for browsers
├── 📚 docs/             # Documentation and examples
└── 🧪 examples/         # Usage examples and tutorials
```

### Technology Stack

- **Core**: Rust with zero-copy deserialization
- **Serialization**: FlatBuffers schema with custom optimizations
- **Spatial Index**: Packed R-tree for efficient range queries
- **Attribute Index**: Static B+Tree for attribute indexing
- **Web Support**: WebAssembly bindings via wasm-pack
- **CLI**: Comprehensive command-line tools

---

## 🚀 Getting Started

### Prerequisites

- **Rust toolchain** (1.83.0 or later)
- **wasm-pack** (for WebAssembly builds)

### 📦 Installation

#### Package Manager Installation (Recommended)

**Rust CLI**: Install from crates.io

```bash
cargo install fcb_cli --locked
```

This installs the `fcb` binary to your Cargo bin directory (usually `~/.cargo/bin/`).

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
cargo build --workspace --all-features --exclude fcb_wasm --release

# Build WebAssembly module (optional)
cd wasm && wasm-pack build --target web --release --out-dir ../../ts
```

### 🛠️ CLI Usage

#### Convert CityJSON/CityJSONSeq to FlatCityBuf

Replace `cargo run -p fcb_cli --` with `fcb` in the following commands if you want to use the installed binary directly.

```bash
# Basic conversion from CityJSONSeq
fcb ser -i input.city.jsonl -o output.fcb

# Convert standard CityJSON file
fcb ser -i city.city.json -o output.fcb

# Multiple input files
fcb ser -i file1.city.jsonl file2.city.jsonl -o merged.fcb

# Glob patterns to process all matching files
fcb ser -i 'data/*.city.jsonl' -o output.fcb
fcb ser -i 'cities/**/*.city.json' -o all_cities.fcb

# With spatial index and attribute index
fcb ser -i data.city.jsonl -o data.fcb --attr-index attribute_name,attribute_name2 --attr-branching-factor 256

# Show information about the file
fcb info -i data.fcb
```

### 🧪 Run Benchmarks

```bash
# Core reading benchmarks
cargo bench -p fcb_core --bench read -- --release
```

---

## 📚 Documentation

- **[API Documentation](https://docs.rs/fcb_core)** - Comprehensive API reference
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
