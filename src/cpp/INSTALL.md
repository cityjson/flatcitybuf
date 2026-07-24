# Installing the FlatCityBuf C++ library

A native C++17 reader. **No Rust toolchain, no CXX bridge, no TLS dependency.**

## Dependencies

| Dependency | Required? | Why |
|---|---|---|
| `flatbuffers` | yes | the on-disk format |
| `nlohmann-json` | `FCB_WITH_JSON=ON` (default) | CityJSON emission |
| `libcurl` | `FCB_WITH_CURL=ON` (default **OFF**) | HTTP range requests |
| `doctest` | `FCB_BUILD_TESTS=ON` (default) | tests only, never installed |

The default build links **neither curl nor any TLS library** — CI asserts
this. When you do enable the HTTP adapter, libcurl brings its own platform
TLS (Schannel / SecureTransport / system OpenSSL), so this library still never
link-depends on a TLS stack.

```bash
# macOS
brew install flatbuffers nlohmann-json doctest
# Debian / Ubuntu
sudo apt-get install libflatbuffers-dev nlohmann-json3-dev doctest-dev
```

## Build and install

```bash
cd src/cpp
cmake -B build -S .
cmake --build build
cmake --install build --prefix /your/prefix
```

Useful options: `-DFCB_WITH_CURL=ON` (HTTP), `-DFCB_WITH_JSON=OFF` (drop
CityJSON emission and the nlohmann dependency), `-DFCB_BUILD_TESTS=OFF`,
`-DFCB_BUILD_EXAMPLES=OFF`.

## Use from CMake

```cmake
find_package(flatcitybuf CONFIG REQUIRED)
target_link_libraries(my_app PRIVATE flatcitybuf::flatcitybuf)
```

That is the whole integration. There is no generated bridge source to compile
alongside your code, unlike the retired FFI bindings.

## Reading a file

```cpp
#include <fcb/reader.hpp>
#include <fcb/cityjson.hpp>

fcb::FcbReader r = fcb::FcbReader::open_file("city.fcb");
auto it = r.select_bbox({84500, 445800, 85000, 446500});
while (it.next()) {
    std::cout << fcb::to_cityjson_feature(it.current(), r.header()).dump() << "\n";
}
```

## Reading over HTTP

Build with `-DFCB_WITH_CURL=ON`:

```cpp
#include <fcb/http/curl_range_reader.hpp>

auto transport = std::make_shared<fcb::CurlRangeReader>("https://example.org/city.fcb");
fcb::FcbReader r = fcb::FcbReader::open(transport);
auto it = r.select_bbox({84500, 445800, 85000, 446500});
```

Only the intersecting features are fetched, not the whole file.

## Bringing your own transport

`fcb::RangeReader` is the extension point — implement it to read from an
engine VFS, an object store, memory, or anything else:

```cpp
class MyReader : public fcb::RangeReader {
    std::uint64_t total_size() override { ... }
    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override { ... }
    // Optionally override read_batch() to pipeline or multiplex.
};
```

The interface is deliberately **synchronous**: batching, not asynchrony, is the
concurrency primitive. A blocking interface is trivially wrapped by whatever
threading model your application already has, whereas an imposed async runtime
is not. Read the contract comment in `include/fcb/range_reader.hpp` before
implementing — short reads, error reporting, ordering and representation
stability are all specified there.

## Not implemented

- **Writing.** Producing `.fcb` files still requires the Rust CLI.
- **Appearance** (texture and material mappings) is not decoded. Everything
  else — attributes, geometry, semantics, geometry templates, extents,
  relationships — is.
