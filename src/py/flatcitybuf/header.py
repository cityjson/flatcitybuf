from __future__ import annotations

import struct
from dataclasses import dataclass, field
from typing import Any

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.generated.header_generated import Header as _Header
from flatcitybuf.generated.header_generated import Vector as _Vector
from flatcitybuf.layout import MAGIC_SIZE
from flatcitybuf.layout import FileLayout
from flatcitybuf.layout import check_magic_bytes
from flatcitybuf.layout import compute_layout
from flatcitybuf.layout import validate_layout_against_size
from flatcitybuf.range_reader import BufferedRangeReader, RangeReader

# http_reader/mod.rs:80-98 -- 2024 assumed header size plus the top
# three R-tree levels ((1 + 16 + 256) * 40 bytes), prefetched in one
# range request on open so a remote reader costs a single round trip
# rather than three (magic, size field, header body).
_OPEN_PREFETCH_SIZE = 12944

# lib.rs:56-58 / reader/mod.rs:97-102 -- the FlatBuffers size-prefix
# field itself: a bare 4-byte LE u32 in front of the Header FlatBuffer
# body.
_HEADER_SIZE_FIELD_SIZE = 4

# const_vars.rs:8, reader/mod.rs:97-102 -- header_size guard. Checked
# here, before the (potentially large) header body is ever read, rather
# than relying on layout.compute_layout()'s identical guard: that one
# only runs once features_count/index_node_size are already known --
# i.e. after the body would already have been read and parsed.
_HEADER_MIN_SIZE = 8
_HEADER_MAX_SIZE = 1024 * 1024 * 512  # 512 MiB

# header.fbs:65-70 -- AttributeIndex is a FlatBuffers *struct* (fixed
# layout, no vtable): 16 bytes, not 12. Field order forces 2 bytes of
# padding after each ushort: 0:u16 index, 2:pad, 4:u32 length, 8:u16
# branching_factor, 10:pad, 12:u32 num_unique_items. Decoded directly
# from raw bytes, not through the FlatBuffers accessors -- see
# _collect_attr_indices.
_ATTR_INDEX_FMT = "<HxxIHxxI"
_ATTR_INDEX_SIZE = struct.calcsize(_ATTR_INDEX_FMT)

# Header field 6 (attribute_index) -> vtable slot (6 + 2) * 2 = 16, per
# the field order in header.fbs's `table Header`.
_ATTRIBUTE_INDEX_VTABLE_OFFSET = 16


@dataclass(frozen=True)
class ColumnInfo:
    """One attribute column's schema, copied out of the header. Mirrors
    fcb::ColumnInfo (header.hpp:26-31)."""

    index: int
    name: str
    type: int  # header_generated.ColumnType, as its raw ubyte
    nullable: bool


@dataclass(frozen=True)
class AttrIndexInfo:
    """Where one column's B+tree attribute index lives, and how it is
    shaped. Mirrors fcb::AttrIndexInfo (header.hpp:34-40)."""

    column_index: int
    length: int  # whole blob, INCLUDING its payload section
    branching_factor: int
    num_unique_items: int  # unique KEYS (leaf count), not features
    begin: int  # absolute byte offset in the file


@dataclass
class FileInfo:
    """Everything a caller normally wants from the header, as owned
    values. Mirrors fcb::FileInfo (header.hpp:43-62), with one
    deliberate difference: transform and geographical_extent are plain
    Optional tuples here instead of a has_x flag alongside a
    zero-filled array. header.fbs declares both as ordinary offset
    fields (`transform: Transform;`, `geographical_extent:
    GeographicalExtent;`) -- not `= null` scalars -- so the generated
    Python accessor already returns None on absence and a real object
    when present. A separate has_x flag would be a speculative
    accessor this task's interface list does not name; see the task
    report.
    """

    features_count: int = 0
    index_node_size: int = 0
    columns: list[ColumnInfo] = field(default_factory=list)
    semantic_columns: list[ColumnInfo] = field(default_factory=list)

    # (minx, miny, minz, maxx, maxy, maxz), or None if the header has no
    # geographical_extent.
    geographical_extent: (
        tuple[float, float, float, float, float, float] | None
    ) = None

    # (x, y, z), or None if the header has no transform.
    scale: tuple[float, float, float] | None = None
    translate: tuple[float, float, float] | None = None

    crs: str = ""
    cityjson_version: str = ""
    identifier: str = ""
    title: str = ""


@dataclass(frozen=True)
class HeaderView:
    """A fully parsed header. Mirrors fcb::HeaderView (header.hpp:69-87).

    `_raw` is the one exception to "read_header() never hands out the
    generated Header object": cityjson.py's to_cityjson_metadata needs
    Header.templates()/templates_vertices() (geometry templates), which
    FileInfo does not carry -- there is no `has_x`-style flag for them
    the way there is for transform/geographical_extent, since the value
    itself (a list of raw Geometry tables) is not a plain scalar worth
    copying out eagerly. C++ solves the equivalent problem with a
    friend-only accessor (detail::HeaderAccess::get); Python has no
    friend-class mechanism, so this is a leading-underscore convention
    instead of an enforced boundary -- nothing outside cityjson.py
    should read this field. `repr`/`compare` are suppressed so printing
    or comparing a HeaderView does not dump/compare raw FlatBuffers
    table internals.
    """

    info: FileInfo
    layout: FileLayout
    attr_indices: list[AttrIndexInfo]
    _raw: Any = field(default=None, repr=False, compare=False)


def _decode_str(b: bytes) -> str:
    # Gotcha 4 (fixed-width B+tree string keys, truncated at the byte
    # level) does not apply to these header fields: every string here
    # is an ordinary length-prefixed FlatBuffers `string`, not a
    # fixed-width key. errors="replace" is still used defensively,
    # since the file is untrusted input and a corrupt length-prefixed
    # string could still contain invalid UTF-8.
    return b.decode("utf-8", errors="replace")


def _column_info_from(col: Any, position: int) -> ColumnInfo:
    name_bytes = col.Name()
    if name_bytes is None:
        raise FcbError(
            ErrorCode.MISSING_REQUIRED_FIELD,
            f"column at position {position}: name is required but absent",
        )
    return ColumnInfo(
        index=col.Index(),
        name=_decode_str(name_bytes),
        type=col.Type(),
        nullable=col.Nullable(),
    )


def _columns_from(length: Any, get: Any) -> list[ColumnInfo]:
    out: list[ColumnInfo] = []
    for j in range(length):
        col = get(j)
        if col is None:
            continue
        out.append(_column_info_from(col, j))
    return out


def _fill_columns(hdr: Any, info: FileInfo) -> None:
    info.columns = _columns_from(hdr.ColumnsLength(), hdr.Columns)
    info.semantic_columns = _columns_from(
        hdr.SemanticColumnsLength(), hdr.SemanticColumns
    )


def _fill_metadata(hdr: Any, info: FileInfo) -> None:
    info.features_count = hdr.FeaturesCount()
    info.index_node_size = hdr.IndexNodeSize()

    # Transform = { Vector scale; Vector translate; }, Vector = 3
    # doubles. header.fbs:34-37/28-32 -- offset field, not `= null`:
    # None on this accessor genuinely means "no transform in the file".
    t = hdr.Transform()
    if t is not None:
        sv = t.Scale(_Vector())
        tv = t.Translate(_Vector())
        info.scale = (sv.X(), sv.Y(), sv.Z())
        info.translate = (tv.X(), tv.Y(), tv.Z())

    # GeographicalExtent = { Vector min; Vector max; }.
    e = hdr.GeographicalExtent()
    if e is not None:
        mn = e.Min(_Vector())
        mx = e.Max(_Vector())
        info.geographical_extent = (
            mn.X(),
            mn.Y(),
            mn.Z(),
            mx.X(),
            mx.Y(),
            mx.Z(),
        )

    rs = hdr.ReferenceSystem()
    if rs is not None:
        authority_bytes = rs.Authority()
        authority = (
            _decode_str(authority_bytes)
            if authority_bytes is not None
            else "EPSG"
        )
        if rs.Code() != 0:
            info.crs = f"{authority}:{rs.Code()}"
        elif rs.CodeString() is not None:
            info.crs = f"{authority}:{_decode_str(rs.CodeString())}"

    version_bytes = hdr.Version()
    if version_bytes is None:
        raise FcbError(
            ErrorCode.MISSING_REQUIRED_FIELD,
            "Header.version is required but absent",
        )
    info.cityjson_version = _decode_str(version_bytes)

    identifier_bytes = hdr.Identifier()
    if identifier_bytes is not None:
        info.identifier = _decode_str(identifier_bytes)

    title_bytes = hdr.Title()
    if title_bytes is not None:
        info.title = _decode_str(title_bytes)


def _collect_attr_indices(
    hdr: Any,
) -> tuple[int, list[tuple[int, int, int, int]]]:
    """Sum the attribute-index lengths and return each entry as a raw
    (column_index, length, branching_factor, num_unique_items) tuple,
    sorted by column_index -- the order the writer concatenated the
    per-column blobs in (writer/mod.rs:190-195). Mirrors
    fcb::collect_attr_indices (header.cpp:118-155).

    Decodes AttributeIndex with struct.unpack_from directly against the
    raw vector bytes, rather than through the FlatBuffers accessors:
    see the module-level _ATTR_INDEX_FMT comment for why (16-byte
    struct, 2 bytes of padding after each ushort field). Reaching into
    `hdr._tab` is deliberate here for exactly that reason -- there is
    no public accessor that returns raw vector bytes.
    """
    o = hdr._tab.Offset(_ATTRIBUTE_INDEX_VTABLE_OFFSET)
    if o == 0:
        return 0, []

    n = hdr._tab.VectorLen(o)
    vec_pos = hdr._tab.Vector(o)
    buf = hdr._tab.Bytes

    # Cross-check the hand-counted vtable slot against the generated
    # accessor's own idea of the same offset (Header.AttributeIndexLength()
    # calls self._tab.Offset(16) internally -- see
    # generated/header_generated.py:945-949). If header.fbs ever gains a
    # field before attribute_index, _ATTRIBUTE_INDEX_VTABLE_OFFSET would
    # silently decode a different vector; this catches that instead of
    # returning garbage lengths/offsets.
    if n != hdr.AttributeIndexLength():
        raise FcbError(
            ErrorCode.INVALID_FLATBUFFER,
            "attribute_index vtable slot mismatch: hand-counted "
            f"offset {_ATTRIBUTE_INDEX_VTABLE_OFFSET} disagrees with "
            "the generated accessor -- header.fbs field order may "
            "have changed",
        )

    raw: list[tuple[int, int, int, int]] = []
    for i in range(n):
        index, length, branching_factor, num_unique_items = struct.unpack_from(
            _ATTR_INDEX_FMT, buf, vec_pos + i * _ATTR_INDEX_SIZE
        )
        raw.append((index, length, branching_factor, num_unique_items))

    raw.sort(key=lambda t: t[0])

    # Two indexes claiming the same column makes the cumulative-offset
    # walk below ambiguous: there is no way to know which blob comes
    # first.
    for i in range(1, len(raw)):
        if raw[i][0] == raw[i - 1][0]:
            raise FcbError(
                ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
                f"duplicate attribute index for column {raw[i][0]}",
            )

    total = sum(t[1] for t in raw)
    return total, raw


def read_header(reader: RangeReader) -> HeaderView:
    """Read and validate the file preamble and header. Mirrors
    fcb::read_header (header.cpp:159-236).

    Wraps `reader` in its own per-query BufferedRangeReader (matching
    the C++ reference), so opening a remote file costs one range
    request rather than several small ones.
    """
    if reader is None:
        raise FcbError(ErrorCode.IO_ERROR, "read_header: reader is None")

    total_size = reader.total_size()
    buffered = BufferedRangeReader(reader, _OPEN_PREFETCH_SIZE)

    magic = buffered.read(0, MAGIC_SIZE)
    if len(magic) < MAGIC_SIZE or not check_magic_bytes(magic):
        raise FcbError(ErrorCode.INVALID_MAGIC_BYTES, "not a FlatCityBuf file")

    size_bytes = buffered.read(MAGIC_SIZE, _HEADER_SIZE_FIELD_SIZE)
    if len(size_bytes) < _HEADER_SIZE_FIELD_SIZE:
        raise FcbError(
            ErrorCode.ILLEGAL_HEADER_SIZE, "truncated before header size"
        )
    (header_size,) = struct.unpack_from("<I", size_bytes, 0)

    if not (_HEADER_MIN_SIZE <= header_size <= _HEADER_MAX_SIZE):
        raise FcbError(
            ErrorCode.ILLEGAL_HEADER_SIZE,
            f"illegal header size: {header_size}",
        )

    # The buffer handed to FlatBuffers MUST include the 4-byte size
    # prefix: header_size is that prefix's value, not a bespoke length
    # field (reader/mod.rs:104-110) -- hence GetRootAs(buf, 4), per
    # Task 3's finding (test_generated.py).
    want = _HEADER_SIZE_FIELD_SIZE + header_size
    raw_buf = buffered.read(MAGIC_SIZE, want)
    if len(raw_buf) < want:
        raise FcbError(ErrorCode.ILLEGAL_HEADER_SIZE, "truncated header")

    try:
        hdr = _Header.GetRootAs(raw_buf, 4)  # type: ignore[no-untyped-call]
        info = FileInfo()
        _fill_metadata(hdr, info)
        _fill_columns(hdr, info)
        attr_index_size, raw_attr_indices = _collect_attr_indices(hdr)
    except FcbError:
        raise
    except Exception as exc:
        # No FlatBuffers Verifier exists in this Python runtime (Task
        # 3's finding), so a malformed body surfaces as whatever
        # exception the flatbuffers runtime or our own decoding raises
        # (IndexError, struct.error, ...). Nothing may escape the
        # public surface un-wrapped.
        raise FcbError(
            ErrorCode.INVALID_FLATBUFFER,
            "failed to parse header FlatBuffer",
        ) from exc

    layout = compute_layout(
        header_size,
        info.features_count,
        info.index_node_size,
        attr_index_size,
    )
    validate_layout_against_size(layout, total_size)

    cursor = layout.attr_index_begin
    attr_indices: list[AttrIndexInfo] = []
    for (
        column_index,
        length,
        branching_factor,
        num_unique_items,
    ) in raw_attr_indices:
        attr_indices.append(
            AttrIndexInfo(
                column_index=column_index,
                length=length,
                branching_factor=branching_factor,
                num_unique_items=num_unique_items,
                begin=cursor,
            )
        )
        cursor += length

    return HeaderView(
        info=info, layout=layout, attr_indices=attr_indices, _raw=hdr
    )
