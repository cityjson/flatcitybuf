"""
FlatCityBuf Python bindings

A cloud-optimized binary format for storing and retrieving 3D city models.
"""

# Import core classes (always available)
from .flatcitybuf import (
    AttrFilter,
    BBox,
    CityJSON,
    CityObject,
    FcbError,
    Feature,
    FeatureIterator,
    FileInfo,
    Geometry,
    Metadata,
    Operator,
    Reader,
    Transform,
    Vertex,
)

# Try to import async classes (available with http feature)
try:
    from .flatcitybuf import (
        AsyncFeatureIterator,  # noqa: F401
        AsyncReader,
        AsyncReaderOpened,
    )

    _ASYNC_AVAILABLE = True
except ImportError:
    _ASYNC_AVAILABLE = False

__version__ = "0.1.0"

__all__ = [
    "Reader",
    "FeatureIterator",
    "Feature",
    "CityObject",
    "Geometry",
    "Vertex",
    "FileInfo",
    "CityJSON",
    "Transform",
    "Metadata",
    "BBox",
    "AttrFilter",
    "Operator",
    "FcbError",
]

# Add async classes to __all__ if available
if _ASYNC_AVAILABLE:
    __all__.extend(
        [
            "AsyncReader",
            "AsyncReaderOpened",
            "AsyncFeatureIterator",
        ]
    )


def open_file(path: str):
    """Convenience function to open and read all features from a file"""
    reader = Reader(path)
    return list(reader)


def query_bbox(
    path: str, min_x: float, min_y: float, max_x: float, max_y: float
):
    """Convenience function for spatial bbox queries"""
    reader = Reader(path)
    return list(reader.query_bbox(min_x, min_y, max_x, max_y))
