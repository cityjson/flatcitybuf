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
    _build_tree,
    _find_exact,
    _find_partition,
    _Tree,
    decode_payload_entry,
    is_payload_ref,
    payload_offset,
    search_stree,
    stree_num_nodes,
    value_satisfies,
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
    reader, two = run(
        path,
        [
            AttrCondition("b3_bouwlagen", Operator.GE, KeyValue.from_u64(1)),
            AttrCondition("b3_bouwlagen", Operator.LE, KeyValue.from_u64(2)),
        ],
    )
    assert one and two
    assert len(two) < len(one)
    assert two <= one

    # Codex review (Task 12): the three assertions above would also pass
    # for an implementation that dropped the second condition entirely
    # and then returned an arbitrary nonempty proper subset of `one` --
    # neither "strictly fewer" nor "a subset" proves the SECOND condition
    # was applied at all, let alone correctly. Compare against the exact
    # answer, independently derived from decoded ground truth.
    truth = values_by_offset(reader, "b3_bouwlagen")
    expected_two = offsets_where(truth, lambda v: 1 <= v <= 2)
    assert two == expected_two


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


def index_for(reader: FcbReader, name: str) -> AttrIndexInfo:
    col = next(c for c in reader.header.info.columns if c.name == name)
    return next(
        a for a in reader.header.attr_indices if a.column_index == col.index
    )


class MemoryRangeReader:
    """In-memory RangeReader, for hostile bytes that must not be written
    to the corpus. Honours range_reader.RangeReader's contract: a read
    crossing the end returns exactly what exists."""

    def __init__(self, data: bytes) -> None:
        self._data = data

    def total_size(self) -> int:
        return len(self._data)

    def read(self, offset: int, length: int) -> bytes:
        if length == 0:
            return b""
        return self._data[offset : offset + length]


def _crafted_payload_count_fixture(count: int) -> tuple[FcbReader, bytes]:
    """duplicate_keys.fcb's `grp` payload entry, with its declared u32
    count overwritten -- shared by the two ceiling tests below."""
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    index = index_for(reader, "grp")
    assert index.num_unique_items == 1  # one key, so one payload entry

    entry_size = 50 + 8  # FixedStringKey<50> + u64 offset
    tree_bytes = (
        stree_num_nodes(index.num_unique_items, index.branching_factor)
        * entry_size
    )
    # Node 0 is the root, node 1 the single leaf; take the leaf entry's
    # offset from the file rather than assuming where the payload starts.
    leaf_at = index.begin + entry_size + 50
    data = bytearray(path.read_bytes())
    leaf_offset = int.from_bytes(data[leaf_at : leaf_at + 8], "little")
    assert is_payload_ref(leaf_offset)
    rel = payload_offset(leaf_offset)

    struct.pack_into("<I", data, index.begin + tree_bytes + rel, count)
    return reader, bytes(data)


def _query_grp_same(reader: FcbReader, data: bytes) -> list[Any]:
    return search_stree(
        MemoryRangeReader(data),
        reader.header,
        [
            AttrCondition(
                "grp",
                Operator.EQ,
                KeyValue.from_string(KeyKind.STRING50, "same"),
            )
        ],
    )


def test_a_crafted_payload_count_is_rejected_by_the_sanity_ceiling() -> None:
    # The brief's hardening case that had no test: a payload entry whose
    # u32 count is 0xFFFFFFFF asks for 4 + 32 GiB. _Tree.emit's sanity
    # ceiling (_MAX_PAYLOAD_ENTRY_SIZE, Codex review Task 12) must reject
    # it BEFORE even checking `payload_size`, since a sparse file could
    # make a much larger count fit that bound too -- see the test right
    # below, which pins exactly that. The specific message is asserted
    # so a bare pytest.raises(FcbError) could not silently start passing
    # for the wrong reason.
    reader, data = _crafted_payload_count_fixture(0xFFFFFFFF)
    with pytest.raises(FcbError) as exc:
        _query_grp_same(reader, data)
    assert exc.value.code == ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND
    assert "exceeding the sanity ceiling" in str(exc.value)


def test_a_payload_count_under_the_ceiling_still_checks_payload_size() -> None:
    # A count comfortably under _MAX_PAYLOAD_ENTRY_SIZE (256 MiB) but
    # still far larger than this tiny fixture's real payload section
    # must still be rejected -- by the OTHER guard, "overruns its
    # section" -- rather than silently reading past it. This is what
    # the previous (now split) test's message assertion actually pinned
    # before the ceiling was added; kept as its own case so the ceiling
    # and the payload_size bound are each independently exercised.
    count = 1_000_000  # 8,000,004 bytes: under the ceiling, over the file
    reader, data = _crafted_payload_count_fixture(count)
    with pytest.raises(FcbError) as exc:
        _query_grp_same(reader, data)
    assert exc.value.code == ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND
    assert "overruns its section" in str(exc.value)


def test_read_entries_refuses_a_range_outside_the_node_region() -> None:
    # _Tree.read_entries' own bound. It is DEFENCE IN DEPTH: every
    # public path clamps the range to a level's end first (node_at) or to
    # leaf_end (_scan_range), and _find_exact validates each child index
    # against its level, so no corrupt file in the corpus reaches this.
    # It is therefore exercised directly -- a test going through
    # search_stree would be absorbed by one of those earlier checks and
    # would not pin this guard at all.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    tree = _build_tree(
        FileRangeReader(path), index_for(reader, "idx"), KeyKind.UINT64
    )
    with pytest.raises(FcbError) as exc:
        tree.read_entries(0, tree.node_count + 1)
    assert exc.value.code == ErrorCode.INDEX_OUT_OF_BOUNDS
    assert "outside the" in str(exc.value)
    # The in-range read it brackets still works.
    assert len(tree.read_entries(0, tree.node_count)) == tree.node_count


class _MemReader:
    """Minimal in-memory RangeReader for a hand-built `_Tree`, matching
    duplicate_keys' MemoryRangeReader but scoped to these two tests."""

    def __init__(self, data: bytes) -> None:
        self._data = data

    def total_size(self) -> int:
        return len(self._data)

    def read(self, offset: int, length: int) -> bytes:
        return self._data[offset : offset + length]


def _misaligned_two_leaf_tree() -> _Tree:
    # UINT64, branching_factor=3 (node_size=2), num_unique_items=2:
    # a root holding one separator (key=U64_MAX sentinel, offset=2) and
    # two leaves (key=0/offset=100, key=1/offset=200). levels =
    # [(1,3),(0,1)] -- leaf level occupies flat indices 1 and 2, so its
    # only valid group starts at 1. The root's offset of 2 is IN that
    # range but not the group's start -- Codex review (Task 12).
    buf = struct.pack(
        "<QQQQQQ",
        0xFFFFFFFFFFFFFFFF,
        2,  # root: corrupt child offset (should be 1)
        0,
        100,  # leaf key 0
        1,
        200,  # leaf key 1
    )
    return _Tree(
        reader=_MemReader(buf),
        index_begin=0,
        payload_begin=len(buf),
        payload_size=0,
        kind=KeyKind.UINT64,
        node_size=2,
        levels=[(1, 3), (0, 1)],
    )


def test_find_exact_rejects_a_child_offset_misaligned_to_its_group() -> None:
    # Before this guard existed, EQ(0) followed the corrupt child
    # offset (2) straight to the SECOND leaf, found nothing there, and
    # returned an empty result -- silently losing the genuine match at
    # offset 100 instead of raising. A corrupt/hostile index must not
    # under-report matches without any error.
    tree = _misaligned_two_leaf_tree()
    with pytest.raises(FcbError) as exc:
        _find_exact(tree, KeyValue.from_u64(0))
    assert exc.value.code == ErrorCode.INDEX_OUT_OF_BOUNDS
    assert "not aligned" in str(exc.value)


def test_find_partition_rejects_a_child_offset_misaligned_to_its_group() -> (
    None
):
    # _find_partition used to apply NEITHER the child-level bounds
    # check NOR the alignment check at all (unlike _find_exact, which
    # at least had the bounds check already) -- Codex review (Task 12).
    tree = _misaligned_two_leaf_tree()
    with pytest.raises(FcbError) as exc:
        _find_partition(tree, KeyValue.from_u64(0))
    assert exc.value.code == ErrorCode.INDEX_OUT_OF_BOUNDS
    assert "not aligned" in str(exc.value)


def test_an_index_blob_reaching_past_the_end_of_the_file_is_rejected() -> None:
    # _build_tree's total_size() check. Asserting only FcbError would
    # pass with the check deleted too: the node read then comes back
    # empty and raises ATTRIBUTE_INDEX_NOT_FOUND instead. Pin the code
    # AND the message.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    col = reader.header.info.columns[1]  # 'idx', a ULong column
    header = _forge(reader.header, column_index=col.index, begin=1 << 40)
    with pytest.raises(FcbError) as exc:
        search_stree(
            FileRangeReader(path),
            header,
            [AttrCondition(col.name, Operator.GE, KeyValue.from_u64(0))],
        )
    assert exc.value.code == ErrorCode.INDEX_OUT_OF_BOUNDS
    assert "outside the file" in str(exc.value)


# ------------------------------------- SearchResultItem.index semantics ---


def test_index_is_the_leaf_relative_key_ordinal_from_a_range_scan() -> None:
    # duplicate_keys.fcb's `idx` has 5 unique keys, one per feature, so
    # the leaf ordinals are exactly 0..4 in KEY order. The tree's leaf
    # level starts at node 1 (5 leaves + 1 root), so an index that forgot
    # to subtract leaf_start would read 1..5.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "idx")
    tree = _build_tree(
        FileRangeReader(path), index_for(reader, "idx"), KeyKind.UINT64
    )
    assert tree.leaf_start == 1

    hits = search_stree(
        FileRangeReader(path),
        reader.header,
        [AttrCondition("idx", Operator.GE, KeyValue.from_u64(0))],
    )
    assert sorted(h.index for h in hits) == [0, 1, 2, 3, 4]
    ranks = {v: r for r, v in enumerate(sorted(v[0] for v in truth.values()))}
    assert {h.offset: h.index for h in hits} == {
        off: ranks[vals[0]] for off, vals in truth.items()
    }


def test_index_from_find_exact_is_the_same_leaf_ordinal() -> None:
    # The other emit site. A constant here would go unnoticed by every
    # test that keeps only `offset`: pin the value, and pin that the two
    # sites agree for the same key.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "idx")
    keys = sorted(v[0] for v in truth.values())
    pivot = keys[2]

    eq = search_stree(
        FileRangeReader(path),
        reader.header,
        [AttrCondition("idx", Operator.EQ, KeyValue.from_u64(pivot))],
    )
    assert [h.index for h in eq] == [2]

    scanned = search_stree(
        FileRangeReader(path),
        reader.header,
        [AttrCondition("idx", Operator.GE, KeyValue.from_u64(0))],
    )
    same = next(h for h in scanned if h.offset == eq[0].offset)
    assert same.index == eq[0].index


def test_every_feature_behind_one_payload_entry_shares_its_index() -> None:
    # Documented semantics: `index` identifies the KEY, not the feature
    # -- unlike the packed R-tree's field of the same name. All five
    # features hang off duplicate_keys.fcb's single `grp` key.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    hits = search_stree(
        FileRangeReader(path),
        reader.header,
        [
            AttrCondition(
                "grp",
                Operator.EQ,
                KeyValue.from_string(KeyKind.STRING50, "same"),
            )
        ],
    )
    assert len(hits) == 5
    assert {h.index for h in hits} == {0}
    assert len({h.offset for h in hits}) == 5


def test_le_on_a_separator_key_needs_the_widened_scan_end() -> None:
    # _scan_range widens the scan end to upper_idx + 2 * node_size.
    # find_partition descends LEFT on an exact hit, so when `upper` is
    # itself a separator key its own leaf entry sits one node PAST the
    # un-widened end and is silently dropped -- exactly one feature here.
    path = EXAMPLES / "delft.fcb"
    reader = FcbReader.open_file(path)
    index = index_for(reader, "b3_volume_lod22")
    raw = FileRangeReader(path)
    separator, _child = struct.unpack_from("<dQ", raw.read(index.begin, 16), 0)

    truth = values_by_offset(reader, "b3_volume_lod22")
    _r, got = run(
        path,
        [
            AttrCondition(
                "b3_volume_lod22",
                Operator.LE,
                KeyValue.from_f64(separator),
            )
        ],
    )
    expected = offsets_where(truth, lambda v: v <= separator)
    assert got == expected
    # 256, not 255: the boundary feature is the one the un-widened end
    # loses.
    assert len(got) == 256


# ------------------------------------- string operator lowering (raw) ---
#
# stree_query WIDENS Gt/Lt/Ne for fixed-width string keys, because two
# values sharing a 50-byte prefix are one key on disk. long_strings.fcb
# holds exactly that pair: "y"*50 + "AAA" and "y"*50 + "BBB", which
# collapse to a single key. Each assertion below is the CANDIDATE set, so
# every one of them fails if the operator is lowered the way a numeric
# column is lowered.

LONG_A = "y" * 50 + "AAA"
LONG_B = "y" * 50 + "BBB"


def _string_candidates(op: Operator, value: str) -> set[int]:
    _r, got = run(
        CORPUS / "long_strings.fcb",
        [
            AttrCondition(
                "label", op, KeyValue.from_string(KeyKind.STRING50, value)
            )
        ],
    )
    return got


def _long_string_offsets() -> set[int]:
    # The pair of byte offsets long_strings.fcb's two features live at.
    # NOT a stable pair of literals: the second feature's offset depends
    # on the encoded size of the first, which -- per Task 16's finding --
    # is not guaranteed identical across a corpus regeneration (cjseq's
    # CityObjects is a HashMap, so CityObject build order, and hence
    # FlatBuffer layout, varies run to run even for identical content).
    # Derive it from the decoded truth instead of hardcoding it.
    reader = FcbReader.open_file(CORPUS / "long_strings.fcb")
    truth = values_by_offset(reader, "label")
    return offsets_where(truth, lambda v: v in (LONG_A, LONG_B))


def test_the_two_long_labels_share_one_on_disk_key() -> None:
    reader = FcbReader.open_file(CORPUS / "long_strings.fcb")
    truth = values_by_offset(reader, "label")
    assert sorted(v[0] for v in truth.values()) == [LONG_A, LONG_B]
    assert len(LONG_A.encode("utf-8")) > 50
    assert LONG_A.encode("utf-8")[:50] == LONG_B.encode("utf-8")[:50]
    assert index_for(reader, "label").num_unique_items == 1


def test_gt_on_a_string_column_keeps_the_equal_prefix_band() -> None:
    # Strict Gt would drop the shared key entirely and return nothing,
    # losing the "BBB" feature that genuinely IS greater.
    assert _string_candidates(Operator.GT, LONG_A) == _long_string_offsets()


def test_lt_on_a_string_column_keeps_the_equal_prefix_band() -> None:
    # Strict Lt would drop the shared key and lose the "AAA" feature.
    assert _string_candidates(Operator.LT, LONG_B) == _long_string_offsets()


def test_ne_on_a_string_column_is_a_full_scan() -> None:
    # The numeric lowering (two half-open scans around the key) would
    # exclude the shared key and return nothing, losing "BBB".
    assert _string_candidates(Operator.NE, LONG_A) == _long_string_offsets()
    assert _string_candidates(Operator.NE, LONG_B) == _long_string_offsets()


# ------------------------------------------ post-filtering the widening ---
#
# Cross-checked against the C++ reader's FcbReader::select_attr, run
# out-of-band on the same fixtures; both result sets are recorded in the
# task report's fix-pass section.


def _verified(path: Path, column: str, op: Operator, value: str) -> set[int]:
    reader = FcbReader.open_file(path)
    return {
        h.offset
        for h in reader.select_attr(
            [
                AttrCondition(
                    column, op, KeyValue.from_string(KeyKind.STRING50, value)
                )
            ]
        )
    }


def test_select_attr_undoes_the_string_widening() -> None:
    # C++ reference (select_attr on long_strings.fcb): Eq AAA -> [a],
    # Ne AAA -> [b], Gt AAA -> [b], Lt BBB -> [a], while the raw index
    # returns both features for every one of them. `off_a`/`off_b` are
    # derived from the decoded truth rather than hardcoded (see
    # `_long_string_offsets`'s docstring for why the physical offsets
    # are not stable literals).
    path = CORPUS / "long_strings.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "label")
    off_a = next(off for off, vals in truth.items() if vals == [LONG_A])
    off_b = next(off for off, vals in truth.items() if vals == [LONG_B])
    both = {off_a, off_b}

    assert _verified(path, "label", Operator.EQ, LONG_A) == {off_a}
    assert _verified(path, "label", Operator.EQ, LONG_B) == {off_b}
    assert _verified(path, "label", Operator.NE, LONG_A) == {off_b}
    assert _verified(path, "label", Operator.GT, LONG_A) == {off_b}
    assert _verified(path, "label", Operator.GT, LONG_B) == set()
    assert _verified(path, "label", Operator.LT, LONG_B) == {off_a}
    assert _verified(path, "label", Operator.LT, LONG_A) == set()
    assert _verified(path, "label", Operator.LE, LONG_A) == {off_a}
    assert _verified(path, "label", Operator.GE, LONG_A) == both


def test_select_attr_exact_index_only_is_the_raw_candidate_set() -> None:
    # Verification can only remove, never add (test_stree.cpp:216-240).
    path = CORPUS / "long_strings.fcb"
    reader = FcbReader.open_file(path)
    both = _long_string_offsets()
    for op in (Operator.EQ, Operator.NE, Operator.GT, Operator.LT):
        query = [
            AttrCondition(
                "label", op, KeyValue.from_string(KeyKind.STRING50, LONG_A)
            )
        ]
        raw = {
            h.offset for h in reader.select_attr(query, exact_index_only=True)
        }
        verified = {h.offset for h in reader.select_attr(query)}
        assert raw == both
        assert verified <= raw


def test_select_attr_agrees_with_the_decoded_truth_on_every_operator() -> None:
    # An independent re-derivation: decode every CityObject's `species`
    # with its own schema and apply the operator to the FULL UTF-8 bytes,
    # existentially over a feature's objects. geom_temp.fcb is the
    # fixture where two of four features carry the attribute at all, and
    # one feature carries TWO different values across its 14 objects.
    path = CORPUS / "geom_temp.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "species")
    assert sorted(len(v) for v in truth.values()) == [0, 0, 1, 14]

    for want in ("1640", "1800"):
        w = want.encode("utf-8")
        cases = [
            (Operator.EQ, lambda v: v.encode("utf-8") == w),
            (Operator.NE, lambda v: v.encode("utf-8") != w),
            (Operator.GT, lambda v: v.encode("utf-8") > w),
            (Operator.GE, lambda v: v.encode("utf-8") >= w),
            (Operator.LT, lambda v: v.encode("utf-8") < w),
            (Operator.LE, lambda v: v.encode("utf-8") <= w),
        ]
        for op, predicate in cases:
            got = _verified(path, "species", op, want)
            assert got == offsets_where(truth, predicate), (op, want)


def test_a_feature_with_no_species_at_all_is_never_a_raw_candidate() -> None:
    # A feature that carries NO `species` value on any of its CityObjects
    # is arguably "!= 1640", but it does not match Ne -- not because
    # select_attr's per-object existential post-filter (reader.cpp:
    # 419-426) removes it (that mechanism is pinned by
    # test_select_attr_agrees_with_the_decoded_truth_on_every_operator,
    # which exercises a feature that has SOME objects with the attribute
    # and some without). It never matches because the `species` B+tree
    # holds no KEY for it at all, so it is never a CANDIDATE in the
    # first place: raw index lookup (exact_index_only=True) already
    # excludes it, before verification ever runs. Assert both the raw
    # and the verified sets equal the same independently-decoded truth,
    # so the test would fail if either stage started including it.
    #
    # Expected offsets are derived from `truth` at runtime rather than
    # hardcoded: Task 16 found the corpus's physical byte layout is not
    # stable across a regeneration (cjseq's CityObjects is a HashMap, so
    # CityObject build order -- and hence FlatBuffer layout -- varies
    # run to run for identical content).
    path = CORPUS / "geom_temp.fcb"
    reader = FcbReader.open_file(path)
    truth = values_by_offset(reader, "species")
    without = {off for off, vals in truth.items() if not vals}
    assert len(without) == 2

    query = [
        AttrCondition(
            "species",
            Operator.NE,
            KeyValue.from_string(KeyKind.STRING50, "1640"),
        )
    ]
    raw = {h.offset for h in reader.select_attr(query, exact_index_only=True)}
    verified = {h.offset for h in reader.select_attr(query)}
    expected = offsets_where(truth, lambda v: v != "1640")

    assert raw == expected
    assert verified == expected
    assert not (raw & without)


def test_select_attr_leaves_a_numeric_column_untouched() -> None:
    # needs_post_filter is false for every non-string kind, so select_attr
    # returns search_stree's answer unchanged -- verifying a numeric
    # column would mean decoding every candidate feature for nothing.
    path = CORPUS / "duplicate_keys.fcb"
    reader = FcbReader.open_file(path)
    query = [AttrCondition("idx", Operator.NE, KeyValue.from_u64(2))]
    assert reader.select_attr(query) == search_stree(
        FileRangeReader(path), reader.header, query
    )


def test_select_attr_rejects_an_empty_or_unknown_query() -> None:
    reader = FcbReader.open_file(CORPUS / "small.fcb")
    with pytest.raises(FcbError):
        reader.select_attr([])
    with pytest.raises(FcbError):
        reader.select_attr(
            [AttrCondition("nope", Operator.EQ, KeyValue.from_u64(1))]
        )


# ------------------------------------------ the post-filter comparator ---


def test_value_satisfies_compares_full_bytes_not_truncated_keys() -> None:
    key = KeyValue.from_string(KeyKind.STRING50, LONG_A)
    # compare_keys would call these EQUAL (same 50-byte key); the
    # post-filter must not.
    assert value_satisfies(LONG_A, Operator.EQ, key)
    assert not value_satisfies(LONG_B, Operator.EQ, key)
    assert value_satisfies(LONG_B, Operator.GT, key)
    assert value_satisfies(LONG_B, Operator.NE, key)
    assert not value_satisfies(LONG_B, Operator.LT, key)


def test_value_satisfies_compares_the_raw_bytes_of_the_query_key() -> None:
    # A key can be built from BYTES that are not valid UTF-8 -- a key
    # read off disk may have been cut mid-codepoint (keys.py's
    # `original_string` says so). There is no `str` that round-trips
    # those, so the comparison is on `raw`.
    key = KeyValue.from_string(KeyKind.STRING50, b"\xc3")
    assert key.original_string == "�"
    assert key.original_string.encode("utf-8") != key.raw
    assert not value_satisfies(key.original_string, Operator.EQ, key)
    assert value_satisfies("~", Operator.LT, key)  # 0x7e < 0xc3
    assert value_satisfies("ÿ", Operator.GT, key)  # c3 bf > c3


def test_value_satisfies_refuses_a_datetime_key_instead_of_saying_no() -> None:
    # _key_from_attr_value had no DATETIME branch, so every DateTime
    # condition fell through to `return None` and value_satisfies
    # answered a silent, confident False -- a wrong answer dressed as an
    # empty result. Unreachable through select_attr (needs_post_filter
    # is true only for string kinds) but value_satisfies is public and
    # in stree.__all__.
    key = KeyValue.from_datetime(1_700_000_000, 0)
    with pytest.raises(FcbError) as excinfo:
        value_satisfies("2023-11-14T22:13:20Z", Operator.EQ, key)
    assert excinfo.value.code is ErrorCode.UNSUPPORTED_COLUMN_TYPE
    # Including for operators whose false answer would have looked right.
    with pytest.raises(FcbError):
        value_satisfies("2023-11-14T22:13:20Z", Operator.NE, key)


def test_value_satisfies_uses_the_ordered_float_total_order() -> None:
    nan = float("nan")
    assert value_satisfies(nan, Operator.EQ, KeyValue.from_f64(nan))
    assert value_satisfies(nan, Operator.GT, KeyValue.from_f64(float("inf")))
    assert value_satisfies(0.0, Operator.EQ, KeyValue.from_f64(-0.0))
    assert value_satisfies(1, Operator.EQ, KeyValue.from_f64(1.0))


def test_value_satisfies_refuses_values_that_are_not_of_the_kind() -> None:
    # None of these can satisfy ANY operator, including Ne: a mismatch
    # means "this attribute is not a value of that column's key kind".
    u64 = KeyValue.from_u64(1)
    assert not value_satisfies("1", Operator.EQ, u64)
    assert not value_satisfies("1", Operator.NE, u64)
    # bool is an int in Python; it must not equal 1 in a numeric column.
    assert not value_satisfies(True, Operator.EQ, u64)
    assert value_satisfies(True, Operator.EQ, KeyValue.from_bool(True))
    assert not value_satisfies(1, Operator.EQ, KeyValue.from_bool(True))
    # Out of range for the column's width, so equal to nothing in it.
    assert not value_satisfies(300, Operator.EQ, KeyValue.from_u8(44))
    assert not value_satisfies(300, Operator.GT, KeyValue.from_u8(44))
    assert not value_satisfies(
        1, Operator.EQ, KeyValue.from_string(KeyKind.STRING50, "1")
    )


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
