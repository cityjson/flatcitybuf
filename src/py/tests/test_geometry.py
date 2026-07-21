from __future__ import annotations

import pytest
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.geometry import decode_boundaries
from flatcitybuf.geometry import decode_material_values
from flatcitybuf.geometry import decode_texture_values
from flatcitybuf.geometry import geometry_type_name

# Every expected value below is copied verbatim from
# src/cpp/tests/test_geometry.cpp, which the task brief identifies as
# already-verified output obtained from the Rust functions via the
# oracle technique (geom_decoder.rs). None are hand-derived here.

_NONE = 0xFFFFFFFF


# --------------------------------------------------------- the brief ---


def test_a_solid_of_one_shell_drops_the_solid_level() -> None:
    # Regression (finding #8): solids == [1] must NOT fall into the
    # MultiSolid branch just because it isn't guarded by `solids[0] > 1`.
    assert decode_material_values([1], [2], [7, 8]) == [[7, 8]]


def test_single_string_multilinestring_keeps_its_depth() -> None:
    # Regression: a single-string MultiLineString must not be collapsed
    # via a `len(strings) > 1` guard.
    assert decode_texture_values([], [], [1], [4], [0, 10, 11, 12]) == [
        [0, 10, 11, 12]
    ]
    # The MultiSurface look-alike is distinguished by its shells entry.
    assert decode_texture_values([], [1], [1], [4], [0, 10, 11, 12]) == [
        [[0, 10, 11, 12]]
    ]


def test_max_u32_becomes_none() -> None:
    assert decode_material_values([], [], [0xFFFFFFFF, 1, 0]) == [
        None,
        1,
        0,
    ]


# --------------------------------------------------- decode_boundaries ---


def test_a_flat_index_list_decodes_as_multipoint() -> None:
    assert decode_boundaries([], [], [], [], [0, 1, 2]) == [0, 1, 2]


def test_collapse_applies_only_at_the_outermost_level() -> None:
    # The spec's example: boundaries [0,1,2], strings [3], surfaces [1].
    # One surface holding one ring yields [[0,1,2]] -- the surface level
    # collapses away, the ring level does not.
    assert decode_boundaries([], [], [1], [3], [0, 1, 2]) == [[0, 1, 2]]


def test_two_surfaces_of_one_ring_each_stay_nested() -> None:
    b = decode_boundaries([], [], [1, 1], [3, 3], [0, 1, 2, 3, 4, 5])
    assert b == [[[0, 1, 2]], [[3, 4, 5]]]


def test_a_surface_with_an_inner_ring_keeps_both_rings() -> None:
    b = decode_boundaries([], [], [2], [4, 3], [0, 1, 2, 3, 10, 11, 12])
    assert b == [[0, 1, 2, 3], [10, 11, 12]]


def test_a_solid_nests_solid_shell_surface_ring() -> None:
    b = decode_boundaries([1], [2], [1, 1], [3, 3], [0, 1, 2, 3, 4, 5])
    # The single solid collapses away, leaving the shell list: one shell
    # holding two surfaces, each wrapped around its single ring.
    assert b == [[[[0, 1, 2]], [[3, 4, 5]]]]


def test_two_solids_do_not_collapse() -> None:
    b = decode_boundaries([1, 1], [1, 1], [1, 1], [3, 3], [0, 1, 2, 3, 4, 5])
    assert isinstance(b, list)
    assert len(b) == 2


def test_a_ring_claiming_more_indices_than_exist_throws() -> None:
    with pytest.raises(FcbError) as exc_info:
        decode_boundaries([], [], [], [99], [0, 1, 2])
    assert exc_info.value.code is ErrorCode.INVALID_FLATBUFFER


def test_a_surface_claiming_more_rings_than_exist_throws() -> None:
    with pytest.raises(FcbError) as exc_info:
        decode_boundaries([], [], [5], [3], [0, 1, 2])
    assert exc_info.value.code is ErrorCode.INVALID_FLATBUFFER


# ---------------------------------------------- decode_material_values ---


def test_material_values_are_flat_when_no_solids() -> None:
    m = decode_material_values([], [], [_NONE, 1, 0])
    assert m == [None, 1, 0]


def test_material_values_stay_flat_when_solids_without_shells() -> None:
    m = decode_material_values([2], [], [0, 1])
    assert m == [0, 1]


def test_one_solid_of_several_shells_drops_the_solid_level() -> None:
    m = decode_material_values([2], [3, 3], [0, 1, _NONE, 2, 3, 4])
    assert m == [[0, 1, None], [2, 3, 4]]


def test_several_solids_nest_solid_shell_indices() -> None:
    m = decode_material_values([1, 1], [1, 1], [5, 6])
    assert m == [[[5]], [[6]]]


def test_material_values_truncate_rather_than_throw() -> None:
    # shells run out inside a single solid: entries are dropped.
    assert decode_material_values([3], [1, 1], [1, 2]) == [[1], [2]]
    # shells run out across solids: the solid stays, empty.
    assert decode_material_values([1, 1], [1], [9]) == [[[9]], []]
    # vertices run out mid-shell: that shell is short.
    assert decode_material_values([2], [3, 3], [1, 2]) == [[1, 2], []]


# ----------------------------------------------- decode_texture_values ---


def test_texture_values_for_a_single_shell_of_surfaces() -> None:
    # geom_temp's shape: shells == [n] with one entry per surface.
    t = decode_texture_values([], [2], [1, 1], [3, 2], [0, 10, 11, 1, 20])
    assert t == [[[0, 10, 11]], [[1, 20]]]


def test_a_single_solid_collapses_only_at_the_outermost_level() -> None:
    t = decode_texture_values([1], [1], [1], [3], [0, 1, 2])
    assert t == [[[[0, 1, 2]]]]


def test_two_solids_keep_the_outermost_level() -> None:
    t = decode_texture_values([1, 1], [1, 1], [1, 1], [1, 1], [7, 8])
    assert t == [[[[[7]]]], [[[[8]]]]]


def test_one_surface_of_rings_is_a_multilinestring() -> None:
    t = decode_texture_values([], [], [2], [3, 3], [0, 10, 20, 1, 11, 21])
    assert t == [[0, 10, 20], [1, 11, 21]]


def test_several_surfaces_nest_surface_ring() -> None:
    t = decode_texture_values([], [], [1, 2], [2, 2, 2], [0, 1, 2, 3, 4, 5])
    assert t == [[[0, 1]], [[2, 3], [4, 5]]]


def test_more_than_one_shell_without_solids_discards_shell_structure() -> None:
    # The shell branch is guarded on shells.size() == 1, so two shells
    # fall through to the surface branch and `shells` is never read.
    t = decode_texture_values([], [1, 1], [1, 1], [1, 1], [3, 4])
    assert t == [[[3]], [[4]]]


def test_texture_values_with_no_count_arrays_are_a_flat_list() -> None:
    assert decode_texture_values([], [], [], [], [0, _NONE, 2]) == [
        0,
        None,
        2,
    ]
    # A lone `strings` entry takes the same branch, and the whole vertex
    # list is emitted even if that entry disagrees with its length.
    assert decode_texture_values([], [], [], [2], [0, _NONE, 2]) == [
        0,
        None,
        2,
    ]


def test_rings_with_no_surface_grouping_stay_a_ring_list() -> None:
    t = decode_texture_values([], [], [], [2, 1], [1, 2, 3])
    assert t == [[1, 2], [3]]


def test_texture_values_truncate_rather_than_throw() -> None:
    # strings run out: the later surface stays, empty.
    assert decode_texture_values([], [], [2, 1], [3], [0, 1]) == [[[0, 1]], []]
    # a solid with no shells collapses to an empty array -- NOT the flat
    # vertex list: the solids branch is taken on `solids` alone, so the
    # vertices are never reached. Diverges from materials, where the
    # same input falls back to a flat list.
    assert decode_texture_values([1], [], [], [], [7]) == []
    assert decode_material_values([1], [], [7]) == [7]
    # shells run out across solids: the trailing solid stays, empty.
    assert decode_texture_values([1, 1], [1], [1], [1], [9]) == [[[[[9]]]], []]


# ------------------------------------------------------ geometry types ---


def test_geometry_type_names_match_cityjson_spelling() -> None:
    assert geometry_type_name(0) == "MultiPoint"
    assert geometry_type_name(2) == "MultiSurface"
    assert geometry_type_name(4) == "Solid"
    assert geometry_type_name(6) == "CompositeSolid"


def test_geometry_type_name_rejects_nonsense() -> None:
    with pytest.raises(FcbError) as exc_info:
        geometry_type_name(99)
    assert exc_info.value.code is ErrorCode.INVALID_FLATBUFFER
