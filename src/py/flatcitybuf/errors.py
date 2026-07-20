from __future__ import annotations

from enum import Enum


class ErrorCode(Enum):
    INVALID_MAGIC_BYTES = "invalid magic bytes"
    ILLEGAL_HEADER_SIZE = "illegal header size"
    INVALID_FLATBUFFER = "invalid flatbuffer"
    MISSING_REQUIRED_FIELD = "missing required field"
    INDEX_OUT_OF_BOUNDS = "index out of bounds"
    IO_ERROR = "io error"
    UNSUPPORTED_COLUMN_TYPE = "unsupported column type"
    ATTRIBUTE_INDEX_NOT_FOUND = "attribute index not found"
    # Mirrors fcb::ErrorCode::InvalidAttributeValue (error.hpp), used by
    # attribute.cpp's `need()` helper (truncated record) and its
    # unknown-column-index check -- both raised while decoding a
    # feature/CityObject attribute blob (attribute.py).
    INVALID_ATTRIBUTE_VALUE = "invalid attribute value"


class FcbError(Exception):
    """Every error this package raises. Mirrors fcb::Error in the C++
    reader."""

    def __init__(self, code: ErrorCode, message: str) -> None:
        super().__init__(f"{code.value}: {message}")
        self.code = code
