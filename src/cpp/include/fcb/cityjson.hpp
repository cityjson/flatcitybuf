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
std::string city_object_type_name(std::uint8_t type);

}  // namespace fcb

#endif  // FCB_WITH_JSON
