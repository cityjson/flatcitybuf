#include <fcb/writer/geom_encoder.hpp>

#ifdef FCB_WITH_JSON

#    include <fcb/error.hpp>

#    include <cstdint>
#    include <limits>

namespace fcb {

namespace {

constexpr std::uint32_t kNull = std::numeric_limits<std::uint32_t>::max();

void push_ring(const nlohmann::json& ring, GMBoundaries& b) {
    b.strings.push_back(static_cast<std::uint32_t>(ring.size()));
    for (const auto& idx : ring)
        b.indices.push_back(idx.get<std::uint32_t>());
}

void push_surface(const nlohmann::json& surface, GMBoundaries& b) {
    for (const auto& ring : surface)
        push_ring(ring, b);
    b.surfaces.push_back(static_cast<std::uint32_t>(surface.size()));
}

void push_shell(const nlohmann::json& shell, GMBoundaries& b) {
    for (const auto& surface : shell)
        push_surface(surface, b);
    b.shells.push_back(static_cast<std::uint32_t>(shell.size()));
}

void push_solid(const nlohmann::json& solid, GMBoundaries& b) {
    for (const auto& shell : solid)
        push_shell(shell, b);
    b.solids.push_back(static_cast<std::uint32_t>(solid.size()));
}

std::uint32_t semantics_index(const nlohmann::json& v) {
    return v.is_null() ? kNull : v.get<std::uint32_t>();
}

/// Appends one shell's worth of semantic indices. `shell` is `nullptr` for a
/// whole-null shell, expanded to `boundaries.shells[shell_cursor]` NULLs --
/// semantics has no wire encoding for a null shell, unlike material.
void push_semantics_shell(const nlohmann::json* shell, const GMBoundaries& boundaries,
                          std::size_t& shell_cursor, std::vector<std::uint32_t>& flattened) {
    const std::uint32_t surface_count =
        shell_cursor < boundaries.shells.size() ? boundaries.shells[shell_cursor] : 0;
    ++shell_cursor;
    if (shell == nullptr) {
        flattened.insert(flattened.end(), surface_count, kNull);
    } else {
        for (const auto& v : *shell)
            flattened.push_back(semantics_index(v));
    }
}

std::uint32_t material_index(const nlohmann::json& v) {
    return v.is_null() ? kNull : v.get<std::uint32_t>();
}

/// Appends one shell's material indices, or (`shell == nullptr`) records a
/// whole-null shell as a `kNull` COUNT -- unlike semantics, material has a
/// wire encoding for this, so it is not expanded into per-surface nulls.
void push_material_shell(const nlohmann::json* shell, MaterialMapping& mv) {
    if (shell == nullptr) {
        mv.shells.push_back(kNull);
    } else {
        mv.shells.push_back(static_cast<std::uint32_t>(shell->size()));
        for (const auto& v : *shell)
            mv.vertices.push_back(material_index(v));
    }
}

void push_textured_ring(const nlohmann::json& ring, TextureMapping& m) {
    m.strings.push_back(static_cast<std::uint32_t>(ring.size()));
    for (const auto& v : ring)
        m.vertices.push_back(material_index(v));  // null-aware, same as material
}

void push_textured_surface(const nlohmann::json& surface, TextureMapping& m) {
    for (const auto& ring : surface)
        push_textured_ring(ring, m);
    m.surfaces.push_back(static_cast<std::uint32_t>(surface.size()));
}

void push_textured_shell(const nlohmann::json& shell, TextureMapping& m) {
    for (const auto& surface : shell)
        push_textured_surface(surface, m);
    m.shells.push_back(static_cast<std::uint32_t>(shell.size()));
}

// ---------------------------------------------------------------------------
// Shape sniffing for semantics.values / material.values / texture.values.
//
// Rust's SemanticsValues/MaterialValues/TextureValues are `#[serde(untagged)]`
// enums: deserialization tries each variant in DECLARATION order (shallowest
// first) and uses the first one the JSON structurally fits. That means the
// depth Rust actually uses is a property of the VALUES ARRAY ITSELF, not of
// the enclosing geometry's type -- the two agree for every cardinality-
// consistent file (which is every real file any encoder produces), but a
// schema-valid, cardinality-INCONSISTENT file (e.g. a `Solid` whose
// `semantics.values` is a flat one-element array instead of properly nested)
// would be read at a different depth by shape alone than by geometry type.
// Sniffing here, rather than keying on GeometryKind, is what keeps this port
// byte-compatible with Rust on those files too.
//
// Known, deliberately unhandled gap (found in the M2 codex review's
// follow-up pass): a leaf here is accepted as long as it is not an array,
// but Rust's leaf type is `Option<usize>`, so a NEGATIVE integer (e.g.
// `semantics.values: [-1]`) fails every variant in Rust -- the whole
// CityJSON document fails to parse before ever reaching this code -- while
// this sniffs it as rank 1 and silently encodes it as NULL. No real
// encoder emits a negative semantic/material/UV index, so this is not
// fixed: doing so would mean threading non-negativity checks through every
// leaf of every rank, for a case with no realistic input.
// ---------------------------------------------------------------------------

/// True if `arr` fits the semantics/material shape at `rank` -- 1: a flat
/// array of null-or-non-array; 2: an array of null-or-(rank 1); 3: an array
/// of null-or-(rank 2). `null` is legal at every level for this hierarchy,
/// unlike texture's.
bool fits_nullable_rank(const nlohmann::json& arr, int rank) {
    if (!arr.is_array())
        return false;
    for (const auto& el : arr) {
        if (el.is_null())
            continue;
        if (rank == 1) {
            if (el.is_array())
                return false;
        } else if (!fits_nullable_rank(el, rank - 1)) {
            return false;
        }
    }
    return true;
}

/// The shallowest rank (1, 2 or 3) `arr` fits, tried in that order -- the
/// same order Rust's untagged enum variants are declared in, so an array
/// readable at more than one depth resolves identically here.
int sniff_nullable_rank(const nlohmann::json& arr) {
    for (int rank = 1; rank <= 3; ++rank)
        if (fits_nullable_rank(arr, rank))
            return rank;
    throw Error(ErrorCode::InvalidAttributeValue, "values array nests deeper than a Solid permits");
}

/// True if `v` is a valid texture ring: an array whose own elements are
/// each null or a non-array (a UV-vertex index). Texture, unlike
/// semantics/material, is never null at an intermediate level -- only a
/// ring's own elements can be.
bool is_texture_ring(const nlohmann::json& v) {
    if (!v.is_array())
        return false;
    for (const auto& el : v)
        if (!el.is_null() && el.is_array())
            return false;
    return true;
}

/// True if `arr` fits a texture shape `depth` levels above a ring:
/// `depth == 0` means `arr` must itself be a ring; `depth > 0` means an
/// array whose every element fits `depth - 1`.
bool fits_texture_depth(const nlohmann::json& arr, int depth) {
    if (depth == 0)
        return is_texture_ring(arr);
    if (!arr.is_array())
        return false;
    for (const auto& el : arr)
        if (!fits_texture_depth(el, depth - 1))
            return false;
    return true;
}

/// The shallowest variant (1: `Surface`, a list of `TexturedSurface`; 2:
/// `Shell`, a list of `TexturedShell`; 3: `Solid`, a list of shell-lists,
/// one per solid) `arr` fits, tried in that order -- mirroring Rust's
/// untagged `TextureValues` enum, declared shallowest-first. A
/// `TexturedSurface` is 2 array levels above a bare ring (a list of
/// rings), so variant `rank` corresponds to `fits_texture_depth(arr, rank
/// + 1)`, not `rank` itself.
int sniff_texture_depth(const nlohmann::json& arr) {
    for (int rank = 1; rank <= 3; ++rank)
        if (fits_texture_depth(arr, rank + 1))
            return rank;
    throw Error(ErrorCode::InvalidAttributeValue,
                "texture values array nests deeper than a Solid permits");
}

}  // namespace

GeometryKind geometry_kind_from_name(const std::string& name) {
    if (name == "MultiPoint")
        return GeometryKind::MultiPoint;
    if (name == "MultiLineString")
        return GeometryKind::MultiLineString;
    if (name == "MultiSurface")
        return GeometryKind::MultiSurface;
    if (name == "CompositeSurface")
        return GeometryKind::CompositeSurface;
    if (name == "Solid")
        return GeometryKind::Solid;
    if (name == "MultiSolid")
        return GeometryKind::MultiSolid;
    if (name == "CompositeSolid")
        return GeometryKind::CompositeSolid;
    if (name == "GeometryInstance")
        return GeometryKind::GeometryInstance;
    throw Error(ErrorCode::InvalidAttributeValue, "unknown CityJSON geometry type '" + name + "'");
}

GMBoundaries encode_boundaries(GeometryKind kind, const nlohmann::json& boundaries) {
    GMBoundaries b;
    switch (kind) {
        case GeometryKind::MultiPoint:
            push_ring(boundaries, b);
            break;
        case GeometryKind::MultiLineString:
            for (const auto& ring : boundaries)
                push_ring(ring, b);
            b.surfaces.push_back(static_cast<std::uint32_t>(boundaries.size()));
            break;
        case GeometryKind::MultiSurface:
        case GeometryKind::CompositeSurface:
            for (const auto& surface : boundaries)
                push_surface(surface, b);
            b.shells.push_back(static_cast<std::uint32_t>(boundaries.size()));
            break;
        case GeometryKind::Solid:
            push_solid(boundaries, b);
            break;
        case GeometryKind::MultiSolid:
        case GeometryKind::CompositeSolid:
            for (const auto& solid : boundaries)
                push_solid(solid, b);
            break;
        case GeometryKind::GeometryInstance:
            // Encoded separately by `to_geometry_instance` (M3).
            break;
    }
    return b;
}

GMSemantics encode_semantics(const nlohmann::json& semantics, const GMBoundaries& boundaries) {
    GMSemantics result;
    result.surfaces = semantics.value("surfaces", nlohmann::json::array());

    auto values_it = semantics.find("values");
    if (values_it == semantics.end() || values_it->is_null())
        return result;  // values stays std::nullopt

    std::vector<std::uint32_t> flattened;
    switch (sniff_nullable_rank(*values_it)) {
        case 1:
            for (const auto& v : *values_it)
                flattened.push_back(semantics_index(v));
            break;

        case 2: {
            std::size_t shell_cursor = 0;
            for (const auto& shell : *values_it)
                push_semantics_shell(shell.is_null() ? nullptr : &shell, boundaries, shell_cursor,
                                     flattened);
            break;
        }

        case 3: {
            std::size_t shell_cursor = 0;
            std::size_t solid_i = 0;
            for (const auto& solid : *values_it) {
                const std::uint32_t shell_count =
                    solid_i < boundaries.solids.size() ? boundaries.solids[solid_i] : 0;
                if (solid.is_null()) {
                    for (std::uint32_t k = 0; k < shell_count; ++k)
                        push_semantics_shell(nullptr, boundaries, shell_cursor, flattened);
                } else {
                    for (const auto& shell : solid)
                        push_semantics_shell(shell.is_null() ? nullptr : &shell, boundaries,
                                             shell_cursor, flattened);
                }
                ++solid_i;
            }
            break;
        }
    }
    result.values = std::move(flattened);
    return result;
}

std::vector<MaterialMapping> encode_material(const nlohmann::json& material) {
    std::vector<MaterialMapping> out;
    if (!material.is_object())
        return out;

    // `material` is a JSON object; nlohmann's default container already
    // iterates it in ascending key order (see the AttributeSchema note in
    // writer/attribute.hpp), matching Rust's explicit sort of its `HashMap`
    // source, so no extra sort is needed here.
    for (const auto& [theme, m] : material.items()) {
        auto value_it = m.find("value");
        if (value_it != m.end() && !value_it->is_null()) {
            MaterialMapping mapping;
            mapping.kind = MaterialMapping::Kind::Value;
            mapping.theme = theme;
            mapping.value = value_it->get<std::uint32_t>();
            out.push_back(std::move(mapping));
            continue;
        }

        auto values_it = m.find("values");
        if (values_it == m.end())
            continue;  // neither `value` nor `values`: nothing to store
        if (values_it->is_null()) {
            MaterialMapping mapping;
            mapping.kind = MaterialMapping::Kind::NullValues;
            mapping.theme = theme;
            out.push_back(std::move(mapping));
            continue;
        }

        MaterialMapping mapping;
        mapping.kind = MaterialMapping::Kind::Values;
        mapping.theme = theme;
        switch (sniff_nullable_rank(*values_it)) {
            case 1:
                // One index per surface.
                for (const auto& v : *values_it)
                    mapping.vertices.push_back(material_index(v));
                break;

            case 2:
                // A single implicit solid: one index per surface, per shell.
                mapping.solids.push_back(static_cast<std::uint32_t>(values_it->size()));
                for (const auto& shell : *values_it)
                    push_material_shell(shell.is_null() ? nullptr : &shell, mapping);
                break;

            case 3:
                for (const auto& solid : *values_it) {
                    if (solid.is_null()) {
                        mapping.solids.push_back(kNull);
                    } else {
                        mapping.solids.push_back(static_cast<std::uint32_t>(solid.size()));
                        for (const auto& shell : solid)
                            push_material_shell(shell.is_null() ? nullptr : &shell, mapping);
                    }
                }
                break;
        }
        out.push_back(std::move(mapping));
    }
    return out;
}

std::vector<TextureMapping> encode_texture(const nlohmann::json& texture) {
    std::vector<TextureMapping> out;
    if (!texture.is_object())
        return out;

    for (const auto& [theme, t] : texture.items()) {
        TextureMapping mapping;
        mapping.theme = theme;

        auto values_it = t.find("values");
        if (values_it != t.end() && !values_it->is_null()) {
            mapping.has_values = true;
            switch (sniff_texture_depth(*values_it)) {
                case 1:
                    for (const auto& surface : *values_it)
                        push_textured_surface(surface, mapping);
                    mapping.shells.push_back(static_cast<std::uint32_t>(values_it->size()));
                    break;
                case 2:
                    for (const auto& shell : *values_it)
                        push_textured_shell(shell, mapping);
                    mapping.solids.push_back(static_cast<std::uint32_t>(values_it->size()));
                    break;
                case 3:
                    for (const auto& solid : *values_it) {
                        for (const auto& shell : solid)
                            push_textured_shell(shell, mapping);
                        mapping.solids.push_back(static_cast<std::uint32_t>(solid.size()));
                    }
                    break;
            }
        }
        out.push_back(std::move(mapping));
    }
    return out;
}

EncodedGeometry encode(const nlohmann::json& geometry) {
    EncodedGeometry result;
    const GeometryKind kind = geometry_kind_from_name(geometry.at("type").get<std::string>());

    auto boundaries_it = geometry.find("boundaries");
    result.boundaries = encode_boundaries(
        kind, boundaries_it != geometry.end() ? *boundaries_it : nlohmann::json::array());

    auto semantics_it = geometry.find("semantics");
    if (semantics_it != geometry.end() && semantics_it->is_object())
        result.semantics = encode_semantics(*semantics_it, result.boundaries);

    auto material_it = geometry.find("material");
    if (material_it != geometry.end() && material_it->is_object())
        result.materials = encode_material(*material_it);

    auto texture_it = geometry.find("texture");
    if (texture_it != geometry.end() && texture_it->is_object())
        result.textures = encode_texture(*texture_it);

    return result;
}

}  // namespace fcb

#endif  // FCB_WITH_JSON
