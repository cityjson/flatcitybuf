#include <fcb/writer/geom_encoder.hpp>

#ifdef FCB_WITH_JSON

#    include <fcb/error.hpp>

namespace fcb {

namespace {

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

}  // namespace fcb

#endif  // FCB_WITH_JSON
