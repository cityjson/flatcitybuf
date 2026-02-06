# FlatCityBuf C++ Bindings

C++ bindings for the FlatCityBuf core library, enabling reading and writing of FCB files from C++ applications.

## Building

### Prerequisites

- Rust toolchain (1.70+)
- CMake 3.16+
- C++17 compiler

### Build Steps

```bash
# Build Rust static library
cd src/rust
cargo build --release -p fcb_cpp

# Build C++ integration
cd ../cpp
mkdir -p build && cd build
cmake ..
make
```

## API Reference

### Types

```cpp
namespace fcb {
    // File metadata
    struct FcbMetadata {
        uint8_t version;           // FCB format version
        uint64_t features_count;   // Total number of features
        bool has_spatial_index;    // Whether R-tree index exists
        bool has_attribute_index;  // Whether attribute index exists
    };

    // Bounding box for spatial queries
    struct BoundingBox {
        double min_x, min_y;
        double max_x, max_y;
    };

    // Feature data returned from iteration
    struct CityFeatureData {
        rust::String id;   // CityObject ID
        rust::String json; // Full CityJSON feature as JSON string
    };

    // Opaque types (use via rust::Box)
    struct FcbFileReader;
    struct FcbFileReaderIterator;
    struct FcbFileWriter;
}
```

### Reader Functions

```cpp
// Open an FCB file for reading
rust::Box<FcbFileReader> fcb_reader_open(rust::Str path);

// Get file metadata
FcbMetadata fcb_reader_metadata(const FcbFileReader& reader);

// Select all features for iteration (consumes reader)
rust::Box<FcbFileReaderIterator> fcb_reader_select_all(
    rust::Box<FcbFileReader> reader
);

// Select features within bounding box (consumes reader)
rust::Box<FcbFileReaderIterator> fcb_reader_select_bbox(
    rust::Box<FcbFileReader> reader,
    BoundingBox bbox
);
```

### Iterator Functions

```cpp
// Advance to next feature, returns false when done
bool fcb_iterator_next(FcbFileReaderIterator& iter);

// Get current feature data (call after next() returns true)
CityFeatureData fcb_iterator_current(const FcbFileReaderIterator& iter);

// Get total feature count
uint64_t fcb_iterator_features_count(const FcbFileReaderIterator& iter);
```

### Writer Functions

```cpp
// Create new writer with CityJSON metadata
rust::Box<FcbFileWriter> fcb_writer_new(rust::Str metadata_json);

// Add a feature (CityJSONFeature as JSON string)
void fcb_writer_add_feature(FcbFileWriter& writer, rust::Str feature_json);

// Write to file (consumes writer)
void fcb_writer_write(rust::Box<FcbFileWriter> writer, rust::Str path);
```

## Usage Examples

### Reading FCB Files

```cpp
#include "lib.rs.h"
#include <iostream>

int main() {
    try {
        // Open file
        auto reader = fcb::fcb_reader_open("buildings.fcb");

        // Check metadata
        auto meta = fcb::fcb_reader_metadata(*reader);
        std::cout << "Features: " << meta.features_count << std::endl;

        // Iterate all features
        auto iter = fcb::fcb_reader_select_all(std::move(reader));
        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);
            std::cout << "ID: " << std::string(feature.id) << std::endl;
            // Parse feature.json as needed
        }
    } catch (const rust::Error& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }
    return 0;
}
```

### Spatial Query

```cpp
// Query features within bounding box
fcb::BoundingBox bbox{4.35, 52.0, 4.40, 52.1};
auto iter = fcb::fcb_reader_select_bbox(std::move(reader), bbox);

while (fcb::fcb_iterator_next(*iter)) {
    auto feature = fcb::fcb_iterator_current(*iter);
    // Process features within bbox
}
```

### Writing FCB Files

```cpp
#include "lib.rs.h"

int main() {
    // CityJSON metadata as JSON string
    std::string metadata = R"({
        "type": "CityJSON",
        "version": "2.0",
        "transform": {"scale": [0.001, 0.001, 0.001], "translate": [0, 0, 0]},
        "metadata": {}
    })";

    auto writer = fcb::fcb_writer_new(metadata);

    // Add features (CityJSONFeature format)
    std::string feature = R"({
        "type": "CityJSONFeature",
        "id": "building_1",
        "CityObjects": {...},
        "vertices": [...]
    })";
    fcb::fcb_writer_add_feature(*writer, feature);

    // Write to file
    fcb::fcb_writer_write(std::move(writer), "output.fcb");

    return 0;
}
```

## Error Handling

All functions that can fail throw `std::exception`. Catch this to handle errors:

```cpp
try {
    auto reader = fcb::fcb_reader_open("nonexistent.fcb");
} catch (const std::exception& e) {
    std::cerr << "Failed: " << e.what() << std::endl;
}
```

## Linking

Link against `libfcb_cpp.a` and the generated CXX bridge code:

```cmake
target_link_libraries(myapp
    ${CMAKE_SOURCE_DIR}/../rust/target/release/libfcb_cpp.a
)

# On macOS, also link system frameworks
if(APPLE)
    target_link_libraries(myapp
        "-framework Security"
        "-framework CoreFoundation"
    )
endif()
```

## Limitations

- **HTTP Reader**: Not yet exposed (requires async support)
- **Thread Safety**: Single-threaded usage only
- **Memory**: Features are returned as JSON strings; parse as needed
