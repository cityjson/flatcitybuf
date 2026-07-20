from __future__ import annotations

import struct
from pathlib import Path
from typing import Any

import pytest
from flatcitybuf.attribute import decode_attributes
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.header import AttrIndexInfo, ColumnInfo, HeaderView
from flatcitybuf.keys import KeyKind, KeyValue
from flatcitybuf.range_reader import FileRangeReader
from flatcitybuf.reader import FcbReader
from flatcitybuf.stree import (
    PAYLOAD_MASK,
    PAYLOAD_TAG,
    AttrCondition,
    Operator,
    decode_payload_entry,
    is_payload_ref,
    payload_offset,
    search_stree,
    stree_num_nodes,
)

CORPUS = Path(__file__).resolve().parents[3] / "conformance"
EXAMPLES = Path(__file__).resolve().parents[3] / "examples" / "data"

# Oracles used below, in order of authority:
#
# 1. src/cpp/tests/test_stree.cpp -- the conformant C++ reader's own
#    expected values for stree_num_nodes and payload decoding, copied
#    verbatim (lines cited per test).
# 2. An INDEPENDENT re-derivation of the truth set by decoding every
#    feature's attributes with the per-object schema (see
#    `values_by_offset` below), exactly as test_stree.cpp:56-75 does.
#    The index says which features match; this says what the data holds.
# 3. A direct cross-check against the C++ binary, run out-of-band and
#    recorded in the task report.


def values_by_offset(reader: FcbReader, field: str) -> dict[int, list[Any]]:
    """Ground truth: every value of `field`, per feature byte offset.

    A feature can carry SEVERAL values of one attribute -- its
    CityObjects each have their own attribute blob and their own column
    schema -- so this returns a list per feature, and a query matches a
    feature if ANY of its values satisfies the operator.
    """
    header = reader.header
    out: dict[int, list[Any]] = {}
    for feature in reader.select_all():
        vals: list[Any] = []
        for obj in feature.city_objects():
            if not obj.attributes:
                continue
            schema = (
                obj.columns
                if obj.has_columns and obj.columns is not None
                else header.info.columns
            )
            decoded = decode_attributes(obj.attributes, schema)
            if field in decoded:
                vals.append(decoded[field])
        out[feature.byte_offset] = vals
    return out


def offsets_where(truth: dict[int, list[Any]], predicate: Any) -> set[int]:
    return {
        off for off, vals in truth.items() if any(predicate(v) for v in vals)
    }


def run(
    path: Path, conditions: list[AttrCondition]
) -> tuple[FcbReader, set[int]]:
    reader = FcbReader.open_file(path)
    raw = FileRangeReader(path)
    hits = search_stree(raw, reader.header, conditions)
    return reader, {h.offset for h in hits}


# ------------------------------------------- pure arithmetic and bits ---


def test_num_nodes_breaks_at_n_less_than_branching_factor() -> None:
    # THE rule that differs from the R-tree, which breaks at n == 1
    # (stree.rs:462-497; Format Reference "Level-bounds divisor").
    # Values from test_stree.cpp:18-26.
    assert stree_num_nodes(100, 16) == 107  # 100 -> 7; 7 < 16, stop
    assert stree_num_nodes(16, 16) == 17
    assert stree_num_nodes(10, 16) == 11
    assert stree_num_nodes(1000, 16) == 1067  # 1000 -> 63 -> 4
    assert stree_num_nodes(0, 16) == 0
    with pytest.raises(FcbError):
        stree_num_nodes(10, 1)


def test_the_rtree_break_condition_would_give_a_different_answer() -> None:
    # Pins the asymmetry itself rather than just its output: with the
    # R-tree's `n == 1` rule, 100 items at fan-out 16 would be
    # 100 + 7 + 1 = 108 nodes, not 107.
    assert stree_num_nodes(100, 16) == 107
    assert stree_num_nodes(100, 16) != 108


def test_payload_tag_is_the_msb_and_mask_is_the_low_63_bits() -> None:
    # stree.rs:15-17; test_stree.cpp:28-34. Written as 1 << 63 so a
    # signed decode would be immediately visible.
    assert PAYLOAD_TAG == 1 << 63
    assert PAYLOAD_TAG == 0x8000000000000000
    assert PAYLOAD_MASK == 0x7FFFFFFFFFFFFFFF
    assert is_payload_ref(PAYLOAD_TAG | 1234)
    assert not is_payload_ref(1234)
    assert payload_offset(PAYLOAD_TAG | 1234) == 1234


def test_payload_entries_decode_as_u32_count_then_u64s() -> None:
    # payload.rs:36-61; test_stree.cpp:36-46.
    raw = (
        b"\x02\x00\x00\x00"
        + (10).to_bytes(8, "little")
        + (20).to_bytes(8, "little")
    )
    assert decode_payload_entry(raw) == [10, 20]


def test_a_truncated_payload_entry_raises() -> None:
    # test_stree.cpp:48-51.
    with pytest.raises(FcbError):
        decode_payload_entry(b"\x05\x00\x00\x00\x01\x02\x03")
    with pytest.raises(FcbError):
        decode_payload_entry(b"\x01\x00")


# ------------------------------------------------ queries over a file ---


def test_eq_on_a_unique_string_column_returns_exactly_one_feature() -> None:
    reader = FcbReader.open_file(CORPUS / "small.fcb")
    truth = values_by_offset(reader, "identificatie")
    want = next(v[0] for v in truth.values() if v)
    expected = offsets_where(truth, lambda v: v == want)
    assert len(expected) == 1

    _r, got = run(
        CORPUS / "small.fcb",
        [
            AttrCondition(
                "identificatie",
                Operator.EQ,
                KeyValue.from_string(KeyKind.STRING50, want),
            )
        ],
    )
    assert got == expected


def test_eq_on_a_duplicated_key_uses_the_payload_section() -> None:
    # duplicate_keys.fcb: 5 features, `grp` has ONE unique key, so its
    # single leaf entry must be a PAYLOAD reference holding all five
    # feature offsets. This is the test that pins PAYLOAD_TAG and the
    # payload-section-relative offset base (stree.rs:652-659).
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "grp")
    want = next(v[0] for v in truth.values() if v)
    expected = offsets_where(truth, lambda v: v == want)
    assert len(expected) == 5

    _r, got = run(
        path,
        [
            AttrCondition(
                "grp",
                Operator.EQ,
                KeyValue.from_string(KeyKind.STRING50, want),
            )
        ],
    )
    assert got == expected


def test_every_operator_agrees_with_the_decoded_truth() -> None:
    # duplicate_keys.fcb's `idx` is a ULong with 5 distinct values, one
    # per feature -- so every operator has a non-trivial, non-equal
    # answer, and Ge/Gt (and Le/Lt) genuinely differ.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "idx")
    seen = sorted({v for vals in truth.values() for v in vals})
    assert len(seen) == 5

    pivot = seen[2]
    cases = [
        (Operator.EQ, lambda v: v == pivot),
        (Operator.NE, lambda v: v != pivot),
        (Operator.GT, lambda v: v > pivot),
        (Operator.GE, lambda v: v >= pivot),
        (Operator.LT, lambda v: v < pivot),
        (Operator.LE, lambda v: v <= pivot),
    ]
    for op, predicate in cases:
        _r, got = run(
            path,
            [AttrCondition("idx", op, KeyValue.from_u64(pivot))],
        )
        assert got == offsets_where(truth, predicate), op


def test_range_scan_walks_the_leaf_array_with_no_sibling_pointers() -> None:
    # An unbounded Ge must reach EVERY leaf. There are no sibling
    # pointers in the format (the entry.rs:15 comment claiming
    # otherwise is stale) -- the scan advances by leaf INDEX, so a
    # traversal that expected a `next` pointer would stop after one node
    # and return a strict subset here.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "idx")
    _r, got = run(
        path,
        [AttrCondition("idx", Operator.GE, KeyValue.from_u64(0))],
    )
    assert got == set(truth)


def test_search_node_size_is_branching_factor_minus_one() -> None:
    # delft.fcb's b3_volume_lod22 is a Double with 1115 unique keys at
    # branching_factor 256, so the leaf level spans FIVE nodes of 255
    # entries and the tree is genuinely two levels deep. That makes both
    # the entries-per-node figure and the descent load-bearing: reading
    # `branching_factor` entries per node instead of
    # `branching_factor - 1` overruns each node into the next one's
    # first entry and mis-places every binary search.
    path = EXAMPLES / "delft.fcb"
    reader = FcbReader.open_file(path)
    index = next(
        a
        for a in reader.header.attr_indices
        if a.column_index
        == next(
            c.index
            for c in reader.header.info.columns
            if c.name == "b3_volume_lod22"
        )
    )
    assert index.num_unique_items == 1115
    assert index.branching_factor == 256
    assert index.num_unique_items > index.branching_factor - 1

    truth = values_by_offset(reader, "b3_volume_lod22")
    have = {off for off, vals in truth.items() if vals}
    assert len(have) == 1115

    # An unbounded Ge must reach every leaf across all five nodes.
    _r, got = run(
        path,
        [
            AttrCondition(
                "b3_volume_lod22",
                Operator.GE,
                KeyValue.from_f64(float("-inf")),
            )
        ],
    )
    assert got == have

    # The file itself proves the figure. Decode the root node directly
    # -- Entry<f64> is 8 key bytes then a u64 LE child index
    # (entry.rs:25-52) -- and read off the spacing of the child groups.
    raw = FileRangeReader(path)
    root = raw.read(index.begin, 5 * 16)
    separators = [struct.unpack_from("<dQ", root, i * 16) for i in range(5)]
    children = [child for _key, child in separators]
    assert children == [5, 260, 515, 770, 1025]
    assert {b - a for a, b in zip(children, children[1:])} == {255}

    # A separator key is the FIRST key of the group to its RIGHT, which
    # is why find_exact adds node_size on an exact hit. With node_size
    # read as `branching_factor` these queries land one entry past their
    # key and return nothing.
    for key, _child in separators[:-1]:
        _r, got_eq = run(
            path,
            [
                AttrCondition(
                    "b3_volume_lod22", Operator.EQ, KeyValue.from_f64(key)
                )
            ],
        )
        assert got_eq == offsets_where(truth, lambda v: v == key)
        assert got_eq

    # find_partition, by contrast, must NOT add node_size on an exact
    # hit -- it has to return the LEFTMOST position the key could sit
    # at. A Ge bounded by a separator key would otherwise start a whole
    # child group too far right and drop ~255 features.
    first_sep = separators[0][0]
    _r, got_ge = run(
        path,
        [
            AttrCondition(
                "b3_volume_lod22", Operator.GE, KeyValue.from_f64(first_sep)
            )
        ],
    )
    assert got_ge == offsets_where(truth, lambda v: v >= first_sep)
    assert len(got_ge) > 255

    # The last separator carries the key_max sentinel (+inf) and its
    # child index ALREADY points at the final group -- adding node_size
    # there would walk off the end of the level (stree.cpp:212-222).
    assert separators[-1][0] == float("inf")


def test_ge_is_gt_plus_eq_and_they_are_disjoint() -> None:
    # test_stree.cpp:125-148, on the same column and pivot.
    path = EXAMPLES / "delft.fcb"

    def ids(op: Operator, v: int) -> set[int]:
        _r, got = run(
            path, [AttrCondition("b3_bouwlagen", op, KeyValue.from_u64(v))]
        )
        return got

    ge, gt, eq = ids(Operator.GE, 2), ids(Operator.GT, 2), ids(Operator.EQ, 2)
    assert ge
    assert eq
    assert ge == gt | eq
    assert not (gt & eq)


def test_le_lt_and_ne_partition_consistently() -> None:
    # test_stree.cpp:150-169.
    path = EXAMPLES / "delft.fcb"

    def ids(op: Operator, v: int) -> set[int]:
        _r, got = run(
            path, [AttrCondition("b3_bouwlagen", op, KeyValue.from_u64(v))]
        )
        return got

    le, lt, eq = ids(Operator.LE, 3), ids(Operator.LT, 3), ids(Operator.EQ, 3)
    assert le == lt | eq
    assert not (lt & eq)
    assert not (ids(Operator.NE, 3) & eq)


def test_multiple_conditions_are_anded_and_strictly_narrow() -> None:
    # test_stree.cpp:171-193. A `<=` assertion would pass even if the
    # second condition were ignored; require a strict reduction.
    path = EXAMPLES / "delft.fcb"
    _r, one = run(
        path,
        [AttrCondition("b3_bouwlagen", Operator.GE, KeyValue.from_u64(1))],
    )
    _r, two = run(
        path,
        [
            AttrCondition("b3_bouwlagen", Operator.GE, KeyValue.from_u64(1)),
            AttrCondition("b3_bouwlagen", Operator.LE, KeyValue.from_u64(2)),
        ],
    )
    assert one and two
    assert len(two) < len(one)
    assert two <= one


def test_results_contain_no_duplicate_offsets() -> None:
    # test_stree.cpp:195-205.
    path = EXAMPLES / "delft.fcb"
    reader = FcbReader.open_file(path)
    hits = search_stree(
        FileRangeReader(path),
        reader.header,
        [AttrCondition("b3_bouwlagen", Operator.GE, KeyValue.from_u64(1))],
    )
    offsets = [h.offset for h in hits]
    assert len(offsets) == len(set(offsets))
    assert offsets == sorted(offsets)


def test_offsets_are_feature_section_relative_and_land_on_a_feature() -> None:
    # Same shape as search_rtree's SearchResultItem: `offset` is
    # relative to feature_begin, NOT absolute (stree.rs:378-384). An
    # absolute offset would not appear in this set.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    starts = {f.byte_offset for f in reader.select_all()}
    _r, got = run(
        path, [AttrCondition("idx", Operator.GE, KeyValue.from_u64(0))]
    )
    assert got
    assert got <= starts
    assert 0 in starts  # the first feature starts AT feature_begin


def test_string_equality_survives_a_key_longer_than_fifty_bytes() -> None:
    # long_strings.fcb's `label` exceeds the 50-byte key width, so the
    # index can only answer with candidates. search_stree must still
    # find them rather than missing the truncated key entirely.
    path = CORPUS / "long_strings.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "label")
    want = next(v[0] for v in truth.values() if v)
    assert len(want.encode("utf-8")) > 50

    _r, got = run(
        path,
        [
            AttrCondition(
                "label",
                Operator.EQ,
                KeyValue.from_string(KeyKind.STRING50, want),
            )
        ],
    )
    assert got >= offsets_where(truth, lambda v: v == want)


def test_bool_eq_true_does_not_walk_off_the_end_of_the_level() -> None:
    # The key_max sentinel guard in find_exact (stree.cpp:212-222):
    # Eq(true) on a Bool column is a query whose key IS the type
    # maximum, which without the clamp indexes past the child level.
    path = CORPUS / "inferable_types.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "a_bool")
    want = next(v[0] for v in truth.values() if v is not None)
    _r, got = run(
        path,
        [AttrCondition("a_bool", Operator.EQ, KeyValue.from_bool(want))],
    )
    assert got == offsets_where(truth, lambda v: v == want)


# ------------------------------------------------ errors and hardening ---


def test_an_unknown_or_unindexed_column_raises() -> None:
    # test_stree.cpp:207-214.
    path = CORPUS / "small.fcb"
    reader = FcbReader.open_file(path)
    with pytest.raises(FcbError) as exc:
        search_stree(
            FileRangeReader(path),
            reader.header,
            [AttrCondition("nope", Operator.EQ, KeyValue.from_u64(1))],
        )
    assert exc.value.code == ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND

    with pytest.raises(FcbError):
        search_stree(FileRangeReader(path), reader.header, [])


def test_divergence_2_json_and_binary_index_queries_are_rejected() -> None:
    # reader/attr_query.rs:273. inferable_types.fcb declares a Json
    # column (a_json), which the writer does not index -- so the
    # rejection is asserted against a synthesized header carrying an
    # index for it, to prove the type check fires BEFORE the
    # "not indexed" check.
    path = CORPUS / "inferable_types.fcb"
    reader = FcbReader.open_file(path)
    header = reader.header
    json_col = next(c for c in header.info.columns if c.name == "a_json")
    assert json_col.type == 12  # ColumnType.Json

    forged = HeaderView(
        info=header.info,
        layout=header.layout,
        attr_indices=[
            AttrIndexInfo(
                column_index=json_col.index,
                length=32,
                branching_factor=256,
                num_unique_items=1,
                begin=header.layout.attr_index_begin,
            )
        ],
    )
    with pytest.raises(FcbError) as exc:
        search_stree(
            FileRangeReader(path),
            forged,
            [
                AttrCondition(
                    "a_json",
                    Operator.EQ,
                    KeyValue.from_string(KeyKind.STRING100, "{}"),
                )
            ],
        )
    assert exc.value.code == ErrorCode.UNSUPPORTED_COLUMN_TYPE


def _forge(header: HeaderView, **over: Any) -> HeaderView:
    idx = header.attr_indices[0]
    fields = {
        "column_index": idx.column_index,
        "length": idx.length,
        "branching_factor": idx.branching_factor,
        "num_unique_items": idx.num_unique_items,
        "begin": idx.begin,
    }
    fields.update(over)
    return HeaderView(
        info=header.info,
        layout=header.layout,
        attr_indices=[AttrIndexInfo(**fields)],
    )


@pytest.mark.parametrize(
    "over",
    [
        {"num_unique_items": 0xFFFFFFFF},
        {"branching_factor": 1},
        {"branching_factor": 0},
        {"num_unique_items": 0},
        {"length": 0},
        {"begin": 1 << 40},
    ],
)
def test_a_corrupt_index_header_raises_rather_than_over_allocating(
    over: dict[str, int],
) -> None:
    # The brief's hardening requirement: a hostile num_unique_items /
    # branching_factor / length must be bounded, not trusted. Each of
    # these would otherwise ask for gigabytes or index off the end.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    col = reader.header.info.columns[1]  # 'idx', a ULong column
    header = _forge(
        reader.header,
        column_index=col.index,
        **over,
    )
    with pytest.raises(FcbError):
        search_stree(
            FileRangeReader(path),
            header,
            [AttrCondition(col.name, Operator.GE, KeyValue.from_u64(0))],
        )


def test_a_condition_whose_value_kind_mismatches_the_column_raises() -> None:
    # Comparing an f64 key against a u64 column would otherwise reach
    # compare_keys and throw from deep inside the traversal; catch it at
    # the boundary instead.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    with pytest.raises(FcbError) as exc:
        search_stree(
            FileRangeReader(path),
            reader.header,
            [AttrCondition("idx", Operator.EQ, KeyValue.from_f64(1.0))],
        )
    assert exc.value.code == ErrorCode.UNSUPPORTED_COLUMN_TYPE


def test_columninfo_lookup_is_by_name_not_position() -> None:
    # `Column.index` is the schema position; `attr_indices` keys off it.
    # A lookup that used the list POSITION would agree by accident on
    # small.fcb, whose columns happen to be in index order -- assert the
    # relationship explicitly instead.
    reader = FcbReader.open_file(CORPUS / "small.fcb")
    by_name = {c.name: c for c in reader.header.info.columns}
    assert isinstance(by_name["b3_bouwlagen"], ColumnInfo)
    assert by_name["b3_bouwlagen"].index == 43
    indexed = {a.column_index for a in reader.header.attr_indices}
    assert 43 in indexed
