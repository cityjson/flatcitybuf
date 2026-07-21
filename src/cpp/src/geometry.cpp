#include <fcb/geometry.hpp>

#include <fcb/generated/geometry_generated.h>

#include <algorithm>

namespace fcb {

namespace {

// GeometryKind is declared in the public header so callers need not include
// the generated flatbuffers code; it must stay in lock-step with it.
static_assert(static_cast<std::uint8_t>(GeometryKind::MultiPoint) ==
                  static_cast<std::uint8_t>(::GeometryType::MultiPoint),
              "GeometryKind has drifted from the generated GeometryType");
static_assert(static_cast<std::uint8_t>(GeometryKind::CompositeSolid) ==
                  static_cast<std::uint8_t>(::GeometryType::CompositeSolid),
              "GeometryKind has drifted from the generated GeometryType");
static_assert(static_cast<std::uint8_t>(GeometryKind::GeometryInstance) ==
                  static_cast<std::uint8_t>(::GeometryType::GeometryInstance),
              "GeometryKind has drifted from the generated GeometryType");

#ifdef FCB_WITH_JSON

[[noreturn]] void overrun(const char* what) {
    throw Error(ErrorCode::InvalidFlatbuffer,
                std::string("geometry boundaries overrun in ") + what);
}

/// UINT32_MAX marks "no index here" and becomes JSON null, not 4294967295.
/// Mirrors `geom_decoder::index`.
nlohmann::json index_to_json(std::uint32_t v) {
    if (v == UINT32_MAX) return nullptr;
    return v;
}

/// `counts[cursor]`, or 0 when the array has run out.
///
/// The reference reads every count array with `.get(cursor).unwrap_or(0)`, so
/// a mapping that claims more shells or surfaces than it stores yields EMPTY
/// entries at the right positions rather than losing them. An earlier C++
/// port broke out of the loop instead, which silently shortened the result.
std::uint32_t count_at(UIntView counts, std::size_t& cursor) {
    const std::uint32_t n = (cursor < counts.size()) ? counts[cursor] : 0;
    ++cursor;
    return n;
}

// ------------------------------------------------------------ boundaries ---

/// Cursors into the parallel count arrays, shared across nesting levels.
struct Cursors {
    std::size_t shell = 0;
    std::size_t surface = 0;
    std::size_t ring = 0;
    std::size_t index = 0;
};

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

// --------------------------------------------------------------- texture ---

/// The texture equivalent of `Cursors`: the same four count arrays, but the
/// leaf holds `[texture index, uv index, ...]` and is nullable, and a missing
/// count reads as zero instead of throwing (see `count_at`).
struct TexCursors {
    UIntView shells;
    UIntView surfaces;
    UIntView strings;
    UIntView vertices;
    std::size_t shell = 0;
    std::size_t surface = 0;
    std::size_t string = 0;
    std::size_t vertex = 0;

    nlohmann::json take_ring() {
        const std::uint32_t size = count_at(strings, string);
        const std::size_t end = std::min(vertex + size, vertices.size());
        auto ring = nlohmann::json::array();
        for (; vertex < end; ++vertex) ring.push_back(index_to_json(vertices[vertex]));
        return ring;
    }

    nlohmann::json take_surface() {
        const std::uint32_t rings = count_at(surfaces, surface);
        auto out = nlohmann::json::array();
        for (std::uint32_t i = 0; i < rings; ++i) out.push_back(take_ring());
        return out;
    }

    nlohmann::json take_shell() {
        const std::uint32_t n = count_at(shells, shell);
        auto out = nlohmann::json::array();
        for (std::uint32_t i = 0; i < n; ++i) out.push_back(take_surface());
        return out;
    }
};

#endif  // FCB_WITH_JSON

}  // namespace

#ifdef FCB_WITH_JSON

nlohmann::json decode_boundaries(GeometryKind type,
                                 UIntView solids,
                                 UIntView shells,
                                 UIntView surfaces,
                                 UIntView strings,
                                 UIntView indices) {
    Cursors c;
    auto out = nlohmann::json::array();

    switch (type) {
        case GeometryKind::MultiPoint:
            // Every index is a point of the one and only ring.
            for (std::size_t i = 0; i < indices.size(); ++i) out.push_back(indices[i]);
            return out;

        case GeometryKind::MultiLineString:
            // One ring per `strings` entry. `surfaces` holds one redundant
            // entry (== strings.size()); it is ignored.
            for (std::size_t i = 0; i < strings.size(); ++i) {
                out.push_back(take_ring(strings, indices, c));
            }
            return out;

        case GeometryKind::MultiSurface:
        case GeometryKind::CompositeSurface:
            // One surface per `surfaces` entry; `shells` is the redundant one.
            for (std::size_t i = 0; i < surfaces.size(); ++i) {
                out.push_back(take_surface(surfaces, strings, indices, c));
            }
            return out;

        case GeometryKind::MultiSolid:
        case GeometryKind::CompositeSolid:
            // `solids[i]` shells in the i-th solid. Nothing above it.
            for (std::size_t i = 0; i < solids.size(); ++i) {
                auto solid = nlohmann::json::array();
                for (std::uint32_t k = 0; k < solids[i]; ++k) {
                    solid.push_back(take_shell(shells, surfaces, strings, indices, c));
                }
                out.push_back(std::move(solid));
            }
            return out;

        case GeometryKind::Solid:
        case GeometryKind::GeometryInstance:
        default:
            // One shell per `shells` entry; `solids` is the redundant one.
            // A Solid is also what the reference falls back to for a tag it
            // does not know (deserializer.rs:780), so the two agree.
            for (std::size_t i = 0; i < shells.size(); ++i) {
                out.push_back(take_shell(shells, surfaces, strings, indices, c));
            }
            return out;
    }
}

nlohmann::json decode_semantics_values(GeometryKind type,
                                       UIntView solids,
                                       UIntView shells,
                                       UIntView values) {
    std::size_t cursor = 0;
    auto take = [&](std::uint32_t n) {
        const std::size_t end = std::min(cursor + n, values.size());
        auto out = nlohmann::json::array();
        for (; cursor < end; ++cursor) out.push_back(index_to_json(values[cursor]));
        return out;
    };

    auto out = nlohmann::json::array();
    switch (type) {
        case GeometryKind::Solid:
            // One array per shell.
            for (std::size_t i = 0; i < shells.size(); ++i) out.push_back(take(shells[i]));
            return out;

        case GeometryKind::MultiSolid:
        case GeometryKind::CompositeSolid: {
            // One array per shell, per solid.
            std::size_t shell_cursor = 0;
            for (std::size_t i = 0; i < solids.size(); ++i) {
                auto solid = nlohmann::json::array();
                for (std::uint32_t k = 0; k < solids[i]; ++k) {
                    solid.push_back(take(count_at(shells, shell_cursor)));
                }
                out.push_back(std::move(solid));
            }
            return out;
        }

        // MultiPoint, MultiLineString, MultiSurface, CompositeSurface: one
        // value per surface, flat. A GeometryInstance carries no semantics of
        // its own; its template does, so the reference reads that flat too.
        default:
            for (std::size_t i = 0; i < values.size(); ++i) {
                out.push_back(index_to_json(values[i]));
            }
            return out;
    }
}

nlohmann::json decode_material_values(GeometryKind type,
                                      UIntView solids,
                                      UIntView shells,
                                      UIntView vertices) {
    std::size_t vertex = 0;
    auto take_shell = [&](std::uint32_t n) {
        const std::size_t end = std::min(vertex + n, vertices.size());
        auto out = nlohmann::json::array();
        for (; vertex < end; ++vertex) out.push_back(index_to_json(vertices[vertex]));
        return out;
    };
    auto flat = [&] {
        auto out = nlohmann::json::array();
        for (std::size_t i = 0; i < vertices.size(); ++i) {
            out.push_back(index_to_json(vertices[i]));
        }
        return out;
    };

    auto out = nlohmann::json::array();
    switch (type) {
        case GeometryKind::Solid:
            // One array per shell; a UINT32_MAX count is a whole null shell.
            for (std::size_t i = 0; i < shells.size(); ++i) {
                if (shells[i] == UINT32_MAX) {
                    out.push_back(nullptr);
                } else {
                    out.push_back(take_shell(shells[i]));
                }
            }
            return out;

        case GeometryKind::MultiSolid:
        case GeometryKind::CompositeSolid: {
            // One array per shell, per solid. Null at either level.
            std::size_t shell_cursor = 0;
            for (std::size_t i = 0; i < solids.size(); ++i) {
                if (solids[i] == UINT32_MAX) {
                    out.push_back(nullptr);
                    continue;
                }
                auto solid = nlohmann::json::array();
                for (std::uint32_t k = 0; k < solids[i]; ++k) {
                    const std::uint32_t n = count_at(shells, shell_cursor);
                    if (n == UINT32_MAX) {
                        solid.push_back(nullptr);
                    } else {
                        solid.push_back(take_shell(n));
                    }
                }
                out.push_back(std::move(solid));
            }
            return out;
        }

        // MultiSurface and CompositeSurface get one index per surface.
        // MultiPoint, MultiLineString and GeometryInstance cannot carry a
        // material at all; if one is somehow present it has no depth of its
        // own, so it is read as the shallowest thing it could be.
        default:
            return flat();
    }
}

nlohmann::json decode_texture_values(GeometryKind type,
                                     UIntView solids,
                                     UIntView shells,
                                     UIntView surfaces,
                                     UIntView strings,
                                     UIntView vertices) {
    TexCursors c{shells, surfaces, strings, vertices, 0, 0, 0, 0};

    auto out = nlohmann::json::array();
    switch (type) {
        case GeometryKind::MultiSurface:
        case GeometryKind::CompositeSurface:
            // Per surface, per ring.
            for (std::size_t i = 0; i < surfaces.size(); ++i) out.push_back(c.take_surface());
            return out;

        case GeometryKind::Solid:
            // ... per shell.
            for (std::size_t i = 0; i < shells.size(); ++i) out.push_back(c.take_shell());
            return out;

        case GeometryKind::MultiSolid:
        case GeometryKind::CompositeSolid:
            // ... per solid.
            for (std::size_t i = 0; i < solids.size(); ++i) {
                auto solid = nlohmann::json::array();
                for (std::uint32_t k = 0; k < solids[i]; ++k) solid.push_back(c.take_shell());
                out.push_back(std::move(solid));
            }
            return out;

        // MultiPoint, MultiLineString and GeometryInstance cannot carry a
        // texture; read whatever is there at the shallowest legal depth. The
        // `max(1)` is the reference's (geom_decoder.rs:542) and is what makes
        // a textureless count array still produce one empty surface.
        default: {
            const std::size_t n = std::max<std::size_t>(surfaces.size(), 1);
            for (std::size_t i = 0; i < n; ++i) out.push_back(c.take_surface());
            return out;
        }
    }
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
