from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.errors import reraise_as_invalid_flatbuffer
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

# feature.fbs:61-66 -- CityFeature's field order: id(0), objects(1),
# vertices(2), appearance(3) -> vertices' vtable slot is (2 + 2) * 2 =
# 8. Vertex is a `struct {x:int; y:int; z:int}` (12 bytes, native LE),
# not a scalar, so flatc does not generate a `VerticesAsNumpy()`
# accessor for it the way it does for plain `[uint]` fields (see
# cityjson.py's _uint_list) -- reaching into `_tab` directly is the only
# way to bulk-decode this one.
_VERTICES_VTABLE_OFFSET = 8


def _decode_str(b: bytes) -> str:
    # Ordinary length-prefixed FlatBuffers strings (CityFeature.id,
    # CityObject.id/extension_type) -- not the fixed-width B+tree keys
    # gotcha 4 warns about. errors="replace" is still defensive, since
    # the file is untrusted input.
    return b.decode("utf-8", errors="replace")


def _import_numpy() -> Any:
    """Optional numpy import behind a stable `Any`-typed helper.

    `try: import numpy as np / except ImportError: np = None` needs a
    `# type: ignore[assignment]` when numpy resolves (assigning a typed
    module to `None`) and mypy flags that very comment as an
    unused-ignore when numpy is absent -- no single environment
    satisfies both, yet this package must pass mypy strict either way
    (numpy is a genuine optional extra; see pyproject.toml). Funnelling
    the import through a function with a declared `Any` return avoids
    the conflict: the call site's assignment is `Any`, never a typed
    module vs. `None`, in either environment.
    """
    try:
        import numpy
    except ImportError:
        return None
    return numpy


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


def _vertices_via_numpy(
    np: Any, raw: Any, length: int
) -> list[tuple[int, int, int]] | None:
    """Bulk-decode CityFeature.vertices with one `numpy.frombuffer` call
    instead of `length` per-element FlatBuffers table reads. Profiling a
    full scan of delft.fcb (Task 12's benchmark) found the per-element
    loop this replaces to be a measurable share of scan time, alongside
    the geometry uint-vector fields cityjson.py's _uint_list already
    accelerates the same way -- see that function's docstring for why
    flatc's own generated `XxxAsNumpy` accessors cover THOSE fields but
    not this one (Vertex is a struct, not a scalar).

    Returns None (falling back to the caller's per-element loop) if the
    vtable-offset cross-check fails, mirroring _read_attribute_bytes's
    defensive pattern -- a schema change to feature.fbs that shifted
    this field's slot would otherwise silently decode the wrong vector
    as if it were vertices.
    """
    tab = raw._tab
    o = tab.Offset(_VERTICES_VTABLE_OFFSET)
    if o == 0:
        return [] if length == 0 else None
    if tab.VectorLen(o) != length:
        return None
    start = tab.Vector(o)
    n_bytes = length * 12
    body = tab.Bytes[start : start + n_bytes]
    arr = np.frombuffer(body, dtype="<i4", count=length * 3).reshape(length, 3)
    return [(int(x), int(y), int(z)) for x, y, z in arr.tolist()]


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

    `_raw` is the one exception to this class never handing out its
    generated FlatBuffers object: cityjson.py needs
    geometry/geometry_instances/geographical_extent/children/parents,
    none of which Task 7 scoped into this view (it was built around
    attributes/columns only). Mirrors detail::FeatureAccess::get in the
    C++ port -- Python has no friend-class mechanism, so this is a
    leading-underscore convention instead of an enforced boundary;
    nothing outside cityjson.py should read it. `repr`/`compare` are
    suppressed so printing or comparing a CityObjectView does not
    dump/compare raw FlatBuffers table internals.
    """

    id: str
    type: int  # feature.fbs CityObjectType, as its raw ubyte
    extension_type: str | None
    has_attributes: bool
    attributes: bytes | None
    has_columns: bool
    columns: list[ColumnInfo] | None
    _raw: Any = field(default=None, repr=False, compare=False)


def raw_city_object(view: CityObjectView) -> Any:
    """Internal gateway to CityObjectView's generated CityObject table.
    See the class docstring's `_raw` note -- only cityjson.py should
    call this."""
    return view._raw


def raw_city_feature(feature: Feature) -> Any:
    """Internal gateway to Feature's generated CityFeature table (for
    `.Vertices()`/`.Appearance()`, which Task 7's public surface does
    not expose). Mirrors detail::FeatureAccess::get in the C++ port --
    only cityjson.py should call this."""
    return feature._raw


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
        _raw=obj,
    )


class Feature:
    """One decoded CityFeature. Mirrors fcb::Feature (feature.hpp,
    reader.cpp:23-109). `_raw` is exposed to cityjson.py only, through
    the module-level `raw_city_feature` gateway above -- see its
    docstring.
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
        # Codex review (Task 12): _parse_feature (reader.py) only wraps
        # the INITIAL GetRootAs/Feature(...) call in
        # reraise_as_invalid_flatbuffer; this deeper accessor -- called
        # directly by a caller of select_all(), and by cityjson.py's
        # to_cityjson_feature -- was not covered, so a corrupt Objects
        # vector could leak a bare IndexError/struct.error past the
        # public surface instead of an FcbError.
        with reraise_as_invalid_flatbuffer(
            "failed to decode feature's city objects"
        ):
            return [
                _city_object_view_from(self._raw.Objects(j))
                for j in range(self._raw.ObjectsLength())
            ]

    def vertices(self) -> list[tuple[int, int, int]]:
        # feature.fbs:55-59 -- Vertex is a struct of 3 plain (non-null)
        # int32 fields; these are the raw scaled integers on disk, not
        # transformed coordinates -- applying header.scale/translate is
        # Task 8's job when emitting CityJSON. Same un-wrapped-exception
        # gap as city_objects() above, closed the same way.
        with reraise_as_invalid_flatbuffer(
            "failed to decode feature's vertices"
        ):
            length = self._raw.VerticesLength()
            if length == 0:
                return []
            np = _import_numpy()
            if np is not None:
                bulk = _vertices_via_numpy(np, self._raw, length)
                if bulk is not None:
                    return bulk
            out: list[tuple[int, int, int]] = []
            for j in range(length):
                v = self._raw.Vertices(j)
                out.append((v.X(), v.Y(), v.Z()))
            return out
