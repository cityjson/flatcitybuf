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

GMSemantics encode_semantics(const nlohmann::json& semantics, GeometryKind kind,
                             const GMBoundaries& boundaries) {
    GMSemantics result;
    result.surfaces = semantics.value("surfaces", nlohmann::json::array());

    auto values_it = semantics.find("values");
    if (values_it == semantics.end() || values_it->is_null())
        return result;  // values stays std::nullopt

    std::vector<std::uint32_t> flattened;
    switch (kind) {
        case GeometryKind::MultiPoint:
        case GeometryKind::MultiLineString:
        case GeometryKind::MultiSurface:
        case GeometryKind::CompositeSurface:
            for (const auto& v : *values_it)
                flattened.push_back(semantics_index(v));
            break;

        case GeometryKind::Solid: {
            std::size_t shell_cursor = 0;
            for (const auto& shell : *values_it)
                push_semantics_shell(shell.is_null() ? nullptr : &shell, boundaries, shell_cursor,
                                     flattened);
            break;
        }

        case GeometryKind::MultiSolid:
        case GeometryKind::CompositeSolid: {
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

        case GeometryKind::GeometryInstance:
            break;
    }
    result.values = std::move(flattened);
    return result;
}

std::vector<MaterialMapping> encode_material(const nlohmann::json& material, GeometryKind kind) {
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
        switch (kind) {
            case GeometryKind::MultiPoint:
            case GeometryKind::MultiLineString:
            case GeometryKind::MultiSurface:
            case GeometryKind::CompositeSurface:
                // One index per surface.
                for (const auto& v : *values_it)
                    mapping.vertices.push_back(material_index(v));
                break;

            case GeometryKind::Solid:
                // A single implicit solid: one index per surface, per shell.
                mapping.solids.push_back(static_cast<std::uint32_t>(values_it->size()));
                for (const auto& shell : *values_it)
                    push_material_shell(shell.is_null() ? nullptr : &shell, mapping);
                break;

            case GeometryKind::MultiSolid:
            case GeometryKind::CompositeSolid:
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

            case GeometryKind::GeometryInstance:
                break;
        }
        out.push_back(std::move(mapping));
    }
    return out;
}

}  // namespace fcb

#endif  // FCB_WITH_JSON
