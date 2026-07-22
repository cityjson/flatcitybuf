from __future__ import annotations

from contextlib import contextmanager
from enum import Enum
from typing import Iterator


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


@contextmanager
def reraise_as_invalid_flatbuffer(message: str) -> Iterator[None]:
    """Wrap a block of generated-FlatBuffers-accessor calls so nothing
    escapes the public surface un-wrapped (reader.py's _parse_feature
    docstring: "No FlatBuffers Verifier exists in this Python runtime
    (Task 3's finding), so a malformed body surfaces as whatever
    exception the flatbuffers runtime or our own decoding raises").

    _parse_feature applies this at the INITIAL GetRootAs/Feature(...)
    call only; deeper accessors reached later -- Feature.city_objects/
    vertices (feature.py) and every field cityjson.py reads off the raw
    generated tables in to_cityjson_feature/to_cityjson_metadata -- were
    not covered, so a corruption that only manifests when reading e.g.
    a geometry's semantics vector could leak a bare IndexError/
    struct.error/AttributeError past the public surface instead of an
    FcbError (Codex review, Task 12: reproduced by pointing
    CityObject.Objects at an out-of-bounds vector).

    FcbError itself passes through unchanged: it is already the
    library's own error, not a symptom of a missing Verifier, and
    wrapping it again would lose the original code/message.

    Deliberate tradeoff: catching bare `Exception` also swallows
    genuine programming bugs under this block (a failed `assert`, a
    `TypeError` from unrelated future code) and relabels them
    `FcbError(INVALID_FLATBUFFER)` instead of letting them surface as
    themselves. Narrowing the `except` to the specific exception types
    the flatbuffers runtime and our own decoders are known to raise
    (IndexError, struct.error, AttributeError, ValueError,
    UnicodeDecodeError, ...) was considered, but this wraps calls into
    generated, un-verified accessors across several modules -- an
    incomplete list would silently let some malformed-file crash modes
    leak past the public surface again, which is the exact bug this
    context manager exists to close. Kept broad on purpose; revisit if
    the narrower list is ever enumerated with confidence.
    """
    try:
        yield
    except FcbError:
        raise
    except Exception as exc:
        raise FcbError(ErrorCode.INVALID_FLATBUFFER, message) from exc
