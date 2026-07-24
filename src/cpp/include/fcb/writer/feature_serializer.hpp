#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/generated/feature_generated.h>
#    include <fcb/generated/geometry_generated.h>
#    include <fcb/generated/header_generated.h>
#    include <fcb/writer/attribute.hpp>
#    include <fcb/writer/geom_encoder.hpp>

#    include <nlohmann/json.hpp>

#    include <optional>
#    include <string>

namespace fcb {

/// A CityObjectType tag, plus the verbatim CityJSON name when the tag is
/// `ExtensionObject` (which has no spelling of its own).
struct CoType {
    ::CityObjectType type;
    std::optional<std::string> extension_type;
};

/// Maps a CityJSON CityObject `type` string to the FlatBuffers tag. A name
/// outside CityJSON's ~33 known object types becomes `ExtensionObject` plus
/// the name verbatim -- never an error. Mirrors `to_co_type`
/// (writer/serializer.rs:745-792).
CoType city_object_type_from_name(const std::string& name);

/// A SemanticSurfaceType tag, plus the verbatim CityJSON name when the tag
/// is `ExtraSemanticSurface`.
struct SurfaceType {
    ::SemanticSurfaceType type;
    std::optional<std::string> extension_type;
};

/// Maps a CityJSON semantic surface `type` string to the FlatBuffers tag.
/// A name outside CityJSON's 18 known surface types becomes
/// `ExtraSemanticSurface` plus the name verbatim. Mirrors
/// `FcbSemanticSurfaceType::from` (writer/serializer.rs:820-883).
SurfaceType semantic_surface_type_from_name(const std::string& name);

/// Builds the `Appearance` table (materials, textures, UV vertices, default
/// themes) from a CityJSON `appearance` object. Mirrors `to_appearance`
/// (writer/serializer.rs:533-622); `fb_wrap_mode`/`fb_texture_type`/
/// `fb_texture_format` are folded in directly since nothing else calls them.
::flatbuffers::Offset<::Appearance> to_appearance(::flatbuffers::FlatBufferBuilder& fbb,
                                                  const nlohmann::json& appearance);

}  // namespace fcb

#endif  // FCB_WITH_JSON
