from __future__ import annotations

import struct
from pathlib import Path

import pytest
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.feature import CityObjectView, Feature
from flatcitybuf.header import read_header
from flatcitybuf.range_reader import FileRangeReader
from flatcitybuf.reader import FcbReader

CORPUS = Path(__file__).resolve().parents[3] / "conformance"

# Expected values below come from two oracles, cross-checked:
#
# 1. conformance/*.expected.jsonl -- emitted by the Rust reader/CLI
#    (scripts/gen_conformance.sh). Feature id ORDER in select_all() must
#    match this file's CityJSONFeature order, since both walk the same
#    on-disk (Hilbert) order.
# 2. Direct reads with the committed flatc-generated bindings against
#    the raw fixture bytes (same technique as test_header.py), used for
#    facts .expected.jsonl doesn't carry (e.g. CityObject ids/types) --
#    see the task report for exactly which script produced which value.


# --------------------------------------------------------- the brief ---


def test_single_feature_file_iterates_exactly_once() -> None:
    r = FcbReader.open_file(CORPUS / "single_feature.fcb")
    assert r.header.info.features_count == 1
    assert len(list(r.select_all())) == 1


# ------------------------------------------------------- basic shape ---


def test_open_file_exposes_the_header() -> None:
    r = FcbReader.open_file(CORPUS / "small.fcb")
    assert r.header.info.features_count == 3


def test_select_all_yields_features_in_file_order_matching_the_oracle() -> (
    None
):
    r = FcbReader.open_file(CORPUS / "small.fcb")
    ids = [f.id for f in r.select_all()]
    assert ids == [
        "NL.IMBAG.Pand.0503100000016459",
        "NL.IMBAG.Pand.0503100000005156",
        "NL.IMBAG.Pand.0503100000012869",
    ]


def test_single_feature_id_and_vertices() -> None:
    r = FcbReader.open_file(CORPUS / "single_feature.fcb")
    (feature,) = list(r.select_all())
    assert isinstance(feature, Feature)
    assert feature.id == "only"
    # single_feature.expected.jsonl: 4 vertices of a unit square.
    assert feature.vertices() == [
        (0, 0, 0),
        (1000, 0, 0),
        (1000, 1000, 0),
        (0, 1000, 0),
    ]


def test_byte_offset_increases_monotonically_and_starts_at_zero() -> None:
    r = FcbReader.open_file(CORPUS / "small.fcb")
    offsets = [f.byte_offset for f in r.select_all()]
    assert offsets == sorted(offsets)
    assert len(set(offsets)) == len(offsets)
    assert offsets[0] == 0


# ----------------------------------------------------- city objects ---


def test_city_objects_shape_for_small_first_feature() -> None:
    r = FcbReader.open_file(CORPUS / "small.fcb")
    feature = next(iter(r.select_all()))
    objects = feature.city_objects()
    assert len(objects) == 2
    assert all(isinstance(o, CityObjectView) for o in objects)
    ids = {o.id for o in objects}
    assert ids == {
        "NL.IMBAG.Pand.0503100000016459-0",
        "NL.IMBAG.Pand.0503100000016459",
    }


def test_city_object_type_is_the_raw_ubyte_enum_value() -> None:
    # CityObjectType: Building=6, BuildingPart=7 (feature.fbs:10-53).
    # Read directly with the generated bindings against small.fcb.
    r = FcbReader.open_file(CORPUS / "small.fcb")
    feature = next(iter(r.select_all()))
    by_id = {o.id: o for o in feature.city_objects()}
    assert by_id["NL.IMBAG.Pand.0503100000016459"].type == 6
    assert by_id["NL.IMBAG.Pand.0503100000016459-0"].type == 7


# --------------------------------------------------------- trap #2 ----


def test_present_but_empty_attributes_differ_from_absent() -> None:
    r = FcbReader.open_file(CORPUS / "small.fcb")
    feature = next(iter(r.select_all()))
    by_id = {o.id: o for o in feature.city_objects()}
    child = by_id["NL.IMBAG.Pand.0503100000016459-0"]
    assert child.has_attributes is True
    assert child.attributes == b""
    assert child.has_columns is False
    assert child.columns is None

    r2 = FcbReader.open_file(CORPUS / "empty_appearance.fcb")
    absent_feature = next(iter(r2.select_all()))
    (absent_obj,) = absent_feature.city_objects()
    assert absent_obj.has_attributes is False
    assert absent_obj.attributes is None


# --------------------------------------------------- error handling ---


def _corrupt_copy(tmp_path: Path, name: str, suffix: bytes) -> Path:
    original = (CORPUS / "single_feature.fcb").read_bytes()
    layout = read_header(FileRangeReader(CORPUS / "single_feature.fcb")).layout
    path = tmp_path / name
    path.write_bytes(original[: layout.feature_begin] + suffix)
    return path


def test_select_all_rejects_an_implausible_feature_length(
    tmp_path: Path,
) -> None:
    path = _corrupt_copy(
        tmp_path, "implausible.fcb", struct.pack("<I", 0xFFFFFFFF)
    )
    r = FcbReader.open_file(path)
    with pytest.raises(FcbError) as exc_info:
        list(r.select_all())
    assert exc_info.value.code is ErrorCode.INVALID_FLATBUFFER


def test_select_all_rejects_a_truncated_feature_body(
    tmp_path: Path,
) -> None:
    path = _corrupt_copy(
        tmp_path,
        "truncated.fcb",
        struct.pack("<I", 1000) + b"\x00" * 10,
    )
    r = FcbReader.open_file(path)
    with pytest.raises(FcbError) as exc_info:
        list(r.select_all())
    assert exc_info.value.code is ErrorCode.IO_ERROR


def test_select_all_rejects_trailing_bytes_after_the_declared_count(
    tmp_path: Path,
) -> None:
    original = (CORPUS / "single_feature.fcb").read_bytes()
    path = tmp_path / "trailing.fcb"
    # features_count still says 1, but extra bytes follow the one real
    # feature -- a file claiming fewer features than it carries.
    path.write_bytes(original + b"\x00\x00\x00\x00")
    r = FcbReader.open_file(path)
    with pytest.raises(FcbError) as exc_info:
        list(r.select_all())
    assert exc_info.value.code is ErrorCode.IO_ERROR


class _CorruptRawFeature:
    """A `CityFeature`-shaped object whose `Objects` vector is corrupt
    in a way `GetRootAs` alone cannot catch -- there is no FlatBuffers
    Verifier in this Python runtime (Task 3's finding), so a bad
    length/offset several fields deep only surfaces once something
    actually walks into it, not at parse time."""

    def Id(self) -> bytes:
        return b"corrupt"

    def ObjectsLength(self) -> int:
        return 3

    def Objects(self, j: int) -> object:
        raise IndexError("simulated out-of-bounds CityObject vector read")


def test_city_objects_wraps_corruption_found_after_parsing() -> None:
    # Codex review (Task 12): _parse_feature wraps only the initial
    # GetRootAs/Feature(...) call; Feature.city_objects() -- called
    # directly here, and by to_cityjson_feature -- previously let a raw
    # IndexError/struct.error escape the public surface instead of an
    # FcbError once something deeper than the initial parse turned out
    # to be corrupt.
    feature = Feature(_CorruptRawFeature(), 0)
    with pytest.raises(FcbError) as exc_info:
        feature.city_objects()
    assert exc_info.value.code is ErrorCode.INVALID_FLATBUFFER
