#include <fcb/writer/feature_serializer.hpp>

#include <doctest/doctest.h>

using namespace fcb;

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
