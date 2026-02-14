# Installing Pre-built C++ Bindings

Pre-built binaries for FlatCityBuf C++ bindings are available on [GitHub Releases](https://github.com/cityjson/flatcitybuf/releases).

## Available Platforms

- **Linux** (x86_64): `fcb_cpp-linux-x86_64.tar.gz`
- **macOS** (x86_64): `fcb_cpp-macos-x86_64.tar.gz`
- **macOS** (ARM64/Apple Silicon): `fcb_cpp-macos-arm64.tar.gz`
- **Windows** (x86_64): `fcb_cpp-windows-x86_64.zip`

## Installation Steps

### Linux/macOS

```bash
# Download the appropriate archive for your platform
wget https://github.com/cityjson/flatcitybuf/releases/latest/download/fcb_cpp-linux-x86_64.tar.gz

# Extract the archive
tar -xzf fcb_cpp-linux-x86_64.tar.gz

# Copy to your project directory
cp lib.rs.h /usr/local/include/fcb/
cp libfcb_cpp.a /usr/local/lib/
cp include/* /usr/local/include/fcb/
```

### Windows

```powershell
# Download and extract using PowerShell
Invoke-WebRequest -Uri "https://github.com/cityjson/flatcitybuf/releases/latest/download/fcb_cpp-windows-x86_64.zip" -OutFile "fcb_cpp-windows-x86_64.zip"
Expand-Archive -Path fcb_cpp-windows-x86_64.zip -DestinationPath .

# Copy to your project directory (adjust paths as needed)
Copy-Item lib.rs.h C:\libs\flatcitybuf\include\
Copy-Item fcb_cpp.lib C:\libs\flatcitybuf\lib\
Copy-Item include\* C:\libs\flatcitybuf\include\
```

## Using in Your Project

### CMake Integration

```cmake
# Set up include paths
include_directories(
    ${CMAKE_SOURCE_DIR}/path/to/fcb_cpp/include
    ${CMAKE_SOURCE_DIR}/path/to/fcb_cpp
)

# Link against the static library
target_link_libraries(your_app
    ${CMAKE_SOURCE_DIR}/path/to/fcb_cpp/libfcb_cpp.a
)

# macOS-specific frameworks
if(APPLE)
    target_link_libraries(your_app
        "-framework Security"
        "-framework CoreFoundation"
        "-framework SystemConfiguration"
    )
endif()
```

### Makefile Integration

```makefile
# Include paths
CXXFLAGS += -I/path/to/fcb_cpp/include -I/path/to/fcb_cpp

# Linker flags
LDFLAGS += /path/to/fcb_cpp/libfcb_cpp.a

# macOS frameworks
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    LDFLAGS += -framework Security -framework CoreFoundation -framework SystemConfiguration
endif()
```

## Version Compatibility

Pre-built binaries are built with:
- **Rust**: 1.70+ toolchain
- **C++**: C++17 standard
- **CMake**: 3.16+

The pre-built libraries use `--no-default-features` to avoid OpenSSL dependency on Linux. If you need HTTP support, you'll need to build from source.

## Building from Source

If pre-built binaries don't meet your needs, see [README.md](README.md) for building from source.
