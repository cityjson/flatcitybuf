from __future__ import annotations

from pathlib import Path

import flatcitybuf

CORPUS = Path(__file__).resolve().parents[3] / "conformance"


def test_every_name_in_all_actually_exists() -> None:
    # The package shipped an __all__ holding only the Task-2 skeleton's
    # symbols, so `import flatcitybuf; flatcitybuf.FcbReader` raised
    # AttributeError while README.md's first example claimed otherwise.
    missing = [n for n in flatcitybuf.__all__ if not hasattr(flatcitybuf, n)]
    assert missing == []


def test_the_readme_entry_points_are_reachable_from_the_package() -> None:
    for name in (
        "FcbReader",
        "to_cityjson_metadata",
        "to_cityjson_feature",
        "search_rtree",
        "search_stree",
        "HttpRangeReader",
        "FileRangeReader",
        "AttrCondition",
        "KeyValue",
        "KeyKind",
        "Operator",
        "Feature",
    ):
        assert name in flatcitybuf.__all__, name


def test_version_is_the_installed_distribution_version() -> None:
    # Derived from distribution metadata rather than hardcoded, so a
    # release bump that only rewrites pyproject.toml cannot drift.
    assert flatcitybuf.__version__
    assert flatcitybuf.__version__ != "0.0.0+unknown"


def test_a_bbox_hit_can_be_turned_into_a_feature_publicly() -> None:
    # The query loop, entirely through public names: no `_reader`, no
    # `_read_feature_at`. search_rtree needs the RangeReader
    # (`range_reader`) and hands back offsets, which only
    # `feature_at` could previously decode.
    r = flatcitybuf.FcbReader.open_file(CORPUS / "small.fcb")
    hits = flatcitybuf.search_rtree(
        r.range_reader,
        r.header.layout.rtree_begin,
        r.header.info.features_count,
        r.header.info.index_node_size,
        (-1e9, -1e9, 1e9, 1e9),
    )
    assert len(hits) == r.header.info.features_count

    by_offset = {f.byte_offset: f.id for f in r.select_all()}
    assert {h.offset for h in hits} == set(by_offset)
    for hit in hits:
        assert r.feature_at(hit).id == by_offset[hit.offset]
        # A bare offset works too, for a caller that kept only that.
        assert r.feature_at(hit.offset).id == by_offset[hit.offset]
