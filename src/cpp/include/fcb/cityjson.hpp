#pragma once

#ifdef FCB_WITH_JSON

#include <nlohmann/json.hpp>

#include <fcb/feature.hpp>
#include <fcb/header.hpp>

namespace fcb {

/// The CityJSON metadata envelope: type, version, transform, extent, CRS.
/// This is the first line of a CityJSONSeq stream.
nlohmann::json to_cityjson_metadata(const HeaderView& header);

/// One feature as a CityJSONFeature object.
///
/// Attributes are decoded against each CityObject's OWN column schema when
/// it declares one, falling back to the header's -- see the per-object
/// schema note in the plan. Using the wrong schema yields silent garbage.
nlohmann::json to_cityjson_feature(const Feature& feature, const HeaderView& header);

/// CityJSON name for a CityObjectType enumerator, e.g. "BuildingPart".
///
/// A tag with no CityJSON name of its own -- `ExtensionObject`, whose real
/// name lives in the object's `extension_type` string, or anything a newer
/// encoder added -- becomes `"+UnknownCityObject"`. See the unknown-tag policy
/// note at the top of cityjson.cpp.
std::string city_object_type_name(std::uint8_t type);

/// CityJSON name for a SemanticSurfaceType enumerator, e.g. "RoofSurface".
///
/// A tag with no CityJSON name of its own -- `ExtraSemanticSurface`, whose
/// real name lives in the surface's `extension_type` string, or anything a
/// newer encoder added -- becomes `"+GenericSurface"`. Same policy note.
std::string semantic_surface_type_name(std::uint8_t type);

}  // namespace fcb

#endif  // FCB_WITH_JSON
