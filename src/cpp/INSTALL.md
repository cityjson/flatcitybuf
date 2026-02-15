# Installing Pre-built C++ Bindings

Pre-built binaries for FlatCityBuf C++ bindings are available on [GitHub Releases](https://github.com/cityjson/flatcitybuf/releases).

## Available Platforms

| Platform         | Asset                         | Archive   |
| ---------------- | ----------------------------- | --------- |
| Linux (x86_64)   | `fcb_cpp-linux-x86_64.tar.gz` | `.tar.gz` |
| macOS (x86_64)   | `fcb_cpp-macos-x86_64.tar.gz` | `.tar.gz` |
| Windows (x86_64) | `fcb_cpp-windows-x86_64.zip`  | `.zip`    |

## Package Contents

Each release package contains:

```text
├── libfcb_cpp.a      # Static library (Rust-compiled core)
├── lib.rs.h          # CXX bridge generated header (type definitions)
├── lib.rs.cc         # CXX bridge generated source (must be compiled with your code)
└── fcb.h             # High-level API header with Doxygen documentation
```

## Installation Steps

### Linux / macOS

```bash
# Download the appropriate archive for your platform
curl -LO https://github.com/cityjson/flatcitybuf/releases/latest/download/fcb_cpp-linux-x86_64.tar.gz

# Create install directory and extract
mkdir -p fcb_cpp
tar -xzf fcb_cpp-linux-x86_64.tar.gz -C fcb_cpp

# Option A: Copy to your project's third-party directory
cp -r fcb_cpp /path/to/your/project/third_party/

# Option B: Install system-wide (requires sudo)
sudo mkdir -p /usr/local/include/fcb
sudo cp fcb_cpp/lib.rs.h fcb_cpp/fcb.h /usr/local/include/fcb/
sudo cp fcb_cpp/libfcb_cpp.a /usr/local/lib/
```

### Windows

```powershell
# Download and extract
Invoke-WebRequest -Uri "https://github.com/cityjson/flatcitybuf/releases/latest/download/fcb_cpp-windows-x86_64.zip" -OutFile "fcb_cpp-windows-x86_64.zip"
Expand-Archive -Path fcb_cpp-windows-x86_64.zip -DestinationPath fcb_cpp
```

## Using in Your Project

### CMake Integration

```cmake
cmake_minimum_required(VERSION 3.16)
project(my_app LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)

# Path to extracted FlatCityBuf bindings
set(FCB_DIR ${CMAKE_SOURCE_DIR}/fcb_cpp)

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
    target_link_libraries(my_app pthread dl m)
endif()
```

### Makefile Integration

```makefile
CXX       = g++
CXXFLAGS  = -std=c++17 -I./fcb_cpp
LDFLAGS   = ./fcb_cpp/libfcb_cpp.a -lpthread -ldl -lm

# macOS: append frameworks
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    LDFLAGS += -framework Security -framework CoreFoundation -framework SystemConfiguration
endif

my_app: main.cpp ./fcb_cpp/lib.rs.cc
    $(CXX) $(CXXFLAGS) -o $@ $^ $(LDFLAGS)
```

## Version Compatibility

Pre-built binaries are built with:

- **Rust**: stable toolchain (1.70+)
- **C++**: C++17 standard
- **CMake**: 3.16+

The pre-built libraries use `--no-default-features` to avoid OpenSSL dependency on Linux. If you need HTTP support, you'll need to build from source.

## Building from Source

If pre-built binaries don't meet your needs, see [README.md](README.md) for instructions on building from source.
