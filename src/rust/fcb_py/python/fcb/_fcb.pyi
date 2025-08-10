"""Type stubs for the _fcb Rust extension module"""

from typing import Optional, List, Dict, Any, Union, Iterator

class FcbError(Exception):
    """Exception raised by FlatCityBuf operations"""
    pass

class Vertex:
    """A 3D vertex with x, y, z coordinates"""
    x: float
    y: float  
    z: float
    
    def __init__(self, x: float, y: float, z: float) -> None: ...
    def to_tuple(self) -> tuple[float, float, float]: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class Geometry:
    """Geometry data with type, vertices, and boundaries"""
    geometry_type: str
    vertices: List[Vertex]
    boundaries: List[List[int]]
    semantics: Optional[Any]
    
    def __init__(
        self, 
        geometry_type: str, 
        vertices: List[Vertex],
        boundaries: List[List[int]], 
        semantics: Optional[Any]
    ) -> None: ...
    def __repr__(self) -> str: ...

class Feature:
    """A FlatCityBuf feature with ID, type, geometry, and attributes"""
    id: Optional[str]
    feature_type: str
    geometry: List[Geometry]
    attributes: Dict[str, Any]
    
    def __init__(
        self,
        id: Optional[str],
        feature_type: str, 
        geometry: List[Geometry],
        attributes: Dict[str, Any]
    ) -> None: ...
    def __repr__(self) -> str: ...

class FileInfo:
    """Metadata about a FlatCityBuf file"""
    feature_count: int
    columns: List[Dict[str, Any]]
    crs: Optional[str]
    bbox: Optional[tuple[float, float, float, float]]
    
    def __init__(
        self,
        feature_count: int,
        columns: List[Dict[str, Any]], 
        crs: Optional[str],
        bbox: Optional[tuple[float, float, float, float]]
    ) -> None: ...
    def __repr__(self) -> str: ...

class BBox:
    """Bounding box for spatial queries"""
    min_x: float
    min_y: float
    max_x: float
    max_y: float
    
    def __init__(self, min_x: float, min_y: float, max_x: float, max_y: float) -> None: ...
    def contains(self, x: float, y: float) -> bool: ...
    def intersects(self, other: "BBox") -> bool: ...
    def area(self) -> float: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class Operator:
    """Query operators for attribute filtering"""
    Eq: "Operator"
    Ne: "Operator" 
    Gt: "Operator"
    Ge: "Operator"
    Lt: "Operator"
    Le: "Operator"
    
    def __repr__(self) -> str: ...

class AttrFilter:
    """Attribute filter for querying features"""
    field: str
    operator: Operator
    value: Any
    
    def __init__(self, field: str, operator: Operator, value: Any) -> None: ...
    @classmethod
    def eq(cls, field: str, value: Any) -> "AttrFilter": ...
    @classmethod 
    def ne(cls, field: str, value: Any) -> "AttrFilter": ...
    @classmethod
    def gt(cls, field: str, value: Any) -> "AttrFilter": ...
    @classmethod
    def ge(cls, field: str, value: Any) -> "AttrFilter": ...
    @classmethod
    def lt(cls, field: str, value: Any) -> "AttrFilter": ...
    @classmethod
    def le(cls, field: str, value: Any) -> "AttrFilter": ...
    def __repr__(self) -> str: ...

class ReaderIterator:
    """Iterator for reading features from a FlatCityBuf file"""
    def __iter__(self) -> "ReaderIterator": ...
    def __next__(self) -> Feature: ...

class Reader:
    """Main reader for FlatCityBuf files"""
    def __init__(self, path: str) -> None: ...
    def info(self) -> FileInfo: ...
    def query_bbox(self, min_x: float, min_y: float, max_x: float, max_y: float) -> List[Feature]: ...
    def query_spatial(self, bbox: BBox) -> List[Feature]: ...
    def query_attr(self, field: str, operator: str, value: Any) -> List[Feature]: ...
    def query_attribute(self, filter: AttrFilter) -> List[Feature]: ...
    def __iter__(self) -> ReaderIterator: ...
    def __repr__(self) -> str: ...

class AsyncReader:
    """Async reader for HTTP URLs"""
    def __init__(self, url: str) -> None: ...
    def info(self) -> FileInfo: ...
    def __repr__(self) -> str: ...