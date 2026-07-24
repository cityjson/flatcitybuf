#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/geometry.hpp>

#    include <nlohmann/json.hpp>

#    include <cstdint>
#    include <optional>
#    include <string>
#    include <vector>

namespace fcb {

/// Flattened geometry boundaries: a flat vertex-index list plus one count
/// array per level of the dimensional hierarchy (rings-per-surface,
/// surfaces-per-shell, shells-per-solid). Mirrors `GMBoundaries`
/// (writer/geom_encoder.rs).
struct GMBoundaries {
    std::vector<std::uint32_t> solids;    // shells per solid
    std::vector<std::uint32_t> shells;    // surfaces per shell
    std::vector<std::uint32_t> surfaces;  // rings per surface
    std::vector<std::uint32_t> strings;   // indices per ring
    std::vector<std::uint32_t> indices;   // flattened vertex indices
};

/// Maps a CityJSON geometry `type` string to `GeometryKind`. Throws
/// `Error{InvalidAttributeValue}` on any string outside CityJSON's eight
/// known geometry types.
GeometryKind geometry_kind_from_name(const std::string& name);

/// Flattens `boundaries` (nested CityJSON boundary arrays, straight off the
/// JSON as parsed -- no intermediate typed representation) at exactly the
/// depth `kind` implies. Mirrors `encode_boundaries`
/// (writer/geom_encoder.rs:160-189). `GeometryKind::GeometryInstance`
/// yields an empty `GMBoundaries`; it is encoded separately (M3).
GMBoundaries encode_boundaries(GeometryKind kind, const nlohmann::json& boundaries);

/// Flattened semantics: `surfaces` is the CityJSON `semantics.surfaces`
/// array, passed through verbatim (FlatBuffers `SemanticObject` encoding,
/// including its "other" attributes, is M3's job). `values` is one flat
/// entry per surface, `UINT32_MAX` for `null`, or `std::nullopt` for an
/// absent/`null` `semantics.values` member (schema-valid and distinct from
/// an empty array). Mirrors `GMSemantics` (writer/geom_encoder.rs).
struct GMSemantics {
    nlohmann::json surfaces = nlohmann::json::array();
    std::optional<std::vector<std::uint32_t>> values;
};

/// Flattens `semantics` (the CityJSON `semantics` object: `surfaces` plus
/// `values`) at the depth `kind` implies, using `boundaries`'s shell/solid
/// counts to expand a `null` shell or solid into the right number of
/// per-surface nulls (there is no wire encoding for a whole-null shell in
/// semantics, unlike material). Mirrors `encode_semantics`
/// (writer/geom_encoder.rs:379-447), dispatching on `kind` rather than on
/// `values`'s own shape (see the M2 plan's Global Constraints).
GMSemantics encode_semantics(const nlohmann::json& semantics, GeometryKind kind,
                             const GMBoundaries& boundaries);

}  // namespace fcb

#endif  // FCB_WITH_JSON
