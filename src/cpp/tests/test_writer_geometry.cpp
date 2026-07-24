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

TEST_CASE("encode_semantics: MultiSurface (flat, depth 1)") {
    auto boundaries_json = nlohmann::json::parse(R"([
        [[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]], [[0, 2, 3, 8]], [[10, 12, 23, 48]]
    ])");
    auto boundaries = encode_boundaries(GeometryKind::MultiSurface, boundaries_json);

    auto semantics_json = nlohmann::json::parse(R"({
        "surfaces": [
            {"type": "WallSurface", "slope": 33.4, "children": [2]},
            {"type": "RoofSurface", "slope": 66.6},
            {"type": "OuterCeilingSurface", "parent": 0, "colour": "blue"}
        ],
        "values": [0, 0, null, 1, 2]
    })");
    auto encoded = encode_semantics(semantics_json, GeometryKind::MultiSurface, boundaries);
    CHECK(encoded.surfaces == semantics_json.at("surfaces"));
    REQUIRE(encoded.values.has_value());
    CHECK(*encoded.values == std::vector<std::uint32_t>{0, 0, UINT32_MAX, 1, 2});

    auto decoded = decode_semantics_values(GeometryKind::MultiSurface, view(boundaries.solids),
                                           view(boundaries.shells), view(*encoded.values));
    CHECK(decoded == semantics_json.at("values"));
}

TEST_CASE("encode_semantics: a null shell expands to one null per surface (Solid, depth 2)") {
    auto boundaries_json = nlohmann::json::parse("[[[[0,1,2]], [[3,4,5]]], [[[6,7,8]]]]");
    auto boundaries = encode_boundaries(GeometryKind::Solid, boundaries_json);

    auto semantics_json = nlohmann::json::parse(R"({
        "surfaces": [{"type": "RoofSurface"}],
        "values": [[0, 0], null]
    })");
    auto encoded = encode_semantics(semantics_json, GeometryKind::Solid, boundaries);
    REQUIRE(encoded.values.has_value());
    CHECK(*encoded.values == std::vector<std::uint32_t>{0, 0, UINT32_MAX});
    // Deliberately NOT round-tripped: a null shell does not round-trip its
    // spelling (it decodes back as a run of per-surface nulls), matching
    // Rust's own documented behavior.
}

TEST_CASE("encode_semantics: CompositeSolid (depth 3)") {
    auto boundaries_json = nlohmann::json::parse(R"([
        [[
            [[0, 3, 2, 1, 22]], [[4, 5, 6, 7]], [[0, 1, 5, 4]], [[1, 2, 6, 5]]
        ]],
        [[
            [[666, 667, 668]], [[74, 75, 76]], [[880, 881, 885]]
        ]]
    ])");
    auto boundaries = encode_boundaries(GeometryKind::CompositeSolid, boundaries_json);

    auto semantics_json = nlohmann::json::parse(R"({
        "surfaces": [{"type": "RoofSurface"}, {"type": "WallSurface"}],
        "values": [[[0, 1, 1, null]], [[null, null, null]]]
    })");
    auto encoded = encode_semantics(semantics_json, GeometryKind::CompositeSolid, boundaries);
    REQUIRE(encoded.values.has_value());
    CHECK(*encoded.values ==
          std::vector<std::uint32_t>{0, 1, 1, UINT32_MAX, UINT32_MAX, UINT32_MAX, UINT32_MAX});
}

TEST_CASE("encode_semantics: absent surfaces/null values are handled") {
    auto boundaries =
        encode_boundaries(GeometryKind::MultiSurface, nlohmann::json::parse("[[[0,1,2]]]"));
    auto encoded = encode_semantics(nlohmann::json::parse(R"({"surfaces": [], "values": null})"),
                                    GeometryKind::MultiSurface, boundaries);
    CHECK_FALSE(encoded.values.has_value());
}

TEST_CASE("encode_material: a single value") {
    auto material = nlohmann::json::parse(R"({"theme1": {"value": 5}})");
    auto encoded = encode_material(material, GeometryKind::MultiSurface);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].kind == MaterialMapping::Kind::Value);
    CHECK(encoded[0].theme == "theme1");
    CHECK(encoded[0].value == 5);
}

TEST_CASE("encode_material: MultiSurface-depth values") {
    auto material = nlohmann::json::parse(R"({"theme2": {"values": [0, 1, null, 2]}})");
    auto encoded = encode_material(material, GeometryKind::MultiSurface);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].kind == MaterialMapping::Kind::Values);
    CHECK(encoded[0].theme == "theme2");
    CHECK(encoded[0].vertices == std::vector<std::uint32_t>{0, 1, UINT32_MAX, 2});
    CHECK(encoded[0].shells.empty());
    CHECK(encoded[0].solids.empty());
}

TEST_CASE("encode_material: Solid-depth values") {
    auto material = nlohmann::json::parse(R"({"theme3": {"values": [[0, 1, null], [2, 3, 4]]}})");
    auto encoded = encode_material(material, GeometryKind::Solid);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].theme == "theme3");
    CHECK(encoded[0].solids == std::vector<std::uint32_t>{2});  // 1 solid, 2 shells
    CHECK(encoded[0].shells == std::vector<std::uint32_t>{3, 3});
    CHECK(encoded[0].vertices == std::vector<std::uint32_t>{0, 1, UINT32_MAX, 2, 3, 4});
}

TEST_CASE("encode_material: multiple themes") {
    auto material =
        nlohmann::json::parse(R"({"theme4": {"value": 7}, "theme5": {"values": [8, 9]}})");
    auto encoded = encode_material(material, GeometryKind::MultiSurface);
    REQUIRE(encoded.size() == 2);

    auto find = [&](const std::string& theme) -> const MaterialMapping& {
        for (auto& m : encoded)
            if (m.theme == theme)
                return m;
        FAIL("missing theme " << theme);
        static MaterialMapping dummy;
        return dummy;
    };
    CHECK(find("theme4").kind == MaterialMapping::Kind::Value);
    CHECK(find("theme4").value == 7);
    CHECK(find("theme5").kind == MaterialMapping::Kind::Values);
    CHECK(find("theme5").vertices == std::vector<std::uint32_t>{8, 9});
}

TEST_CASE("encode_material: CompositeSolid-depth values") {
    auto material = nlohmann::json::parse(
        R"({"theme6": {"values": [[[0, 1, null], [2, null, null]], [[3, 4, null]]]}})");
    auto encoded = encode_material(material, GeometryKind::CompositeSolid);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].solids == std::vector<std::uint32_t>{2, 1});  // 2 solids: 2 shells, then 1
    CHECK(encoded[0].shells == std::vector<std::uint32_t>{3, 3, 3});
    CHECK(encoded[0].vertices == std::vector<std::uint32_t>{0, 1, UINT32_MAX, 2, UINT32_MAX,
                                                            UINT32_MAX, 3, 4, UINT32_MAX});
}

TEST_CASE("encode_material: a null shell or solid is recorded as a null count") {
    auto encoded = encode_material(nlohmann::json::parse(R"({"t": {"values": [[0, 1], null]}})"),
                                   GeometryKind::Solid);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].solids == std::vector<std::uint32_t>{2});
    CHECK(encoded[0].shells == std::vector<std::uint32_t>{2, UINT32_MAX});
    CHECK(encoded[0].vertices == std::vector<std::uint32_t>{0, 1});

    auto encoded2 = encode_material(nlohmann::json::parse(R"({"t": {"values": [[[0, 1]], null]}})"),
                                    GeometryKind::CompositeSolid);
    REQUIRE(encoded2.size() == 1);
    CHECK(encoded2[0].solids == std::vector<std::uint32_t>{1, UINT32_MAX});
    CHECK(encoded2[0].shells == std::vector<std::uint32_t>{2});
    CHECK(encoded2[0].vertices == std::vector<std::uint32_t>{0, 1});
}

TEST_CASE("encode_material: values: null is a NullValues mapping, not dropped") {
    auto encoded = encode_material(nlohmann::json::parse(R"({"t": {"values": null}})"),
                                   GeometryKind::MultiSurface);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].kind == MaterialMapping::Kind::NullValues);
    CHECK(encoded[0].theme == "t");
}

TEST_CASE("encode_texture: MultiSurface-depth values (the shallowest a texture can be)") {
    auto texture = nlohmann::json::parse(R"({
        "t": {"values": [[[0, 10, 20, 30]], [[1, 11, 21, null]], [[2, 12, null, 32]]]}
    })");
    auto encoded = encode_texture(texture, GeometryKind::MultiSurface);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].theme == "t");
    CHECK(encoded[0].has_values);
    CHECK(encoded[0].vertices ==
          std::vector<std::uint32_t>{0, 10, 20, 30, 1, 11, 21, UINT32_MAX, 2, 12, UINT32_MAX, 32});
    CHECK(encoded[0].strings == std::vector<std::uint32_t>{4, 4, 4});
    CHECK(encoded[0].surfaces == std::vector<std::uint32_t>{1, 1, 1});
    CHECK(encoded[0].shells == std::vector<std::uint32_t>{3});
    CHECK(encoded[0].solids.empty());
}

TEST_CASE("encode_texture: Solid-depth values") {
    auto texture = nlohmann::json::parse(R"({
        "t": {"values": [
            [[[0, 10, 20, 30]], [[1, 11, 21, null]], [[2, 12, null, 32]]],
            [[[3, 13, 23, 33]], [[4, 14, 24, null]]]
        ]}
    })");
    auto encoded = encode_texture(texture, GeometryKind::Solid);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].vertices ==
          std::vector<std::uint32_t>{0, 10, 20, 30, 1, 11, 21, UINT32_MAX, 2, 12, UINT32_MAX, 32,
                                     3, 13, 23, 33, 4, 14, 24, UINT32_MAX});
    CHECK(encoded[0].strings == std::vector<std::uint32_t>{4, 4, 4, 4, 4});
    CHECK(encoded[0].surfaces == std::vector<std::uint32_t>{1, 1, 1, 1, 1});
    CHECK(encoded[0].shells == std::vector<std::uint32_t>{3, 2});
    CHECK(encoded[0].solids == std::vector<std::uint32_t>{2});
}

TEST_CASE("encode_texture: CompositeSolid-depth values") {
    auto texture = nlohmann::json::parse(R"({
        "t": {"values": [
            [
                [[[0, 10, 20]], [[1, 11, null]]],
                [[[2, 12, 22]], [[3, null, 23]]]
            ],
            [[[[4, 14, 24]], [[5, 15, 25]]]]
        ]}
    })");
    auto encoded = encode_texture(texture, GeometryKind::CompositeSolid);
    REQUIRE(encoded.size() == 1);
    CHECK(encoded[0].vertices == std::vector<std::uint32_t>{0, 10, 20, 1, 11, UINT32_MAX, 2, 12, 22,
                                                            3, UINT32_MAX, 23, 4, 14, 24, 5, 15,
                                                            25});
    CHECK(encoded[0].strings == std::vector<std::uint32_t>{3, 3, 3, 3, 3, 3});
    CHECK(encoded[0].surfaces == std::vector<std::uint32_t>{1, 1, 1, 1, 1, 1});
    CHECK(encoded[0].shells == std::vector<std::uint32_t>{2, 2, 2});
    CHECK(encoded[0].solids == std::vector<std::uint32_t>{2, 1});
}

TEST_CASE("encode_texture: multiple themes") {
    auto texture = nlohmann::json::parse(R"({
        "winter": {"values": [[[0, 10, 20]]]},
        "summer": {"values": [[[1, 11, null]]]}
    })");
    auto encoded = encode_texture(texture, GeometryKind::MultiSurface);
    REQUIRE(encoded.size() == 2);

    auto find = [&](const std::string& theme) -> const TextureMapping& {
        for (auto& t : encoded)
            if (t.theme == theme)
                return t;
        FAIL("missing theme " << theme);
        static TextureMapping dummy;
        return dummy;
    };
    CHECK(find("winter").vertices == std::vector<std::uint32_t>{0, 10, 20});
    CHECK(find("winter").strings == std::vector<std::uint32_t>{3});
    CHECK(find("summer").vertices == std::vector<std::uint32_t>{1, 11, UINT32_MAX});
    CHECK(find("summer").strings == std::vector<std::uint32_t>{3});
}

TEST_CASE("encode_texture: a theme with no values member has_values is false") {
    auto encoded =
        encode_texture(nlohmann::json::parse(R"({"t": {}})"), GeometryKind::MultiSurface);
    REQUIRE(encoded.size() == 1);
    CHECK_FALSE(encoded[0].has_values);
    CHECK(encoded[0].vertices.empty());
}

TEST_CASE("encode: a MultiSurface with boundaries, semantics and material all populate") {
    auto geometry = nlohmann::json::parse(R"({
        "type": "MultiSurface",
        "lod": "2",
        "boundaries": [[[0, 1, 2]], [[0, 2, 3]]],
        "semantics": {
            "surfaces": [{"type": "RoofSurface"}],
            "values": [0, 0]
        },
        "material": {"t": {"values": [0, 1]}}
    })");
    auto encoded = encode(geometry);

    CHECK(encoded.boundaries.indices == std::vector<std::uint32_t>{0, 1, 2, 0, 2, 3});
    REQUIRE(encoded.semantics.has_value());
    CHECK(*encoded.semantics->values == std::vector<std::uint32_t>{0, 0});
    REQUIRE(encoded.materials.has_value());
    CHECK(encoded.materials->size() == 1);
    CHECK_FALSE(encoded.textures.has_value());  // no "texture" key at all
}

TEST_CASE("encode: a GeometryInstance yields empty arrays, not a crash") {
    auto geometry = nlohmann::json::parse(R"({"type": "GeometryInstance", "boundaries": 5})");
    auto encoded = encode(geometry);
    CHECK(encoded.boundaries.indices.empty());
    CHECK(encoded.boundaries.solids.empty());
    CHECK_FALSE(encoded.semantics.has_value());
    CHECK_FALSE(encoded.materials.has_value());
    CHECK_FALSE(encoded.textures.has_value());
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
