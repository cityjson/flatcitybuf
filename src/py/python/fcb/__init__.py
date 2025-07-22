"""
FlatCityBuf Python bindings

A cloud-optimized binary format for storing and retrieving 3D city models.
"""

from ._fcb import (
    Reader,
    AsyncReader, 
    Feature,
    Geometry,
    Vertex,
    FileInfo,
    BBox,
    AttrFilter,
    Operator,
    FcbError,
)

__version__ = "0.1.0"

__all__ = [
    "Reader",
    "AsyncReader",
    "Feature", 
    "Geometry",
    "Vertex",
    "FileInfo",
    "BBox",
    "AttrFilter",
    "Operator",
    "FcbError",
]

# Convenience functions
def open_file(path: str) -> Reader:
    """
    Open a FlatCityBuf file for reading.
    
    Args:
        path: Path to the .fcb file (local path or HTTP URL)
        
    Returns:
        Reader instance
        
    Example:
        >>> reader = fcb.open_file("data.fcb")
        >>> for feature in reader:
        ...     print(feature.id)
    """
    return Reader(path)

def query_bbox(path: str, min_x: float, min_y: float, max_x: float, max_y: float) -> list[Feature]:
    """
    Query features by bounding box from a file.
    
    Args:
        path: Path to the .fcb file
        min_x, min_y, max_x, max_y: Bounding box coordinates
        
    Returns:
        List of features within the bounding box
        
    Example:
        >>> features = fcb.query_bbox("data.fcb", 0, 0, 100, 100)
        >>> print(f"Found {len(features)} features")
    """
    reader = Reader(path)
    return reader.query_bbox(min_x, min_y, max_x, max_y)