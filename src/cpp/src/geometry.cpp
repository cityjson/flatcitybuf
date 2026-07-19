#include <fcb/geometry.hpp>

#include <fcb/generated/geometry_generated.h>

namespace fcb {

namespace {

/// Cursors into the parallel count arrays, shared across nesting levels.
struct Cursors {
    std::size_t shell = 0;
    std::size_t surface = 0;
    std::size_t ring = 0;
    std::size_t index = 0;
};

[[noreturn]] void overrun(const char* what) {
    throw Error(ErrorCode::InvalidFlatbuffer,
                std::string("geometry boundaries overrun in ") + what);
}

#ifdef FCB_WITH_JSON

/// One ring: `strings[ring]` vertex indices taken from `indices`.
nlohmann::json take_ring(UIntView strings, UIntView indices, Cursors& c) {
    if (c.ring >= strings.size()) overrun("strings");
    const std::uint32_t ring_size = strings[c.ring++];

    if (c.index > indices.size() || indices.size() - c.index < ring_size) {
        overrun("indices");
    }
    auto ring = nlohmann::json::array();
    for (std::uint32_t i = 0; i < ring_size; ++i) {
        ring.push_back(indices[c.index + i]);
    }
    c.index += ring_size;
    return ring;
}

/// One surface: `surfaces[surface]` rings.
nlohmann::json take_surface(UIntView surfaces, UIntView strings, UIntView indices,
                            Cursors& c) {
    if (c.surface >= surfaces.size()) overrun("surfaces");
    const std::uint32_t ring_count = surfaces[c.surface++];

    auto surface = nlohmann::json::array();
    for (std::uint32_t i = 0; i < ring_count; ++i) {
        surface.push_back(take_ring(strings, indices, c));
    }
    return surface;
}

/// One shell: `shells[shell]` surfaces.
nlohmann::json take_shell(UIntView shells, UIntView surfaces, UIntView strings,
                          UIntView indices, Cursors& c) {
    if (c.shell >= shells.size()) overrun("shells");
    const std::uint32_t surface_count = shells[c.shell++];

    auto shell = nlohmann::json::array();
    for (std::uint32_t i = 0; i < surface_count; ++i) {
        shell.push_back(take_surface(surfaces, strings, indices, c));
    }
    return shell;
}

/// The reference collapses a single-element level into that element rather
/// than wrapping it (geom_decoder.rs:80-84 and the equivalent at each depth).
nlohmann::json collapse(nlohmann::json arr) {
    if (arr.is_array() && arr.size() == 1) return arr[0];
    return arr;
}

#endif  // FCB_WITH_JSON

}  // namespace

#ifdef FCB_WITH_JSON

nlohmann::json decode_boundaries(UIntView solids,
                                 UIntView shells,
                                 UIntView surfaces,
                                 UIntView strings,
                                 UIntView indices) {
    Cursors c;

    // Dispatch on the outermost populated array, not on the geometry type:
    // that is what the reference does, and it keeps the two in step even for
    // types whose nesting depth is ambiguous.
    if (!solids.empty()) {
        auto out = nlohmann::json::array();
        for (std::size_t s = 0; s < solids.size(); ++s) {
            auto solid = nlohmann::json::array();
            for (std::uint32_t i = 0; i < solids[s]; ++i) {
                solid.push_back(take_shell(shells, surfaces, strings, indices, c));
            }
            out.push_back(std::move(solid));
        }
        return collapse(std::move(out));
    }

    if (!shells.empty()) {
        auto out = nlohmann::json::array();
        for (std::size_t s = 0; s < shells.size(); ++s) {
            auto shell = nlohmann::json::array();
            const std::uint32_t surface_count = shells[s];
            ++c.shell;  // this level consumes the shell entry itself
            for (std::uint32_t i = 0; i < surface_count; ++i) {
                shell.push_back(take_surface(surfaces, strings, indices, c));
            }
            out.push_back(std::move(shell));
        }
        return collapse(std::move(out));
    }

    if (!surfaces.empty()) {
        auto out = nlohmann::json::array();
        for (std::size_t s = 0; s < surfaces.size(); ++s) {
            auto surface = nlohmann::json::array();
            const std::uint32_t ring_count = surfaces[s];
            ++c.surface;  // this level consumes the surface entry itself
            for (std::uint32_t i = 0; i < ring_count; ++i) {
                surface.push_back(take_ring(strings, indices, c));
            }
            out.push_back(std::move(surface));
        }
        return collapse(std::move(out));
    }

    if (!strings.empty()) {
        auto out = nlohmann::json::array();
        for (std::size_t s = 0; s < strings.size(); ++s) {
            out.push_back(take_ring(strings, indices, c));
        }
        return collapse(std::move(out));
    }

    // No count arrays at all: a flat list of vertex indices (MultiPoint).
    auto out = nlohmann::json::array();
    for (std::size_t i = 0; i < indices.size(); ++i) out.push_back(indices[i]);
    return out;
}

#endif  // FCB_WITH_JSON

std::string geometry_type_name(std::uint8_t type) {
    switch (static_cast<::GeometryType>(type)) {
        case ::GeometryType::MultiPoint: return "MultiPoint";
        case ::GeometryType::MultiLineString: return "MultiLineString";
        case ::GeometryType::MultiSurface: return "MultiSurface";
        case ::GeometryType::CompositeSurface: return "CompositeSurface";
        case ::GeometryType::Solid: return "Solid";
        case ::GeometryType::MultiSolid: return "MultiSolid";
        case ::GeometryType::CompositeSolid: return "CompositeSolid";
        case ::GeometryType::GeometryInstance: return "GeometryInstance";
    }
    throw Error(ErrorCode::InvalidFlatbuffer,
                "unknown geometry type " + std::to_string(type));
}

}  // namespace fcb
