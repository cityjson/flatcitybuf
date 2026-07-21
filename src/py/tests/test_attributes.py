from __future__ import annotations

import json
import struct
from pathlib import Path

import pytest
from flatcitybuf.attribute import decode_attributes
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.feature import CityObjectView
from flatcitybuf.generated.header_generated import ColumnType
from flatcitybuf.header import ColumnInfo
from flatcitybuf.reader import FcbReader

CORPUS = Path(__file__).resolve().parents[3] / "conformance"
DATA = Path(__file__).resolve().parents[3] / "examples" / "data"

# Expected values below come from two kinds of oracle:
#
# 1. conformance/*.expected.jsonl -- emitted by the Rust reader/CLI
#    (scripts/gen_conformance.sh), an oracle independent of this
#    package. inferable_types.fcb's single CityObject exercises every
#    scalar ColumnType this decoder handles in one shot: a_bool=True,
#    a_double=1.5, a_long=-42, a_ulong=42, a_string="hello",
#    a_json={"nested": [1, 2, 3]}.
# 2. Hand-built bytes for the two schema-desync tests below -- these are
#    NOT derived from any fixture; they are deliberately constructed to
#    pin exact byte-level behavior and are labeled as such.


def _schema_for(
    obj: CityObjectView, header_columns: list[ColumnInfo]
) -> list[ColumnInfo]:
    # The per-object resolution rule under test: CityObject.columns
    # overrides Header.columns whenever the object declares its own.
    if obj.has_columns:
        assert obj.columns is not None
        return obj.columns
    return header_columns


# --------------------------------------------------------- the brief ---


def test_every_object_with_attributes_uses_its_own_schema() -> None:
    r = FcbReader.open_file(DATA / "delft.fcb")
    checked_any = False
    for feature in r.select_all():
        for obj in feature.city_objects():
            if obj.has_attributes and obj.has_columns:
                assert obj.columns is not None
                assert obj.attributes is not None
                # A wrong schema shows up as a nonsense key, not an
                # exception.
                for key in decode_attributes(obj.attributes, obj.columns):
                    assert key.isprintable()
                checked_any = True
    # Format Reference / task brief: delft.fcb has 1115 such objects --
    # if this were 0 the test above would pass vacuously.
    assert checked_any


# ------------------------------------------------- empty blob ---------


def test_decode_attributes_of_an_empty_blob_is_an_empty_dict() -> None:
    assert decode_attributes(b"", []) == {}


# ------------------------------------------- cross-type oracle --------


def test_decode_attributes_matches_the_rust_oracle_for_every_scalar_type() -> (
    None
):
    r = FcbReader.open_file(CORPUS / "inferable_types.fcb")
    (feature,) = list(r.select_all())
    (obj,) = feature.city_objects()
    schema = _schema_for(obj, r.header.info.columns)
    assert obj.attributes is not None
    decoded = decode_attributes(obj.attributes, schema)

    assert decoded["a_bool"] is True
    assert decoded["a_double"] == 1.5
    assert decoded["a_long"] == -42
    assert decoded["a_ulong"] == 42
    assert decoded["a_string"] == "hello"
    # Json columns are kept as raw text at this layer (Task 8 re-parses
    # them when emitting CityJSON); round-tripping through json.loads
    # proves the bytes decoded correctly without duplicating that logic.
    assert json.loads(decoded["a_json"]) == {"nested": [1, 2, 3]}


def test_object_without_its_own_columns_falls_back_to_header_columns() -> None:
    # noise_extension.fcb: object "1234" has a non-empty attributes blob
    # but declares no columns of its own -- the fallback half of the
    # per-object resolution rule.
    r = FcbReader.open_file(CORPUS / "noise_extension.fcb")
    (feature,) = [f for f in r.select_all() if f.id == "1234"]
    (obj,) = feature.city_objects()
    assert obj.has_attributes is True
    assert obj.has_columns is False
    assert obj.attributes is not None

    decoded = decode_attributes(obj.attributes, r.header.info.columns)
    assert decoded["class"] == "22"
    assert decoded["roofType"] == "pointy"
    assert json.loads(decoded["+noise-buildingLNightMax"]) == {
        "uom": "dB",
        "value": 43.123,
    }


# ------------------------------------------------------- trap #2 ------


def test_present_but_empty_attributes_differ_from_absent() -> None:
    # small.fcb: the BuildingPart child ("-0" suffix) declares an
    # attributes vector with zero elements -- present, decodes to {} --
    # while its Building parent carries the real (non-empty) blob. Read
    # directly off the raw fixture with the generated bindings (see the
    # task report).
    r = FcbReader.open_file(CORPUS / "small.fcb")
    feature = next(iter(r.select_all()))
    by_id = {o.id: o for o in feature.city_objects()}
    child = by_id["NL.IMBAG.Pand.0503100000016459-0"]
    assert child.has_attributes is True
    assert child.attributes == b""
    assert decode_attributes(child.attributes, []) == {}

    parent = by_id["NL.IMBAG.Pand.0503100000016459"]
    assert parent.has_attributes is True
    assert parent.attributes != b""

    # empty_appearance.fcb: attributes are wholly ABSENT (no vector at
    # all) -- the other half of the distinction.
    r2 = FcbReader.open_file(CORPUS / "empty_appearance.fcb")
    absent_feature = next(iter(r2.select_all()))
    (absent_obj,) = absent_feature.city_objects()
    assert absent_obj.has_attributes is False
    assert absent_obj.attributes is None


# ------------------------------------------ signed vs unsigned width ---


def test_ulong_decodes_unsigned_past_the_i64_boundary() -> None:
    # Codex review (Task 12): the oracle-backed ULong test above only
    # ever exercises the value 42, which round-trips identically whether
    # attribute.py's format table used "<Q" (correct) or "<q" -- a
    # regression there would pass unnoticed. 0x8000000000000000 does
    # not: it is exactly 2**63, negative if ever decoded signed.
    blob = struct.pack("<HQ", 0, 0x8000000000000000)
    schema = [
        ColumnInfo(index=0, name="a", type=ColumnType.ULong, nullable=True)
    ]
    decoded = decode_attributes(blob, schema)
    assert decoded["a"] == 9223372036854775808
    assert decoded["a"] > 0


# --------------------------------------------- schema desync (trap) ---


def test_wrong_schema_same_width_produces_a_silently_wrong_value() -> None:
    # Hand-built bytes, not derived from any fixture. A same-width type
    # mismatch (schema says UInt, but the WRONG schema claims Int) does
    # NOT desynchronise the blob -- decoding "succeeds" with a
    # plausible-looking but wrong value. This is the brief's warning
    # made concrete: a wrong schema does not fail loudly.
    blob = struct.pack("<HI", 0, 0xFFFFFFFE)
    correct_schema = [
        ColumnInfo(index=0, name="a", type=ColumnType.UInt, nullable=True)
    ]
    assert decode_attributes(blob, correct_schema) == {"a": 4294967294}

    wrong_schema = [
        ColumnInfo(index=0, name="a", type=ColumnType.Int, nullable=True)
    ]
    assert decode_attributes(blob, wrong_schema) == {"a": -2}


def test_wrong_schema_different_width_desynchronises_and_raises() -> None:
    # Hand-built bytes. Correct encoding: column 0 (Short, 2 bytes) = 7,
    # then column 1 (Long, 8 bytes) = 1234.
    blob = struct.pack("<HhHq", 0, 7, 1, 1234)
    correct_schema = [
        ColumnInfo(index=0, name="a", type=ColumnType.Short, nullable=True),
        ColumnInfo(index=1, name="b", type=ColumnType.Long, nullable=True),
    ]
    assert decode_attributes(blob, correct_schema) == {"a": 7, "b": 1234}

    # Wrong schema swaps which index is Short vs Long: reading column 0
    # as an 8-byte Long consumes bytes that belonged to column 1's index
    # tag and value, desynchronising everything after -- caught as a
    # truncated/unknown record rather than silently accepted.
    wrong_schema = [
        ColumnInfo(index=0, name="a", type=ColumnType.Long, nullable=True),
        ColumnInfo(index=1, name="b", type=ColumnType.Short, nullable=True),
    ]
    with pytest.raises(FcbError) as exc_info:
        decode_attributes(blob, wrong_schema)
    assert exc_info.value.code is ErrorCode.INVALID_ATTRIBUTE_VALUE


# --------------------------------------------------- error handling ---


def test_decode_attributes_rejects_an_unknown_column_index() -> None:
    blob = struct.pack("<HI", 99, 42)
    with pytest.raises(FcbError) as exc_info:
        decode_attributes(blob, [])
    assert exc_info.value.code is ErrorCode.INVALID_ATTRIBUTE_VALUE


def test_decode_attributes_rejects_a_truncated_record() -> None:
    # Column declared as Int (4 bytes) but only 2 bytes of value follow.
    blob = struct.pack("<H", 0) + b"\x01\x02"
    schema = [
        ColumnInfo(index=0, name="a", type=ColumnType.Int, nullable=True)
    ]
    with pytest.raises(FcbError) as exc_info:
        decode_attributes(blob, schema)
    assert exc_info.value.code is ErrorCode.INVALID_ATTRIBUTE_VALUE


def test_decode_attributes_rejects_byte_ubyte_and_binary() -> None:
    # attribute.cpp:145-157 -- the writer can emit these, but the
    # reference reader rejects them rather than guess a width.
    for bad_type in (ColumnType.Byte, ColumnType.UByte, ColumnType.Binary):
        blob = struct.pack("<HB", 0, 7)
        schema = [ColumnInfo(index=0, name="a", type=bad_type, nullable=True)]
        with pytest.raises(FcbError) as exc_info:
            decode_attributes(blob, schema)
        assert exc_info.value.code is ErrorCode.UNSUPPORTED_COLUMN_TYPE
