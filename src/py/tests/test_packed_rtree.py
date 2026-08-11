from __future__ import annotations

import struct
from pathlib import Path

import pytest
from flatcitybuf.errors import FcbError
from flatcitybuf.header import read_header
from flatcitybuf.layout import NODE_ITEM_SIZE
from flatcitybuf.packed_rtree import NodeItem
from flatcitybuf.packed_rtree import SearchResultItem
from flatcitybuf.packed_rtree import search_rtree
from flatcitybuf.range_reader import FileRangeReader
from flatcitybuf.reader import FcbReader

CORPUS = Path(__file__).resolve().parents[3] / "conformance"

# Expected values below come from two oracles:
#
# 1. `cargo run -p fcb_core --example oracle_bbox` -- a temporary example
#    added under src/rust/fcb_core/examples/oracle_bbox.rs that opened
#    conformance/small.fcb and conformance/degenerate_extent.fcb with
#    FcbReader::open(..).select_query(Query::BBox(..)) (the Rust reader,
#    fcb_core/src/reader/mod.rs) and printed the resulting feature ids.
#    The example was deleted again after recording its output here; see
#    task-9-report.md for the full transcript. This gives:
#      - small.fcb, bbox covering everything -> all 3 ids, in stored
#        (Hilbert) order: 016459, 005156, 012869 (matches
#        test_features.py's independently-established select_all order).
#      - small.fcb, bbox far outside -> 0 hits.
#      - small.fcb, tight bbox around 012869's own vertex extent (decoded
#        with the file's transform: x IN [84593.2, 84597.5], y IN
#        [446459.6, 446462.8], well clear of the other two buildings in
#        x) -> exactly 1 hit: 012869.
#      - degenerate_extent.fcb, bbox (0, 0, 0, 0) (the header's own
#        degenerate extent) -> all 3 ids (p0, p1, p2).
# 2. Hand-derived (flagged explicitly): the synthetic 3-node tree in
#    test_search_rtree_distinguishes_node_index_from_byte_offset is
#    built by hand from the documented NodeItem layout and
#    generate_level_bounds formula (packed_rtree/mod.rs:342-375,
#    cited in the Format Reference), not read from a real reference
#    implementation run. It is a unit test of the decode/traversal
#    arithmetic in isolation, not a conformance oracle.


class InMemoryReader:
    """Minimal in-memory RangeReader for synthetic R-tree buffers.
    Mirrors the CountingReader pattern in test_range_reader.py."""

    def __init__(self, data: bytes) -> None:
        self.data = data
        self.reads: list[tuple[int, int]] = []

    def read(self, offset: int, length: int) -> bytes:
        self.reads.append((offset, length))
        return self.data[offset : offset + length]

    def total_size(self) -> int:
        return len(self.data)


def _encode_node(
    min_x: float, min_y: float, max_x: float, max_y: float, offset: int
) -> bytes:
    return struct.pack("<4dQ", min_x, min_y, max_x, max_y, offset)


# --------------------------------------------------------- the brief ---


def test_bbox_covering_everything_returns_every_feature_in_small_fcb() -> None:
    r = FcbReader.open_file(CORPUS / "small.fcb")
    offset_to_id = {f.byte_offset: f.id for f in r.select_all()}

    reader = FileRangeReader(CORPUS / "small.fcb")
    header = read_header(reader)
    hits = search_rtree(
        reader,
        header.layout.rtree_begin,
        header.info.features_count,
        header.info.index_node_size,
        (-1e9, -1e9, 1e9, 1e9),
    )

    ids = [offset_to_id[h.offset] for h in hits]
    assert ids == [
        "NL.IMBAG.Pand.0503100000016459",
        "NL.IMBAG.Pand.0503100000005156",
        "NL.IMBAG.Pand.0503100000012869",
    ]


def test_bbox_outside_the_extent_returns_nothing() -> None:
    reader = FileRangeReader(CORPUS / "small.fcb")
    header = read_header(reader)
    hits = search_rtree(
        reader,
        header.layout.rtree_begin,
        header.info.features_count,
        header.info.index_node_size,
        (-1e9, -1e9, -1e9 + 1.0, -1e9 + 1.0),
    )
    assert hits == []


def test_bbox_around_one_known_building_returns_exactly_it() -> None:
    r = FcbReader.open_file(CORPUS / "small.fcb")
    offset_to_id = {f.byte_offset: f.id for f in r.select_all()}

    reader = FileRangeReader(CORPUS / "small.fcb")
    header = read_header(reader)
    hits = search_rtree(
        reader,
        header.layout.rtree_begin,
        header.info.features_count,
        header.info.index_node_size,
        (84590.0, 446455.0, 84600.0, 446465.0),
    )

    assert len(hits) == 1
    assert offset_to_id[hits[0].offset] == "NL.IMBAG.Pand.0503100000012869"


def test_degenerate_zero_area_extent_does_not_break_the_query() -> None:
    reader = FileRangeReader(CORPUS / "degenerate_extent.fcb")
    header = read_header(reader)
    assert header.info.geographical_extent is not None
    ext = header.info.geographical_extent
    hits = search_rtree(
        reader,
        header.layout.rtree_begin,
        header.info.features_count,
        header.info.index_node_size,
        (ext[0], ext[1], ext[3], ext[4]),
    )
    assert len(hits) == header.info.features_count


# ------------------------------------------------------- NodeItem ---


def test_node_item_decodes_40_little_endian_bytes() -> None:
    # Codex review (Task 12): all-1.0 coordinates plus checking only two
    # of five fields would pass even with fields swapped (e.g. min_y and
    # max_x transposed). Distinct values and every field, instead.
    raw = _encode_node(1.0, 2.0, 3.0, 4.0, 42)
    n = NodeItem.decode(raw)
    assert n.min_x == 1.0
    assert n.min_y == 2.0
    assert n.max_x == 3.0
    assert n.max_y == 4.0
    assert n.offset == 42


def test_node_item_offset_is_unsigned_past_2_63() -> None:
    # Gotcha 1: offset is a u64, decoded with "<Q". Decoding with "<q"
    # (signed) would turn any offset >= 2**63 negative -- the single most
    # likely way to get this task wrong. Use a value only representable
    # correctly as unsigned.
    big = (1 << 63) + 12345
    raw = _encode_node(0.0, 0.0, 0.0, 0.0, big)
    n = NodeItem.decode(raw)
    assert n.offset == big
    assert n.offset > 0


def test_node_item_intersects_boundary_semantics() -> None:
    # packed_rtree.cpp's NodeItem::intersects (mod.rs:122-134 origin):
    # strict < and > only, so TOUCHING edges DO intersect.
    n = NodeItem(min_x=0.0, min_y=0.0, max_x=10.0, max_y=10.0, offset=0)

    assert n.intersects((5.0, 5.0, 6.0, 6.0))  # fully inside
    assert n.intersects((-5.0, -5.0, 5.0, 5.0))  # overlapping
    assert n.intersects((-5.0, -5.0, 20.0, 20.0))  # enclosing
    assert n.intersects((10.0, 10.0, 20.0, 20.0))  # corner touch
    assert n.intersects((-5.0, -5.0, 0.0, 0.0))  # corner touch

    assert not n.intersects((10.1, 0.0, 20.0, 10.0))  # past max_x
    assert not n.intersects((-20.0, 0.0, -0.1, 10.0))  # before min_x
    assert not n.intersects((0.0, 10.1, 10.0, 20.0))  # past max_y
    assert not n.intersects((0.0, -20.0, 10.0, -0.1))  # before min_y


# ------------------------------------------------- level bounds rules ---


def test_search_rtree_distinguishes_node_index_from_byte_offset() -> None:
    # Hand-built 3-node tree: num_items=2, node_size=2.
    # level_num_nodes = [2, 1] -> num_nodes = 3.
    # level_bounds[0] (leaf) = (1, 3); level_bounds[1] (root) = (0, 1).
    # So the LEAF level occupies the higher indices (1, 2) and is stored
    # LAST, while the root (index 0) comes first in storage -- Format
    # Reference: "level_bounds[0] is the leaf level and is last in
    # storage order".
    #
    # Root's own `offset` field is 1: a CHILD NODE INDEX (the first
    # leaf), not a byte offset. Each leaf's `offset` field (100, 200) IS
    # a byte offset into the features section. Conflating the two would
    # either send the root query into "node 1" as a byte position (wrong
    # kind of number) or return leaf byte offsets 100/200 as if they were
    # further node indices.
    root = _encode_node(0.0, 0.0, 20.0, 20.0, 1)
    leaf_a = _encode_node(0.0, 0.0, 1.0, 1.0, 100)
    leaf_b = _encode_node(10.0, 10.0, 11.0, 11.0, 200)
    buf = root + leaf_a + leaf_b

    reader = InMemoryReader(buf)

    hits_a = search_rtree(reader, 0, 2, 2, (0.0, 0.0, 1.0, 1.0))
    assert hits_a == [SearchResultItem(offset=100, index=0)]

    hits_b = search_rtree(reader, 0, 2, 2, (10.0, 10.0, 11.0, 11.0))
    assert hits_b == [SearchResultItem(offset=200, index=1)]

    hits_both = search_rtree(reader, 0, 2, 2, (0.0, 0.0, 20.0, 20.0))
    assert hits_both == [
        SearchResultItem(offset=100, index=0),
        SearchResultItem(offset=200, index=1),
    ]


def test_search_rtree_results_are_sorted_by_offset() -> None:
    # Leaves stored in index order (0, 1) but with byte offsets that
    # happen to run backwards (300, then 100), so a naive "results in
    # traversal order" implementation would return them unsorted.
    root = _encode_node(0.0, 0.0, 20.0, 20.0, 1)
    leaf_a = _encode_node(0.0, 0.0, 1.0, 1.0, 300)
    leaf_b = _encode_node(10.0, 10.0, 11.0, 11.0, 100)
    buf = root + leaf_a + leaf_b
    reader = InMemoryReader(buf)

    hits = search_rtree(reader, 0, 2, 2, (0.0, 0.0, 20.0, 20.0))
    assert [h.offset for h in hits] == [100, 300]


# ---------------------------------------------------------- guards ---


def test_search_rtree_rejects_a_node_size_below_two() -> None:
    reader = InMemoryReader(b"\x00" * NODE_ITEM_SIZE * 4)
    with pytest.raises(FcbError):
        search_rtree(reader, 0, 4, 1, (0.0, 0.0, 1.0, 1.0))
    with pytest.raises(FcbError):
        search_rtree(reader, 0, 4, 0, (0.0, 0.0, 1.0, 1.0))


def test_search_rtree_with_zero_items_returns_empty_without_reading() -> None:
    reader = InMemoryReader(b"")
    hits = search_rtree(reader, 0, 0, 16, (0.0, 0.0, 1.0, 1.0))
    assert hits == []
    assert reader.reads == []


def test_search_rtree_raises_on_a_truncated_node_block() -> None:
    # Only one full node's worth of bytes for a tree that claims two
    # items (three nodes, 120 bytes) -- the file is corrupt/truncated,
    # and the RangeReader contract can only ever return the bytes that
    # exist, so search_rtree must raise rather than decode a short slice.
    root = _encode_node(0.0, 0.0, 20.0, 20.0, 1)
    reader = InMemoryReader(root)
    with pytest.raises(FcbError):
        search_rtree(reader, 0, 2, 2, (0.0, 0.0, 20.0, 20.0))


def test_search_rtree_rejects_a_corrupt_child_index_outside_its_level() -> (
    None
):
    # Root claims child node index 5, but with num_items=2, node_size=2
    # the only valid leaf indices are 1 and 2. A corrupt/hostile file
    # must not be followed off the end of the tree.
    root = _encode_node(0.0, 0.0, 20.0, 20.0, 5)
    leaf_a = _encode_node(0.0, 0.0, 1.0, 1.0, 100)
    leaf_b = _encode_node(10.0, 10.0, 11.0, 11.0, 200)
    buf = root + leaf_a + leaf_b
    reader = InMemoryReader(buf)
    with pytest.raises(FcbError):
        search_rtree(reader, 0, 2, 2, (0.0, 0.0, 20.0, 20.0))


def test_search_rtree_rejects_a_child_index_misaligned_to_its_group() -> None:
    # Codex review (Task 12): with num_items=2, node_size=2 the leaf
    # level is node indices [1, 3) and its only group starts at 1, so a
    # root child offset of 2 -- IN range but not group-aligned -- used
    # to pass the outside-the-level check above and then get read as
    # if it were the start of a node_size=2 group: `end = min(2 + 2,
    # 3) = 3`, so only the leaf at index 2 (offset 200) was visited and
    # the one at index 1 (offset 100) silently never was, with NO
    # error raised at all. A corrupt/hostile file must be rejected, not
    # silently under-report matches.
    root = _encode_node(0.0, 0.0, 20.0, 20.0, 2)
    leaf_a = _encode_node(0.0, 0.0, 1.0, 1.0, 100)
    leaf_b = _encode_node(10.0, 10.0, 11.0, 11.0, 200)
    buf = root + leaf_a + leaf_b
    reader = InMemoryReader(buf)
    with pytest.raises(FcbError):
        search_rtree(reader, 0, 2, 2, (0.0, 0.0, 20.0, 20.0))


def test_a_multi_level_search_buffers_instead_of_reading_per_node() -> None:
    # The bbox phase is one of four that read over a RangeReader, and it
    # was the only one not wrapping it in a BufferedRangeReader: every
    # R-tree node cost its own physical read. Over HTTP that is one
    # request per node -- a bbox query on the 68 GB 3DBAG file cost 240
    # requests where the C++ reader cost 37, for identical results.
    #
    # Asserted as "strictly fewer physical reads than nodes visited"
    # rather than an exact count, so the test pins the property (reads
    # are combined) and not the window size.
    reader = FileRangeReader(CORPUS / "small.fcb")
    header = read_header(reader)

    counting = InMemoryReader(Path(CORPUS / "small.fcb").read_bytes())
    hits = search_rtree(
        counting,
        header.layout.rtree_begin,
        header.info.features_count,
        header.info.index_node_size,
        (-1e9, -1e9, 1e9, 1e9),
    )

    assert len(hits) == 3
    # One buffered window covers this whole tiny index.
    assert len(counting.reads) == 1, counting.reads
