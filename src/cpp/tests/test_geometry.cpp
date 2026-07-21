#include <doctest/doctest.h>

#include <fcb/geometry.hpp>

#include <cstdint>
#include <vector>

using namespace fcb;
using nlohmann::json;

static UIntView v(const std::vector<std::uint32_t>& x) { return UIntView(x.data(), x.size()); }

static constexpr std::uint32_t kNone = UINT32_MAX;

// Every expected value below was produced by RUNNING THE RUST REFERENCE, not
// by hand-derivation: a temporary `oracle_dump` test was injected into
// fcb_core/src/reader/geom_decoder.rs's test module, run with --nocapture, and
// its printed output pinned here. See the task report for the raw dump. The
// injection was reverted; `git status` in src/rust is clean.

// ------------------------------------------------------------ boundaries ---
//
// The nesting depth comes from the geometry type alone. The count arrays are
// ambiguous: the encoder writes one redundant count level above the geometry's
// own depth, so a Solid and a one-solid MultiSolid store byte-identical
// arrays. Anything that dispatches on "which array is populated" gets one of
// the two wrong, whichever way it guesses.

TEST_CASE("MultiPoint boundaries are the flat index list") {
    std::vector<std::uint32_t> idx = {0, 1, 2};
    auto b = decode_boundaries(GeometryKind::MultiPoint, UIntView(), UIntView(), UIntView(),
                               UIntView(), v(idx));
    CHECK(b == json::parse("[0,1,2]"));
}

TEST_CASE("MultiLineString boundaries are one ring per string") {
    // A SINGLE string is the interesting case: it is indistinguishable from a
    // one-ring MultiSurface by the arrays alone.
    std::vector<std::uint32_t> strings = {4}, idx = {0, 1, 2, 3};
    CHECK(decode_boundaries(GeometryKind::MultiLineString, UIntView(), UIntView(), UIntView(),
                            v(strings), v(idx)) == json::parse("[[0,1,2,3]]"));

    std::vector<std::uint32_t> two = {3, 3}, idx6 = {0, 1, 2, 3, 4, 5};
    CHECK(decode_boundaries(GeometryKind::MultiLineString, UIntView(), UIntView(), UIntView(),
                            v(two), v(idx6)) == json::parse("[[0,1,2],[3,4,5]]"));
}

TEST_CASE("MultiSurface boundaries are three levels deep, never collapsed") {
    // One surface of one ring. The old decoder collapsed the outermost
    // single-element level and returned [[0,1,2]] -- two levels, one short.
    std::vector<std::uint32_t> surfaces = {1}, strings = {3}, idx = {0, 1, 2};
    CHECK(decode_boundaries(GeometryKind::MultiSurface, UIntView(), UIntView(), v(surfaces),
                            v(strings), v(idx)) == json::parse("[[[0,1,2]]]"));
    CHECK(decode_boundaries(GeometryKind::CompositeSurface, UIntView(), UIntView(), v(surfaces),
                            v(strings), v(idx)) == json::parse("[[[0,1,2]]]"));
}

TEST_CASE("a MultiSurface with an inner ring keeps both rings in one surface") {
    std::vector<std::uint32_t> surfaces = {2}, strings = {4, 3};
    std::vector<std::uint32_t> idx = {0, 1, 2, 3, 10, 11, 12};
    CHECK(decode_boundaries(GeometryKind::MultiSurface, UIntView(), UIntView(), v(surfaces),
                            v(strings), v(idx)) == json::parse("[[[0,1,2,3],[10,11,12]]]"));
}

TEST_CASE("Solid boundaries are four levels deep and ignore the solids array") {
    // One shell of two surfaces. `solids` holds one redundant entry, which a
    // type-driven reader never reads.
    std::vector<std::uint32_t> solids = {1}, shells = {2}, surfaces = {1, 1};
    std::vector<std::uint32_t> strings = {3, 3}, idx = {0, 1, 2, 3, 4, 5};
    CHECK(decode_boundaries(GeometryKind::Solid, v(solids), v(shells), v(surfaces), v(strings),
                            v(idx)) == json::parse("[[[[0,1,2]],[[3,4,5]]]]"));
}

TEST_CASE("the SAME arrays give a MultiSolid one more level than a Solid") {
    // This is finding #8 in one assertion, on the boundary side.
    std::vector<std::uint32_t> solids = {1}, shells = {2}, surfaces = {1, 1};
    std::vector<std::uint32_t> strings = {3, 3}, idx = {0, 1, 2, 3, 4, 5};

    CHECK(decode_boundaries(GeometryKind::Solid, v(solids), v(shells), v(surfaces), v(strings),
                            v(idx)) == json::parse("[[[[0,1,2]],[[3,4,5]]]]"));
    CHECK(decode_boundaries(GeometryKind::MultiSolid, v(solids), v(shells), v(surfaces),
                            v(strings), v(idx)) == json::parse("[[[[[0,1,2]],[[3,4,5]]]]]"));
    CHECK(decode_boundaries(GeometryKind::CompositeSolid, v(solids), v(shells), v(surfaces),
                            v(strings), v(idx)) == json::parse("[[[[[0,1,2]],[[3,4,5]]]]]"));
}

TEST_CASE("two solids each keep their own shell list") {
    std::vector<std::uint32_t> solids = {1, 1}, shells = {1, 1}, surfaces = {1, 1};
    std::vector<std::uint32_t> strings = {3, 3}, idx = {0, 1, 2, 3, 4, 5};
    CHECK(decode_boundaries(GeometryKind::CompositeSolid, v(solids), v(shells), v(surfaces),
                            v(strings), v(idx)) ==
          json::parse("[[[[[0,1,2]]]],[[[[3,4,5]]]]]"));
}

TEST_CASE("a ring claiming more indices than exist throws") {
    // A deliberate divergence, documented in geometry.hpp: the reference
    // clamps and yields a short array; we choose to report the corruption
    // instead. Clamping would be equally safe here -- the appearance decoders
    // below do it -- so this is a choice about what a reader should tell its
    // caller, not a C++ constraint. Only reachable on a corrupt file.
    std::vector<std::uint32_t> strings = {99};
    std::vector<std::uint32_t> idx = {0, 1, 2};
    CHECK_THROWS_AS(decode_boundaries(GeometryKind::MultiLineString, UIntView(), UIntView(),
                                      UIntView(), v(strings), v(idx)),
                    Error);
}

TEST_CASE("a surface claiming more rings than exist throws") {
    std::vector<std::uint32_t> surfaces = {5}, strings = {3};
    std::vector<std::uint32_t> idx = {0, 1, 2};
    CHECK_THROWS_AS(decode_boundaries(GeometryKind::MultiSurface, UIntView(), UIntView(),
                                      v(surfaces), v(strings), v(idx)),
                    Error);
}

// ------------------------------------------------------------- semantics ---

TEST_CASE("semantics values are flat for every surface-level type") {
    std::vector<std::uint32_t> values = {0, kNone, 1};
    for (auto t : {GeometryKind::MultiPoint, GeometryKind::MultiLineString,
                   GeometryKind::MultiSurface, GeometryKind::CompositeSurface}) {
        CAPTURE(static_cast<int>(t));
        CHECK(decode_semantics_values(t, UIntView(), UIntView(), v(values)) ==
              json::parse("[0,null,1]"));
    }
}

TEST_CASE("a Solid groups semantics values by shell") {
    std::vector<std::uint32_t> solids = {2}, shells = {2, 1}, values = {0, kNone, 1};
    CHECK(decode_semantics_values(GeometryKind::Solid, v(solids), v(shells), v(values)) ==
          json::parse("[[0,null],[1]]"));
}

TEST_CASE("the SAME arrays give MultiSolid semantics one more level than Solid") {
    std::vector<std::uint32_t> solids = {1}, shells = {2}, values = {0, 1};
    CHECK(decode_semantics_values(GeometryKind::Solid, v(solids), v(shells), v(values)) ==
          json::parse("[[0,1]]"));
    CHECK(decode_semantics_values(GeometryKind::MultiSolid, v(solids), v(shells), v(values)) ==
          json::parse("[[[0,1]]]"));
    CHECK(decode_semantics_values(GeometryKind::CompositeSolid, v(solids), v(shells),
                                  v(values)) == json::parse("[[[0,1]]]"));
}

TEST_CASE("a CompositeSolid groups semantics values by shell, then by solid") {
    std::vector<std::uint32_t> solids = {2, 1}, shells = {1, 1, 1}, values = {0, 1, kNone};
    CHECK(decode_semantics_values(GeometryKind::CompositeSolid, v(solids), v(shells),
                                  v(values)) == json::parse("[[[0],[1]],[[null]]]"));
}

TEST_CASE("semantics values clamp rather than throw when the counts over-claim") {
    SUBCASE("values run out inside a shell") {
        std::vector<std::uint32_t> solids = {2}, shells = {3, 3}, values = {1, 2};
        CHECK(decode_semantics_values(GeometryKind::Solid, v(solids), v(shells), v(values)) ==
              json::parse("[[1,2],[]]"));
    }
    SUBCASE("shells run out across solids: the trailing solid keeps an empty shell") {
        std::vector<std::uint32_t> solids = {1, 1}, shells = {1}, values = {9};
        CHECK(decode_semantics_values(GeometryKind::MultiSolid, v(solids), v(shells),
                                      v(values)) == json::parse("[[[9]],[[]]]"));
    }
}

// -------------------------------------------------------------- material ---

TEST_CASE("material values are one index per surface for the surface types") {
    std::vector<std::uint32_t> vertices = {0, 1, kNone, 2};
    CHECK(decode_material_values(GeometryKind::MultiSurface, UIntView(), UIntView(),
                                 v(vertices)) == json::parse("[0,1,null,2]"));
    CHECK(decode_material_values(GeometryKind::CompositeSurface, UIntView(), UIntView(),
                                 v(vertices)) == json::parse("[0,1,null,2]"));
}

TEST_CASE("a material on a type that cannot carry one is read flat") {
    // MultiPoint and MultiLineString name no `material` and declare
    // additionalProperties: false, so this is not valid CityJSON; the
    // reference reads it as the shallowest thing it could be rather than
    // guessing a depth.
    std::vector<std::uint32_t> vertices = {7};
    CHECK(decode_material_values(GeometryKind::MultiPoint, UIntView(), UIntView(), v(vertices)) ==
          json::parse("[7]"));
}

TEST_CASE("a Solid's material values are one array per shell") {
    std::vector<std::uint32_t> solids = {2}, shells = {3, 3};
    std::vector<std::uint32_t> vertices = {0, 1, kNone, 2, 3, 4};
    CHECK(decode_material_values(GeometryKind::Solid, v(solids), v(shells), v(vertices)) ==
          json::parse("[[0,1,null],[2,3,4]]"));
}

TEST_CASE("the SAME arrays give MultiSolid material values one more level than Solid") {
    // THE regression. `solids = [1]` is what a one-shell Solid AND a one-solid
    // MultiSolid both write. Any guard over these arrays gets one of the two
    // wrong; the old C++ reader returned [[0,1]] for all three types, so both
    // solid types came back a level too shallow.
    std::vector<std::uint32_t> solids = {1}, shells = {2}, vertices = {0, 1};
    CHECK(decode_material_values(GeometryKind::Solid, v(solids), v(shells), v(vertices)) ==
          json::parse("[[0,1]]"));
    CHECK(decode_material_values(GeometryKind::MultiSolid, v(solids), v(shells), v(vertices)) ==
          json::parse("[[[0,1]]]"));
    CHECK(decode_material_values(GeometryKind::CompositeSolid, v(solids), v(shells),
                                 v(vertices)) == json::parse("[[[0,1]]]"));
}

TEST_CASE("a CompositeSolid's material values nest solid -> shell -> index") {
    std::vector<std::uint32_t> solids = {2, 1}, shells = {3, 3, 3};
    std::vector<std::uint32_t> vertices = {0, 1, kNone, 2, kNone, kNone, 3, 4, kNone};
    CHECK(decode_material_values(GeometryKind::CompositeSolid, v(solids), v(shells),
                                 v(vertices)) ==
          json::parse("[[[0,1,null],[2,null,null]],[[3,4,null]]]"));
}

TEST_CASE("a null shell or solid in material values decodes as null, not []") {
    // material.values is nullable at EVERY level (geomprimitives.schema.json),
    // and UINT32_MAX in a count array is how the format says so.
    std::vector<std::uint32_t> solids = {2}, shells = {2, kNone}, vertices = {0, 1};
    CHECK(decode_material_values(GeometryKind::Solid, v(solids), v(shells), v(vertices)) ==
          json::parse("[[0,1],null]"));

    std::vector<std::uint32_t> nsolids = {1, kNone}, nshells = {2};
    CHECK(decode_material_values(GeometryKind::CompositeSolid, v(nsolids), v(nshells),
                                 v(vertices)) == json::parse("[[[0,1]],null]"));
}

TEST_CASE("material values clamp rather than throw when the counts over-claim") {
    SUBCASE("shells run out inside a Solid: the shell list is just short") {
        std::vector<std::uint32_t> solids = {3}, shells = {1, 1}, vertices = {1, 2};
        CHECK(decode_material_values(GeometryKind::Solid, v(solids), v(shells), v(vertices)) ==
              json::parse("[[1],[2]]"));
    }
    SUBCASE("shells run out inside a solid: the missing shell is empty, not dropped") {
        std::vector<std::uint32_t> solids = {3}, shells = {1, 1}, vertices = {1, 2};
        CHECK(decode_material_values(GeometryKind::MultiSolid, v(solids), v(shells),
                                     v(vertices)) == json::parse("[[[1],[2],[]]]"));
    }
    SUBCASE("shells run out across solids: the trailing solid keeps an empty shell") {
        std::vector<std::uint32_t> solids = {1, 1}, shells = {1}, vertices = {9};
        CHECK(decode_material_values(GeometryKind::MultiSolid, v(solids), v(shells),
                                     v(vertices)) == json::parse("[[[9]],[[]]]"));
    }
    SUBCASE("vertices run out mid-shell: that shell is short and the next empty") {
        std::vector<std::uint32_t> solids = {2}, shells = {3, 3}, vertices = {1, 2};
        CHECK(decode_material_values(GeometryKind::Solid, v(solids), v(shells), v(vertices)) ==
              json::parse("[[1,2],[]]"));
    }
    SUBCASE("no shells at all under a solid: one empty shell, not a flat list") {
        std::vector<std::uint32_t> solids = {1}, vertices = {7};
        CHECK(decode_material_values(GeometryKind::MultiSolid, v(solids), UIntView(),
                                     v(vertices)) == json::parse("[[[]]]"));
    }
    SUBCASE("an empty vertices vector still produces the shell structure") {
        std::vector<std::uint32_t> solids = {1}, shells = {2};
        CHECK(decode_material_values(GeometryKind::Solid, v(solids), v(shells), UIntView()) ==
              json::parse("[[]]"));
    }
}

// --------------------------------------------------------------- texture ---

TEST_CASE("a MultiSurface's texture values nest surface -> ring") {
    std::vector<std::uint32_t> shells = {3}, surfaces = {1, 1, 1}, strings = {4, 4, 4};
    std::vector<std::uint32_t> vertices = {0, 10, 20, 30, 1, 11, 21, kNone, 2, 12, kNone, 32};
    CHECK(decode_texture_values(GeometryKind::MultiSurface, UIntView(), v(shells), v(surfaces),
                                v(strings), v(vertices)) ==
          json::parse("[[[0,10,20,30]],[[1,11,21,null]],[[2,12,null,32]]]"));
}

TEST_CASE("the SAME arrays give MultiSolid texture values one more level than Solid") {
    // The texture half of the regression: four levels for a Solid, five for a
    // one-solid MultiSolid, from identical arrays.
    std::vector<std::uint32_t> solids = {1}, shells = {1}, surfaces = {1}, strings = {3};
    std::vector<std::uint32_t> vertices = {0, 10, 20};
    CHECK(decode_texture_values(GeometryKind::Solid, v(solids), v(shells), v(surfaces),
                                v(strings), v(vertices)) == json::parse("[[[[0,10,20]]]]"));
    CHECK(decode_texture_values(GeometryKind::MultiSolid, v(solids), v(shells), v(surfaces),
                                v(strings), v(vertices)) == json::parse("[[[[[0,10,20]]]]]"));
    CHECK(decode_texture_values(GeometryKind::CompositeSolid, v(solids), v(shells), v(surfaces),
                                v(strings), v(vertices)) == json::parse("[[[[[0,10,20]]]]]"));
}

TEST_CASE("a Solid's texture values are one entry per shell") {
    std::vector<std::uint32_t> solids = {2}, shells = {2, 1}, surfaces = {1, 1, 1};
    std::vector<std::uint32_t> strings = {3, 3, 3};
    std::vector<std::uint32_t> vertices = {0, 10, 20, 1, 11, kNone, 2, 12, 22};
    CHECK(decode_texture_values(GeometryKind::Solid, v(solids), v(shells), v(surfaces),
                                v(strings), v(vertices)) ==
          json::parse("[[[[0,10,20]],[[1,11,null]]],[[[2,12,22]]]]"));
}

TEST_CASE("a CompositeSolid's texture values nest solid -> shell -> surface -> ring") {
    std::vector<std::uint32_t> solids = {2, 1}, shells = {2, 2, 2};
    std::vector<std::uint32_t> surfaces = {1, 1, 1, 1, 1, 1}, strings = {3, 3, 3, 3, 3, 3};
    std::vector<std::uint32_t> vertices = {0, 10, 20, 1, 11, kNone, 2,  12, 22,
                                           3, kNone, 23, 4, 14, 24, 5, 15, 25};
    CHECK(decode_texture_values(GeometryKind::CompositeSolid, v(solids), v(shells), v(surfaces),
                                v(strings), v(vertices)) ==
          json::parse("[[[[[0,10,20]],[[1,11,null]]],[[[2,12,22]],[[3,null,23]]]],"
                      "[[[[4,14,24]],[[5,15,25]]]]]"));
}

TEST_CASE("a texture on a type that cannot carry one falls back to one surface") {
    // The reference reads `max(surfaces.len(), 1)` surfaces, so even a mapping
    // with no count arrays produces one (empty) surface rather than a flat
    // index list. Not valid CityJSON either way; pinned so the two agree.
    std::vector<std::uint32_t> surfaces = {2}, strings = {3, 3};
    std::vector<std::uint32_t> vertices = {0, 10, 20, 1, 11, 21};
    CHECK(decode_texture_values(GeometryKind::MultiLineString, UIntView(), UIntView(),
                                v(surfaces), v(strings), v(vertices)) ==
          json::parse("[[[0,10,20],[1,11,21]]]"));

    std::vector<std::uint32_t> flat = {0, kNone, 2};
    CHECK(decode_texture_values(GeometryKind::MultiPoint, UIntView(), UIntView(), UIntView(),
                                UIntView(), v(flat)) == json::parse("[[]]"));
}

TEST_CASE("texture values clamp rather than throw when the counts over-claim") {
    SUBCASE("strings run out: the later rings and surfaces stay, empty") {
        std::vector<std::uint32_t> surfaces = {2, 1}, strings = {3}, vertices = {0, 1};
        CHECK(decode_texture_values(GeometryKind::MultiSurface, UIntView(), UIntView(),
                                    v(surfaces), v(strings), v(vertices)) ==
              json::parse("[[[0,1],[]],[[]]]"));
    }
    SUBCASE("a solid with no shells yields one empty shell") {
        // Diverges from the old reader, which returned [] here, and from
        // materials, whose same-shaped input is [[[]]] too.
        std::vector<std::uint32_t> solids = {1}, vertices = {7};
        CHECK(decode_texture_values(GeometryKind::MultiSolid, v(solids), UIntView(), UIntView(),
                                    UIntView(), v(vertices)) == json::parse("[[[]]]"));
    }
    SUBCASE("shells run out across solids: the trailing solid keeps an empty shell") {
        std::vector<std::uint32_t> solids = {1, 1}, shells = {1}, surfaces = {1};
        std::vector<std::uint32_t> strings = {1}, vertices = {9};
        CHECK(decode_texture_values(GeometryKind::MultiSolid, v(solids), v(shells), v(surfaces),
                                    v(strings), v(vertices)) == json::parse("[[[[[9]]]],[[]]]"));
    }
    SUBCASE("an empty vertices vector still produces the full structure") {
        std::vector<std::uint32_t> solids = {1}, shells = {1}, surfaces = {1}, strings = {3};
        CHECK(decode_texture_values(GeometryKind::Solid, v(solids), v(shells), v(surfaces),
                                    v(strings), UIntView()) == json::parse("[[[[]]]]"));
    }
}

TEST_CASE("geometry type names match CityJSON spelling") {
    CHECK(geometry_type_name(0) == "MultiPoint");
    CHECK(geometry_type_name(2) == "MultiSurface");
    CHECK(geometry_type_name(4) == "Solid");
    CHECK(geometry_type_name(6) == "CompositeSolid");
    CHECK_THROWS_AS(geometry_type_name(99), Error);
}
