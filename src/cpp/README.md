# FlatCityBuf C++ Bindings

C++ bindings for the [FlatCityBuf](https://github.com/cityjson/flatcitybuf) core library, enabling reading and writing of FCB files from C++ applications. FCB is a cloud-optimized binary format for 3D city models based on [CityJSON](https://www.cityjson.org/).

## Quick Start with Pre-built Binaries

Pre-built binaries are available on [GitHub Releases](https://github.com/cityjson/flatcitybuf/releases) for:

| Platform          | Asset                          | Archive   |
| ----------------- | ------------------------------ | --------- |
| Linux (x86_64)    | `fcb_cpp-linux-x86_64.tar.gz`  | `.tar.gz` |
| Linux (aarch64)   | `fcb_cpp-linux-aarch64.tar.gz` | `.tar.gz` |
| macOS (x86_64)    | `fcb_cpp-macos-x86_64.tar.gz`  | `.tar.gz` |
| macOS (aarch64)   | `fcb_cpp-macos-aarch64.tar.gz` | `.tar.gz` |
| Windows (x86_64)  | `fcb_cpp-windows-x86_64.zip`   | `.zip`    |

Each release package contains:

```text
├── libfcb_cpp.a      # Static library (Rust-compiled core)
├── lib.rs.h          # CXX bridge generated header (type definitions)
├── lib.rs.cc         # CXX bridge generated source (must be compiled with your code)
└── fcb.h             # High-level API header with documentation
```

### 1. Download and Extract

**Linux / macOS:**

```bash
# Download the latest release (replace with your platform)
curl -LO https://github.com/cityjson/flatcitybuf/releases/latest/download/fcb_cpp-linux-x86_64.tar.gz

# Extract
mkdir -p fcb_cpp && tar -xzf fcb_cpp-linux-x86_64.tar.gz -C fcb_cpp
```

**Windows (PowerShell):**

```powershell
Invoke-WebRequest -Uri "https://github.com/cityjson/flatcitybuf/releases/latest/download/fcb_cpp-windows-x86_64.zip" -OutFile "fcb_cpp-windows-x86_64.zip"
Expand-Archive -Path fcb_cpp-windows-x86_64.zip -DestinationPath fcb_cpp
```

### 2. Integrate with Your Project

#### CMake (Recommended)

Add the following to your `CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.16)
project(my_city_app LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Path to extracted FlatCityBuf C++ bindings
set(FCB_DIR ${CMAKE_SOURCE_DIR}/fcb_cpp)

# Your application — must compile lib.rs.cc (CXX bridge source) alongside your code
add_executable(my_app main.cpp ${FCB_DIR}/lib.rs.cc)

target_include_directories(my_app PRIVATE ${FCB_DIR})

target_link_libraries(my_app ${FCB_DIR}/libfcb_cpp.a)

# Platform-specific dependencies
if(APPLE)
    target_link_libraries(my_app
        "-framework Security"
        "-framework CoreFoundation"
        "-framework SystemConfiguration"
    )
elseif(WIN32)
    target_link_libraries(my_app
        ws2_32
        userenv
        bcrypt
        ntdll
    )
elseif(UNIX)
    find_package(OpenSSL REQUIRED)
    target_link_libraries(my_app
        OpenSSL::SSL
        OpenSSL::Crypto
        pthread dl m
    )
endif()
```

Then build:

```bash
mkdir build && cd build
cmake ..
cmake --build .
```

#### Makefile

```makefile
CXX       = g++
CXXFLAGS  = -std=c++17 -I./fcb_cpp
LDFLAGS   = ./fcb_cpp/libfcb_cpp.a -lpthread -ldl -lm

# Platform-specific dependencies
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    LDFLAGS += -framework Security -framework CoreFoundation -framework SystemConfiguration
else ifeq ($(UNAME_S),Linux)
    LDFLAGS += -lssl -lcrypto
endif

my_app: main.cpp ./fcb_cpp/lib.rs.cc
    $(CXX) $(CXXFLAGS) -o $@ $^ $(LDFLAGS)
```

### 3. Write Your First Program

```cpp
#include "lib.rs.h"
#include <iostream>
#include <string>

int main() {
    try {
        // Open an FCB file
        auto reader = fcb::fcb_reader_open("buildings.fcb");

        // Read metadata
        auto meta = fcb::fcb_reader_metadata(*reader);
        std::cout << "Features: " << meta.features_count << std::endl;

        // Iterate over all features
        auto iter = fcb::fcb_reader_select_all(std::move(reader));
        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);
            std::cout << "ID: " << std::string(feature.id) << std::endl;
            // feature.json contains the full CityJSONFeature as a JSON string
        }
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }
    return 0;
}
```

> **Note:** Include `lib.rs.h` (the CXX-generated header) for type definitions. Optionally include `fcb.h` for Doxygen-documented declarations that re-export `lib.rs.h`.

---

## Building from Source

If pre-built binaries don't meet your needs (e.g., different architecture or custom features), you can build from source.

### Prerequisites

- Rust toolchain (stable, 1.70+)
- CMake 3.16+
- C++17 compatible compiler (GCC 7+, Clang 5+, MSVC 2017+)
- [cxxbridge CLI](https://cxx.rs/): `cargo install cxxbridge-cmd`

### Build Steps

```bash
# Clone the repository
git clone https://github.com/cityjson/flatcitybuf.git
cd flatcitybuf

# Build Rust static library
cd src/rust
cargo build --release -p fcb_cpp --no-default-features

# Build C++ integration
cd ../cpp
mkdir -p build && cd build
cmake ..
cmake --build .
```

The built artifacts will be at:

- Static library: `src/rust/target/release/libfcb_cpp.a` (Unix) or `src/rust/target/release/fcb_cpp.lib` (Windows)
- Generated header: `src/cpp/build/generated/lib.rs.h`

---

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
        double min_x, min_y;       // Southwest corner
        double max_x, max_y;       // Northeast corner
    };

    // Feature data returned from iteration
    struct CityFeatureData {
        rust::String id;   // CityObject ID (e.g., "NL.IMBAG.Pand.0503100000031902")
        rust::String json; // Full CityJSONFeature as JSON string
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

---

## Usage Examples

### Reading an FCB File

```cpp
#include "lib.rs.h"
#include <iostream>

int main(int argc, char* argv[]) {
    try {
        auto reader = fcb::fcb_reader_open(argv[1]);

        // Inspect metadata
        auto meta = fcb::fcb_reader_metadata(*reader);
        std::cout << "Version: " << static_cast<int>(meta.version) << std::endl;
        std::cout << "Features: " << meta.features_count << std::endl;
        std::cout << "Spatial index: " << (meta.has_spatial_index ? "yes" : "no") << std::endl;

        // Iterate all features
        auto iter = fcb::fcb_reader_select_all(std::move(reader));
        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);
            std::cout << "ID: " << std::string(feature.id) << std::endl;

            // feature.json is a CityJSONFeature JSON string
            // Parse with your preferred JSON library (e.g., nlohmann/json)
        }
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }
    return 0;
}
```

### Spatial Query (Bounding Box)

```cpp
auto reader = fcb::fcb_reader_open("buildings.fcb");

// Query features within a bounding box (coordinates in file's CRS)
fcb::BoundingBox bbox{4.35, 52.0, 4.40, 52.1};
auto iter = fcb::fcb_reader_select_bbox(std::move(reader), bbox);

while (fcb::fcb_iterator_next(*iter)) {
    auto feature = fcb::fcb_iterator_current(*iter);
    // Process features within the bounding box
}
```

### Writing an FCB File

```cpp
#include "lib.rs.h"
#include <fstream>
#include <sstream>

int main() {
    // 1. Prepare CityJSON metadata
    std::string metadata = R"({
        "type": "CityJSON",
        "version": "2.0",
        "transform": {
            "scale": [0.001, 0.001, 0.001],
            "translate": [0.0, 0.0, 0.0]
        },
        "metadata": {
            "referenceSystem": "https://www.opengis.net/def/crs/EPSG/0/7415"
        }
    })";

    auto writer = fcb::fcb_writer_new(metadata);

    // 2. Add CityJSONFeature objects
    std::string feature = R"({
        "type": "CityJSONFeature",
        "id": "building_1",
        "CityObjects": {
            "building_1": {
                "type": "Building",
                "attributes": {"yearOfConstruction": 2005},
                "geometry": []
            }
        },
        "vertices": []
    })";
    fcb::fcb_writer_add_feature(*writer, feature);

    // 3. Write to disk (consumes the writer)
    fcb::fcb_writer_write(std::move(writer), "output.fcb");

    return 0;
}
```

### Roundtrip: CityJSONSeq → FCB → Read Back

```cpp
#include "lib.rs.h"
#include <fstream>
#include <iostream>

int main() {
    // Read a CityJSONSeq file (.city.jsonl)
    std::ifstream infile("input.city.jsonl");
    std::string header_line;
    std::getline(infile, header_line);

    // First line is the CityJSON metadata
    auto writer = fcb::fcb_writer_new(header_line);

    // Remaining lines are CityJSONFeature objects
    std::string line;
    while (std::getline(infile, line)) {
        if (!line.empty()) {
            fcb::fcb_writer_add_feature(*writer, line);
        }
    }

    fcb::fcb_writer_write(std::move(writer), "output.fcb");

    // Read it back
    auto reader = fcb::fcb_reader_open("output.fcb");
    auto meta = fcb::fcb_reader_metadata(*reader);
    std::cout << "Written " << meta.features_count << " features" << std::endl;

    return 0;
}
```

---

## Included Examples

Pre-built examples are included in this directory:

### Local FCB File Reader

See `examples/local_fcb_example.cpp` — demonstrates opening local FCB files, reading metadata, iterating features, and spatial filtering.

```bash
cd build
./local_fcb_example /path/to/buildings.fcb
```

### Comprehensive Operations

See `examples/comprehensive_example.cpp` — demonstrates reading, writing, accessing attributes and geometry types.

> **Requires** [nlohmann/json](https://github.com/nlohmann/json): `brew install nlohmann-json` (macOS) or `apt install nlohmann-json3-dev` (Ubuntu)

### Roundtrip Test

See `tests/roundtrip_test.cpp` — end-to-end tests verifying data integrity through CityJSON → FCB → CityJSON serialization cycles.

---

## Error Handling

All functions that can fail throw `std::exception` (specifically `rust::Error` from the CXX bridge). Wrap calls in try-catch blocks:

```cpp
try {
    auto reader = fcb::fcb_reader_open("nonexistent.fcb");
} catch (const std::exception& e) {
    std::cerr << "Failed: " << e.what() << std::endl;
}
```

---

## Platform-Specific Linking

The pre-built static library (`libfcb_cpp.a`) is a Rust-compiled library and requires platform-specific system libraries at link time:

| Platform    | Required Libraries                                                                |
| ----------- | --------------------------------------------------------------------------------- |
| **Linux**   | `ssl`, `crypto` (OpenSSL), `pthread`, `dl`, `m`                                  |
| **macOS**   | `Security.framework`, `CoreFoundation.framework`, `SystemConfiguration.framework` |
| **Windows** | `ws2_32`, `userenv`, `bcrypt`, `ntdll`                                            |

These are automatically handled if you follow the CMake or Makefile examples above.

> **Linux note:** OpenSSL (`libssl-dev`) must be installed on your system. Install with:
> ```bash
> sudo apt-get install libssl-dev   # Debian/Ubuntu
> sudo dnf install openssl-devel    # Fedora/RHEL
> ```

## HTTP / Remote File Access

HTTP/remote file reading is currently **not supported** through the C++ API.

For remote access, use the CLI tool:

```bash
# Get metadata from remote FCB file
fcb info -i https://example.com/data.fcb

# Spatial query on remote file
fcb info -i https://example.com/data.fcb --bbox 85000 446000 85100 446100
```

The CXX bridge has limitations that make it difficult to expose async HTTP operations (no async/await support, complex runtime bridging for tokio). For remote files, download first and use the C++ bindings on the local copy.

## Documentation

API documentation can be generated via Doxygen:

```bash
cd build
make docs
```

Generated docs will be in `docs/html/`.

## Development

### C++ Bridge Code

The Rust bridge definitions are in `src/rust/fcb_cpp/`:

- `lib.rs` — Main module with CXX bridge definitions
- `reader.rs` — Local file reader implementation
- `writer.rs` — FCB file writer implementation

## Limitations

- **HTTP Reader**: Not yet exposed through C++ bindings (requires async runtime)
- **Thread Safety**: Single-threaded usage only; do not share reader/writer across threads
- **Memory**: Features are returned as JSON strings; parse with your preferred C++ JSON library
- **C++ Standard**: Requires C++17 or later

## License

MIT License — see [LICENSE](../../LICENSE) for details.
