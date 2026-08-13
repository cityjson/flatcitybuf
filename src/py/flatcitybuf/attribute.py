from __future__ import annotations

import struct
from typing import Any, Sequence

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.generated.header_generated import ColumnType
from flatcitybuf.header import ColumnInfo

# attribute.cpp:15-23 -- every fixed-width value is packed little-endian;
# get_le<T> there is a manual, byteswap-safe read. struct.unpack_from
# with an explicit "<" format does the same job, and additionally
# forces the right signedness per column type -- the task's gotcha 3: a
# u64 read with the *signed* format goes negative past 2**63, which
# matters here directly since attribute values span every integer
# width.
#
# Byte and UByte BOTH use the *unsigned* "<B": the writer stores a Byte
# with `out[offset] = b as u8` and the B+tree index path reads it back as
# a u8, so the feature-attribute path must match -- reading it signed
# turned a stored 200 into -56 (reader/deserializer.rs, arm ColumnType::
# Byte).
_FIXED_WIDTH_FORMATS: dict[int, str] = {
    ColumnType.Byte: "<B",
    ColumnType.UByte: "<B",
    ColumnType.Short: "<h",
    ColumnType.UShort: "<H",
    ColumnType.Int: "<i",
    ColumnType.UInt: "<I",
    ColumnType.Long: "<q",
    ColumnType.ULong: "<Q",
    ColumnType.Float: "<f",
    ColumnType.Double: "<d",
}

# attribute.cpp:129-143 -- String, DateTime and Json share the same
# length-prefixed-UTF8 wire encoding in the ATTRIBUTE BLOB (unlike the
# B+tree KEY encoding, where DateTime is 12 packed bytes and strings are
# fixed-width -- Format Reference, "Attribute B+tree" -> "Key
# encodings"). Json's text is kept raw here, not re-parsed: that is Task
# 8's job when emitting CityJSON.
_STRING_LIKE_TYPES = frozenset(
    {ColumnType.String, ColumnType.DateTime, ColumnType.Json}
)


def _need(blob: bytes, at: int, n: int, what: str) -> None:
    if len(blob) - at < n:
        raise FcbError(
            ErrorCode.INVALID_ATTRIBUTE_VALUE,
            f"truncated attribute blob reading {what}",
        )


def decode_attributes(
    blob: bytes, schema: Sequence[ColumnInfo]
) -> dict[str, Any]:
    """Decode one feature's (or CityObject's) attribute blob against a
    column schema. Mirrors fcb::decode_attributes (attribute.cpp:48-162).

    Wire format (reader/deserializer.rs:249-372, cited by
    attribute.hpp:43-56): repeated records of a little-endian u16 column
    index, then the value encoded per that column's type. Fixed-width
    types are packed little-endian; String, DateTime and Json are a
    little-endian u32 byte length then UTF-8 text; Binary is that same
    u32 length then raw bytes, decoded to a list of ints (the JSON array
    of numbers the Rust reader emits). Byte and UByte are both a single
    UNSIGNED byte -- see the note on _FIXED_WIDTH_FORMATS.

    CRITICAL -- `schema` MUST be the schema of whichever CityObject
    actually owns `blob`: CityObject.columns overrides Header.columns
    whenever it is set, and it is the caller's job to resolve that (see
    feature.CityObjectView.columns / .has_columns). Records are not
    self-delimiting -- each value's width comes from its column's type
    -- so passing the wrong schema desynchronises the rest of the blob
    and produces plausible-looking garbage rather than an exception
    (Format Reference -> "Attribute schema resolution").

    An empty blob decodes to an empty dict -- this is also what a
    present-but-empty attributes vector on a CityObject looks like once
    decoded; distinguishing that case from an ABSENT attributes vector
    is CityObjectView.has_attributes's job, one layer up.

    Raises FcbError(INVALID_ATTRIBUTE_VALUE) on a column index absent
    from `schema` or a truncated record, and
    FcbError(UNSUPPORTED_COLUMN_TYPE) on any ColumnType value this
    schema does not recognise.
    """
    out: dict[str, Any] = {}
    if not blob:
        return out

    by_index = {c.index: c for c in schema}
    at = 0
    n = len(blob)
    while at < n:
        _need(blob, at, 2, "column index")
        (col_index,) = struct.unpack_from("<H", blob, at)
        at += 2

        col = by_index.get(col_index)
        if col is None:
            raise FcbError(
                ErrorCode.INVALID_ATTRIBUTE_VALUE,
                f"attribute references unknown column index {col_index}",
            )
        ctype = col.type

        value: Any
        if ctype == ColumnType.Bool:
            _need(blob, at, 1, "Bool")
            value = blob[at] != 0
            at += 1
        elif ctype in _FIXED_WIDTH_FORMATS:
            fmt = _FIXED_WIDTH_FORMATS[ctype]
            size = struct.calcsize(fmt)
            _need(blob, at, size, col.name)
            (value,) = struct.unpack_from(fmt, blob, at)
            at += size
        elif ctype in _STRING_LIKE_TYPES:
            _need(blob, at, 4, "string length")
            (length,) = struct.unpack_from("<I", blob, at)
            at += 4
            _need(blob, at, length, "string body")
            value = blob[at : at + length].decode("utf-8", errors="replace")
            at += length
        elif ctype == ColumnType.Binary:
            # Same length prefix as the string-like types, but the body
            # is raw bytes. Emitted as a list of ints, matching the JSON
            # array of numbers the Rust reader produces -- and unlike
            # `bytes`, a list survives JSON serialisation downstream.
            _need(blob, at, 4, "binary length")
            (length,) = struct.unpack_from("<I", blob, at)
            at += 4
            _need(blob, at, length, "binary body")
            value = list(blob[at : at + length])
            at += length
        else:
            raise FcbError(
                ErrorCode.UNSUPPORTED_COLUMN_TYPE,
                f"column '{col.name}' has unrecognised type {ctype}",
            )

        out[col.name] = value

    return out
