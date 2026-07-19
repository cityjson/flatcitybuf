# Installing Pre-built C++ Bindings

Pre-built binaries for FlatCityBuf C++ bindings are available on [GitHub Releases](https://github.com/cityjson/flatcitybuf/releases).

## Install via vcpkg (Recommended)

The easiest way to use FlatCityBuf in your C++ project is through [vcpkg](https://vcpkg.io/).

### 1. Install flatcitybuf

```bash
vcpkg install flatcitybuf
```

### 2. Configure your CMake project

```cmake
cmake_minimum_required(VERSION 3.16)
project(my_city_app LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(flatcitybuf CONFIG REQUIRED)

# FLATCITYBUF_CXX_BRIDGE_SOURCE is set by find_package — it points to the
# CXX bridge source (lib.rs.cc) that must be compiled alongside your code.
add_executable(my_app main.cpp ${FLATCITYBUF_CXX_BRIDGE_SOURCE})
target_link_libraries(my_app PRIVATE flatcitybuf::flatcitybuf)
```

### 3. Build

```bash
cmake -B build -S . -DCMAKE_TOOLCHAIN_FILE=$VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake
cmake --build build
```

That's it — vcpkg handles downloading the correct platform binary, installing headers and the static library, and the CMake config automatically links all platform-specific dependencies (macOS frameworks, Linux OpenSSL, Windows system libs).

### Supported platforms

| Platform         | Architecture    |
| ---------------- | --------------- |
| Linux            | x86_64, aarch64 |
| macOS            | x86_64, aarch64 |
| Windows          | x86_64          |

> **Linux note:** OpenSSL is installed as a vcpkg dependency automatically.

---

## Manual Installation (without vcpkg)

If you prefer not to use vcpkg, you can download pre-built binaries directly from GitHub Releases.

## Available Platforms

| Platform          | Asset                          | Archive   |
| ----------------- | ------------------------------ | --------- |
| Linux (x86_64)    | `fcb_cpp-linux-x86_64.tar.gz`  | `.tar.gz` |
| Linux (aarch64)   | `fcb_cpp-linux-aarch64.tar.gz` | `.tar.gz` |
| macOS (x86_64)    | `fcb_cpp-macos-x86_64.tar.gz`  | `.tar.gz` |
| macOS (aarch64)   | `fcb_cpp-macos-aarch64.tar.gz` | `.tar.gz` |
| Windows (x86_64)  | `fcb_cpp-windows-x86_64.zip`   | `.zip`    |

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
# Detect your platform and download the matching archive
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

if [[ "$OS" == "darwin" ]]; then
  ASSET="fcb_cpp-macos-${ARCH}.tar.gz"
else
  # Map x86_64 / aarch64 directly to asset names
  ASSET="fcb_cpp-linux-${ARCH}.tar.gz"
fi

curl -LO "https://github.com/cityjson/flatcitybuf/releases/latest/download/${ASSET}"

# Create install directory and extract
mkdir -p fcb_cpp
tar -xzf "${ASSET}" -C fcb_cpp

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
    find_package(OpenSSL REQUIRED)
    target_link_libraries(my_app
        OpenSSL::SSL
        OpenSSL::Crypto
        pthread dl m
    )
endif()
```

### Makefile Integration

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

## Version Compatibility

Pre-built binaries are built with:

- **Rust**: stable toolchain (1.70+)
- **C++**: C++17 standard
- **CMake**: 3.16+

> **Linux prerequisite:** OpenSSL development libraries are required at link time. Install with:
> ```bash
> sudo apt-get install libssl-dev   # Debian/Ubuntu
> sudo dnf install openssl-devel    # Fedora/RHEL
> ```

## Building from Source

If pre-built binaries don't meet your needs, see [README.md](README.md) for instructions on building from source.
