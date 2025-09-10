# FlatCityBuf Python Bindings

Python bindings for [FlatCityBuf](../../README.md), a cloud-optimized binary format for storing and retrieving 3D city models with full CityJSON compatibility.

## Features

- **Fast reading** of FlatCityBuf (.fcb) files with zero-copy access
- **Local and HTTP** file support with async iteration
- **CityJSON integration** with transform, metadata, and proper structure
- **Spatial queries** using bounding boxes with R-tree indexing
- **Attribute queries** for filtering features with B+tree indices
- **Async iterators** for efficient streaming from HTTP sources
- **Nested boundaries** support for complex 3D geometries
- **Pythonic API** with comprehensive type support

## Installation

### From Source

```bash
# Prerequisites: Rust toolchain and maturin
pip install maturin

# Build and install with HTTP support
cd src/rust/fcb_py
maturin develop --features http
```

### Using pip (when available)

```bash
pip install flatcitybuf
```

## Quick Start

### Local File Access

```python
import flatcitybuf as fcb

# Read a local file
reader = fcb.Reader("https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb")

# Get file information
info = reader.info()
print(f"Features: {info.feature_count}")

# Get CityJSON header with transform and metadata
cityjson = reader.cityjson_header()
print(f"CityJSON version: {cityjson.version}")
print(f"Transform: {cityjson.transform.scale}, {cityjson.transform.translate}")

# Iterate all features (CityJSON format)
for feature in reader:
    print(f"ID: {feature.id}, Type: {feature.type}")
    print(f"City Objects: {len(feature.city_objects)}")
    print(f"Vertices: {len(feature.vertices)}")

# Spatial query
features = list(reader.query_bbox(84227.77, 445377.33, 85323.23, 446334.69))
print(f"Found {len(features)} features in bounding box")

# Attribute query
id_filter = fcb.AttrFilter("identificatie", fcb.Operator.Eq, "building_123")
buildings = list(reader.query_attr([id_filter]))
print(f"Found {len(buildings)} matching buildings")
```

### HTTP Access with Async Iterators

```python
import asyncio
import flatcitybuf as fcb

async def main():
    # Create async reader for HTTP URL
    async_reader = fcb.AsyncReader("https://storage.googleapis.com/flatcitybuf/3dbag_subset_all_index.fcb")
    opened_reader = await async_reader.open()

    # Get file info
    info = opened_reader.info()
    print(f"Features: {info.feature_count}")

    # Async iteration - efficient streaming
    async_iter = opened_reader.select_all()

    # Process features one by one
    count = 0
    while count < 10:  # Get first 10 features
        feature = await async_iter.next()
        if feature is None:
            break
        print(f"Feature {count}: {feature.id}")
        count += 1

    # Or collect all at once
    all_features = await opened_reader.select_all().collect()
    print(f"Total features: {len(all_features)}")

    # Async spatial query
    bbox_iter = opened_reader.query_bbox(84227.77, 445377.33, 85323.23, 446334.69)
    spatial_features = await bbox_iter.collect()
    print(f"Spatial query result: {len(spatial_features)} features")

asyncio.run(main())
```

## API Reference

### Reader (Synchronous)

Main class for reading local FlatCityBuf files.

```python
reader = fcb.Reader(path: str)
```

**Methods:**

- `info() -> FileInfo` - Get file metadata and statistics
- `cityjson_header() -> CityJSON` - Get CityJSON header with transform/metadata
- `query_bbox(min_x, min_y, max_x, max_y) -> Iterator[Feature]` - Spatial query
- `query_attr(filters: List[AttrFilter]) -> Iterator[Feature]` - Attribute query
- `__iter__() -> Iterator[Feature]` - Iterate all features

### AsyncReader (HTTP)

Asynchronous reader for HTTP-based FlatCityBuf files.

```python
async_reader = fcb.AsyncReader(url: str)
opened_reader = await async_reader.open()
```

**Methods:**

- `info() -> FileInfo` - Get file metadata
- `cityjson_header() -> CityJSON` - Get CityJSON header information
- `select_all() -> AsyncFeatureIterator` - Get all features as async iterator
- `query_bbox(min_x, min_y, max_x, max_y) -> AsyncFeatureIterator` - Async spatial query
- `query_attr(filters: List[AttrFilter]) -> AsyncFeatureIterator` - Async attribute query

### AsyncFeatureIterator

Efficient async iterator for streaming features from HTTP sources.

```python
# Get features one by one
feature = await async_iter.next()  # Returns Feature or None

# Collect all remaining features
features = await async_iter.collect()  # Returns List[Feature]
```

### Feature (CityJSON Format)

Represents a CityJSON feature with proper structure.

```python
class Feature:
    id: str                           # Feature ID
    type: str                         # Feature type
    vertices: List[List[float]]       # 3D vertices as [x, y, z] arrays
    city_objects: Dict[str, CityObject]  # Dictionary of city objects
```

### CityObject

Individual city object within a feature.

```python
class CityObject:
    type: str                         # Object type (e.g., "Building")
    geometry: List[Geometry]          # List of geometries
    attributes: Dict[str, Any]        # Object attributes
    children: List[str]               # Child object IDs
    parents: List[str]                # Parent object IDs
```

### Geometry

3D geometry with nested boundary structure.

```python
class Geometry:
    type: str                         # Geometry type
    boundaries: PyObject              # Nested boundary arrays (preserves structure)
    semantics: Optional[Any]          # Semantic information
```

### CityJSON Header

CityJSON metadata and transform information.

```python
class CityJSON:
    type: str                         # Always "CityJSON"
    version: str                      # CityJSON version
    transform: Transform              # Coordinate transformation
    metadata: Optional[Metadata]      # Optional metadata
    feature_count: int                # Number of features
```

### Transform

Coordinate transformation parameters.

```python
class Transform:
    scale: List[float]                # Scale factors [x, y, z]
    translate: List[float]            # Translation [x, y, z]
```

### Query Types

```python
# Bounding box
bbox = fcb.BBox(min_x=0, min_y=0, max_x=100, max_y=100)

# Attribute filters
filter_eq = fcb.AttrFilter("type", fcb.Operator.Eq, "building")
filter_gt = fcb.AttrFilter("height", fcb.Operator.Gt, 50.0)
filter_le = fcb.AttrFilter("floors", fcb.Operator.Le, 10)

# Available operators
fcb.Operator.Eq    # Equal
fcb.Operator.Ne    # Not equal
fcb.Operator.Gt    # Greater than
fcb.Operator.Ge    # Greater than or equal
fcb.Operator.Lt    # Less than
fcb.Operator.Le    # Less than or equal
```

## Performance

The Python bindings leverage Rust's zero-copy deserialization and efficient indexing:

- **10-20× faster** than parsing equivalent JSON formats
- **2-6× less memory** usage compared to text formats
- **Efficient spatial indexing** with packed R-tree queries
- **HTTP range requests** for cloud-optimized partial access
- **Async streaming** minimizes memory usage for large datasets
- **Nested boundary preservation** avoids flattening overhead

## Nested Boundaries

The bindings properly preserve CityJSON's nested boundary structure:

```python
# Boundaries can be nested arrays at arbitrary depth
# Example: [[0, 1, 2, 3]] for a single surface
# Example: [[[0, 1, 2, 3], [4, 5, 6, 7]]] for surfaces with holes
geometry.boundaries  # Preserves exact nesting from CityJSON
```

## HTTP Optimization

AsyncReader is optimized for HTTP access:

- **Persistent connections** - HTTP client maintained across iterator calls
- **Range request batching** - Efficient partial downloads
- **Prefetching strategies** - Reduces round-trip latency
- **Async streaming** - Process features as they arrive

## Examples

See the [examples/](examples/) directory for comprehensive usage examples including:

- Basic local file reading
- HTTP access with async iteration
- CityJSON header processing
- Spatial and attribute queries
- Nested geometry handling

## Development

### Building from Source

```bash
# Install development dependencies
pip install maturin pytest pytest-asyncio

# Build in development mode
maturin develop --features http

# Run tests
pytest tests/

# Run with coverage
pytest tests/ --cov=flatcitybuf
```

### Testing

```bash
# Run all tests
python -m pytest tests/test_e2e.py -v

# Run only sync tests
python -m pytest tests/test_e2e.py::TestE2EIntegration -v

# Run only async tests
python -m pytest tests/test_e2e.py::TestAsyncReaderE2E -v
```

### Project Structure

```
src/rust/fcb_py/
├── python/             # Python stub files
├── src/                # Rust PyO3 bindings
│   ├── lib.rs         # Main module
│   ├── reader.rs      # Sync reader
│   ├── async_reader.rs # Async reader
│   ├── types.rs       # Python types
│   ├── utils.rs       # Conversion utilities
│   └── query.rs       # Query types
├── tests/             # Python tests
├── examples/          # Usage examples
├── Cargo.toml         # Rust dependencies
└── pyproject.toml     # Python project config
```

## License

MIT License - see [LICENSE](../../LICENSE) file for details.
