# FlatCityBuf Python Bindings

Python bindings for [FlatCityBuf](../../README.md), a cloud-optimized binary format for storing and retrieving 3D city models.

## Features

- **Fast reading** of FlatCityBuf (.fcb) files
- **Local and HTTP** file support
- **Spatial queries** using bounding boxes
- **Attribute queries** for filtering features
- **Zero-copy access** for efficient memory usage
- **Pythonic API** with type hints

## Installation

### From Source

```bash
# Prerequisites: Rust toolchain and maturin
pip install maturin

# Build and install
cd src/py
maturin develop --features http
```

### Using pip

```bash
pip install fcb
```

## Quick Start

```python
import fcb

# Read a local file
reader = fcb.Reader("data.fcb")

# Get file information
info = reader.info()
print(f"Features: {info.feature_count}")

# Iterate all features
for feature in reader:
    print(f"ID: {feature.id}, Type: {feature.feature_type}")
    print(f"Geometries: {len(feature.geometry)}")

# Spatial query
features = reader.query_bbox(
    min_x=0, min_y=0,
    max_x=1000, max_y=1000
)
print(f"Found {len(features)} features in bounding box")

# Attribute query
tall_buildings = reader.query_attr("building_height", ">", 50.0)
print(f"Found {len(tall_buildings)} tall buildings")
```

### HTTP Access

```python
# For HTTP URLs, use AsyncReader
async_reader = fcb.AsyncReader("https://example.com/data.fcb")
info = async_reader.info()
```

## API Reference

### Reader

Main class for reading FlatCityBuf files.

```python
reader = fcb.Reader(path: str)
```

**Methods:**

- `info() -> FileInfo` - Get file metadata
- `query_bbox(min_x, min_y, max_x, max_y) -> List[Feature]` - Spatial query
- `query_attr(field, operator, value) -> List[Feature]` - Attribute query
- `__iter__() -> Iterator[Feature]` - Iterate all features

### Feature

Represents a 3D city feature.

```python
class Feature:
    id: Optional[str]
    feature_type: str
    geometry: List[Geometry]
    attributes: Dict[str, Any]
```

### Geometry

Geometry data with vertices and boundaries.

```python
class Geometry:
    geometry_type: str
    vertices: List[Vertex]
    boundaries: List[List[int]]
    semantics: Optional[Any]
```

### Vertex

3D vertex coordinates.

```python
class Vertex:
    x: float
    y: float
    z: float
```

### Query Types

```python
# Bounding box
bbox = fcb.BBox(min_x=0, min_y=0, max_x=100, max_y=100)

# Attribute filter
filter = fcb.AttrFilter.gt("height", 50.0)  # height > 50.0
filter = fcb.AttrFilter.eq("type", "building")  # type == "building"
```

## Performance

The Python bindings leverage Rust's zero-copy deserialization for excellent performance:

- **10-20× faster** than parsing equivalent JSON formats
- **2-6× less memory** usage compared to text formats
- **Efficient spatial indexing** with R-tree queries
- **HTTP range requests** for cloud-optimized access

## Examples

See the [examples/](examples/) directory for more usage examples.

## Development

### Building from Source

```bash
# Install development dependencies
pip install maturin pytest pytest-asyncio

# Build in development mode
maturin develop --features http

# Run tests
pytest tests/
```

### Project Structure

```
src/py/
├── python/fcb/          # Python package
├── src/                 # Rust PyO3 bindings
├── tests/               # Python tests
├── examples/            # Usage examples
├── Cargo.toml          # Rust dependencies
└── pyproject.toml      # Python project config
```

## License

MIT License - see [LICENSE](../../LICENSE) file for details.
