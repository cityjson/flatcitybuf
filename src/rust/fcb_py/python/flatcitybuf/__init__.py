"""
FlatCityBuf Python bindings

A cloud-optimized binary format for storing and retrieving 3D city models.
"""

# Import core classes (always available)
from .flatcitybuf import (
    Reader,
    FeatureIterator,
    Feature,
    Geometry,
    Vertex,
    FileInfo,
    BBox,
    AttrFilter,
    Operator,
    FcbError,
)

# Try to import async classes (available with http feature)
try:
    from .flatcitybuf import (
        AsyncReader,
        AsyncReaderOpened,
        AsyncFeatureIterator,
    )
    _ASYNC_AVAILABLE = True
except ImportError:
    _ASYNC_AVAILABLE = False

__version__ = "0.1.0"

__all__ = [
    "Reader",
    "FeatureIterator",
    "Feature",
    "Geometry", 
    "Vertex",
    "FileInfo",
    "BBox",
    "AttrFilter",
    "Operator",
    "FcbError",
]

# Add async classes to __all__ if available
if _ASYNC_AVAILABLE:
    __all__.extend([
        "AsyncReader",
        "AsyncReaderOpened",
        "AsyncFeatureIterator",
    ])

def open_file(path: str):
    """Convenience function to open and read all features from a file"""
    with Reader(path) as reader:
        return list(reader)

def query_bbox(path: str, min_x: float, min_y: float, max_x: float, max_y: float):
    """Convenience function for spatial bbox queries"""
    with Reader(path) as reader:
        return list(reader.query_bbox(min_x, min_y, max_x, max_y))
