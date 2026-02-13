# FlatCityBuf Go Bindings

Go bindings for [FlatCityBuf](https://github.com/cityjson/flatcitybuf) — a binary format for CityJSON data with built-in spatial and attribute indexing.

Built with CGO linking to a Rust static library, these bindings provide file-based read access to FCB files with spatial query support.

## Requirements

- Go 1.21+
- Rust toolchain (for building the static library)
- [just](https://github.com/casey/just) (task runner)

## Building

Build the Rust static library first, then use `go build` or `go test` as normal:

```bash
# From the project root
just build-go-lib       # release build
just build-go-lib-dev   # debug build

# Run tests
just test-go
```

## Quick Start

```go
package main

import (
    "encoding/json"
    "fmt"
    "log"

    "github.com/cityjson/flatcitybuf-go/fcb"
)

func main() {
    // Open a local FCB file
    reader, err := fcb.Open("path/to/file.fcb")
    if err != nil {
        log.Fatal(err)
    }
    defer reader.Close()

    fmt.Printf("Features: %d\n", reader.FeaturesCount())
    fmt.Printf("Has spatial index: %v\n", reader.HasSpatialIndex())

    // Read CityJSON metadata
    meta, _ := reader.CityJSONMetadata()
    fmt.Printf("CityJSON type: %s\n", meta["type"])

    // Iterate over all features
    iter, err := reader.SelectAll()
    if err != nil {
        log.Fatal(err)
    }
    defer iter.Close()

    for iter.Next() {
        feature, err := iter.Feature()
        if err != nil {
            log.Fatal(err)
        }
        fmt.Printf("Feature: %s\n", feature.ID)
    }
    if err := iter.Err(); err != nil {
        log.Fatal(err)
    }
}
```

## API Reference

### `fcb.Open(path string) (*Reader, error)`

Opens a local FCB file for reading. Returns a Reader that must be closed when done.

```go
reader, err := fcb.Open("/data/buildings.fcb")
if err != nil {
    log.Fatal(err)
}
defer reader.Close()
```

### `Reader`

#### `reader.FeaturesCount() uint64`

Returns the total number of features in the file.

#### `reader.HasSpatialIndex() bool`

Returns true if the file includes a spatial index (required for `SelectBBox`).

#### `reader.CityJSONMetadata() (map[string]interface{}, error)`

Returns the CityJSON metadata as a parsed JSON map. Contains type, version, transform, and other metadata fields.

```go
meta, err := reader.CityJSONMetadata()
version := meta["version"]       // e.g., "2.0"
transform := meta["transform"]   // { scale: [...], translate: [...] }
```

#### `reader.SelectAll() (*FeatureIter, error)`

Select all features for iteration. **Consumes the reader** — after calling `SelectAll`, the reader should not be used again.

```go
iter, err := reader.SelectAll()
if err != nil {
    log.Fatal(err)
}
defer iter.Close()
// reader.Close() is now a no-op
```

#### `reader.SelectBBox(bbox BBox) (*FeatureIter, error)`

Select features within a 2D bounding box. Requires the file to have a spatial index. **Consumes the reader.**

```go
bbox := fcb.BBox{
    MinX: 84400.0,
    MinY: 447200.0,
    MaxX: 84600.0,
    MaxY: 447400.0,
}
iter, err := reader.SelectBBox(bbox)
```

#### `reader.Close()`

Frees the reader resources. Safe to call multiple times. Becomes a no-op after `SelectAll` or `SelectBBox`.

### `FeatureIter`

Iterator over selected features, following Go's `rows.Next()` / `rows.Scan()` pattern.

#### `iter.Next() bool`

Advances to the next feature. Returns `true` if a feature is available, `false` when iteration is complete or an error occurred. Always check `Err()` after the loop.

#### `iter.Feature() (*CityFeature, error)`

Returns the current feature. Must be called after `Next()` returns `true`.

```go
for iter.Next() {
    feature, err := iter.Feature()
    if err != nil {
        log.Fatal(err)
    }
    fmt.Println(feature.ID)

    // Parse the JSON if needed
    var parsed map[string]interface{}
    json.Unmarshal([]byte(feature.JSON), &parsed)
}
```

#### `iter.Err() error`

Returns the first error encountered during iteration, or `nil`.

#### `iter.FeaturesCount() uint64`

Returns the total number of features matching the selection.

#### `iter.Close()`

Frees the iterator resources. Safe to call multiple times.

### Types

#### `BBox`

```go
type BBox struct {
    MinX float64
    MinY float64
    MaxX float64
    MaxY float64
}
```

#### `CityFeature`

```go
type CityFeature struct {
    ID   string  // Feature identifier
    JSON string  // Full CityJSONFeature as a JSON string
}
```

## Ownership Model

The Go bindings follow a **consume-on-select** pattern that mirrors the underlying Rust ownership:

1. `Open()` returns a `Reader`
2. `SelectAll()` or `SelectBBox()` **consumes** the reader and returns a `FeatureIter`
3. After selection, the reader pointer is set to `nil` (calling `Close()` is safe but a no-op)
4. You must `Close()` the iterator when done

This prevents double-free errors and ensures memory safety at the FFI boundary.

```go
reader, _ := fcb.Open("file.fcb")
// reader is valid here

iter, _ := reader.SelectAll()
// reader is now consumed (nil internally)
// reader.Close() is safe but does nothing

defer iter.Close()  // this frees the memory
```

## Architecture

```
Go application
    │
    ▼
fcb/fcb.go  (Go wrapper with CGO)
    │
    ▼
fcb_core.h  (auto-generated C header via cbindgen)
    │
    ▼
libfcb_go.a (Rust static library)
    │
    ▼
fcb_core    (Rust core library)
```

The Rust FFI layer (`src/rust/fcb_go`) uses type-erased iterators (`Box<dyn IteratorHelper>`) to avoid exposing Rust generics across the C boundary. Error handling uses C-style `error_out` pointer parameters.
