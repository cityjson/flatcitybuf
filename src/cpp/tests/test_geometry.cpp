#include <doctest/doctest.h>

#include <fcb/geometry.hpp>

#include <cstdint>
#include <vector>

using namespace fcb;
using nlohmann::json;

static UIntView v(const std::vector<std::uint32_t>& x) { return UIntView(x.data(), x.size()); }

TEST_CASE("a flat index list decodes as MultiPoint") {
    std::vector<std::uint32_t> idx = {0, 1, 2};
    auto b = decode_boundaries(UIntView(), UIntView(), UIntView(), UIntView(), v(idx));
    CHECK(b == json::array({0, 1, 2}));
}

TEST_CASE("collapse applies ONLY at the outermost level") {
    // The spec's example: boundaries [0,1,2], strings [3], surfaces [1].
    //
    // geom_decoder.rs wraps every inner level unconditionally and applies
    // the len()==1 collapse only to the OUTERMOST vector. So one surface
    // holding one ring yields [[0,1,2]] -- the surface level collapses away,
    // the ring level does not. Collapsing at every depth would give [0,1,2],
    // which still looks plausible and is wrong.
    std::vector<std::uint32_t> surfaces = {1}, strings = {3}, idx = {0, 1, 2};
    auto b = decode_boundaries(UIntView(), UIntView(), v(surfaces), v(strings), v(idx));
    CHECK(b == json::parse("[[0,1,2]]"));
}

TEST_CASE("two surfaces of one ring each stay nested") {
    std::vector<std::uint32_t> surfaces = {1, 1}, strings = {3, 3};
    std::vector<std::uint32_t> idx = {0, 1, 2, 3, 4, 5};
    auto b = decode_boundaries(UIntView(), UIntView(), v(surfaces), v(strings), v(idx));
    // Outer array has 2 entries so it does not collapse; each surface is
    // still wrapped around its single ring.
    REQUIRE(b.is_array());
    REQUIRE(b.size() == 2);
    CHECK(b[0] == json::parse("[[0,1,2]]"));
    CHECK(b[1] == json::parse("[[3,4,5]]"));
}

TEST_CASE("a surface with an inner ring keeps both rings") {
    std::vector<std::uint32_t> surfaces = {2}, strings = {4, 3};
    std::vector<std::uint32_t> idx = {0, 1, 2, 3, 10, 11, 12};
    auto b = decode_boundaries(UIntView(), UIntView(), v(surfaces), v(strings), v(idx));
    REQUIRE(b.is_array());
    REQUIRE(b.size() == 2);
    CHECK(b[0] == json::array({0, 1, 2, 3}));
    CHECK(b[1] == json::array({10, 11, 12}));
}

TEST_CASE("a solid nests solid -> shell -> surface -> ring") {
    // One solid, one shell, two surfaces of one ring each.
    std::vector<std::uint32_t> solids = {1}, shells = {2}, surfaces = {1, 1};
    std::vector<std::uint32_t> strings = {3, 3};
    std::vector<std::uint32_t> idx = {0, 1, 2, 3, 4, 5};
    auto b = decode_boundaries(v(solids), v(shells), v(surfaces), v(strings), v(idx));

    // The single solid collapses away, leaving the shell list: one shell
    // holding two surfaces, each wrapped around its single ring.
    CHECK(b == json::parse("[[[[0,1,2]],[[3,4,5]]]]"));
}

TEST_CASE("two solids do not collapse") {
    std::vector<std::uint32_t> solids = {1, 1}, shells = {1, 1}, surfaces = {1, 1};
    std::vector<std::uint32_t> strings = {3, 3};
    std::vector<std::uint32_t> idx = {0, 1, 2, 3, 4, 5};
    auto b = decode_boundaries(v(solids), v(shells), v(surfaces), v(strings), v(idx));
    REQUIRE(b.is_array());
    CHECK(b.size() == 2);
}

TEST_CASE("a ring claiming more indices than exist throws") {
    std::vector<std::uint32_t> strings = {99};
    std::vector<std::uint32_t> idx = {0, 1, 2};
    CHECK_THROWS_AS(
        decode_boundaries(UIntView(), UIntView(), UIntView(), v(strings), v(idx)), Error);
}

TEST_CASE("a surface claiming more rings than exist throws") {
    std::vector<std::uint32_t> surfaces = {5}, strings = {3};
    std::vector<std::uint32_t> idx = {0, 1, 2};
    CHECK_THROWS_AS(
        decode_boundaries(UIntView(), UIntView(), v(surfaces), v(strings), v(idx)), Error);
}

// --------------------------------------------------------- appearance ---
//
// Both decoders mirror geom_decoder.rs branch for branch, so a change here
// must land in both. Two branches that lost a nesting level on round trip
// were fixed on both sides and are marked "Regression" below; the branches
// that remain quirky are unreachable from our own writer (proved in
// appearance_roundtrip.rs) and are pinned as reference behaviour.

static constexpr std::uint32_t kNone = UINT32_MAX;

TEST_CASE("material values are flat when the mapping carries no solids") {
    // The geom_temp fixture's shape: one index per surface, u32::MAX for
    // "no material on this surface".
    std::vector<std::uint32_t> vertices = {kNone, 1, 0};
    auto m = decode_material_values(UIntView(), UIntView(), v(vertices));
    CHECK(m == json::parse("[null,1,0]"));
}

TEST_CASE("material values stay flat when solids are declared without shells") {
    std::vector<std::uint32_t> solids = {2}, vertices = {0, 1};
    auto m = decode_material_values(v(solids), UIntView(), v(vertices));
    CHECK(m == json::parse("[0,1]"));
}

TEST_CASE("one solid of several shells drops the solid level") {
    std::vector<std::uint32_t> solids = {2}, shells = {3, 3};
    std::vector<std::uint32_t> vertices = {0, 1, kNone, 2, 3, 4};
    auto m = decode_material_values(v(solids), v(shells), v(vertices));
    CHECK(m == json::parse("[[0,1,null],[2,3,4]]"));
}

TEST_CASE("a solid of exactly one shell drops the solid level too") {
    // Regression: solids == [1] used to fail a `solids[0] > 1` guard and
    // fall into the MultiSolid branch, so a Solid with a single exterior
    // shell -- the commonest shape there is -- came back one level deeper
    // than it was written. Round-tripped in the Rust suite
    // (appearance_roundtrip.rs::material_solid_single_shell_roundtrips).
    std::vector<std::uint32_t> solids = {1}, shells = {2}, vertices = {7, 8};
    auto m = decode_material_values(v(solids), v(shells), v(vertices));
    CHECK(m == json::parse("[[7,8]]"));
}

TEST_CASE("several solids nest solid -> shell -> indices") {
    std::vector<std::uint32_t> solids = {1, 1}, shells = {1, 1}, vertices = {5, 6};
    auto m = decode_material_values(v(solids), v(shells), v(vertices));
    CHECK(m == json::parse("[[[5]],[[6]]]"));
}

TEST_CASE("material values truncate rather than throw when counts overrun") {
    // Unlike decode_boundaries, the reference guards every read and emits
    // the short result. A mapping that over-claims must not abort the read.
    SUBCASE("shells run out inside a single solid: entries are dropped") {
        std::vector<std::uint32_t> solids = {3}, shells = {1, 1}, vertices = {1, 2};
        CHECK(decode_material_values(v(solids), v(shells), v(vertices)) ==
              json::parse("[[1],[2]]"));
    }
    SUBCASE("shells run out across solids: the solid stays, empty") {
        std::vector<std::uint32_t> solids = {1, 1}, shells = {1}, vertices = {9};
        CHECK(decode_material_values(v(solids), v(shells), v(vertices)) ==
              json::parse("[[[9]],[]]"));
    }
    SUBCASE("vertices run out mid-shell: that shell is short") {
        std::vector<std::uint32_t> solids = {2}, shells = {3, 3}, vertices = {1, 2};
        CHECK(decode_material_values(v(solids), v(shells), v(vertices)) ==
              json::parse("[[1,2],[]]"));
    }
}

TEST_CASE("texture values for a single shell of surfaces") {
    // The geom_temp shape: shells == [n] with one entry per surface. Each
    // ring is (texture index, then one UV index per vertex).
    std::vector<std::uint32_t> shells = {2}, surfaces = {1, 1}, strings = {3, 2};
    std::vector<std::uint32_t> vertices = {0, 10, 11, 1, 20};
    auto t = decode_texture_values(UIntView(), v(shells), v(surfaces), v(strings), v(vertices));
    CHECK(t == json::parse("[[[0,10,11]],[[1,20]]]"));
}

TEST_CASE("a single solid collapses only at the outermost level") {
    std::vector<std::uint32_t> solids = {1}, shells = {1}, surfaces = {1}, strings = {3};
    std::vector<std::uint32_t> vertices = {0, 1, 2};
    auto t = decode_texture_values(v(solids), v(shells), v(surfaces), v(strings), v(vertices));
    // The solid list collapses away; shell, surface and ring all stay.
    CHECK(t == json::parse("[[[[0,1,2]]]]"));
}

TEST_CASE("two solids keep the outermost level") {
    std::vector<std::uint32_t> solids = {1, 1}, shells = {1, 1}, surfaces = {1, 1};
    std::vector<std::uint32_t> strings = {1, 1}, vertices = {7, 8};
    auto t = decode_texture_values(v(solids), v(shells), v(surfaces), v(strings), v(vertices));
    CHECK(t == json::parse("[[[[[7]]]],[[[[8]]]]]"));
}

TEST_CASE("one surface of rings is a MultiLineString") {
    std::vector<std::uint32_t> surfaces = {2}, strings = {3, 3};
    std::vector<std::uint32_t> vertices = {0, 10, 20, 1, 11, 21};
    auto t = decode_texture_values(UIntView(), UIntView(), v(surfaces), v(strings), v(vertices));
    // The surface wrapper is dropped, leaving the ring list.
    CHECK(t == json::parse("[[0,10,20],[1,11,21]]"));

    // Regression: a SINGLE string used to fail a `strings.size() > 1` guard
    // and fall through to the surface branch, gaining a level. The
    // MultiSurface look-alike below is distinguishable by its shells entry.
    std::vector<std::uint32_t> one = {1}, four = {4};
    std::vector<std::uint32_t> uv = {0, 10, 11, 12};
    CHECK(decode_texture_values(UIntView(), UIntView(), v(one), v(four), v(uv)) ==
          json::parse("[[0,10,11,12]]"));
    std::vector<std::uint32_t> shells = {1};
    CHECK(decode_texture_values(UIntView(), v(shells), v(one), v(four), v(uv)) ==
          json::parse("[[[0,10,11,12]]]"));
}

TEST_CASE("several surfaces nest surface -> ring") {
    std::vector<std::uint32_t> surfaces = {1, 2}, strings = {2, 2, 2};
    std::vector<std::uint32_t> vertices = {0, 1, 2, 3, 4, 5};
    auto t = decode_texture_values(UIntView(), UIntView(), v(surfaces), v(strings), v(vertices));
    CHECK(t == json::parse("[[[0,1]],[[2,3],[4,5]]]"));
}

TEST_CASE("more than one shell without solids discards the shell structure") {
    // The shell branch is guarded on shells.size() == 1, so two shells fall
    // through to the surface branch and `shells` is never read. Faithful to
    // the reference; noted because the lost grouping is invisible in the
    // output.
    std::vector<std::uint32_t> shells = {1, 1}, surfaces = {1, 1}, strings = {1, 1};
    std::vector<std::uint32_t> vertices = {3, 4};
    auto t = decode_texture_values(UIntView(), v(shells), v(surfaces), v(strings), v(vertices));
    CHECK(t == json::parse("[[[3]],[[4]]]"));
}

TEST_CASE("texture values with no count arrays are a flat list") {
    std::vector<std::uint32_t> vertices = {0, kNone, 2};
    CHECK(decode_texture_values(UIntView(), UIntView(), UIntView(), UIntView(), v(vertices)) ==
          json::parse("[0,null,2]"));

    // A lone `strings` entry takes the same branch, and the whole vertex
    // list is emitted even if that entry disagrees with its length.
    std::vector<std::uint32_t> strings = {2};
    CHECK(decode_texture_values(UIntView(), UIntView(), UIntView(), v(strings), v(vertices)) ==
          json::parse("[0,null,2]"));
}

TEST_CASE("rings with no surface grouping stay a ring list") {
    std::vector<std::uint32_t> strings = {2, 1}, vertices = {1, 2, 3};
    CHECK(decode_texture_values(UIntView(), UIntView(), UIntView(), v(strings), v(vertices)) ==
          json::parse("[[1,2],[3]]"));
}

TEST_CASE("texture values truncate rather than throw when counts overrun") {
    SUBCASE("strings run out: the later surface stays, empty") {
        std::vector<std::uint32_t> surfaces = {2, 1}, strings = {3};
        std::vector<std::uint32_t> vertices = {0, 1};
        CHECK(decode_texture_values(UIntView(), UIntView(), v(surfaces), v(strings),
                                    v(vertices)) == json::parse("[[[0,1]],[]]"));
    }
    SUBCASE("a solid with no shells collapses to an empty array") {
        // Not the flat vertex list: the solids branch is taken on `solids`
        // alone, so the vertices are never reached. Diverges from materials,
        // where the same input falls back to a flat list.
        std::vector<std::uint32_t> solids = {1}, vertices = {7};
        CHECK(decode_texture_values(v(solids), UIntView(), UIntView(), UIntView(),
                                    v(vertices)) == json::parse("[]"));
        CHECK(decode_material_values(v(solids), UIntView(), v(vertices)) ==
              json::parse("[7]"));
    }
    SUBCASE("shells run out across solids: the trailing solid stays, empty") {
        std::vector<std::uint32_t> solids = {1, 1}, shells = {1}, surfaces = {1};
        std::vector<std::uint32_t> strings = {1}, vertices = {9};
        CHECK(decode_texture_values(v(solids), v(shells), v(surfaces), v(strings),
                                    v(vertices)) == json::parse("[[[[[9]]]],[]]"));
    }
}

TEST_CASE("geometry type names match CityJSON spelling") {
    CHECK(geometry_type_name(0) == "MultiPoint");
    CHECK(geometry_type_name(2) == "MultiSurface");
    CHECK(geometry_type_name(4) == "Solid");
    CHECK(geometry_type_name(6) == "CompositeSolid");
    CHECK_THROWS_AS(geometry_type_name(99), Error);
}
