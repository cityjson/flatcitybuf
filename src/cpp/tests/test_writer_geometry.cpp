#include <fcb/geometry.hpp>
#include <fcb/writer/geom_encoder.hpp>

#include <doctest/doctest.h>

using namespace fcb;

static UIntView view(const std::vector<std::uint32_t>& v) { return UIntView(v); }

TEST_CASE("encode_boundaries: MultiPoint") {
    auto boundaries = nlohmann::json::parse("[2, 44, 0, 7]");
    auto encoded = encode_boundaries(GeometryKind::MultiPoint, boundaries);
    CHECK(encoded.indices == std::vector<std::uint32_t>{2, 44, 0, 7});
    CHECK(encoded.strings == std::vector<std::uint32_t>{4});
    CHECK(encoded.surfaces.empty());
    CHECK(encoded.shells.empty());
    CHECK(encoded.solids.empty());

    auto decoded =
        decode_boundaries(GeometryKind::MultiPoint, view(encoded.solids), view(encoded.shells),
                          view(encoded.surfaces), view(encoded.strings), view(encoded.indices));
    CHECK(decoded == boundaries);
}

TEST_CASE("encode_boundaries: MultiLineString") {
    auto boundaries = nlohmann::json::parse("[[2, 3, 5], [77, 55, 212]]");
    auto encoded = encode_boundaries(GeometryKind::MultiLineString, boundaries);
    CHECK(encoded.indices == std::vector<std::uint32_t>{2, 3, 5, 77, 55, 212});
    CHECK(encoded.strings == std::vector<std::uint32_t>{3, 3});
    CHECK(encoded.surfaces == std::vector<std::uint32_t>{2});
    CHECK(encoded.shells.empty());
    CHECK(encoded.solids.empty());

    auto decoded =
        decode_boundaries(GeometryKind::MultiLineString, view(encoded.solids), view(encoded.shells),
                          view(encoded.surfaces), view(encoded.strings), view(encoded.indices));
    CHECK(decoded == boundaries);
}

TEST_CASE("encode_boundaries: MultiSurface") {
    auto boundaries = nlohmann::json::parse("[[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]]");
    auto encoded = encode_boundaries(GeometryKind::MultiSurface, boundaries);
    CHECK(encoded.indices == std::vector<std::uint32_t>{0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4});
    CHECK(encoded.strings == std::vector<std::uint32_t>{4, 4, 4});
    CHECK(encoded.surfaces == std::vector<std::uint32_t>{1, 1, 1});
    CHECK(encoded.shells == std::vector<std::uint32_t>{3});
    CHECK(encoded.solids.empty());

    auto decoded =
        decode_boundaries(GeometryKind::MultiSurface, view(encoded.solids), view(encoded.shells),
                          view(encoded.surfaces), view(encoded.strings), view(encoded.indices));
    CHECK(decoded == boundaries);
}

TEST_CASE("encode_boundaries: Solid") {
    auto boundaries = nlohmann::json::parse(R"([
        [
            [[0, 3, 2, 1, 22], [1, 2, 3, 4]],
            [[4, 5, 6, 7]],
            [[0, 1, 5, 4]],
            [[1, 2, 6, 5]]
        ],
        [
            [[240, 243, 124]],
            [[244, 246, 724]],
            [[34, 414, 45]],
            [[111, 246, 5]]
        ]
    ])");
    auto encoded = encode_boundaries(GeometryKind::Solid, boundaries);
    CHECK(encoded.indices == std::vector<std::uint32_t>{0,  3,   2,  1,   22,  1,   2,   3,   4,
                                                        4,  5,   6,  7,   0,   1,   5,   4,   1,
                                                        2,  6,   5,  240, 243, 124, 244, 246, 724,
                                                        34, 414, 45, 111, 246, 5});
    CHECK(encoded.strings == std::vector<std::uint32_t>{5, 4, 4, 4, 4, 3, 3, 3, 3});
    CHECK(encoded.surfaces == std::vector<std::uint32_t>{2, 1, 1, 1, 1, 1, 1, 1});
    CHECK(encoded.shells == std::vector<std::uint32_t>{4, 4});
    CHECK(encoded.solids == std::vector<std::uint32_t>{2});

    auto decoded =
        decode_boundaries(GeometryKind::Solid, view(encoded.solids), view(encoded.shells),
                          view(encoded.surfaces), view(encoded.strings), view(encoded.indices));
    CHECK(decoded == boundaries);
}

TEST_CASE("encode_boundaries: CompositeSolid") {
    auto boundaries = nlohmann::json::parse(R"([
        [
            [
                [[0, 3, 2, 1, 22]],
                [[4, 5, 6, 7]],
                [[0, 1, 5, 4]],
                [[1, 2, 6, 5]]
            ],
            [
                [[240, 243, 124]],
                [[244, 246, 724]],
                [[34, 414, 45]],
                [[111, 246, 5]]
            ]
        ],
        [[
            [[666, 667, 668]],
            [[74, 75, 76]],
            [[880, 881, 885]],
            [[111, 122, 226]]
        ]]
    ])");
    auto encoded = encode_boundaries(GeometryKind::CompositeSolid, boundaries);
    CHECK(encoded.strings == std::vector<std::uint32_t>{5, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3});
    CHECK(encoded.surfaces == std::vector<std::uint32_t>{1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1});
    CHECK(encoded.shells == std::vector<std::uint32_t>{4, 4, 4});
    CHECK(encoded.solids == std::vector<std::uint32_t>{2, 1});

    auto decoded =
        decode_boundaries(GeometryKind::CompositeSolid, view(encoded.solids), view(encoded.shells),
                          view(encoded.surfaces), view(encoded.strings), view(encoded.indices));
    CHECK(decoded == boundaries);
}

TEST_CASE("MultiSurface and CompositeSurface of equal depth flatten identically") {
    auto boundaries = nlohmann::json::parse("[[[0, 1, 2]], [[3, 4, 5]]]");
    auto ms = encode_boundaries(GeometryKind::MultiSurface, boundaries);
    auto cs = encode_boundaries(GeometryKind::CompositeSurface, boundaries);
    CHECK(ms.shells == cs.shells);
    CHECK(ms.surfaces == cs.surfaces);
    CHECK(ms.indices == cs.indices);
}

TEST_CASE("MultiSolid and CompositeSolid of equal depth flatten identically") {
    auto boundaries = nlohmann::json::parse("[[[[[0, 1, 2]]]], [[[[3, 4, 5]]]]]");
    auto msol = encode_boundaries(GeometryKind::MultiSolid, boundaries);
    auto csol = encode_boundaries(GeometryKind::CompositeSolid, boundaries);
    CHECK(msol.solids == csol.solids);
    CHECK(msol.shells == csol.shells);
}

TEST_CASE("geometry_kind_from_name maps every known CityJSON type string") {
    CHECK(geometry_kind_from_name("MultiPoint") == GeometryKind::MultiPoint);
    CHECK(geometry_kind_from_name("MultiLineString") == GeometryKind::MultiLineString);
    CHECK(geometry_kind_from_name("MultiSurface") == GeometryKind::MultiSurface);
    CHECK(geometry_kind_from_name("CompositeSurface") == GeometryKind::CompositeSurface);
    CHECK(geometry_kind_from_name("Solid") == GeometryKind::Solid);
    CHECK(geometry_kind_from_name("MultiSolid") == GeometryKind::MultiSolid);
    CHECK(geometry_kind_from_name("CompositeSolid") == GeometryKind::CompositeSolid);
    CHECK(geometry_kind_from_name("GeometryInstance") == GeometryKind::GeometryInstance);
    CHECK_THROWS_AS(geometry_kind_from_name("NotAType"), Error);
}
