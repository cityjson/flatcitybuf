from __future__ import annotations

import math
import struct
from dataclasses import dataclass
from enum import Enum

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.generated.header_generated import ColumnType

# Format Reference -> "Attribute B+tree" -> Key encodings
# (docs/superpowers/plans/2026-07-19-native-cpp-core.md:155-165), whose
# every row is cited to a line of the Rust origin
# (src/rust/fcb_core/src/static_btree/key.rs). Ported from the
# conformant C++ port at src/cpp/src/key.cpp; that file's comments carry
# the same citations and this module keeps them.
#
# ALL formats are explicit little-endian ("<"), never native "@": the
# wire format is LE regardless of host endianness.
_FMT_I8 = "<b"
_FMT_U8 = "<B"
_FMT_I16 = "<h"
_FMT_U16 = "<H"
_FMT_I32 = "<i"
_FMT_U32 = "<I"
_FMT_I64 = "<q"
_FMT_U64 = "<Q"
_FMT_F32 = "<f"
_FMT_F64 = "<d"
# DateTime: i64 seconds then u32 nanos, 12 bytes (key.rs:396-425).
_FMT_DATETIME = "<qI"

# key.rs:242 -- key_max(DateTime). 9999-12-31T23:59:59.999999999Z.
_DATETIME_MAX_SECONDS = 253402300799
_DATETIME_MAX_NANOS = 999999999


class KeyKind(Enum):
    """The concrete key types the B+tree index can hold. Mirrors
    fcb::KeyKind (key.hpp:14-30).

    STRING20 exists in the format but the writer never emits it
    (Format Reference: "StringKey20 is defined but never produced by
    the writer").
    """

    INT8 = "i8"
    UINT8 = "u8"
    INT16 = "i16"
    UINT16 = "u16"
    INT32 = "i32"
    UINT32 = "u32"
    INT64 = "i64"
    UINT64 = "u64"
    FLOAT32 = "f32"
    FLOAT64 = "f64"
    BOOL = "bool"
    DATETIME = "datetime"
    STRING20 = "string20"
    STRING50 = "string50"
    STRING100 = "string100"


_STRING_KINDS = frozenset(
    {KeyKind.STRING20, KeyKind.STRING50, KeyKind.STRING100}
)


def is_string_kind(kind: KeyKind) -> bool:
    """True for the fixed-width string kinds, whose on-disk keys are
    TRUNCATED and therefore only ever yield query candidates. Mirrors
    fcb::needs_post_filter (reader.cpp:319-322).

    Exported so callers (stree.py's operator lowering and select_attr's
    post-filter) test membership through one predicate instead of
    re-spelling the kind tuple.
    """
    return kind in _STRING_KINDS


_SIZES = {
    KeyKind.INT8: 1,
    KeyKind.UINT8: 1,
    KeyKind.BOOL: 1,
    KeyKind.INT16: 2,
    KeyKind.UINT16: 2,
    KeyKind.INT32: 4,
    KeyKind.UINT32: 4,
    KeyKind.FLOAT32: 4,
    KeyKind.INT64: 8,
    KeyKind.UINT64: 8,
    KeyKind.FLOAT64: 8,
    KeyKind.DATETIME: 12,
    KeyKind.STRING20: 20,
    KeyKind.STRING50: 50,
    KeyKind.STRING100: 100,
}

# The signed/unsigned pairing is the whole point of listing these
# separately: Python ints are arbitrary precision, so reading a u64 with
# "<q" silently yields a negative number past 2**63 rather than wrapping
# visibly. PAYLOAD_TAG (1 << 63) sits exactly on that boundary.
_INT_FORMATS = {
    KeyKind.INT8: _FMT_I8,
    KeyKind.UINT8: _FMT_U8,
    KeyKind.INT16: _FMT_I16,
    KeyKind.UINT16: _FMT_U16,
    KeyKind.INT32: _FMT_I32,
    KeyKind.UINT32: _FMT_U32,
    KeyKind.INT64: _FMT_I64,
    KeyKind.UINT64: _FMT_U64,
}

_SIGNED_KINDS = frozenset(
    {KeyKind.INT8, KeyKind.INT16, KeyKind.INT32, KeyKind.INT64}
)
_UNSIGNED_KINDS = frozenset(
    {KeyKind.UINT8, KeyKind.UINT16, KeyKind.UINT32, KeyKind.UINT64}
)

_INT_RANGES = {
    KeyKind.INT8: (-(2**7), 2**7 - 1),
    KeyKind.UINT8: (0, 2**8 - 1),
    KeyKind.INT16: (-(2**15), 2**15 - 1),
    KeyKind.UINT16: (0, 2**16 - 1),
    KeyKind.INT32: (-(2**31), 2**31 - 1),
    KeyKind.UINT32: (0, 2**32 - 1),
    KeyKind.INT64: (-(2**63), 2**63 - 1),
    KeyKind.UINT64: (0, 2**64 - 1),
}

# key.cpp:326-352 / Format Reference "Column type -> key type", as the
# WRITER actually emits (writer/attr_index.rs:240, :272, :288).
_COLUMN_TYPE_TO_KIND = {
    # DIVERGENCE 1: Byte -> UInt8, deliberately. See
    # column_type_to_key_kind's docstring.
    ColumnType.Byte: KeyKind.UINT8,
    ColumnType.UByte: KeyKind.UINT8,
    ColumnType.Bool: KeyKind.BOOL,
    ColumnType.Short: KeyKind.INT16,
    ColumnType.UShort: KeyKind.UINT16,
    ColumnType.Int: KeyKind.INT32,
    ColumnType.UInt: KeyKind.UINT32,
    ColumnType.Long: KeyKind.INT64,
    ColumnType.ULong: KeyKind.UINT64,
    ColumnType.Float: KeyKind.FLOAT32,
    ColumnType.Double: KeyKind.FLOAT64,
    ColumnType.String: KeyKind.STRING50,
    ColumnType.DateTime: KeyKind.DATETIME,
    ColumnType.Json: KeyKind.STRING100,
    ColumnType.Binary: KeyKind.STRING100,
}


def key_serialized_size(kind: KeyKind) -> int:
    """Serialized width in bytes. Mirrors fcb::key_serialized_size
    (key.cpp:60-79). DateTime is 12: i64 seconds + u32 nanos."""
    try:
        return _SIZES[kind]
    except KeyError:  # pragma: no cover - KeyKind is a closed enum
        raise FcbError(
            ErrorCode.UNSUPPORTED_COLUMN_TYPE, f"unknown key kind: {kind}"
        ) from None


@dataclass(frozen=True)
class KeyValue:
    """A decoded index key. Mirrors fcb::KeyValue (key.hpp:40-74).

    Ordering is NOT bytewise for floats: the on-disk bytes are the plain
    IEEE-754 bit pattern, and ordered_float semantics are applied after
    decoding -- see compare_keys.

    `raw` holds fixed-width string keys as BYTES, untruncated. C++ can
    keep a std::string here because std::string is a byte string;
    Python's str is not, and a key decoded from a buffer cut
    mid-codepoint has no str that round-trips (str(errors="replace")
    turns 0xE2 0x82 into U+FFFD, which re-encodes to three different
    bytes). Comparison and encoding therefore operate on `raw` only;
    `original_string` decodes lazily, for display.
    """

    kind: KeyKind
    i: int = 0  # signed integers, and DateTime seconds
    u: int = 0  # unsigned integers, bool, and DateTime nanos
    f: float = 0.0  # Float32/Float64
    raw: bytes = b""  # fixed-string kinds, untruncated

    # -- constructors, mirroring key.cpp's static factories ------------

    @staticmethod
    def from_i8(v: int) -> KeyValue:
        return _int_key(KeyKind.INT8, v)

    @staticmethod
    def from_u8(v: int) -> KeyValue:
        return _int_key(KeyKind.UINT8, v)

    @staticmethod
    def from_i16(v: int) -> KeyValue:
        return _int_key(KeyKind.INT16, v)

    @staticmethod
    def from_u16(v: int) -> KeyValue:
        return _int_key(KeyKind.UINT16, v)

    @staticmethod
    def from_i32(v: int) -> KeyValue:
        return _int_key(KeyKind.INT32, v)

    @staticmethod
    def from_u32(v: int) -> KeyValue:
        return _int_key(KeyKind.UINT32, v)

    @staticmethod
    def from_i64(v: int) -> KeyValue:
        return _int_key(KeyKind.INT64, v)

    @staticmethod
    def from_u64(v: int) -> KeyValue:
        return _int_key(KeyKind.UINT64, v)

    @staticmethod
    def from_f32(v: float) -> KeyValue:
        # Round-trip through the 4-byte encoding so that comparing a
        # query key against a decoded on-disk key does not fail on the
        # f64->f32 precision the file already lost.
        (narrowed,) = struct.unpack(_FMT_F32, struct.pack(_FMT_F32, v))
        return KeyValue(kind=KeyKind.FLOAT32, f=narrowed)

    @staticmethod
    def from_f64(v: float) -> KeyValue:
        return KeyValue(kind=KeyKind.FLOAT64, f=float(v))

    @staticmethod
    def from_bool(v: bool) -> KeyValue:
        return KeyValue(kind=KeyKind.BOOL, u=1 if v else 0)

    @staticmethod
    def from_datetime(seconds: int, nanos: int) -> KeyValue:
        _check_range(KeyKind.INT64, seconds, "DateTime seconds")
        _check_range(KeyKind.UINT32, nanos, "DateTime nanos")
        return KeyValue(kind=KeyKind.DATETIME, i=seconds, u=nanos)

    @staticmethod
    def from_string(kind: KeyKind, v: str | bytes) -> KeyValue:
        if kind not in _STRING_KINDS:
            raise FcbError(
                ErrorCode.UNSUPPORTED_COLUMN_TYPE,
                f"from_string on a non-string kind: {kind}",
            )
        raw = v.encode("utf-8") if isinstance(v, str) else bytes(v)
        # Kept UNTRUNCATED, so a caller's post-filter can still see the
        # full value the key was built from (key.hpp:60-62).
        return KeyValue(kind=kind, raw=raw)

    @property
    def original_string(self) -> str:
        """The string this key was built from, for display only.

        Decoded with errors="replace": a key read off disk may have been
        truncated mid-codepoint, and there is no lossless str for that.
        Never feed this back through from_string and expect the same
        key -- compare on `raw`.
        """
        return self.raw.decode("utf-8", errors="replace")


def _check_range(kind: KeyKind, v: int, what: str) -> None:
    lo, hi = _INT_RANGES[kind]
    if not (lo <= v <= hi):
        raise FcbError(
            ErrorCode.INVALID_ATTRIBUTE_VALUE,
            f"{what} out of range for {kind.value}: {v}",
        )


def _int_key(kind: KeyKind, v: int) -> KeyValue:
    # Python ints are unbounded, so a caller can hand in a value no
    # column of this width could hold. C++ would silently truncate;
    # rejecting is honest, and catches an f64 passed where a u64 was
    # meant before it reaches the traversal.
    if isinstance(v, bool) or not isinstance(v, int):
        raise FcbError(
            ErrorCode.INVALID_ATTRIBUTE_VALUE,
            f"{kind.value} key requires an int, got {type(v).__name__}",
        )
    _check_range(kind, v, f"{kind.value} key")
    if kind in _SIGNED_KINDS:
        return KeyValue(kind=kind, i=v)
    return KeyValue(kind=kind, u=v)


def encode_key(value: KeyValue) -> bytes:
    """Serialize one key to its on-disk bytes. Mirrors fcb::encode_key
    (key.cpp:138-190).

    Floats are the plain IEEE-754 bit pattern (key.rs:323-370): there is
    NO order-preserving sign-flip transform, and applying one would
    disagree with every file the reference has written.

    Fixed-width strings are copied min(len, N) BYTES and zero-padded.
    Truncation is silent and does NOT respect UTF-8 boundaries
    (key.rs:483-489), so two distinct strings sharing an N-byte prefix
    encode identically -- which is why a caller must post-filter.
    """
    kind = value.kind
    n = key_serialized_size(kind)

    if kind in _INT_FORMATS:
        raw = value.i if kind in _SIGNED_KINDS else value.u
        return struct.pack(_INT_FORMATS[kind], raw)
    if kind is KeyKind.BOOL:
        return b"\x01" if value.u != 0 else b"\x00"
    if kind is KeyKind.FLOAT32:
        return struct.pack(_FMT_F32, value.f)
    if kind is KeyKind.FLOAT64:
        return struct.pack(_FMT_F64, value.f)
    if kind is KeyKind.DATETIME:
        return struct.pack(_FMT_DATETIME, value.i, value.u)
    if kind in _STRING_KINDS:
        return value.raw[:n].ljust(n, b"\x00")
    raise FcbError(  # pragma: no cover - KeyKind is a closed enum
        ErrorCode.UNSUPPORTED_COLUMN_TYPE, f"unknown key kind: {kind}"
    )


def decode_key(kind: KeyKind, b: bytes) -> KeyValue:
    """Decode one key from `b`'s first key_serialized_size(kind) bytes.
    Mirrors fcb::decode_key (key.cpp:192-234)."""
    n = key_serialized_size(kind)
    if len(b) < n:
        raise FcbError(
            ErrorCode.INVALID_ATTRIBUTE_VALUE,
            f"short key buffer for {kind.value}: {len(b)} < {n}",
        )

    if kind in _INT_FORMATS:
        (v,) = struct.unpack_from(_INT_FORMATS[kind], b, 0)
        return (
            KeyValue(kind=kind, i=v)
            if kind in _SIGNED_KINDS
            else KeyValue(kind=kind, u=v)
        )
    if kind is KeyKind.BOOL:
        return KeyValue.from_bool(b[0] != 0)
    if kind is KeyKind.FLOAT32:
        (v,) = struct.unpack_from(_FMT_F32, b, 0)
        return KeyValue(kind=KeyKind.FLOAT32, f=v)
    if kind is KeyKind.FLOAT64:
        (v,) = struct.unpack_from(_FMT_F64, b, 0)
        return KeyValue(kind=KeyKind.FLOAT64, f=v)
    if kind is KeyKind.DATETIME:
        seconds, nanos = struct.unpack_from(_FMT_DATETIME, b, 0)
        return KeyValue(kind=KeyKind.DATETIME, i=seconds, u=nanos)
    if kind in _STRING_KINDS:
        # Stop at the first NUL, as to_string_lossy does (key.rs:511).
        body = bytes(b[:n])
        nul = body.find(b"\x00")
        if nul >= 0:
            body = body[:nul]
        return KeyValue(kind=kind, raw=body)
    raise FcbError(  # pragma: no cover - KeyKind is a closed enum
        ErrorCode.UNSUPPORTED_COLUMN_TYPE, f"unknown key kind: {kind}"
    )


def _cmp_ordered_float(a: float, b: float) -> int:
    """The `ordered_float` total order, which Python's `<` is not.

    NaN equals itself and sorts above everything, including +inf;
    -0.0 == +0.0. Mirrors cmp_ordered_double (key.cpp:40-49).

    Python differs from C++ in BOTH directions here: `float('nan') ==
    float('nan')` is False, and `sorted()` places NaN wherever the
    comparison happens to leave it, producing a non-total order that
    would break the tree's ordering invariants. Hence the explicit
    comparator; do not replace it with `<`.
    """
    na = math.isnan(a)
    nb = math.isnan(b)
    if na and nb:
        return 0
    if na:
        return 1
    if nb:
        return -1
    if a < b:
        return -1
    if a > b:
        return 1
    return 0  # also covers -0.0 == +0.0


def _cmp(a: int, b: int) -> int:
    return -1 if a < b else (1 if a > b else 0)


def compare_keys(a: KeyValue, b: KeyValue) -> int:
    """Three-way comparison. Mirrors fcb::compare_keys (key.cpp:236-274).

    Raises rather than inventing an ordering between unrelated types: a
    silently wrong comparison inside the traversal returns
    plausible-looking wrong answers.
    """
    if a.kind is not b.kind:
        raise FcbError(
            ErrorCode.UNSUPPORTED_COLUMN_TYPE,
            f"comparing keys of different kinds: {a.kind} vs {b.kind}",
        )
    kind = a.kind
    if kind in _SIGNED_KINDS:
        return _cmp(a.i, b.i)
    if kind in _UNSIGNED_KINDS or kind is KeyKind.BOOL:
        return _cmp(a.u, b.u)
    if kind is KeyKind.FLOAT32 or kind is KeyKind.FLOAT64:
        return _cmp_ordered_float(a.f, b.f)
    if kind is KeyKind.DATETIME:
        if a.i != b.i:
            return _cmp(a.i, b.i)
        return _cmp(a.u, b.u)
    if kind in _STRING_KINDS:
        # Compare the ENCODED (truncated, zero-padded) forms, because
        # that is what the tree stores and orders by. bytes comparison,
        # never str -- gotcha 2 in the task brief.
        ea = encode_key(a)
        eb = encode_key(b)
        return -1 if ea < eb else (1 if ea > eb else 0)
    raise FcbError(  # pragma: no cover - KeyKind is a closed enum
        ErrorCode.UNSUPPORTED_COLUMN_TYPE, f"unknown key kind: {kind}"
    )


def key_min(kind: KeyKind) -> KeyValue:
    """Lower sentinel for open-ended range queries. Mirrors
    fcb::key_min (key.cpp:276-299).

    DIVERGENCE 4: the DateTime minimum is epoch 0 (key.rs:242), NOT the
    true i64 minimum, even though the wire format stores a signed i64
    and permits negative seconds. Pre-1970 timestamps are therefore
    invisible to Le/Ne range queries, in every implementation.
    Reproduced deliberately so results match.
    """
    if kind in _SIGNED_KINDS or kind in _UNSIGNED_KINDS:
        return _int_key(kind, _INT_RANGES[kind][0])
    if kind is KeyKind.FLOAT32:
        return KeyValue.from_f32(float("-inf"))
    if kind is KeyKind.FLOAT64:
        return KeyValue.from_f64(float("-inf"))
    if kind is KeyKind.BOOL:
        return KeyValue.from_bool(False)
    if kind is KeyKind.DATETIME:
        return KeyValue.from_datetime(0, 0)
    if kind in _STRING_KINDS:
        return KeyValue.from_string(kind, b"")
    raise FcbError(  # pragma: no cover - KeyKind is a closed enum
        ErrorCode.UNSUPPORTED_COLUMN_TYPE, f"unknown key kind: {kind}"
    )


def key_max(kind: KeyKind) -> KeyValue:
    """Upper sentinel for open-ended range queries. Mirrors
    fcb::key_max (key.cpp:301-324).

    DIVERGENCE 3: the float maximum is +inf (key.rs:139), but NaN sorts
    ABOVE +inf in the ordered_float total order -- so range-lowered
    operators (Ge, Ne) silently EXCLUDE NaN-keyed features. Reproduced
    deliberately so results match the reference.
    """
    if kind in _SIGNED_KINDS or kind in _UNSIGNED_KINDS:
        return _int_key(kind, _INT_RANGES[kind][1])
    if kind is KeyKind.FLOAT32:
        return KeyValue.from_f32(float("inf"))
    if kind is KeyKind.FLOAT64:
        return KeyValue.from_f64(float("inf"))
    if kind is KeyKind.BOOL:
        return KeyValue.from_bool(True)
    if kind is KeyKind.DATETIME:
        return KeyValue.from_datetime(
            _DATETIME_MAX_SECONDS, _DATETIME_MAX_NANOS
        )
    if kind in _STRING_KINDS:
        return KeyValue.from_string(kind, b"\xff" * key_serialized_size(kind))
    raise FcbError(  # pragma: no cover - KeyKind is a closed enum
        ErrorCode.UNSUPPORTED_COLUMN_TYPE, f"unknown key kind: {kind}"
    )


def column_type_to_key_kind(column_type: int) -> KeyKind:
    """Column type to key kind, following what the WRITER emits.
    Mirrors fcb::key_kind_for_column (key.cpp:326-352).

    Takes the raw ubyte (ColumnInfo.type carries exactly this value), so
    callers never see the generated FlatBuffers API.

    DIVERGENCE 1: Byte maps to UINT8, not INT8. The writer stores Byte
    as u8 (writer/attribute.rs:209) and builds its index as
    MemoryIndex<u8> (writer/attr_index.rs:240), but Rust's READER
    decodes that index as i8 (reader/attr_query.rs:118) -- so for stored
    values above 127 it returns a negative number that was never
    written. Matching the writer decodes files correctly, at the cost of
    disagreeing with the Rust reader on those values.
    """
    try:
        return _COLUMN_TYPE_TO_KIND[column_type]
    except KeyError:
        raise FcbError(
            ErrorCode.UNSUPPORTED_COLUMN_TYPE,
            f"unknown column type: {column_type}",
        ) from None
