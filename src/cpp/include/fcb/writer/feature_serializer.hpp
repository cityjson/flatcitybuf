#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/generated/feature_generated.h>
#    include <fcb/generated/geometry_generated.h>
#    include <fcb/generated/header_generated.h>
#    include <fcb/packed_rtree.hpp>
#    include <fcb/writer/attribute.hpp>
#    include <fcb/writer/geom_encoder.hpp>

#    include <nlohmann/json.hpp>

#    include <array>
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

/// Builds one `Geometry` table -- boundaries, and whatever semantics,
/// material and texture it carries -- from a CityJSON geometry object
/// (everything but a `GeometryInstance`, which `to_geometry_instance`
/// handles). `semantic_attr_schema` is `nullptr` when the file has no
/// semantic attribute schema at all; a present-but-empty schema is a
/// distinct, valid state. Mirrors `to_geometry` (writer/serializer.rs:891-1065).
::flatbuffers::Offset<::Geometry> to_geometry(::flatbuffers::FlatBufferBuilder& fbb,
                                              const nlohmann::json& geometry,
                                              const AttributeSchema* semantic_attr_schema);

/// Builds a `GeometryInstance` table: the template index, the 4x4
/// transformation matrix, and the single-element boundaries array holding
/// the reference vertex index. Throws `fcb::Error` if `geometry`'s `type`
/// is not `"GeometryInstance"` (Rust `panic!`s here instead -- a
/// programmer-error assertion that becomes a catchable exception in a
/// library rather than an abort). Mirrors `to_geometry_instance`
/// (writer/serializer.rs:1067-1102).
::flatbuffers::Offset<::GeometryInstance>
to_geometry_instance(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::json& geometry);

/// Builds a `GeographicalExtent` struct from a 6-element
/// `[minx,miny,minz,maxx,maxy,maxz]` array. Shared with the header (M4),
/// which has its own `geographicalExtent`. Mirrors `to_geographical_extent`
/// (writer/serializer.rs:233-250).
::GeographicalExtent to_geographical_extent(const std::array<double, 6>& extent);

/// Builds one `CityObject` table: type, geographical extent, geometry
/// (split into non-instance and `GeometryInstance` entries), attributes
/// (against `attr_schema`, or the object's own schema when its attribute
/// keys are not all present in `attr_schema`), children/children-roles/
/// parents. Mirrors `to_city_object` (writer/serializer.rs:632-730).
::flatbuffers::Offset<::CityObject> to_city_object(::flatbuffers::FlatBufferBuilder& fbb,
                                                   const std::string& id, const nlohmann::json& co,
                                                   const AttributeSchema& attr_schema,
                                                   const AttributeSchema* semantic_attr_schema);

/// Builds one `CityFeature` table -- its CityObjects (visited in ascending
/// id order), vertices, and optional feature-level appearance -- from a
/// CityJSONFeature JSON object, plus its 2D bounding box over the RAW
/// (untransformed, still-integer) vertex coordinates. Applying the file's
/// `Transform` scale/translate to that bbox, and everything about hilbert
/// ordering and index assembly, is the caller's job (M7's `FcbWriter`).
/// Mirrors `to_fcb_city_feature` (writer/serializer.rs:410-489).
std::pair<::flatbuffers::Offset<::CityFeature>, NodeItem>
to_fcb_city_feature(::flatbuffers::FlatBufferBuilder& fbb, const std::string& id,
                    const nlohmann::json& city_feature, const AttributeSchema& attr_schema,
                    const AttributeSchema* semantic_attr_schema);

}  // namespace fcb

#endif  // FCB_WITH_JSON
