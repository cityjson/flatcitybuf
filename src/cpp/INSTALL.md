# Installing the FlatCityBuf C++ library

A native C++17 reader and writer. **No Rust toolchain, no CXX bridge, no TLS
dependency.**

## Dependencies

| Dependency | Required? | Why |
|---|---|---|
| `flatbuffers` | yes | the on-disk format |
| `nlohmann-json` | `FCB_WITH_JSON=ON` (default) | CityJSON emission, and the writer |
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

## Install via vcpkg

The port lives in a [custom vcpkg registry](https://github.com/HideBa/vcpkg)
(not the built-in microsoft/vcpkg one). Next to your project's `vcpkg.json`,
add a `vcpkg-configuration.json`:

```json
{
  "default-registry": {
    "kind": "git",
    "repository": "https://github.com/microsoft/vcpkg",
    "baseline": "2f1d605400c8727cc00c15797aba796c88ccd523"
  },
  "registries": [
    {
      "kind": "git",
      "repository": "https://github.com/HideBa/vcpkg",
      "baseline": "9171cc35ccca68dc481c7a3718d7785a2fb5c20e",
      "packages": ["flatcitybuf"]
    }
  ]
}
```

and declare the dependency in `vcpkg.json`:

```json
{
  "name": "my-app",
  "version": "0.1.0",
  "dependencies": ["flatcitybuf"]
}
```

Use `{ "name": "flatcitybuf", "features": ["curl"] }` instead to get the
HTTP range-request reader. Configure with vcpkg's toolchain file
(`-DCMAKE_TOOLCHAIN_FILE=<vcpkg>/scripts/buildsystems/vcpkg.cmake`) and
integrate exactly as in [Use from CMake](#use-from-cmake) below — the port
installs the same `flatcitybuf::flatcitybuf` target the manual build does.

The port builds this directory from the `cpp-v0.9.0` tag (C++ releases are tagged `cpp-v<version>`; bare `v<version>` names the Rust crate releases) with tests and examples
off, JSON on. One packaging note: vcpkg ships a newer FlatBuffers than the
generated headers' exact-version assert expects, so the port patches the
assert to major-version-only.

## Build and install from source

```bash
cd src/cpp
cmake -B build -S .
cmake --build build
cmake --install build --prefix /your/prefix
```

Useful options: `-DFCB_WITH_CURL=ON` (HTTP), `-DFCB_WITH_JSON=OFF` (drop
CityJSON emission, the writer, and the nlohmann dependency),
`-DFCB_BUILD_TESTS=OFF`, `-DFCB_BUILD_EXAMPLES=OFF`.

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

## Writing a file

`fcb::FcbWriter` produces `.fcb` natively — no Rust toolchain here either.
Needs `FCB_WITH_JSON` (on by default). Parse each CityJSONSeq line as
`nlohmann::ordered_json`, never plain `nlohmann::json` — the latter stores
object members alphabetically, which silently renumbers the columns. The
attribute schema must reflect every feature you will add, so scan them once
before constructing the writer:

```cpp
#include <fcb/writer/attribute.hpp>
#include <fcb/writer/fcb_writer.hpp>

fcb::AttributeSchema schema;
for (const auto& feature : features)              // pass one: the schema
    for (const auto& obj : feature.at("CityObjects"))
        if (auto a = obj.find("attributes"); a != obj.end())
            fcb::add_attributes(schema, *a);

fcb::FcbWriterOptions options;                    // R-tree on by default
for (const auto& entry : schema)
    options.attribute_indices.emplace_back(entry.first, 256);  // B+tree, branching 256

fcb::FcbWriter w(metadata_line, options, schema, std::nullopt);
for (const auto& feature : features)              // pass two: the features
    w.add_feature(feature);
std::ofstream out("city.fcb", std::ios::binary);
w.write(out);
```

Both indices are the writer's own: `options.write_index` / `index_node_size`
control the packed Hilbert R-tree, `options.attribute_indices` names the
columns that get a static B+tree. `add_feature` spools each encoded feature
to a temp file and `write(std::ostream&)` streams the result out in chunks,
so peak memory does not grow with the number of features — the
`std::vector`-returning `write()` overload is a convenience for small files
and does not have that property. The output is checked byte-for-byte against
real Rust-written files, not merely for decoding correctly
(`tests/test_writer_oracle.cpp`).

Column numbering is the order `add_attributes` first *sees* a name — document
order, never alphabetical — so visit CityObjects deterministically (the
example sorts by id, as the Rust CLI does) if the columns must line up with a
Rust-written file. `examples/write_cityjson.cpp` is the full version of the
above, including the separate schema semantic-surface attributes need.

## Not implemented

- **A CLI.** This is a library; `fcb_write_cityjson` in `examples/` is a
  demonstration, not a conversion tool. The Rust CLI still covers that
  ground, and more.
