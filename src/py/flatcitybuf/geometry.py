from __future__ import annotations

from typing import Any, List, Optional, Sequence, cast

from flatcitybuf.errors import ErrorCode, FcbError

# u32::MAX marks "no index/material/texture here" on the wire and
# becomes JSON null, never the literal 4294967295. Python integers have
# no unsigned overflow, so nothing forces this translation automatically
# -- it must be applied explicitly at every leaf (geometry.cpp:73-77,
# 56-59 in cityjson.cpp for the semantics analogue).
_U32_MAX = 0xFFFFFFFF

_GEOMETRY_TYPE_NAMES = {
    0: "MultiPoint",
    1: "MultiLineString",
    2: "MultiSurface",
    3: "CompositeSurface",
    4: "Solid",
    5: "MultiSolid",
    6: "CompositeSolid",
    7: "GeometryInstance",
}


def geometry_type_name(type_: int) -> str:
    """geometry.cpp:319-332. Raises on any ubyte value outside the
    GeometryType enum (geometry.fbs)."""
    name = _GEOMETRY_TYPE_NAMES.get(type_)
    if name is None:
        raise FcbError(
            ErrorCode.INVALID_FLATBUFFER,
            f"unknown geometry type {type_}",
        )
    return name


def _overrun(what: str) -> None:
    raise FcbError(
        ErrorCode.INVALID_FLATBUFFER,
        f"geometry boundaries overrun in {what}",
    )


class _Cursors:
    """Cursors into the parallel count arrays, shared across nesting
    levels while decode_boundaries recurses. Mirrors geometry.cpp's
    anonymous-namespace `Cursors` struct (geometry.cpp:9-15) -- a plain
    mutable object rather than threading four return values through
    _take_ring/_take_surface/_take_shell by hand.
    """

    __slots__ = ("shell", "surface", "ring", "index")

    def __init__(self) -> None:
        self.shell = 0
        self.surface = 0
        self.ring = 0
        self.index = 0


def _take_ring(
    strings: Sequence[int], indices: Sequence[int], c: _Cursors
) -> List[int]:
    """One ring: `strings[c.ring]` vertex indices taken from `indices`.
    Mirrors geometry.cpp:25-38. Unlike the appearance decoders below,
    this one THROWS on overrun rather than truncating -- boundaries are
    the primary geometry and a short read there is a corrupt file, not
    a normal shape (geometry.cpp:75-92's docstring explains why the two
    decoders differ)."""
    if c.ring >= len(strings):
        _overrun("strings")
    ring_size = strings[c.ring]
    c.ring += 1

    if c.index > len(indices) or len(indices) - c.index < ring_size:
        _overrun("indices")
    ring = list(indices[c.index : c.index + ring_size])
    c.index += ring_size
    return ring


def _take_surface(
    surfaces: Sequence[int],
    strings: Sequence[int],
    indices: Sequence[int],
    c: _Cursors,
) -> List[Any]:
    """One surface: `surfaces[c.surface]` rings. Mirrors
    geometry.cpp:40-51."""
    if c.surface >= len(surfaces):
        _overrun("surfaces")
    ring_count = surfaces[c.surface]
    c.surface += 1
    return [_take_ring(strings, indices, c) for _ in range(ring_count)]


def _take_shell(
    shells: Sequence[int],
    surfaces: Sequence[int],
    strings: Sequence[int],
    indices: Sequence[int],
    c: _Cursors,
) -> List[Any]:
    """One shell: `shells[c.shell]` surfaces. Mirrors
    geometry.cpp:53-64."""
    if c.shell >= len(shells):
        _overrun("shells")
    surface_count = shells[c.shell]
    c.shell += 1
    return [
        _take_surface(surfaces, strings, indices, c)
        for _ in range(surface_count)
    ]


def _collapse(arr: List[Any]) -> List[Any]:
    """The reference collapses a single-element level into that element
    rather than wrapping it (geometry.cpp:66-71). Applied ONLY at the
    outermost level of decode_boundaries/decode_texture_values -- every
    inner level always wraps, regardless of its own length.

    Every call site here passes a list whose single element (when there
    is exactly one) is itself a list -- a shell/surface/ring/solid
    grouping, never a bare scalar -- so the cast below just tells mypy
    what geometry.cpp's dynamically-typed nlohmann::json equivalent
    gets for free.
    """
    if len(arr) == 1:
        return cast(List[Any], arr[0])
    return arr


def _appearance_index(v: int) -> Optional[int]:
    """geometry.cpp:74-77."""
    return None if v == _U32_MAX else v


def _take_appearance_indices(
    vertices: Sequence[int], cursor: int, count: int
) -> tuple[List[Optional[int]], int]:
    """`count` indices from `vertices`, starting at `cursor`. Returns the
    decoded values and the advanced cursor (C++ takes `cursor` by
    reference; Python threads it back out instead).

    Stops early when `vertices` runs out instead of raising:
    geometry.cpp:79-92 guards every push with `if vertex_index <
    vertices.len()`, so a mapping that over-claims yields a SHORT array
    rather than an error, and that is what the expected output contains.
    """
    out: List[Optional[int]] = []
    i = 0
    while i < count and cursor < len(vertices):
        out.append(_appearance_index(vertices[cursor]))
        cursor += 1
        i += 1
    return out, cursor


def _flat_appearance_indices(vertices: Sequence[int]) -> List[Optional[int]]:
    """Every material/texture index, flat -- the shape used whenever a
    mapping carries no usable solids/shells/surfaces structure.
    Mirrors geometry.cpp:96-102."""
    return [_appearance_index(v) for v in vertices]


def decode_boundaries(
    solids: Sequence[int],
    shells: Sequence[int],
    surfaces: Sequence[int],
    strings: Sequence[int],
    indices: Sequence[int],
) -> List[Any]:
    """Decodes the flattened solids/shells/surfaces/strings/indices
    arrays back into a nested CityJSON `boundaries` structure. Mirrors
    fcb::decode_boundaries (geometry.cpp:110-172).

    Dispatches on the outermost POPULATED array, not on the geometry
    type: that is what the reference does, and it keeps this function
    in step with decode_material_values/decode_texture_values even for
    geometry types whose nesting depth is otherwise ambiguous.

    Raises FcbError(INVALID_FLATBUFFER) if a ring or surface claims more
    entries than the backing array holds -- unlike the appearance
    decoders below, boundaries are not guarded against overrun in the
    reference; a short read there is a corrupt file.
    """
    c = _Cursors()

    if solids:
        solids_out: List[Any] = [
            [
                _take_shell(shells, surfaces, strings, indices, c)
                for _ in range(solids[s])
            ]
            for s in range(len(solids))
        ]
        return _collapse(solids_out)

    if shells:
        shells_out: List[Any] = []
        for s in range(len(shells)):
            surface_count = shells[s]
            # This level consumes the shell entry directly (by index
            # `s`, not via take_shell); c.shell is bumped to stay in
            # step with the reference but is not read again here.
            c.shell += 1
            shells_out.append(
                [
                    _take_surface(surfaces, strings, indices, c)
                    for _ in range(surface_count)
                ]
            )
        return _collapse(shells_out)

    if surfaces:
        surfaces_out: List[Any] = []
        for s in range(len(surfaces)):
            ring_count = surfaces[s]
            c.surface += 1  # vestigial, see the shells branch above
            surfaces_out.append(
                [_take_ring(strings, indices, c) for _ in range(ring_count)]
            )
        return _collapse(surfaces_out)

    if strings:
        strings_out: List[Any] = [
            _take_ring(strings, indices, c) for _ in range(len(strings))
        ]
        return _collapse(strings_out)

    # No count arrays at all: a flat list of vertex indices (MultiPoint).
    return list(indices)


def decode_material_values(
    solids: Sequence[int],
    shells: Sequence[int],
    vertices: Sequence[int],
) -> List[Any]:
    """Decodes a MaterialMapping's solids/shells/vertices into the
    nested `values` array CityJSON expects under `material.<theme>`.
    Mirrors fcb::decode_material_values (geometry.cpp:174-208).

    Regression (upstream finding #8): a single Solid -- solids == [N]
    for ANY N, including N == 1 -- drops the solid wrapper and returns
    one array of indices per shell. This is guarded on `len(solids) ==
    1` alone; an earlier `solids[0] > 1` guard sent a Solid with exactly
    one shell (the commonest shape there is) into the MultiSolid branch
    below, returning it one level deeper than it was written.
    """
    # No structure to rebuild: one index per surface, in file order.
    # Covers MultiSurface/CompositeSurface, and also a mapping that
    # declares solids but no shells -- the reference falls back to flat
    # there too.
    if not solids or not shells:
        return list(_flat_appearance_indices(vertices))

    vertex = 0
    shell = 0

    if len(solids) == 1:
        out: List[Any] = []
        i = 0
        while i < solids[0] and shell < len(shells):
            values, vertex = _take_appearance_indices(
                vertices, vertex, shells[shell]
            )
            out.append(values)
            shell += 1
            i += 1
        return out

    # MultiSolid/CompositeSolid: solid -> shell -> indices.
    out = []
    for s in range(len(solids)):
        solid: List[Any] = []
        i = 0
        while i < solids[s] and shell < len(shells):
            values, vertex = _take_appearance_indices(
                vertices, vertex, shells[shell]
            )
            solid.append(values)
            shell += 1
            i += 1
        # Pushed even when the shell array ran out mid-solid, matching
        # the reference: a truncated walk still contributes an (empty)
        # entry.
        out.append(solid)
    return out


class _TexCursors:
    """Cursors for the texture walk. Separate from _Cursors because the
    texture arrays are walked with skip-on-exhaustion (like the
    appearance decoders), not throw-on-overrun (like decode_boundaries).
    Mirrors geometry.cpp's anonymous-namespace `TexCursors` struct
    (geometry.cpp:214-219)."""

    __slots__ = ("shell", "surface", "string", "vertex")

    def __init__(self) -> None:
        self.shell = 0
        self.surface = 0
        self.string = 0
        self.vertex = 0


def _take_tex_surface(
    surfaces: Sequence[int],
    strings: Sequence[int],
    vertices: Sequence[int],
    c: _TexCursors,
) -> List[Any]:
    """One surface: `surfaces[c.surface]` rings, each a (texture index,
    UVs) list. The caller has already checked that `surfaces` is not
    exhausted. Mirrors geometry.cpp:223-231."""
    ring_count = surfaces[c.surface]
    c.surface += 1
    out = []
    i = 0
    while i < ring_count and c.string < len(strings):
        values, c.vertex = _take_appearance_indices(
            vertices, c.vertex, strings[c.string]
        )
        c.string += 1
        out.append(values)
        i += 1
    return out


def _take_tex_shell(
    shells: Sequence[int],
    surfaces: Sequence[int],
    strings: Sequence[int],
    vertices: Sequence[int],
    c: _TexCursors,
) -> List[Any]:
    """One shell: `shells[c.shell]` surfaces. Caller has checked
    `shells`. Mirrors geometry.cpp:234-242."""
    surface_count = shells[c.shell]
    c.shell += 1
    out = []
    i = 0
    while i < surface_count and c.surface < len(surfaces):
        out.append(_take_tex_surface(surfaces, strings, vertices, c))
        i += 1
    return out


def decode_texture_values(
    solids: Sequence[int],
    shells: Sequence[int],
    surfaces: Sequence[int],
    strings: Sequence[int],
    vertices: Sequence[int],
) -> List[Any]:
    """Decodes a TextureMapping's solids/shells/surfaces/strings/
    vertices into the nested `values` array CityJSON expects under
    `texture.<theme>`. Mirrors fcb::decode_texture_values
    (geometry.cpp:246-315).

    The branches below are the reference's, in its exact order. They
    are NOT mutually exclusive by geometry type -- several test
    length == 1 on more than one array -- so reordering them changes
    the output for inputs that satisfy more than one branch's guard.

    Regression (upstream finding #8): a single-string MultiLineString
    is not guarded by `len(strings) > 1` -- that guard used to send a
    lone string down to the "no surface grouping" branch, gaining a
    level. The MultiSurface look-alike (`surfaces == [1]` alongside a
    `shells == [1]` entry) is still distinguished correctly, because
    the shells branch above claims it first.
    """
    c = _TexCursors()

    if solids:
        solids_out: List[Any] = []
        for s in range(len(solids)):
            solid: List[Any] = []
            i = 0
            while i < solids[s] and c.shell < len(shells):
                solid.append(
                    _take_tex_shell(shells, surfaces, strings, vertices, c)
                )
                i += 1
            solids_out.append(solid)
        # Collapse ONLY here, at the outermost level, and only for a
        # single solid. Every inner level always wraps.
        return _collapse(solids_out)

    # A single shell of surfaces (MultiSurface written with a shell
    # entry). Guarded on shells.size() == 1: two shells fall through to
    # the surface branch below, which ignores `shells` entirely.
    if shells and surfaces and len(shells) == 1:
        shell_out: List[Any] = []
        i = 0
        while i < shells[0] and c.surface < len(surfaces):
            shell_out.append(_take_tex_surface(surfaces, strings, vertices, c))
            i += 1
        return shell_out

    # One surface holding rings: MultiLineString, whose strings are the
    # lines. Yields the ring list without the surface wrapper. Not
    # guarded on strings.size() > 1 -- see the regression note above.
    if len(surfaces) == 1 and strings:
        line_out: List[Any] = []
        i = 0
        while i < surfaces[0] and c.string < len(strings):
            values, c.vertex = _take_appearance_indices(
                vertices, c.vertex, strings[c.string]
            )
            c.string += 1
            line_out.append(values)
            i += 1
        return line_out

    # MultiSurface/CompositeSurface: surface -> ring.
    if surfaces:
        return [
            _take_tex_surface(surfaces, strings, vertices, c)
            for _ in range(len(surfaces))
        ]

    # Rings with no surface grouping.
    if len(strings) > 1:
        rings_out: List[Any] = []
        for s in range(len(strings)):
            values, c.vertex = _take_appearance_indices(
                vertices, c.vertex, strings[s]
            )
            rings_out.append(values)
        return rings_out

    # MultiPoint, or a single ring: a flat index list.
    return list(_flat_appearance_indices(vertices))
