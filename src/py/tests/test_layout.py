from __future__ import annotations

import pytest
from flatcitybuf.errors import FcbError
from flatcitybuf.layout import check_magic_bytes, compute_layout
from flatcitybuf.layout import rtree_index_size
from flatcitybuf.layout import validate_layout_against_size


def test_magic_ignores_byte_seven() -> None:
    # Byte 7 is written as 0 but never validated (lib.rs:56-58).
    assert check_magic_bytes(b"fcb\x01fcb\x00")
    assert check_magic_bytes(b"fcb\x01fcb\xff")
    assert not check_magic_bytes(b"xcb\x01fcb\x00")


def test_magic_rejects_a_future_version() -> None:
    assert not check_magic_bytes(b"fcb\x02fcb\x00")


def test_rtree_index_size_matches_the_reference_formula() -> None:
    # num_nodes accumulates ceil-divisions until one node remains.
    # NOTE: the task brief's snippet asserted rtree_index_size(1, 16) ==
    # 40, but that contradicts the cited reference formula itself, the
    # Rust source (packed_rtree/mod.rs:879-898: num_nodes starts at 1,
    # then n=ceil_div(1,16)=1 is added *again* before the loop breaks on
    # n==1, giving num_nodes=2), and src/cpp/tests/test_layout.cpp:35-38
    # which asserts 80 with an identical trace. Corrected here to 80 --
    # see task-4-report.md for the full discrepancy writeup.
    assert rtree_index_size(1, 16) == 80
    assert rtree_index_size(16, 16) == (16 + 1) * 40
    assert rtree_index_size(17, 16) == (17 + 2 + 1) * 40


def test_layout_rejects_a_header_larger_than_the_file() -> None:
    layout = compute_layout(header_size=64, features_count=1,
                            index_node_size=16, attr_index_size=0)
    with pytest.raises(FcbError):
        validate_layout_against_size(layout, total_size=10)
