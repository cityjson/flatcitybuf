#include <fcb/writer/feature_serializer.hpp>

#ifdef FCB_WITH_JSON

#    include <fcb/error.hpp>

#    include <algorithm>

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
to_color(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::ordered_json& obj,
         const char* key) {
    auto it = obj.find(key);
    if (it == obj.end() || it->is_null())
        return std::nullopt;
    std::vector<double> v;
    for (const auto& c : *it)
        v.push_back(c.get<double>());
    return fbb.CreateVector(v);
}

/// One `SemanticObject`, built from a raw CityJSON semantic surface object
/// (a JSON object, not a typed struct -- there is no cjseq-equivalent in
/// C++). `other` -- every member besides `type`/`parent`/`children` -- is
/// encoded against `semantic_attr_schema` when present and non-empty;
/// `semantic_attr_schema == nullptr` (no schema at all, as opposed to an
/// empty one) always yields no attributes, matching Rust's `Option<&..>`.
::flatbuffers::Offset<::SemanticObject>
to_semantic_object(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::ordered_json& surface,
                   const AttributeSchema* semantic_attr_schema) {
    auto [type_, extension_type_name] =
        semantic_surface_type_from_name(surface.at("type").get<std::string>());
    auto extension_type =
        extension_type_name ? std::optional(fbb.CreateString(*extension_type_name)) : std::nullopt;

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<std::uint32_t>>> children;
    if (auto it = surface.find("children"); it != surface.end() && it->is_array()) {
        std::vector<std::uint32_t> c;
        for (const auto& v : *it)
            c.push_back(v.get<std::uint32_t>());
        children = fbb.CreateVector(c);
    }

    ::flatbuffers::Optional<std::uint32_t> parent = ::flatbuffers::nullopt;
    if (auto it = surface.find("parent"); it != surface.end() && !it->is_null())
        parent = it->get<std::uint32_t>();

    nlohmann::ordered_json other = nlohmann::ordered_json::object();
    for (const auto& [key, val] : surface.items())
        if (key != "type" && key != "parent" && key != "children")
            other[key] = val;

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<std::uint8_t>>> attributes;
    if (!other.empty() && semantic_attr_schema != nullptr) {
        attributes = fbb.CreateVector(encode_attributes_with_schema(other, *semantic_attr_schema));
    }

    return CreateSemanticObject(fbb, type_, attributes.value_or(0), children.value_or(0), parent,
                                extension_type.value_or(0));
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
                                                  const nlohmann::ordered_json& appearance) {
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

::flatbuffers::Offset<::Geometry> to_geometry(::flatbuffers::FlatBufferBuilder& fbb,
                                              const nlohmann::ordered_json& geometry,
                                              const AttributeSchema* semantic_attr_schema) {
    const GeometryKind kind = geometry_kind_from_name(geometry.at("type").get<std::string>());
    const auto type_ = static_cast<::GeometryType>(kind);
    auto lod = geometry.find("lod");
    auto lod_off = (lod != geometry.end() && lod->is_string())
                       ? std::optional(fbb.CreateString(lod->get<std::string>()))
                       : std::nullopt;

    EncodedGeometry encoded = encode(geometry);
    auto solids_off = fbb.CreateVector(encoded.boundaries.solids);
    auto shells_off = fbb.CreateVector(encoded.boundaries.shells);
    auto surfaces_off = fbb.CreateVector(encoded.boundaries.surfaces);
    auto strings_off = fbb.CreateVector(encoded.boundaries.strings);
    auto boundaries_off = fbb.CreateVector(encoded.boundaries.indices);

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<std::uint32_t>>> semantics_values_off;
    std::optional<
        ::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::SemanticObject>>>>
        semantics_objects_off;
    if (encoded.semantics) {
        std::vector<::flatbuffers::Offset<::SemanticObject>> objects;
        for (const auto& surface : encoded.semantics->surfaces)
            objects.push_back(to_semantic_object(fbb, surface, semantic_attr_schema));
        semantics_objects_off = fbb.CreateVector(objects);
        if (encoded.semantics->values)
            semantics_values_off = fbb.CreateVector(*encoded.semantics->values);
    }

    std::optional<
        ::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::MaterialMapping>>>>
        material_off;
    if (encoded.materials) {
        std::vector<::flatbuffers::Offset<::MaterialMapping>> mappings;
        for (const auto& m : *encoded.materials) {
            auto theme = fbb.CreateString(m.theme);
            switch (m.kind) {
                case fcb::MaterialMapping::Kind::Value:
                    mappings.push_back(CreateMaterialMapping(
                        fbb, theme, 0, 0, 0, ::flatbuffers::Optional<std::uint32_t>(m.value)));
                    break;
                case fcb::MaterialMapping::Kind::Values:
                    // Present-but-empty: created unconditionally, even when
                    // a level genuinely has zero entries, so `[]` stays
                    // distinct from an absent field.
                    mappings.push_back(CreateMaterialMapping(
                        fbb, theme, fbb.CreateVector(m.solids), fbb.CreateVector(m.shells),
                        fbb.CreateVector(m.vertices), ::flatbuffers::nullopt));
                    break;
                case fcb::MaterialMapping::Kind::NullValues:
                    mappings.push_back(
                        CreateMaterialMapping(fbb, theme, 0, 0, 0, ::flatbuffers::nullopt));
                    break;
            }
        }
        material_off = fbb.CreateVector(mappings);
    }

    std::optional<
        ::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::TextureMapping>>>>
        texture_off;
    if (encoded.textures) {
        std::vector<::flatbuffers::Offset<::TextureMapping>> mappings;
        for (const auto& t : *encoded.textures) {
            auto theme = fbb.CreateString(t.theme);
            if (t.has_values) {
                // As with material Values: all five arrays are created
                // unconditionally, even where empty.
                mappings.push_back(CreateTextureMapping(
                    fbb, theme, fbb.CreateVector(t.solids), fbb.CreateVector(t.shells),
                    fbb.CreateVector(t.surfaces), fbb.CreateVector(t.strings),
                    fbb.CreateVector(t.vertices)));
            } else {
                mappings.push_back(CreateTextureMapping(fbb, theme, 0, 0, 0, 0, 0));
            }
        }
        texture_off = fbb.CreateVector(mappings);
    }

    return CreateGeometry(fbb, type_, lod_off.value_or(0), solids_off, shells_off, surfaces_off,
                          strings_off, boundaries_off, semantics_values_off.value_or(0),
                          semantics_objects_off.value_or(0), material_off.value_or(0),
                          texture_off.value_or(0));
}

::flatbuffers::Offset<::GeometryInstance>
to_geometry_instance(::flatbuffers::FlatBufferBuilder& fbb,
                     const nlohmann::ordered_json& geometry) {
    if (geometry.at("type").get<std::string>() != "GeometryInstance") {
        throw Error(ErrorCode::InvalidAttributeValue,
                    "to_geometry_instance called on a non-GeometryInstance geometry");
    }

    const std::uint32_t template_ = geometry.at("template").get<std::uint32_t>();

    std::vector<std::uint32_t> indices;
    for (const auto& v : geometry.at("boundaries"))
        indices.push_back(v.get<std::uint32_t>());
    auto boundaries_off = fbb.CreateVector(indices);

    const auto& m = geometry.at("transformationMatrix");
    ::TransformationMatrix matrix(
        m.at(0).get<double>(), m.at(1).get<double>(), m.at(2).get<double>(), m.at(3).get<double>(),
        m.at(4).get<double>(), m.at(5).get<double>(), m.at(6).get<double>(), m.at(7).get<double>(),
        m.at(8).get<double>(), m.at(9).get<double>(), m.at(10).get<double>(),
        m.at(11).get<double>(), m.at(12).get<double>(), m.at(13).get<double>(),
        m.at(14).get<double>(), m.at(15).get<double>());

    return CreateGeometryInstance(fbb, &matrix, template_, boundaries_off);
}

::GeographicalExtent to_geographical_extent(const std::array<double, 6>& extent) {
    return ::GeographicalExtent(::Vector(extent[0], extent[1], extent[2]),
                                ::Vector(extent[3], extent[4], extent[5]));
}

namespace {

/// Encodes `attr` against `schema`, or against a freshly-built schema of
/// its own when `attr` carries a key `schema` does not know. Mirrors
/// `to_fcb_attribute` (writer/serializer.rs:1151-1174).
struct FcbAttribute {
    ::flatbuffers::Offset<::flatbuffers::Vector<std::uint8_t>> attr_offset;
    std::optional<AttributeSchema> own_schema;
};

FcbAttribute to_fcb_attribute(::flatbuffers::FlatBufferBuilder& fbb,
                              const nlohmann::ordered_json& attr, const AttributeSchema& schema) {
    bool is_own_schema = false;
    for (const auto& [key, val] : attr.items()) {
        if (schema.find(key) == schema.end()) {
            is_own_schema = true;
            break;
        }
    }
    if (is_own_schema) {
        AttributeSchema own_schema;
        add_attributes(own_schema, attr);
        auto encoded = encode_attributes_with_schema(attr, own_schema);
        return {fbb.CreateVector(encoded), std::move(own_schema)};
    }
    auto encoded = encode_attributes_with_schema(attr, schema);
    return {fbb.CreateVector(encoded), std::nullopt};
}

}  // namespace

::flatbuffers::Offset<::CityObject> to_city_object(::flatbuffers::FlatBufferBuilder& fbb,
                                                   const std::string& id,
                                                   const nlohmann::ordered_json& co,
                                                   const AttributeSchema& attr_schema,
                                                   const AttributeSchema* semantic_attr_schema) {
    auto id_off = fbb.CreateString(id);
    auto [type_, extension_type_name] =
        city_object_type_from_name(co.at("type").get<std::string>());
    auto extension_type_off =
        extension_type_name ? std::optional(fbb.CreateString(*extension_type_name)) : std::nullopt;

    std::optional<::GeographicalExtent> extent;
    if (auto it = co.find("geographicalExtent");
        it != co.end() && it->is_array() && it->size() == 6) {
        extent = to_geographical_extent({it->at(0).get<double>(), it->at(1).get<double>(),
                                         it->at(2).get<double>(), it->at(3).get<double>(),
                                         it->at(4).get<double>(), it->at(5).get<double>()});
    }

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Geometry>>>>
        geometry_off;
    std::optional<
        ::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::GeometryInstance>>>>
        geometry_instances_off;
    if (auto git = co.find("geometry"); git != co.end() && git->is_array()) {
        std::vector<::flatbuffers::Offset<::Geometry>> geoms;
        std::vector<::flatbuffers::Offset<::GeometryInstance>> instances;
        for (const auto& g : *git) {
            if (g.at("type").get<std::string>() == "GeometryInstance")
                instances.push_back(to_geometry_instance(fbb, g));
            else
                geoms.push_back(to_geometry(fbb, g, semantic_attr_schema));
        }
        // Both created -- even empty -- whenever "geometry" is present at
        // all, matching Rust's Option<Vec<_>> filtered from ONE Option: it
        // is the presence of the key, not either resulting list's own
        // emptiness, that decides presence on the wire.
        geometry_off = fbb.CreateVector(geoms);
        geometry_instances_off = fbb.CreateVector(instances);
    }

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<std::uint8_t>>> attributes_off;
    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Column>>>>
        columns_off;
    if (auto ait = co.find("attributes"); ait != co.end() && ait->is_object()) {
        FcbAttribute fcb_attr = to_fcb_attribute(fbb, *ait, attr_schema);
        attributes_off = fcb_attr.attr_offset;
        if (fcb_attr.own_schema)
            columns_off = to_columns(fbb, *fcb_attr.own_schema);
    }

    std::optional<
        ::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::flatbuffers::String>>>>
        children_off;
    if (auto it = co.find("children"); it != co.end() && it->is_array()) {
        std::vector<::flatbuffers::Offset<::flatbuffers::String>> c;
        for (const auto& s : *it)
            c.push_back(fbb.CreateString(s.get<std::string>()));
        children_off = fbb.CreateVector(c);
    }

    // "childrenRoles" (CityObjectGroup only): an unspecified role is `null`
    // in CityJSON; the header has no way to spell that, so it is written
    // as the empty string, mirroring the equivalent handling for
    // point-of-contact strings elsewhere in this writer.
    std::optional<
        ::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::flatbuffers::String>>>>
        children_roles_off;
    if (auto it = co.find("childrenRoles"); it != co.end() && it->is_array()) {
        std::vector<::flatbuffers::Offset<::flatbuffers::String>> r;
        for (const auto& role : *it)
            r.push_back(
                fbb.CreateString(role.is_string() ? role.get<std::string>() : std::string()));
        children_roles_off = fbb.CreateVector(r);
    }

    std::optional<
        ::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::flatbuffers::String>>>>
        parents_off;
    if (auto it = co.find("parents"); it != co.end() && it->is_array()) {
        std::vector<::flatbuffers::Offset<::flatbuffers::String>> p;
        for (const auto& s : *it)
            p.push_back(fbb.CreateString(s.get<std::string>()));
        parents_off = fbb.CreateVector(p);
    }

    return CreateCityObject(fbb, type_, extension_type_off.value_or(0), id_off,
                            extent ? &*extent : nullptr, geometry_off.value_or(0),
                            geometry_instances_off.value_or(0), attributes_off.value_or(0),
                            columns_off.value_or(0), children_off.value_or(0),
                            children_roles_off.value_or(0), parents_off.value_or(0));
}

std::pair<::flatbuffers::Offset<::CityFeature>, NodeItem>
to_fcb_city_feature(::flatbuffers::FlatBufferBuilder& fbb, const std::string& id,
                    const nlohmann::ordered_json& city_feature, const AttributeSchema& attr_schema,
                    const AttributeSchema* semantic_attr_schema) {
    auto id_off = fbb.CreateString(id);

    // `CityObjects` is a JSON object, so visited in ascending id order for
    // reproducibility -- same determinism reasoning as
    // cityfeature_to_index_entries (writer/attribute.hpp).
    static const nlohmann::ordered_json kEmptyObjects = nlohmann::ordered_json::object();
    auto co_it = city_feature.find("CityObjects");
    const nlohmann::ordered_json& city_objects =
        (co_it != city_feature.end() && co_it->is_object()) ? *co_it : kEmptyObjects;

    std::vector<std::string> object_ids;
    object_ids.reserve(city_objects.size());
    for (const auto& [oid, unused] : city_objects.items())
        object_ids.push_back(oid);
    std::sort(object_ids.begin(), object_ids.end());

    std::vector<::flatbuffers::Offset<::CityObject>> objects;
    objects.reserve(object_ids.size());
    for (const auto& oid : object_ids)
        objects.push_back(
            to_city_object(fbb, oid, city_objects.at(oid), attr_schema, semantic_attr_schema));
    auto objects_off = fbb.CreateVector(objects);

    std::vector<::Vertex> fb_vertices;
    double min_x = 0, min_y = 0, max_x = 0, max_y = 0;
    bool first = true;
    if (auto it = city_feature.find("vertices"); it != city_feature.end()) {
        fb_vertices.reserve(it->size());
        for (const auto& v : *it) {
            const double x = v.at(0).get<double>();
            const double y = v.at(1).get<double>();
            fb_vertices.emplace_back(v.at(0).get<std::int32_t>(), v.at(1).get<std::int32_t>(),
                                     v.at(2).get<std::int32_t>());
            if (first) {
                min_x = max_x = x;
                min_y = max_y = y;
                first = false;
            } else {
                min_x = std::min(min_x, x);
                max_x = std::max(max_x, x);
                min_y = std::min(min_y, y);
                max_y = std::max(max_y, y);
            }
        }
    }
    auto vertices_off = fbb.CreateVectorOfStructs(fb_vertices);

    std::optional<::flatbuffers::Offset<::Appearance>> appearance_off;
    if (auto it = city_feature.find("appearance"); it != city_feature.end() && it->is_object())
        appearance_off = to_appearance(fbb, *it);

    NodeItem bbox{min_x, min_y, max_x, max_y, 0};
    auto feature_off =
        CreateCityFeature(fbb, id_off, objects_off, vertices_off, appearance_off.value_or(0));
    return {feature_off, bbox};
}

}  // namespace fcb

#endif  // FCB_WITH_JSON
