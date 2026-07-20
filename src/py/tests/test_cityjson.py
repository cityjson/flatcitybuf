from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest
from flatcitybuf.cityjson import city_object_type_name
from flatcitybuf.cityjson import to_cityjson_feature
from flatcitybuf.cityjson import to_cityjson_metadata
from flatcitybuf.errors import ErrorCode, FcbError
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
        "thoroughfareNumber": 42,
        "thoroughfareName": "Example Street",
        "locality": "Delft",
        "postalCode": "2600AA",
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
