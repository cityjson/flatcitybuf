from __future__ import annotations

import functools
import math

import pytest
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.keys import (
    KeyKind,
    KeyValue,
    column_type_to_key_kind,
    compare_keys,
    decode_key,
    encode_key,
    key_min,
    key_max,
    key_serialized_size,
)

# Every expected value below is the C++ reader's, which was itself
# ported from Rust: src/cpp/tests/test_keys.cpp asserts the same bytes
# and the same orderings, and src/cpp/src/key.cpp cites the originating
# Rust line for each rule. Nothing here is hand-derived from the spec.
#
# Shorthands matching the plan's Task 10 snippet
# (docs/superpowers/plans/2026-07-20-native-python-core.md:512-523).
F64 = KeyValue.from_f64
F32 = KeyValue.from_f32


def Str(s: str) -> KeyValue:
    return KeyValue.from_string(KeyKind.STRING50, s)


# ------------------------------------- sizes: Format Reference table ---


def test_serialized_sizes_match_the_rust_key_encoders() -> None:
    # Format Reference -> "Attribute B+tree" -> Key encodings table
    # (plan 2026-07-19-native-cpp-core.md:155-163); test_keys.cpp:11-27.
    assert key_serialized_size(KeyKind.INT8) == 1
    assert key_serialized_size(KeyKind.UINT8) == 1
    assert key_serialized_size(KeyKind.BOOL) == 1
    assert key_serialized_size(KeyKind.INT16) == 2
    assert key_serialized_size(KeyKind.UINT16) == 2
    assert key_serialized_size(KeyKind.INT32) == 4
    assert key_serialized_size(KeyKind.UINT32) == 4
    assert key_serialized_size(KeyKind.FLOAT32) == 4
    assert key_serialized_size(KeyKind.INT64) == 8
    assert key_serialized_size(KeyKind.UINT64) == 8
    assert key_serialized_size(KeyKind.FLOAT64) == 8
    assert key_serialized_size(KeyKind.STRING20) == 20
    assert key_serialized_size(KeyKind.STRING50) == 50
    assert key_serialized_size(KeyKind.STRING100) == 100


def test_datetime_is_twelve_bytes() -> None:
    # i64 seconds + u32 nanos, key.rs:396-425. The brief calls this out
    # separately because 8 or 16 are the plausible wrong answers.
    assert key_serialized_size(KeyKind.DATETIME) == 12
    assert len(encode_key(KeyValue.from_datetime(0, 0))) == 12


# ------------------------------------------ encoding 1: raw byte ints ---


def test_int8_and_uint8_are_a_single_raw_byte() -> None:
    # key.rs:284-314.
    assert encode_key(KeyValue.from_i8(-2)) == b"\xfe"
    assert encode_key(KeyValue.from_u8(200)) == b"\xc8"


# ---------------------------- encoding 2: LE two's complement integers ---


def test_integers_round_trip_as_little_endian_twos_complement() -> None:
    # key.rs:260-280; test_keys.cpp:29-45.
    v = KeyValue.from_i32(-2)
    assert encode_key(v) == b"\xfe\xff\xff\xff"
    assert compare_keys(decode_key(KeyKind.INT32, encode_key(v)), v) == 0

    u = KeyValue.from_u64(0xDEADBEEFCAFE)
    assert encode_key(u) == b"\xfe\xca\xef\xbe\xad\xde\x00\x00"
    assert compare_keys(decode_key(KeyKind.UINT64, encode_key(u)), u) == 0

    assert encode_key(KeyValue.from_i16(-1)) == b"\xff\xff"
    assert encode_key(KeyValue.from_u16(0x0102)) == b"\x02\x01"
    assert encode_key(KeyValue.from_u32(0x01020304)) == b"\x04\x03\x02\x01"
    assert encode_key(KeyValue.from_i64(-1)) == b"\xff" * 8


def test_u64_above_2_63_stays_positive() -> None:
    # Gotcha 3 from the brief: decoding a u64 with "<q" would send this
    # negative and index the tree backwards. PAYLOAD_TAG is exactly this
    # value, so the failure would be immediate.
    v = decode_key(KeyKind.UINT64, b"\x00" * 7 + b"\x80")
    assert compare_keys(v, KeyValue.from_u64(1 << 63)) == 0
    assert compare_keys(v, KeyValue.from_u64(0)) > 0


# ----------------------------------- encoding 3/4: raw IEEE-754 floats ---


def test_floats_are_raw_ieee754_le_bits_with_no_order_transform() -> None:
    # key.rs:347-370; test_keys.cpp:47-58. NO sign-flip total-order
    # trick -- that would disagree with every file the reference wrote.
    b = encode_key(F64(1.0))
    assert b == b"\x00\x00\x00\x00\x00\x00\xf0\x3f"
    assert encode_key(F32(1.0)) == b"\x00\x00\x80\x3f"
    assert compare_keys(decode_key(KeyKind.FLOAT64, b), F64(1.0)) == 0


def test_float_ordering_is_ordered_float_not_python() -> None:
    # The plan's own Task 10 snippet, verbatim in intent. Python says
    # nan != nan and sorts it arbitrarily; ordered_float says NaN equals
    # itself and sorts greatest (key.rs:139, plan line 165).
    nan = float("nan")
    assert compare_keys(F64(nan), F64(nan)) == 0
    assert compare_keys(F64(nan), F64(float("inf"))) > 0
    assert compare_keys(F64(-0.0), F64(0.0)) == 0

    # And the ordinary directions, so the comparator is not just
    # "everything equal": test_keys.cpp:60-75.
    assert compare_keys(F64(float("inf")), F64(nan)) < 0
    assert compare_keys(F64(-1.0), F64(1.0)) < 0
    assert compare_keys(F64(float("-inf")), F64(-1.0)) < 0
    assert compare_keys(F64(1.0), F64(float("inf"))) < 0


def test_nan_would_break_a_naive_python_sort() -> None:
    # Guards against a future "just use sorted()" simplification: the
    # explicit comparator must impose a TOTAL order, which Python's `<`
    # does not once NaN is present.
    nan = float("nan")
    vals = [F64(nan), F64(1.0), F64(float("-inf")), F64(float("inf"))]
    ordered = sorted(vals, key=functools.cmp_to_key(compare_keys))
    assert ordered[0].f == float("-inf")
    assert ordered[1].f == 1.0
    assert ordered[2].f == float("inf")
    assert math.isnan(ordered[3].f)


# ----------------------------------------------- encoding 5: booleans ---


def test_bool_encodes_as_zero_or_one_and_decodes_as_nonzero() -> None:
    # key.rs:373-393.
    assert encode_key(KeyValue.from_bool(True)) == b"\x01"
    assert encode_key(KeyValue.from_bool(False)) == b"\x00"
    assert (
        compare_keys(
            decode_key(KeyKind.BOOL, b"\x07"), KeyValue.from_bool(True)
        )
        == 0
    )
    assert (
        compare_keys(KeyValue.from_bool(False), KeyValue.from_bool(True)) < 0
    )


# ----------------------------------------------- encoding 6: DateTime ---


def test_datetime_is_i64_seconds_then_u32_nanos_both_le() -> None:
    # key.rs:396-425; test_keys.cpp:132-139.
    b = encode_key(KeyValue.from_datetime(1, 2))
    assert b == b"\x01" + b"\x00" * 7 + b"\x02\x00\x00\x00"
    assert (
        compare_keys(
            decode_key(KeyKind.DATETIME, b), KeyValue.from_datetime(1, 2)
        )
        == 0
    )


def test_datetime_ordering_handles_negative_seconds() -> None:
    # The wire field is a SIGNED i64, so pre-1970 encodes fine even
    # though key_min is epoch 0 (divergence 4). test_keys.cpp:141-146.
    assert encode_key(KeyValue.from_datetime(-1, 0))[:8] == b"\xff" * 8
    assert (
        compare_keys(
            KeyValue.from_datetime(-100, 0), KeyValue.from_datetime(100, 0)
        )
        < 0
    )
    # Nanos are the tiebreak, not the primary key.
    assert (
        compare_keys(
            KeyValue.from_datetime(5, 1), KeyValue.from_datetime(5, 2)
        )
        < 0
    )


# ------------------------------------ encoding 7: fixed-width strings ---


def test_string_keys_compare_as_bytes_and_truncate_at_50() -> None:
    # The plan's own Task 10 snippet: 40 x U+00E9 is 80 UTF-8 bytes, so
    # the 50-byte cut lands mid-codepoint.
    long = "é" * 40
    assert len(long.encode("utf-8")) == 80
    assert len(encode_key(Str(long))) == 50


def test_truncation_splits_multibyte_utf8_without_complaint() -> None:
    # key.rs:483-489 copies min(len, N) BYTES. test_keys.cpp:91-103:
    # 18 'a' + 11 EURO SIGNs puts a continuation byte at index 49.
    s = "a" * 18 + "€" * 11
    b = encode_key(Str(s))
    assert len(b) == 50
    assert b[49] & 0xC0 == 0x80


def test_short_strings_zero_pad_to_the_full_width() -> None:
    # test_keys.cpp:77-89.
    b = encode_key(Str("abc"))
    assert b == b"abc" + b"\x00" * 47
    assert encode_key(Str("x" * 60)) == b"x" * 50


def test_strings_sharing_an_n_byte_prefix_collide() -> None:
    # WHY a caller must post-filter: the index cannot tell these apart.
    # test_keys.cpp:105-112.
    assert compare_keys(Str("y" * 50 + "AAA"), Str("y" * 50 + "BBB")) == 0


def test_a_string_with_an_embedded_nul_collides_with_its_prefix() -> None:
    # test_keys.cpp:114-121: zero padding makes "a" and "a\0" identical
    # on disk, so post-filtering cannot be gated on the query length.
    assert compare_keys(Str("a"), Str("a\x00")) == 0


def test_string_comparison_is_bytewise_not_unicode() -> None:
    # Gotcha 2: compare the raw bytes, never the str. U+00E9 is
    # 0xC3 0xA9 in UTF-8, so it sorts AFTER "z" (0x7A) bytewise even
    # though "é" < "z" is False in Python's str order too -- the
    # decisive case is a decoded key whose truncation left invalid
    # UTF-8, which no str can round-trip.
    assert compare_keys(Str("é"), Str("z")) > 0

    # A key decoded from a buffer that was cut mid-codepoint must
    # re-encode to the SAME bytes. Storing it as a str and decoding
    # with errors="replace" would turn 0xE2 0x82 into U+FFFD and
    # re-encode as 0xEF 0xBF 0xBD -- silently a different key.
    raw = b"a" * 48 + b"\xe2\x82"
    k = decode_key(KeyKind.STRING50, raw)
    assert encode_key(k) == raw
    assert k.original_string.endswith("�")


def test_string_sentinels_are_all_ff_and_all_zero() -> None:
    # test_keys.cpp:123-130.
    assert encode_key(key_max(KeyKind.STRING50)) == b"\xff" * 50
    assert encode_key(key_min(KeyKind.STRING50)) == b"\x00" * 50


# ----------------------------------------------- sentinels/divergences ---


def test_numeric_sentinels_match_the_reference() -> None:
    assert (
        compare_keys(key_min(KeyKind.INT32), KeyValue.from_i32(-(2**31))) == 0
    )
    assert (
        compare_keys(key_max(KeyKind.INT32), KeyValue.from_i32(2**31 - 1)) == 0
    )
    assert compare_keys(key_min(KeyKind.UINT64), KeyValue.from_u64(0)) == 0
    assert (
        compare_keys(key_max(KeyKind.UINT64), KeyValue.from_u64(2**64 - 1))
        == 0
    )
    assert compare_keys(key_min(KeyKind.BOOL), KeyValue.from_bool(False)) == 0
    assert compare_keys(key_max(KeyKind.BOOL), KeyValue.from_bool(True)) == 0


def test_divergence_3_float_max_is_inf_so_nan_is_invisible() -> None:
    # key.rs:139 / key.cpp:311-315. NaN sorts ABOVE +inf, so a
    # range-lowered operator bounded by key_max never reaches it.
    assert key_max(KeyKind.FLOAT64).f == float("inf")
    assert key_min(KeyKind.FLOAT64).f == float("-inf")
    assert compare_keys(F64(float("nan")), key_max(KeyKind.FLOAT64)) > 0


def test_divergence_4_datetime_min_is_epoch_zero() -> None:
    # key.rs:242 / key.cpp:289-292: NOT the true i64 minimum, so
    # pre-1970 timestamps fall outside every Le/Ne range.
    assert (
        compare_keys(key_min(KeyKind.DATETIME), KeyValue.from_datetime(0, 0))
        == 0
    )
    assert (
        compare_keys(KeyValue.from_datetime(-1, 0), key_min(KeyKind.DATETIME))
        < 0
    )
    assert (
        compare_keys(
            key_max(KeyKind.DATETIME),
            KeyValue.from_datetime(253402300799, 999999999),
        )
        == 0
    )


# ---------------------------------------------- column type -> keykind ---


def test_column_type_maps_to_the_key_kind_the_writer_produces() -> None:
    # header.fbs ColumnType declaration order, as raw ubytes, so this
    # does not depend on the generated API. test_keys.cpp:148-170.
    byte, ubyte, boolean, short, ushort, int_, uint = 0, 1, 2, 3, 4, 5, 6
    long_, ulong, float_, double, string, json, datetime, binary = (
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14,
    )
    assert column_type_to_key_kind(boolean) == KeyKind.BOOL
    assert column_type_to_key_kind(short) == KeyKind.INT16
    assert column_type_to_key_kind(ushort) == KeyKind.UINT16
    assert column_type_to_key_kind(int_) == KeyKind.INT32
    assert column_type_to_key_kind(uint) == KeyKind.UINT32
    assert column_type_to_key_kind(long_) == KeyKind.INT64
    assert column_type_to_key_kind(ulong) == KeyKind.UINT64
    assert column_type_to_key_kind(float_) == KeyKind.FLOAT32
    assert column_type_to_key_kind(double) == KeyKind.FLOAT64
    assert column_type_to_key_kind(string) == KeyKind.STRING50
    assert column_type_to_key_kind(datetime) == KeyKind.DATETIME
    assert column_type_to_key_kind(json) == KeyKind.STRING100
    assert column_type_to_key_kind(binary) == KeyKind.STRING100

    # Divergence 1: Byte -> u8, matching the WRITER
    # (writer/attribute.rs:209, writer/attr_index.rs:240), not Rust's
    # reader (reader/attr_query.rs:118, which decodes i8).
    assert column_type_to_key_kind(byte) == KeyKind.UINT8
    assert column_type_to_key_kind(ubyte) == KeyKind.UINT8


def test_divergence_1_a_byte_above_127_decodes_unsigned() -> None:
    # test_keys.cpp:172-177. Rust's reader would return -56 here.
    v = decode_key(KeyKind.UINT8, b"\xc8")
    assert compare_keys(v, KeyValue.from_u8(200)) == 0
    assert compare_keys(v, KeyValue.from_u8(0)) > 0


def test_unknown_column_type_is_rejected() -> None:
    with pytest.raises(FcbError) as exc:
        column_type_to_key_kind(99)
    assert exc.value.code == ErrorCode.UNSUPPORTED_COLUMN_TYPE


# ------------------------------------------------------------- errors ---


def test_decode_rejects_a_buffer_shorter_than_the_key() -> None:
    # test_keys.cpp:179-182.
    with pytest.raises(FcbError) as exc:
        decode_key(KeyKind.INT64, b"\x01\x02")
    assert exc.value.code == ErrorCode.INVALID_ATTRIBUTE_VALUE


def test_comparing_different_kinds_is_rejected() -> None:
    # test_keys.cpp:184-186: inventing an ordering between unrelated
    # types would silently corrupt a traversal.
    with pytest.raises(FcbError) as exc:
        compare_keys(KeyValue.from_i32(1), F64(1.0))
    assert exc.value.code == ErrorCode.UNSUPPORTED_COLUMN_TYPE


def test_from_string_on_a_non_string_kind_is_rejected() -> None:
    with pytest.raises(FcbError) as exc:
        KeyValue.from_string(KeyKind.INT32, "nope")
    assert exc.value.code == ErrorCode.UNSUPPORTED_COLUMN_TYPE
