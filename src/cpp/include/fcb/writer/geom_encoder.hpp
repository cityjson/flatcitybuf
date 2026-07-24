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

/// One theme's material mapping. Tagged-struct style (matching this
/// codebase's `AttrValue` convention) rather than `std::variant`, since only
/// one of `value`/`{solids,shells,vertices}` is meaningful per `kind`.
/// Mirrors the `MaterialMapping` enum (writer/geom_encoder.rs).
struct MaterialMapping {
    enum class Kind {
        Value,      // a single material index for the whole theme
        Values,     // one index per surface (nested per `solids`/`shells`)
        NullValues  // `"values": null` -- a theme with no arrays at all
    };
    Kind kind = Kind::Value;
    std::string theme;
    std::uint32_t value = 0;                              // Kind::Value
    std::vector<std::uint32_t> solids, shells, vertices;  // Kind::Values
};

/// Flattens `material` (the CityJSON `geometry.material` object: theme name
/// -> `{"value": N}` / `{"values": [...]}` / `{"values": null}`) at the
/// depth `kind` implies. Themes are visited in ascending name order (`material`
/// is a JSON object, so this is automatic given nlohmann's default ordered
/// container -- see the AttributeSchema note in `writer/attribute.hpp` for
/// why that already matches Rust's determinism fix for its `HashMap`
/// source). A `null` shell or solid -- legal at every level -- is recorded as
/// a `UINT32_MAX` count, not dropped, so it decodes back as `null` rather
/// than an empty array. Mirrors `encode_material`
/// (writer/geom_encoder.rs:204-284), dispatching on `kind` rather than on
/// `values`'s own shape.
std::vector<MaterialMapping> encode_material(const nlohmann::json& material, GeometryKind kind);

/// One theme's texture mapping. `has_values` distinguishes a theme with no
/// `values` member at all (schema-valid; the per-theme texture object has
/// no `required`) from one whose `values` array is present. Mirrors
/// `TextureMapping` (writer/geom_encoder.rs).
struct TextureMapping {
    std::string theme;
    bool has_values = false;
    std::vector<std::uint32_t> solids, shells, surfaces, strings, vertices;
};

/// Flattens `texture` (the CityJSON `geometry.texture` object) at the depth
/// `kind` implies. `texture.values` nests exactly as deeply as `boundaries`
/// itself (one UV-vertex index per boundary vertex index), unlike material,
/// which is one level shallower. Nullable only at the leaf: unlike
/// material, there is no wire encoding for a whole-null intermediate level,
/// so nothing here decodes an intermediate `null`. Mirrors `encode_texture`
/// (writer/geom_encoder.rs:290-361), dispatching on `kind` rather than on
/// `values`'s own shape.
std::vector<TextureMapping> encode_texture(const nlohmann::json& texture, GeometryKind kind);

/// Everything one CityJSON geometry object flattens to. Mirrors
/// `EncodedGeometry` (writer/geom_encoder.rs).
struct EncodedGeometry {
    GMBoundaries boundaries;
    std::optional<GMSemantics> semantics;
    std::optional<std::vector<MaterialMapping>> materials;
    std::optional<std::vector<TextureMapping>> textures;
};

/// Flattens one CityJSON geometry object -- its boundaries and whatever
/// semantics, material and texture it carries -- into the FlatCityBuf
/// arrays. `geometry` is read directly off the parsed JSON (`type`,
/// `boundaries`, and optionally `semantics`/`material`/`texture`); a
/// `GeometryInstance` carries none of these except a differently-shaped
/// `boundaries` (a single reference vertex index, not nested arrays) that
/// this never touches -- it is encoded separately by M3's
/// `to_geometry_instance`, and yields empty arrays here. Mirrors `encode`
/// (writer/geom_encoder.rs:102-120).
EncodedGeometry encode(const nlohmann::json& geometry);

}  // namespace fcb

#endif  // FCB_WITH_JSON
