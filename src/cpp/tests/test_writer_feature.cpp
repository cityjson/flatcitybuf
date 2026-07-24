#include <fcb/writer/feature_serializer.hpp>

#include <doctest/doctest.h>

using namespace fcb;

static const ::Geometry* build_geometry(::flatbuffers::FlatBufferBuilder& fbb,
                                        const nlohmann::json& geometry,
                                        const AttributeSchema* semantic_schema = nullptr) {
    auto off = to_geometry(fbb, geometry, semantic_schema);
    fbb.Finish(off);
    return flatbuffers::GetRoot<::Geometry>(fbb.GetBufferPointer());
}

TEST_CASE("city_object_type_from_name maps every known CityJSON object type") {
    CHECK(city_object_type_from_name("Building").type == ::CityObjectType::Building);
    CHECK_FALSE(city_object_type_from_name("Building").extension_type.has_value());
    CHECK(city_object_type_from_name("BuildingPart").type == ::CityObjectType::BuildingPart);
    CHECK(city_object_type_from_name("WaterBody").type == ::CityObjectType::WaterBody);
    CHECK(city_object_type_from_name("Bridge").type == ::CityObjectType::Bridge);
}

TEST_CASE("city_object_type_from_name maps an unknown name to ExtensionObject") {
    auto result = city_object_type_from_name("+NoiseCityObject");
    CHECK(result.type == ::CityObjectType::ExtensionObject);
    REQUIRE(result.extension_type.has_value());
    CHECK(*result.extension_type == "+NoiseCityObject");
}

TEST_CASE("semantic_surface_type_from_name maps every known surface type") {
    CHECK(semantic_surface_type_from_name("RoofSurface").type ==
          ::SemanticSurfaceType::RoofSurface);
    CHECK(semantic_surface_type_from_name("WallSurface").type ==
          ::SemanticSurfaceType::WallSurface);
    CHECK(semantic_surface_type_from_name("TransportationHole").type ==
          ::SemanticSurfaceType::TransportationHole);
}

TEST_CASE("semantic_surface_type_from_name maps an unknown name to ExtraSemanticSurface") {
    auto result = semantic_surface_type_from_name("+NoiseSurface");
    CHECK(result.type == ::SemanticSurfaceType::ExtraSemanticSurface);
    REQUIRE(result.extension_type.has_value());
    CHECK(*result.extension_type == "+NoiseSurface");
}

TEST_CASE("to_appearance builds materials, textures and vertices-texture") {
    auto appearance = nlohmann::json::parse(R"({
        "materials": [
            {"name": "roofandground", "ambientIntensity": 0.2, "diffuseColor": [0.9, 0.5, 0.1],
             "isSmooth": false}
        ],
        "textures": [
            {"type": "PNG", "image": "textures/facade.png", "wrapMode": "wrap",
             "textureType": "unknown"}
        ],
        "vertices-texture": [[0.5, 0.5], [0.0, 0.0]]
    })");

    flatbuffers::FlatBufferBuilder fbb;
    auto app_off = to_appearance(fbb, appearance);
    fbb.Finish(app_off);

    const ::Appearance* app = flatbuffers::GetRoot<::Appearance>(fbb.GetBufferPointer());
    REQUIRE(app->materials() != nullptr);
    REQUIRE(app->materials()->size() == 1);
    const ::Material* mat = app->materials()->Get(0);
    CHECK(mat->name()->str() == "roofandground");
    REQUIRE(mat->ambient_intensity().has_value());
    CHECK(*mat->ambient_intensity() == doctest::Approx(0.2));
    CHECK_FALSE(mat->is_smooth().value());
    REQUIRE(mat->diffuse_color() != nullptr);
    CHECK(mat->diffuse_color()->size() == 3);

    REQUIRE(app->textures() != nullptr);
    REQUIRE(app->textures()->size() == 1);
    const ::Texture* tex = app->textures()->Get(0);
    CHECK(tex->type() == ::TextureFormat::PNG);
    CHECK(tex->image()->str() == "textures/facade.png");
    REQUIRE(tex->wrap_mode().has_value());
    CHECK(*tex->wrap_mode() == ::WrapMode::Wrap);
    REQUIRE(tex->texture_type().has_value());
    CHECK(*tex->texture_type() == ::TextureType::Unknown);

    REQUIRE(app->vertices_texture() != nullptr);
    REQUIRE(app->vertices_texture()->size() == 2);
    CHECK(app->vertices_texture()->Get(0)->u() == doctest::Approx(0.5));
}

TEST_CASE("to_geometry: boundaries only, no semantics/material/texture") {
    flatbuffers::FlatBufferBuilder fbb;
    auto geometry = nlohmann::json::parse(R"({
        "type": "MultiSurface", "lod": "2",
        "boundaries": [[[0, 1, 2]], [[0, 2, 3]]]
    })");
    const ::Geometry* g = build_geometry(fbb, geometry);

    CHECK(g->type() == ::GeometryType::MultiSurface);
    CHECK(g->lod()->str() == "2");
    REQUIRE(g->boundaries() != nullptr);
    CHECK(g->boundaries()->size() == 6);
    CHECK(g->semantics() == nullptr);
    CHECK(g->semantics_objects() == nullptr);
    CHECK(g->material() == nullptr);
    CHECK(g->texture() == nullptr);
}

TEST_CASE("to_geometry: semantic surfaces carry children, parent and encoded attributes") {
    AttributeSchema semantic_schema;
    add_attributes(semantic_schema, nlohmann::json{{"slope", 33.4}});

    flatbuffers::FlatBufferBuilder fbb;
    auto geometry = nlohmann::json::parse(R"({
        "type": "MultiSurface", "lod": "2",
        "boundaries": [[[0, 1, 2]], [[0, 2, 3]], [[1, 2, 3]]],
        "semantics": {
            "surfaces": [
                {"type": "WallSurface", "slope": 33.4, "children": [2]},
                {"type": "RoofSurface", "parent": 0}
            ],
            "values": [0, 1, 0]
        }
    })");
    const ::Geometry* g = build_geometry(fbb, geometry, &semantic_schema);

    REQUIRE(g->semantics() != nullptr);
    CHECK(g->semantics()->size() == 3);
    REQUIRE(g->semantics_objects() != nullptr);
    REQUIRE(g->semantics_objects()->size() == 2);

    const ::SemanticObject* wall = g->semantics_objects()->Get(0);
    CHECK(wall->type() == ::SemanticSurfaceType::WallSurface);
    REQUIRE(wall->children() != nullptr);
    CHECK(wall->children()->Get(0) == 2);
    CHECK_FALSE(wall->parent().has_value());
    REQUIRE(wall->attributes() != nullptr);
    CHECK(wall->attributes()->size() > 0);

    const ::SemanticObject* roof = g->semantics_objects()->Get(1);
    CHECK(roof->type() == ::SemanticSurfaceType::RoofSurface);
    REQUIRE(roof->parent().has_value());
    CHECK(*roof->parent() == 0);
    CHECK(roof->children() == nullptr);
    CHECK(roof->attributes() == nullptr);  // no extra members beyond type/parent
}

TEST_CASE("to_geometry: material Values arrays are present-but-empty, not absent") {
    flatbuffers::FlatBufferBuilder fbb;
    // MultiSurface-depth material: solids/shells are legitimately empty
    // (one index per surface, no per-shell/per-solid counts), but Rust
    // still creates them as PRESENT empty vectors, not absent fields.
    auto geometry = nlohmann::json::parse(R"({
        "type": "MultiSurface", "lod": "2",
        "boundaries": [[[0, 1, 2]], [[0, 2, 3]]],
        "material": {"roofandground": {"values": [0, 1]}}
    })");
    const ::Geometry* g = build_geometry(fbb, geometry);

    REQUIRE(g->material() != nullptr);
    REQUIRE(g->material()->size() == 1);
    const ::MaterialMapping* mm = g->material()->Get(0);
    CHECK(mm->theme()->str() == "roofandground");
    REQUIRE(mm->solids() != nullptr);
    CHECK(mm->solids()->size() == 0);
    REQUIRE(mm->shells() != nullptr);
    CHECK(mm->shells()->size() == 0);
    REQUIRE(mm->vertices() != nullptr);
    CHECK(mm->vertices()->size() == 2);
    CHECK_FALSE(mm->value().has_value());
}

TEST_CASE("to_geometry: a single material value creates no solids/shells/vertices at all") {
    flatbuffers::FlatBufferBuilder fbb;
    auto geometry = nlohmann::json::parse(R"({
        "type": "MultiSurface", "lod": "2",
        "boundaries": [[[0, 1, 2]]],
        "material": {"t": {"value": 5}}
    })");
    const ::Geometry* g = build_geometry(fbb, geometry);
    const ::MaterialMapping* mm = g->material()->Get(0);
    REQUIRE(mm->value().has_value());
    CHECK(*mm->value() == 5);
    CHECK(mm->solids() == nullptr);
    CHECK(mm->shells() == nullptr);
    CHECK(mm->vertices() == nullptr);
}

TEST_CASE("to_geometry: texture has_values distinguishes present-but-empty from absent arrays") {
    flatbuffers::FlatBufferBuilder fbb;
    auto geometry = nlohmann::json::parse(R"({
        "type": "MultiSurface", "lod": "2",
        "boundaries": [[[0, 1, 2]]],
        "texture": {
            "with_values": {"values": [[[0, 1, 2]]]},
            "without_values": {}
        }
    })");
    const ::Geometry* g = build_geometry(fbb, geometry);
    REQUIRE(g->texture() != nullptr);
    REQUIRE(g->texture()->size() == 2);

    const ::TextureMapping* with_values = nullptr;
    const ::TextureMapping* without_values = nullptr;
    for (const auto* t : *g->texture()) {
        if (t->theme()->str() == "with_values")
            with_values = t;
        else
            without_values = t;
    }
    REQUIRE(with_values != nullptr);
    CHECK(with_values->shells() != nullptr);  // present, even though empty
    CHECK(with_values->vertices()->size() == 3);

    REQUIRE(without_values != nullptr);
    CHECK(without_values->solids() == nullptr);
    CHECK(without_values->shells() == nullptr);
    CHECK(without_values->vertices() == nullptr);
}

TEST_CASE("to_geometry_instance: builds template, transformation and boundaries") {
    flatbuffers::FlatBufferBuilder fbb;
    auto geometry = nlohmann::json::parse(R"({
        "type": "GeometryInstance",
        "template": 2,
        "boundaries": [7],
        "transformationMatrix": [1,0,0,0, 0,1,0,0, 0,0,1,0, 3,4,5,1]
    })");
    auto off = to_geometry_instance(fbb, geometry);
    fbb.Finish(off);
    const ::GeometryInstance* gi = flatbuffers::GetRoot<::GeometryInstance>(fbb.GetBufferPointer());

    CHECK(gi->template_() == 2);
    REQUIRE(gi->boundaries() != nullptr);
    REQUIRE(gi->boundaries()->size() == 1);
    CHECK(gi->boundaries()->Get(0) == 7);
    REQUIRE(gi->transformation() != nullptr);
    CHECK(gi->transformation()->m30() == doctest::Approx(3.0));
}

TEST_CASE("to_geometry_instance: throws on a non-instance geometry type") {
    flatbuffers::FlatBufferBuilder fbb;
    auto geometry = nlohmann::json::parse(R"({"type": "MultiSurface", "boundaries": []})");
    CHECK_THROWS_AS(to_geometry_instance(fbb, geometry), Error);
}
