from __future__ import annotations

import json
from pathlib import Path

import pytest

from flatcitybuf.cityjson import to_cityjson_feature, to_cityjson_metadata
from flatcitybuf.reader import FcbReader

CORPUS = Path(__file__).resolve().parents[3] / "conformance"

# Class A conformance corpus (scripts/gen_conformance.sh): for each case,
# a .fcb built by the RUST writer and a .expected.jsonl produced by
# reading that .fcb back with the RUST reader. Comparing this Python
# reader's output against the Rust reader's view of the same bytes (not
# against the original CityJSONSeq source) isolates defects in THIS
# reader from anything the Rust writer/reader pair already agree on.
#
# geom_decoder_edges (Task 8) covers SemanticObject.parent and header
# pointOfContact/referenceDate -- fields neither the C++ reader nor its
# conformance suite exercises.
CASES = [
    "small",
    "geom_temp",
    "noise_extension",
    "single_feature",
    "long_strings",
    "duplicate_keys",
    "degenerate_extent",
    "inferable_types",
    "empty_appearance",
    "geom_decoder_edges",
]


@pytest.mark.parametrize("name", CASES)
def test_matches_the_rust_reader(name: str) -> None:
    expected = [
        json.loads(line)
        for line in (CORPUS / f"{name}.expected.jsonl")
        .read_text()
        .splitlines()
        if line.strip()
    ]
    r = FcbReader.open_file(CORPUS / f"{name}.fcb")
    actual = [to_cityjson_metadata(r.header)] + [
        to_cityjson_feature(f, r.header) for f in r.select_all()
    ]
    assert len(actual) == len(expected)

    # Line 0: the parts a reader must get right. We deliberately do not
    # reproduce every optional metadata field the Rust writer round
    # trips (see to_cityjson_metadata's docstring for pointOfContact/
    # referenceDate, which ARE reproduced and checked elsewhere).
    for key in ("type", "version", "transform", "geometry-templates"):
        if key in expected[0]:
            assert actual[0][key] == expected[0][key]

    # Features: compare the WHOLE line. Comparing selected keys is
    # exactly what hid a missing per-feature `appearance` object during
    # the C++ port -- do not narrow this.
    for i in range(1, len(actual)):
        assert actual[i] == expected[i], f"{name} line {i}"
