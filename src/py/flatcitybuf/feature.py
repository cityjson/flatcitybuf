from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.header import ColumnInfo
from flatcitybuf.header import _column_info_from

# feature.fbs:68-80 -- CityObject's field order fixes each field's
# vtable slot at (field_index + 2) * 2: attributes is field 6, so its
# slot is 16 -- matching the generated accessor's own
# self._tab.Offset(16) (generated/feature_generated.py). Only
# `attributes` needs hand-rolled `_tab` access (see
# _read_attribute_bytes); `columns` goes through the ordinary generated
# Columns()/ColumnsLength() accessors, which are unaffected by the numpy
# / per-element-call concerns that motivate reaching in for attributes.
_ATTRIBUTES_VTABLE_OFFSET = 16


def _decode_str(b: bytes) -> str:
    # Ordinary length-prefixed FlatBuffers strings (CityFeature.id,
    # CityObject.id/extension_type) -- not the fixed-width B+tree keys
    # gotcha 4 warns about. errors="replace" is still defensive, since
    # the file is untrusted input.
    return b.decode("utf-8", errors="replace")


def _read_attribute_bytes(obj: Any) -> bytes:
    """Raw bytes of CityObject.attributes.

    Deliberately does not use AttributesAsNumpy() (needs numpy, which
    this package's "no compiled dependency, ever" constraint forbids at
    module scope) or the generated per-element Attributes(j) accessor
    (an O(n) Python-level call per byte). Reaches into `_tab` directly,
    the same way header.py's _collect_attr_indices does, and
    cross-checks the hand-counted vtable slot against the generated
    accessor's own length for the same defensive reason: a schema
    change to feature.fbs that shifts this field's slot would otherwise
    silently decode a different vector.
    """
    o = obj._tab.Offset(_ATTRIBUTES_VTABLE_OFFSET)
    if o == 0:
        return b""
    length = obj._tab.VectorLen(o)
    if length != obj.AttributesLength():
        raise FcbError(
            ErrorCode.INVALID_FLATBUFFER,
            "attributes vtable slot mismatch: hand-counted offset "
            f"{_ATTRIBUTES_VTABLE_OFFSET} disagrees with the generated "
            "accessor -- feature.fbs field order may have changed",
        )
    start = obj._tab.Vector(o)
    return bytes(obj._tab.Bytes[start : start + length])


def _columns_from_object(obj: Any) -> list[ColumnInfo]:
    # CityObject.Columns(j) (feature.fbs) returns the exact same
    # generated Column type Header.Columns(j) (header.fbs) does, so
    # header.py's field-extraction helper applies unchanged -- reused
    # here rather than duplicated.
    out: list[ColumnInfo] = []
    for j in range(obj.ColumnsLength()):
        col = obj.Columns(j)
        if col is None:
            continue
        out.append(_column_info_from(col, j))
    return out


@dataclass(frozen=True)
class CityObjectView:
    """One CityObject inside a Feature. Mirrors the per-object accessors
    on fcb::Feature (feature.hpp; reader.cpp:35-109 /
    object_attributes/object_has_attributes/object_has_columns/
    object_columns/object_id), collected into a single value type since
    Python has no lifetime hazard forcing a lazy, buffer-owning accessor
    style the way C++ does.

    `has_attributes`/`has_columns` distinguish an ABSENT vector from a
    PRESENT-but-empty one -- `AttributesIsNone()`/`ColumnsIsNone()` test
    the vtable offset directly (o == 0), unlike the length-based
    accessors, which return 0 for both (task brief trap #2). `attributes`
    and `columns` are None exactly when absent, never when merely empty.

    `columns` -- when set (`has_columns`) -- OVERRIDES the header's
    columns for decoding `attributes`; see attribute.py's
    decode_attributes docstring and the task report for why this is the
    normal case in real data, not an edge case.
    """

    id: str
    type: int  # feature.fbs CityObjectType, as its raw ubyte
    extension_type: str | None
    has_attributes: bool
    attributes: bytes | None
    has_columns: bool
    columns: list[ColumnInfo] | None


def _city_object_view_from(obj: Any) -> CityObjectView:
    id_bytes = obj.Id()
    if id_bytes is None:
        raise FcbError(
            ErrorCode.MISSING_REQUIRED_FIELD,
            "CityObject.id is required but absent",
        )
    ext_bytes = obj.ExtensionType()

    has_attributes = not obj.AttributesIsNone()
    has_columns = not obj.ColumnsIsNone()

    return CityObjectView(
        id=_decode_str(id_bytes),
        type=obj.Type(),
        extension_type=(
            _decode_str(ext_bytes) if ext_bytes is not None else None
        ),
        has_attributes=has_attributes,
        attributes=_read_attribute_bytes(obj) if has_attributes else None,
        has_columns=has_columns,
        columns=_columns_from_object(obj) if has_columns else None,
    )


class Feature:
    """One decoded CityFeature. Mirrors fcb::Feature (feature.hpp,
    reader.cpp:23-109), minus the C++ friend-class dance around a raw
    generated pointer: Python has no equivalent lifetime hazard to guard
    against, since this class never hands `_raw` out to callers.
    """

    def __init__(self, raw: Any, byte_offset: int) -> None:
        id_bytes = raw.Id()
        if id_bytes is None:
            raise FcbError(
                ErrorCode.MISSING_REQUIRED_FIELD,
                "CityFeature.id is required but absent",
            )
        self.id: str = _decode_str(id_bytes)
        # Feature-section-relative byte offset (Format Reference ->
        # "Features"): the primitive Tasks 9-11 will need once the
        # R-tree/B+tree hand back offsets instead of a sequential
        # cursor.
        self.byte_offset = byte_offset
        self._raw = raw

    def city_objects(self) -> list[CityObjectView]:
        return [
            _city_object_view_from(self._raw.Objects(j))
            for j in range(self._raw.ObjectsLength())
        ]

    def vertices(self) -> list[tuple[int, int, int]]:
        # feature.fbs:55-59 -- Vertex is a struct of 3 plain (non-null)
        # int32 fields; these are the raw scaled integers on disk, not
        # transformed coordinates -- applying header.scale/translate is
        # Task 8's job when emitting CityJSON.
        out: list[tuple[int, int, int]] = []
        for j in range(self._raw.VerticesLength()):
            v = self._raw.Vertices(j)
            out.append((v.X(), v.Y(), v.Z()))
        return out
