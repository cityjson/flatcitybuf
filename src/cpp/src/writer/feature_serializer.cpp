#include <fcb/writer/feature_serializer.hpp>

#ifdef FCB_WITH_JSON

#    include <fcb/error.hpp>

namespace fcb {

namespace {

// Same 33 strings, in the same enum order, as cityjson.cpp's
// kCityObjectTypeNames -- kept separate rather than shared, mirroring
// Rust's own split between deserializer.rs's to_cj_co_type (read) and
// serializer.rs's to_co_type (write).
const char* const kCityObjectTypeNames[] = {
    "Bridge",
    "BridgePart",
    "BridgeInstallation",
    "BridgeConstructiveElement",
    "BridgeRoom",
    "BridgeFurniture",
    "Building",
    "BuildingPart",
    "BuildingInstallation",
    "BuildingConstructiveElement",
    "BuildingFurniture",
    "BuildingStorey",
    "BuildingRoom",
    "BuildingUnit",
    "CityFurniture",
    "CityObjectGroup",
    "GenericCityObject",
    "LandUse",
    "OtherConstruction",
    "PlantCover",
    "SolitaryVegetationObject",
    "TINRelief",
    "Road",
    "Railway",
    "Waterway",
    "TransportSquare",
    "Tunnel",
    "TunnelPart",
    "TunnelInstallation",
    "TunnelConstructiveElement",
    "TunnelHollowSpace",
    "TunnelFurniture",
    "WaterBody",
};
constexpr std::size_t kCityObjectTypeCount =
    sizeof(kCityObjectTypeNames) / sizeof(kCityObjectTypeNames[0]);

const char* const kSemanticSurfaceTypeNames[] = {
    "RoofSurface",           "GroundSurface",       "WallSurface",  "ClosureSurface",
    "OuterCeilingSurface",   "OuterFloorSurface",   "Window",       "Door",
    "InteriorWallSurface",   "CeilingSurface",      "FloorSurface", "WaterSurface",
    "WaterGroundSurface",    "WaterClosureSurface", "TrafficArea",  "AuxiliaryTrafficArea",
    "TransportationMarking", "TransportationHole",
};
constexpr std::size_t kSemanticSurfaceTypeCount =
    sizeof(kSemanticSurfaceTypeNames) / sizeof(kSemanticSurfaceTypeNames[0]);

std::optional<::flatbuffers::Offset<::flatbuffers::Vector<double>>>
to_color(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::json& obj, const char* key) {
    auto it = obj.find(key);
    if (it == obj.end() || it->is_null())
        return std::nullopt;
    std::vector<double> v;
    for (const auto& c : *it)
        v.push_back(c.get<double>());
    return fbb.CreateVector(v);
}

}  // namespace

CoType city_object_type_from_name(const std::string& name) {
    for (std::size_t i = 0; i < kCityObjectTypeCount; ++i) {
        if (name == kCityObjectTypeNames[i])
            return {static_cast<::CityObjectType>(i), std::nullopt};
    }
    return {::CityObjectType::ExtensionObject, name};
}

SurfaceType semantic_surface_type_from_name(const std::string& name) {
    for (std::size_t i = 0; i < kSemanticSurfaceTypeCount; ++i) {
        if (name == kSemanticSurfaceTypeNames[i])
            return {static_cast<::SemanticSurfaceType>(i), std::nullopt};
    }
    return {::SemanticSurfaceType::ExtraSemanticSurface, name};
}

::flatbuffers::Offset<::Appearance> to_appearance(::flatbuffers::FlatBufferBuilder& fbb,
                                                  const nlohmann::json& appearance) {
    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Material>>>>
        materials_off;
    if (auto it = appearance.find("materials"); it != appearance.end() && it->is_array()) {
        std::vector<::flatbuffers::Offset<::Material>> materials;
        for (const auto& m : *it) {
            auto name = fbb.CreateString(m.at("name").get<std::string>());
            auto ambient = m.find("ambientIntensity");
            auto shininess = m.find("shininess");
            auto transparency = m.find("transparency");
            auto is_smooth = m.find("isSmooth");
            materials.push_back(
                CreateMaterial(fbb, name,
                               ambient != m.end() && !ambient->is_null()
                                   ? ::flatbuffers::Optional<double>(ambient->get<double>())
                                   : ::flatbuffers::nullopt,
                               to_color(fbb, m, "diffuseColor").value_or(0),
                               to_color(fbb, m, "emissiveColor").value_or(0),
                               to_color(fbb, m, "specularColor").value_or(0),
                               shininess != m.end() && !shininess->is_null()
                                   ? ::flatbuffers::Optional<double>(shininess->get<double>())
                                   : ::flatbuffers::nullopt,
                               transparency != m.end() && !transparency->is_null()
                                   ? ::flatbuffers::Optional<double>(transparency->get<double>())
                                   : ::flatbuffers::nullopt,
                               is_smooth != m.end() && !is_smooth->is_null()
                                   ? ::flatbuffers::Optional<bool>(is_smooth->get<bool>())
                                   : ::flatbuffers::nullopt));
        }
        materials_off = fbb.CreateVector(materials);
    }

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Texture>>>>
        textures_off;
    if (auto it = appearance.find("textures"); it != appearance.end() && it->is_array()) {
        std::vector<::flatbuffers::Offset<::Texture>> textures;
        for (const auto& t : *it) {
            // `type` maps `Some(PNG)|absent -> PNG`; only "JPG" (any case
            // handled by exact string match) selects the other tag.
            auto type_it = t.find("type");
            const ::TextureFormat format = (type_it != t.end() && *type_it == "JPG")
                                               ? ::TextureFormat::JPG
                                               : ::TextureFormat::PNG;
            auto image = fbb.CreateString(t.value("image", std::string()));

            ::flatbuffers::Optional<::WrapMode> wrap_mode = ::flatbuffers::nullopt;
            if (auto w = t.find("wrapMode"); w != t.end() && !w->is_null()) {
                const std::string& s = w->get_ref<const std::string&>();
                if (s == "none")
                    wrap_mode = ::WrapMode::None;
                else if (s == "wrap")
                    wrap_mode = ::WrapMode::Wrap;
                else if (s == "mirror")
                    wrap_mode = ::WrapMode::Mirror;
                else if (s == "clamp")
                    wrap_mode = ::WrapMode::Clamp;
                else if (s == "border")
                    wrap_mode = ::WrapMode::Border;
            }

            ::flatbuffers::Optional<::TextureType> texture_type = ::flatbuffers::nullopt;
            if (auto tt = t.find("textureType"); tt != t.end() && !tt->is_null()) {
                const std::string& s = tt->get_ref<const std::string&>();
                if (s == "unknown")
                    texture_type = ::TextureType::Unknown;
                else if (s == "specific")
                    texture_type = ::TextureType::Specific;
                else if (s == "typical")
                    texture_type = ::TextureType::Typical;
            }

            textures.push_back(CreateTexture(fbb, format, image, wrap_mode, texture_type,
                                             to_color(fbb, t, "borderColor").value_or(0)));
        }
        textures_off = fbb.CreateVector(textures);
    }

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<const ::Vec2*>>> vertices_texture_off;
    if (auto it = appearance.find("vertices-texture"); it != appearance.end() && it->is_array()) {
        std::vector<::Vec2> uvs;
        for (const auto& v : *it)
            uvs.push_back(::Vec2(v.at(0).get<double>(), v.at(1).get<double>()));
        vertices_texture_off = fbb.CreateVectorOfStructs(uvs);
    }

    std::optional<::flatbuffers::Offset<::flatbuffers::String>> default_theme_texture_off;
    if (auto it = appearance.find("default-theme-texture");
        it != appearance.end() && it->is_string())
        default_theme_texture_off = fbb.CreateString(it->get<std::string>());

    std::optional<::flatbuffers::Offset<::flatbuffers::String>> default_theme_material_off;
    if (auto it = appearance.find("default-theme-material");
        it != appearance.end() && it->is_string())
        default_theme_material_off = fbb.CreateString(it->get<std::string>());

    return CreateAppearance(fbb, materials_off.value_or(0), textures_off.value_or(0),
                            vertices_texture_off.value_or(0), default_theme_texture_off.value_or(0),
                            default_theme_material_off.value_or(0));
}

}  // namespace fcb

#endif  // FCB_WITH_JSON
