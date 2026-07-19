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

TEST_CASE("geometry type names match CityJSON spelling") {
    CHECK(geometry_type_name(0) == "MultiPoint");
    CHECK(geometry_type_name(2) == "MultiSurface");
    CHECK(geometry_type_name(4) == "Solid");
    CHECK(geometry_type_name(6) == "CompositeSolid");
    CHECK_THROWS_AS(geometry_type_name(99), Error);
}
