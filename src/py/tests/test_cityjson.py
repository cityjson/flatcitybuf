from __future__ import annotations

import json
import struct
import sys
from pathlib import Path
from typing import Any

import pytest
from flatcitybuf.cityjson import _decode_attributes_for_json
from flatcitybuf.cityjson import _decode_semantics_values
from flatcitybuf.cityjson import city_object_type_name
from flatcitybuf.cityjson import to_cityjson_feature
from flatcitybuf.cityjson import to_cityjson_metadata
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.generated.geometry_generated import GeometryType
from flatcitybuf.generated.header_generated import ColumnType
from flatcitybuf.header import ColumnInfo, FileInfo, HeaderView
from flatcitybuf.layout import FileLayout
from flatcitybuf.reader import FcbReader

CORPUS = Path(__file__).resolve().parents[3] / "conformance"
DELFT = Path(__file__).resolve().parents[3] / "examples" / "data" / "delft.fcb"


def _features_by_id(path: Path) -> dict[str, Any]:
    r = FcbReader.open_file(path)
    return {f.id: to_cityjson_feature(f, r.header) for f in r.select_all()}


def _expected_lines(name: str) -> list[dict[str, Any]]:
    lines = (CORPUS / f"{name}.expected.jsonl").read_text().splitlines()
    return [json.loads(line) for line in lines if line.strip()]


def _expected_features_by_id(name: str) -> dict[str, dict[str, Any]]:
    # The header line has no "id" -- only CityJSONFeature lines do.
    return {line["id"]: line for line in _expected_lines(name) if "id" in line}


# ------------------------------------------------------------ metadata ---


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_metadata_emits_a_valid_cityjson_envelope() -> None:
    r = FcbReader.open_file(DELFT)
    cj = to_cityjson_metadata(r.header)

    assert cj["type"] == "CityJSON"
    assert cj["version"] == "2.0"
    assert len(cj["transform"]["scale"]) == 3
    assert len(cj["transform"]["translate"]) == 3
    assert len(cj["metadata"]["geographicalExtent"]) == 6
    assert cj["CityObjects"] == {}
    assert cj["vertices"] == []


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_point_of_contact_is_emitted_when_the_header_declares_one() -> None:
    # No C++ equivalent to port from -- src/cpp/src/cityjson.cpp does
    # not emit pointOfContact/referenceDate at all (see the task
    # report). Expected value cross-checked two ways: (1)
    # examples/data/delft.city.jsonl's own
    # metadata.pointOfContact -- the writer/reader round trip preserves
    # it unchanged, and there is no address sub-object in the source,
    # so none is expected back either; (2) the Rust reference reader
    # directly, via `cargo run --release -p fcb_cli -- deser -i
    # examples/data/delft.fcb` (oracle technique), which emits the
    # identical object for the header line.
    r = FcbReader.open_file(DELFT)
    cj = to_cityjson_metadata(r.header)
    assert cj["metadata"]["pointOfContact"] == {
        "contactName": "3DBAG Team",
        "emailAddress": "info@3dbag.nl",
        "role": "owner",
        "website": "https://3dbag.nl",
    }
    assert "referenceDate" not in cj["metadata"]


def test_metadata_from_geom_temp_matches_the_oracle_exactly() -> None:
    # geom_temp.expected.jsonl's header line is the Rust reader's own
    # output for a file the writer produced with 3 geometry templates.
    # Full-dict equality against it, rather than shape-only assertions,
    # is what would have caught upstream finding #8 -- both readers
    # agreed on a plausible-looking WRONG shape until compared this way.
    r = FcbReader.open_file(CORPUS / "geom_temp.fcb")
    cj = to_cityjson_metadata(r.header)
    (expected,) = [
        line
        for line in _expected_lines("geom_temp")
        if line.get("type") == "CityJSON"
    ]
    assert cj == expected


def test_metadata_defaults_when_the_header_declares_nothing() -> None:
    # Synthetic, because NO corpus case has an absent Transform or an
    # absent GeographicalExtent -- widening test_conformance.py's line-0
    # comparison therefore cannot catch a reader that presence-gates
    # them (upstream findings 13/14). Rust's to_cj_metadata
    # (deserializer.rs:22-90) starts from CityJSON::new(), whose
    # Transform defaults to scale [1,1,1] / translate [0,0,0], and
    # always sets `metadata` with `geographical_extent:
    # Some(unwrap_or_default())` -- six zeros. Neither key may be
    # omitted.
    header = HeaderView(
        info=FileInfo(cityjson_version="2.0"),
        layout=FileLayout(
            header_len=0,
            rtree_begin=0,
            rtree_size=0,
            attr_index_begin=0,
            attr_index_size=0,
            feature_begin=0,
        ),
        attr_indices=[],
    )
    cj = to_cityjson_metadata(header)
    assert cj == {
        "type": "CityJSON",
        "version": "2.0",
        "transform": {
            "scale": [1.0, 1.0, 1.0],
            "translate": [0.0, 0.0, 0.0],
        },
        "metadata": {"geographicalExtent": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0]},
        "CityObjects": {},
        "vertices": [],
    }


# ------------------------------------------------------------ feature ---


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_a_feature_emits_a_valid_cityjsonfeature() -> None:
    r = FcbReader.open_file(DELFT)
    feature = next(iter(r.select_all()))
    f = to_cityjson_feature(feature, r.header)

    assert f["type"] == "CityJSONFeature"
    assert f["id"]
    assert isinstance(f["CityObjects"], dict)
    assert f["CityObjects"]
    assert isinstance(f["vertices"], list)
    for v in f["vertices"]:
        assert len(v) == 3
        assert isinstance(v[0], int)  # quantised, not floating point


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_every_feature_in_delft_emits_without_error() -> None:
    r = FcbReader.open_file(DELFT)
    n = with_geom = with_attrs = 0
    for feature in r.select_all():
        f = to_cityjson_feature(feature, r.header)
        assert f["type"] == "CityJSONFeature"
        for co in f["CityObjects"].values():
            if "geometry" in co:
                with_geom += 1
            if "attributes" in co:
                with_attrs += 1
        n += 1
    assert n == r.header.info.features_count
    assert with_geom > 0
    assert with_attrs > 0


def test_json_columns_are_reparsed_not_left_as_text() -> None:
    # decode_attributes (attribute.py) deliberately keeps a `Json`
    # column's value as raw text -- its own docstring says re-parsing it
    # is "Task 8's job when emitting CityJSON". That re-parse was
    # missing: to_cityjson_feature passed the raw string straight
    # through, which test_conformance.py's whole-line comparison
    # caught (both inferable_types.fcb and noise_extension.fcb disagreed
    # with the Rust oracle only on their Json-typed attribute). Mirrors
    # the Rust reader's serde_json::from_str (deserializer.rs:363-369)
    # and cityjson.cpp's nlohmann::json::parse (attribute.cpp:179-183).
    r = FcbReader.open_file(CORPUS / "inferable_types.fcb")
    (feature,) = list(r.select_all())
    f = to_cityjson_feature(feature, r.header)
    (co,) = f["CityObjects"].values()
    assert co["attributes"]["a_json"] == {"nested": [1, 2, 3]}

    features_by_id = _features_by_id(CORPUS / "noise_extension.fcb")
    co2 = features_by_id["1234"]["CityObjects"]["1234"]
    assert co2["attributes"]["+noise-buildingLNightMax"] == {
        "uom": "dB",
        "value": 43.123,
    }


def test_malformed_json_column_text_raises_rather_than_passing_through() -> (
    None
):
    # Hand-built blob: a real reader must not silently emit a string (or
    # swallow the error) for a Json column whose stored text does not
    # actually parse -- that would hide file corruption. Neither
    # reference reader tolerates this either (Rust's serde_json::from_str
    # unwraps; deserializer.rs:363-369).
    text = b"{not valid json"
    blob = struct.pack("<HI", 0, len(text)) + text
    schema = [
        ColumnInfo(index=0, name="a_json", type=ColumnType.Json, nullable=True)
    ]
    with pytest.raises(FcbError) as exc_info:
        _decode_attributes_for_json(blob, schema)
    assert exc_info.value.code is ErrorCode.INVALID_ATTRIBUTE_VALUE


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_object_types_are_real_cityjson_names() -> None:
    r = FcbReader.open_file(DELFT)
    feature = next(iter(r.select_all()))
    f = to_cityjson_feature(feature, r.header)
    for co in f["CityObjects"].values():
        assert co["type"] in ("Building", "BuildingPart") or co[
            "type"
        ].startswith("+")


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_geometry_boundaries_reach_vertex_indices_at_some_depth() -> None:
    r = FcbReader.open_file(DELFT)
    checked = False
    for feature in r.select_all():
        f = to_cityjson_feature(feature, r.header)
        nverts = len(f["vertices"])
        for co in f["CityObjects"].values():
            for g in co.get("geometry", []):
                assert "boundaries" in g
                assert "type" in g
                cur = g["boundaries"]
                while (
                    isinstance(cur, list) and cur and isinstance(cur[0], list)
                ):
                    cur = cur[0]
                if isinstance(cur, list) and cur and isinstance(cur[0], int):
                    assert cur[0] < nverts
                    checked = True
        if checked:
            break
    assert checked


def test_city_object_type_names_cover_the_enum_and_reject_nonsense() -> None:
    assert city_object_type_name(6) == "Building"
    assert city_object_type_name(7) == "BuildingPart"
    with pytest.raises(FcbError) as exc_info:
        city_object_type_name(200)
    assert exc_info.value.code is ErrorCode.INVALID_FLATBUFFER


def test_u32_max_becomes_none_in_the_flat_semantics_values_shape() -> None:
    # Codex review (Task 12): every committed fixture's semantics.values
    # happens to be either fully populated or nested one/two levels deep
    # (small.fcb's Solid geometries, geom_temp's), so the u32::MAX ->
    # null conversion was pinned only for the (unrelated) material-index
    # path in test_geometry.py, never for THIS function's flat shape --
    # the one MultiSurface/CompositeSurface/MultiLineString/MultiPoint
    # actually use (neither one_deep nor two_deep in
    # _decode_semantics_values, cityjson.py:249).
    assert _decode_semantics_values(
        GeometryType.MultiSurface, [], [], [0, 0xFFFFFFFF, 2]
    ) == [0, None, 2]


def test_semantics_values_nest_twice_for_solid_collections() -> None:
    # NO committed fixture reaches the two-deep branch: `grep -l
    # 'MultiSolid\|CompositeSolid' conformance/*.expected.jsonl
    # examples/data/*.jsonl` is empty, so every conformance case
    # exercises only the flat and one-deep shapes. This is the same
    # nesting-depth class as upstream findings 7 and 8, both of which
    # were off-by-one-LEVEL bugs that still produced structurally valid
    # JSON -- exactly what a shape-blind test misses. Hence a direct
    # unit test on the decoder.
    #
    # Expected value derived from the RUST decoder
    # (fcb_core/src/reader/geom_decoder.rs:238-336), not from this
    # implementation: at d=4 it walks `solids` (shells per solid),
    # recursing at d=3 over that slice of `shells` (surfaces per
    # shell), which slices the flat values array in order.
    #   solids [2, 1] -> solid 0 owns shells[0:2], solid 1 owns shells[2:3]
    #   shells [2, 1, 3] -> values[0:2], values[2:3], values[3:6]
    solids = [2, 1]
    shells = [2, 1, 3]
    values = [0, 1, 2, 0xFFFFFFFF, 1, 2]

    for geom_type in (GeometryType.MultiSolid, GeometryType.CompositeSolid):
        assert _decode_semantics_values(geom_type, solids, shells, values) == [
            [[0, 1], [2]],
            [[None, 1, 2]],
        ]

    # The neighbouring depths, on the SAME inputs -- an off-by-one level
    # in either direction is a different shape, not a different value.
    assert _decode_semantics_values(
        GeometryType.Solid, solids, shells, values
    ) == [[0, 1], [2], [None, 1, 2]]
    assert _decode_semantics_values(
        GeometryType.MultiSurface, solids, shells, values
    ) == [0, 1, 2, None, 1, 2]


def test_two_deep_semantics_stops_at_the_end_of_a_short_values_array() -> None:
    # Truncated/hostile input: fewer values than the shell counts claim.
    # The decoder must run out of values rather than IndexError -- the
    # `cursor < len(values)` guard -- and must not invent entries.
    assert _decode_semantics_values(
        GeometryType.CompositeSolid, [2], [2, 2], [7]
    ) == [[[7], []]]
    # Likewise more shells than the solid counts account for: the extra
    # shell is simply not reachable from any solid.
    assert _decode_semantics_values(
        GeometryType.MultiSolid, [1], [1, 1], [7, 8]
    ) == [[[7]]]


# --------------------------------------------- empty_appearance.fcb ---


def test_empty_material_and_texture_vectors_omit_the_key() -> None:
    # empty_appearance.city.jsonl writes "material": {} / "texture": {}
    # for "empty_maps" -- a PRESENT but EMPTY mapping vector must omit
    # the key entirely on read back, matching the Rust oracle exactly.
    features = _features_by_id(CORPUS / "empty_appearance.fcb")
    expected = _expected_features_by_id("empty_appearance")
    assert features["empty_maps"] == expected["empty_maps"]
    geom = features["empty_maps"]["CityObjects"]["empty_maps"]["geometry"][0]
    assert "material" not in geom
    assert "texture" not in geom


def test_null_indices_in_material_values_match_the_oracle() -> None:
    features = _features_by_id(CORPUS / "empty_appearance.fcb")
    expected = _expected_features_by_id("empty_appearance")
    assert features["null_indices"] == expected["null_indices"]
    geom = features["null_indices"]["CityObjects"]["null_indices"]["geometry"][
        0
    ]
    assert geom["material"]["visual"]["values"] == [None, 0]


def test_per_feature_appearance_always_emitted_even_when_empty() -> None:
    # C++ initially forgot this field entirely; its conformance test did
    # not catch it because it compared only selected keys. Here we
    # assert full-dict equality against the oracle for a feature whose
    # appearance is all-empty, which would have caught that omission.
    features = _features_by_id(CORPUS / "geom_temp.fcb")
    expected = _expected_features_by_id("geom_temp")
    fid = "UUID_f488e8ce-b953-4b35-a3fe-a394fb203868"
    assert features[fid] == expected[fid]
    assert features[fid]["appearance"] == {
        "materials": [],
        "textures": [],
        "vertices-texture": [],
    }


def test_geometry_templates_round_trip_against_the_oracle() -> None:
    r = FcbReader.open_file(CORPUS / "geom_temp.fcb")
    cj = to_cityjson_metadata(r.header)
    (expected,) = [
        line
        for line in _expected_lines("geom_temp")
        if line.get("type") == "CityJSON"
    ]
    assert (
        cj["geometry-templates"]["templates"]
        == expected["geometry-templates"]["templates"]
    )
    assert (
        cj["geometry-templates"]["vertices-templates"]
        == expected["geometry-templates"]["vertices-templates"]
    )


# ----------------------------------------------- geom_decoder_edges.fcb ---
#
# A dedicated fixture built through the Rust WRITER (oracle technique,
# scripts/gen_conformance.sh's `ser` then `deser`) for two shapes no
# committed corpus fixture exercises:
#
#  * SemanticObject.parent -- present in the Rust reference reader's
#    output but, on inspection, NOT emitted by src/cpp/src/cityjson.cpp
#    (see the task report: this is a genuine divergence from the C++
#    "reference", not a port of one of its behaviours).
#  * MaterialMapping.value == 0 -- the exact optional-scalar trap the
#    task brief warns about: `uint = null` reads back as Python's
#    default 0 unless the vtable presence is checked, so "shared
#    material 0" and "absent" must not collapse to the same output.


def test_point_of_contact_address_and_reference_date_round_trip() -> None:
    # The address sub-object and referenceDate are untested by delft.fcb
    # (it has a pointOfContact but no address, and no reference date at
    # all) -- round-tripped here through the Rust writer instead.
    r = FcbReader.open_file(CORPUS / "geom_decoder_edges.fcb")
    cj = to_cityjson_metadata(r.header)
    (expected,) = [
        line
        for line in _expected_lines("geom_decoder_edges")
        if line.get("type") == "CityJSON"
    ]
    assert cj == expected
    assert cj["metadata"]["referenceDate"] == "2024-01-15"
    assert cj["metadata"]["pointOfContact"]["address"] == {
        "thoroughfareNumber": "42",
        "thoroughfareName": "Example Street",
        "locality": "Delft",
        "postcode": "2600AA",
        "country": "NL",
    }


def test_semantic_surface_parent_is_emitted_when_present() -> None:
    features = _features_by_id(CORPUS / "geom_decoder_edges.fcb")
    expected = _expected_features_by_id("geom_decoder_edges")
    assert features["semantics_parent"] == expected["semantics_parent"]

    surfaces = features["semantics_parent"]["CityObjects"]["semantics_parent"][
        "geometry"
    ][0]["semantics"]["surfaces"]
    assert surfaces[0] == {"type": "WallSurface", "children": [1]}
    assert surfaces[1] == {"type": "Door", "parent": 0}
    assert "parent" not in surfaces[0]


def test_shared_material_value_zero_is_not_confused_with_absent() -> None:
    features = _features_by_id(CORPUS / "geom_decoder_edges.fcb")
    expected = _expected_features_by_id("geom_decoder_edges")
    assert (
        features["shared_material_value"] == expected["shared_material_value"]
    )

    material = features["shared_material_value"]["CityObjects"][
        "shared_material_value"
    ]["geometry"][0]["material"]
    assert material == {"visual": {"value": 0}, "risk": {"value": 7}}


def test_no_material_key_omitted_when_geometry_declares_none() -> None:
    features = _features_by_id(CORPUS / "geom_decoder_edges.fcb")
    expected = _expected_features_by_id("geom_decoder_edges")
    assert features["no_material"] == expected["no_material"]
    geom = features["no_material"]["CityObjects"]["no_material"]["geometry"][0]
    assert "material" not in geom


# ------------------------------------ numpy vs pure-Python parity (Task 12) --

# Task 12's benchmark found the per-element FlatBuffers accessor loop
# behind cityjson.py's _uint_list (geometry solids/shells/surfaces/
# strings/boundaries/semantics, SemanticObject.children,
# MaterialMapping/TextureMapping's own uint vectors) and feature.py's
# Feature.vertices() to be the dominant cost of a full scan, and added an
# optional numpy.frombuffer bulk-decode path for both. Both paths MUST
# produce identical output -- this is not assumed, it is tested, by
# forcing `import numpy` to fail from inside those functions (without
# uninstalling the package) and comparing against the same fixtures read
# with numpy available.
#
# `sys.modules["numpy"] = None` is the standard trick for this: any
# future bare `import numpy` sees the poisoned cache entry and raises
# ImportError immediately, but flatbuffers' OWN already-executed
# `import numpy as np` (flatbuffers/number_types.py, evaluated once at
# that module's import time, long before this test runs) keeps the real
# module object it already bound -- so the generated `XxxAsNumpy()`
# accessors this file's numpy path calls into keep working normally.
# Only OUR function-local imports (inside _uint_list and
# _vertices_via_numpy) are affected, which is exactly the "numpy not
# installed" case this proves the fallback for.
_NUMPY_PARITY_CASES = ["geom_temp", "geom_decoder_edges", "small"]


def _all_lines(name: str) -> list[dict[str, Any]]:
    r = FcbReader.open_file(CORPUS / f"{name}.fcb")
    return [to_cityjson_metadata(r.header)] + [
        to_cityjson_feature(f, r.header) for f in r.select_all()
    ]


@pytest.mark.parametrize("name", _NUMPY_PARITY_CASES)
def test_numpy_and_pure_python_paths_agree(
    name: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    pytest.importorskip("numpy")  # the "numpy available" half needs it

    with_numpy = _all_lines(name)

    monkeypatch.setitem(sys.modules, "numpy", None)
    without_numpy = _all_lines(name)

    assert with_numpy == without_numpy


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_numpy_and_pure_python_paths_agree_on_delft(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("numpy")

    r = FcbReader.open_file(DELFT)
    with_numpy = [to_cityjson_feature(f, r.header) for f in r.select_all()]

    monkeypatch.setitem(sys.modules, "numpy", None)
    r2 = FcbReader.open_file(DELFT)
    without_numpy = [
        to_cityjson_feature(f, r2.header) for f in r2.select_all()
    ]

    assert with_numpy == without_numpy
