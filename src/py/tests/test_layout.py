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


def test_magic_accepts_version_zero() -> None:
    # test_layout.cpp:19-20 -- b[3] <= VERSION is a forward-compat
    # rejection, not an equality check, so a *past* version (0) is
    # still accepted.
    assert check_magic_bytes(b"fcb\x00fcb\x00")


def test_magic_rejects_a_buffer_shorter_than_eight_bytes() -> None:
    # test_layout.cpp:31-32
    assert not check_magic_bytes(b"fcb\x01")


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
    layout = compute_layout(
        header_size=64, features_count=1, index_node_size=16, attr_index_size=0
    )
    with pytest.raises(FcbError):
        validate_layout_against_size(layout, total_size=10)


def test_rtree_index_size_rejects_a_node_size_below_two() -> None:
    # test_layout.cpp:47-53 -- a node_size below 2 is a corrupt file,
    # not something to clamp.
    with pytest.raises(FcbError):
        rtree_index_size(4, 0)
    with pytest.raises(FcbError):
        rtree_index_size(4, 1)
    assert rtree_index_size(4, 2) == (4 + 2 + 1) * 40


def test_rtree_index_size_rejects_zero_items() -> None:
    # layout.py:62-66 -- num_items == 0 would never terminate the
    # accumulation loop. Not exercised by test_layout.cpp because
    # compute_layout short-circuits features_count == 0 before ever
    # calling rtree_index_size(0, ...); calling it directly pins the
    # guard itself.
    with pytest.raises(FcbError):
        rtree_index_size(0, 16)


def test_rtree_index_size_does_not_clamp_an_out_of_range_node_size() -> None:
    # Finding 4: the Format Reference's `clamp(ns, 2, 65535)` is a no-op
    # in both reference implementations -- Rust asserts node_size >= 2
    # before the clamp ever runs, and C++ takes node_size as a
    # std::uint16_t, so it can never exceed 65535 in the first place.
    # Neither implementation has an observable "clamp" branch to port.
    # Python's plain `int` has no such type ceiling, but every real
    # caller sources this value from an actual 2-byte field, so an
    # out-of-range int here is not a case that occurs on real files.
    # We deliberately did not add a new upper-bound guard (that would
    # be inventing behaviour neither reference exhibits); this test
    # pins the current, documented pass-through behaviour instead.
    # n=4, ns=70000: n=ceil_div(4,70000)=1, num_nodes=4+1=5, break. 5*40
    assert rtree_index_size(4, 70000) == 200


def test_compute_layout_stacks_sections_with_no_padding() -> None:
    # test_layout.cpp:62-72 -- pins every field to a concrete, nonzero
    # value so dropping a term (e.g. rtree_size from attr_index_begin,
    # or attr_index_size from feature_begin) cannot go unnoticed.
    # header_size 100 -> header_len = 8 + 4 + 100 = 112
    layout = compute_layout(
        header_size=100,
        features_count=17,
        index_node_size=16,
        attr_index_size=500,
    )
    assert layout.header_len == 112
    assert layout.rtree_begin == 112
    assert layout.rtree_size == 800
    assert layout.attr_index_begin == 912
    assert layout.attr_index_size == 500
    assert layout.feature_begin == 1412


def test_compute_layout_suppresses_the_rtree_when_it_is_absent() -> None:
    # test_layout.cpp:74-82 -- rtree_size is forced to 0 (short-
    # circuiting rtree_index_size entirely) when either trigger
    # condition holds, instead of raising via rtree_index_size's
    # num_items == 0 guard.
    no_index = compute_layout(
        header_size=100,
        features_count=17,
        index_node_size=0,
        attr_index_size=0,
    )
    assert no_index.rtree_size == 0
    assert no_index.feature_begin == 112

    no_features = compute_layout(
        header_size=100,
        features_count=0,
        index_node_size=16,
        attr_index_size=0,
    )
    assert no_features.rtree_size == 0
    assert no_features.feature_begin == 112


def test_validate_layout_against_size_accepts_an_exact_fit() -> None:
    # test_layout.cpp:95-100 -- feature_begin == total_size is a legal
    # exact fit, not an overflow; only strictly-greater should raise.
    layout = compute_layout(
        header_size=100,
        features_count=17,
        index_node_size=16,
        attr_index_size=500,
    )
    validate_layout_against_size(layout, total_size=1412)


def test_compute_layout_rejects_illegal_header_sizes() -> None:
    # test_layout.cpp:84-89 -- the header-size guard, both directions,
    # including the exact upper boundary (536870912 == 512 MiB).
    with pytest.raises(FcbError):
        compute_layout(
            header_size=7,
            features_count=1,
            index_node_size=16,
            attr_index_size=0,
        )
    with pytest.raises(FcbError):
        compute_layout(
            header_size=536870913,
            features_count=1,
            index_node_size=16,
            attr_index_size=0,
        )
    compute_layout(
        header_size=8, features_count=1, index_node_size=16, attr_index_size=0
    )
    compute_layout(
        header_size=536870912,
        features_count=1,
        index_node_size=16,
        attr_index_size=0,
    )
