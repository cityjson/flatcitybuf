# FlatCityBuf C++ Bindings

High-performance C++ bindings for FlatCityBuf (FCB) binary format.

## Building

```bash
mkdir -p build && cd build
cmake ..
make
```

## Requirements

- C++17 compiler
- CMake 3.16+
- cxxbridge CLI tool (auto-installed via CMake)
- Rust toolchain (for building `libfcb_cpp.a`)

## Library API

The C++ bindings provide the following functions for reading FCB files:

### Opening Files
```cpp
#include "fcb.h"

// Open local FCB file
auto reader = fcb::fcb_reader_open("buildings.fcb");
```

### Getting Metadata
```cpp
// Get metadata from reader
auto meta = fcb::fcb_reader_metadata(*reader);

std::cout << "Features: " << meta.features_count << std::endl;
std::cout << "Has spatial index: " << (meta.has_spatial_index ? "yes" : "no") << std::endl;
std::cout << "Has attribute index: " << (meta.has_attribute_index ? "yes" : "no") << std::endl;
```

### Iterating Features

```cpp
// Select all features
auto iter = fcb::fcb_reader_select_all(std::move(reader));

// Iterate through features
while (fcb::fcb_iterator_next(*iter)) {
    auto feature = fcb::fcb_iterator_current(*iter);

    std::string id = feature.id;       // Feature ID
    std::string json = feature.json;  // Feature data as JSON string

    // Access specific fields from JSON...
}
```

### Spatial Queries

```cpp
// Define bounding box
fcb::BoundingBox bbox;
bbox.min_x = 85000.0;
bbox.min_y = 446000.0;
bbox.max_x = 85100.0;
bbox.max_y = = 446100.0;

// Select features within bbox
auto iter = fcb::fcb_reader_select_bbox(std::move(reader), bbox);
```

## Examples

### Example 1: Local FCB File Reader

See `examples/local_fcb_example.cpp` for a complete example demonstrating:
- Opening local FCB files
- Reading file metadata
- Iterating through all features
- Spatial filtering with bounding boxes

**Usage:**
```bash
cd build
./local_fcb_example /path/to/buildings.fcb
```

### Example 2: Comprehensive Operations

See `examples/comprehensive_example.cpp` for advanced features including:
- Reading FCB files
- Accessing feature attributes (height, year, etc.)
- Accessing geometry types (Solid, MultiSurface, etc.)
- Writing FCB files from CityJSON data

**Note**: This example requires `nlohmann/json` for JSON parsing:
```bash
brew install nlohmann-json
```

## HTTP/Remote File Access

**Note**: HTTP/remote file reading is currently **not supported** through C++ API.

### For Remote Access, Use CLI Tool:

```bash
# Get metadata from remote FCB file
fcb info -i https://example.com/data.fcb

# Spatial query on remote file
fcb info -i https://example.com/data.fcb --bbox 85000 446000 85100 446100
```

### Why No HTTP in C++ Bindings?

The CXX bridge has limitations that make it difficult to expose async HTTP operations:
- No async/await support in CXX bridge
- Complex runtime bridging required for tokio
- Type system conflicts with generic HTTP reader from fcb_core

### Recommended Workflows

1. **For Remote Files**: Use CLI tool (`fcb`) or download first
2. **For Local Files**: Use C++ bindings (fully functional)
3. **For Writing**: Use C++ `fcb_writer_new()` API

## Documentation

API documentation is available via Doxygen:

```bash
cd build
make docs
```

Generated docs will be in `build/docs/`.

## Development

### Building FCB Library

The Rust static library is built automatically by CMake:

```bash
cargo build --release -p fcb_cpp
```

### C++ Bridge Code

C++ bindings are in `src/rust/fcb_cpp/`:
- `lib.rs` - Main module with CXX bridge definitions
- `reader.rs` - Local file reader implementation
- `writer.rs` - FCB file writer implementation

### Testing

```bash
# Build examples
cd src/cpp/build
make local_fcb_example

# Run example
./local_fcb_example ../../examples/data/delft.fcb
```

## License

MIT License - see LICENSE file for details.
