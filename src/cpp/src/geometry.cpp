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

/// u32::MAX marks "no index here" and becomes JSON null, not 4294967295.
nlohmann::json appearance_index_to_json(std::uint32_t v) {
    if (v == UINT32_MAX) return nullptr;
    return v;
}

/// `count` indices from `vertices`, starting at `cursor`.
///
/// Stops early when `vertices` runs out instead of throwing: the reference
/// guards every push with `if vertex_index < vertices.len()`, so a mapping
/// that over-claims yields a SHORT array rather than an error, and that is
/// what the expected output contains.
nlohmann::json take_appearance_indices(UIntView vertices, std::size_t& cursor,
                                       std::uint32_t count) {
    auto out = nlohmann::json::array();
    for (std::uint32_t i = 0; i < count && cursor < vertices.size(); ++i) {
        out.push_back(appearance_index_to_json(vertices[cursor++]));
    }
    return out;
}

/// Every material index, flat. The shape used whenever the mapping carries
/// no usable solids/shells structure.
nlohmann::json flat_appearance_indices(UIntView vertices) {
    auto out = nlohmann::json::array();
    for (std::size_t i = 0; i < vertices.size(); ++i) {
        out.push_back(appearance_index_to_json(vertices[i]));
    }
    return out;
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

nlohmann::json decode_material_values(UIntView solids, UIntView shells, UIntView vertices) {
    // No structure to rebuild: one index per surface, in file order. This
    // covers MultiSurface/CompositeSurface, and also a mapping that declares
    // solids but no shells -- the reference falls back to flat there too.
    if (solids.empty() || shells.empty()) return flat_appearance_indices(vertices);

    std::size_t vertex = 0;
    std::size_t shell = 0;

    // One solid holding several shells: the solid level is dropped, leaving
    // a list of per-shell index arrays. Note this is NOT the general
    // collapse rule -- solids == [1] takes the branch below and stays
    // wrapped (geom_decoder.rs:487).
    if (solids.size() == 1 && solids[0] > 1) {
        auto out = nlohmann::json::array();
        for (std::uint32_t i = 0; i < solids[0] && shell < shells.size(); ++i) {
            out.push_back(take_appearance_indices(vertices, vertex, shells[shell++]));
        }
        return out;
    }

    // MultiSolid/CompositeSolid: solid -> shell -> indices.
    auto out = nlohmann::json::array();
    for (std::size_t s = 0; s < solids.size(); ++s) {
        auto solid = nlohmann::json::array();
        for (std::uint32_t i = 0; i < solids[s] && shell < shells.size(); ++i) {
            solid.push_back(take_appearance_indices(vertices, vertex, shells[shell++]));
        }
        // Pushed even when the shell array ran out mid-solid, matching the
        // reference: a truncated walk still contributes an (empty) entry.
        out.push_back(std::move(solid));
    }
    return out;
}

namespace {

/// Cursors for the texture walk. Separate from Cursors above because the
/// texture arrays are walked with skip-on-exhaustion, not throw-on-overrun.
struct TexCursors {
    std::size_t shell = 0;
    std::size_t surface = 0;
    std::size_t string = 0;
    std::size_t vertex = 0;
};

/// One surface: `surfaces[surface]` rings, each a (texture index, UVs) list.
/// The caller has already checked that `surfaces` is not exhausted.
nlohmann::json take_tex_surface(UIntView surfaces, UIntView strings, UIntView vertices,
                                TexCursors& c) {
    const std::uint32_t ring_count = surfaces[c.surface++];
    auto out = nlohmann::json::array();
    for (std::uint32_t i = 0; i < ring_count && c.string < strings.size(); ++i) {
        out.push_back(take_appearance_indices(vertices, c.vertex, strings[c.string++]));
    }
    return out;
}

/// One shell: `shells[shell]` surfaces. Caller has checked `shells`.
nlohmann::json take_tex_shell(UIntView shells, UIntView surfaces, UIntView strings,
                              UIntView vertices, TexCursors& c) {
    const std::uint32_t surface_count = shells[c.shell++];
    auto out = nlohmann::json::array();
    for (std::uint32_t i = 0; i < surface_count && c.surface < surfaces.size(); ++i) {
        out.push_back(take_tex_surface(surfaces, strings, vertices, c));
    }
    return out;
}

}  // namespace

nlohmann::json decode_texture_values(UIntView solids,
                                     UIntView shells,
                                     UIntView surfaces,
                                     UIntView strings,
                                     UIntView vertices) {
    TexCursors c;

    // The branches below are the reference's, in its order. They are not
    // mutually exclusive by geometry type -- several test length == 1 -- so
    // reordering them changes the output.
    if (!solids.empty()) {
        auto out = nlohmann::json::array();
        for (std::size_t s = 0; s < solids.size(); ++s) {
            auto solid = nlohmann::json::array();
            for (std::uint32_t i = 0; i < solids[s] && c.shell < shells.size(); ++i) {
                solid.push_back(take_tex_shell(shells, surfaces, strings, vertices, c));
            }
            out.push_back(std::move(solid));
        }
        // Collapse ONLY here, at the outermost level, and only for a single
        // solid. Every inner level always wraps.
        return collapse(std::move(out));
    }

    // A single shell of surfaces (MultiSurface written with a shell entry).
    // Guarded on shells.size() == 1: two shells fall through to the surface
    // branch below, which ignores `shells` entirely.
    if (!shells.empty() && !surfaces.empty() && shells.size() == 1) {
        auto out = nlohmann::json::array();
        for (std::uint32_t i = 0; i < shells[0] && c.surface < surfaces.size(); ++i) {
            out.push_back(take_tex_surface(surfaces, strings, vertices, c));
        }
        return out;
    }

    // One surface holding several rings: MultiLineString, whose strings are
    // the lines. Yields the ring list without the surface wrapper.
    if (surfaces.size() == 1 && strings.size() > 1) {
        auto out = nlohmann::json::array();
        for (std::uint32_t i = 0; i < surfaces[0] && c.string < strings.size(); ++i) {
            out.push_back(take_appearance_indices(vertices, c.vertex, strings[c.string++]));
        }
        return out;
    }

    // MultiSurface/CompositeSurface: surface -> ring.
    if (!surfaces.empty()) {
        auto out = nlohmann::json::array();
        for (std::size_t s = 0; s < surfaces.size(); ++s) {
            out.push_back(take_tex_surface(surfaces, strings, vertices, c));
        }
        return out;
    }

    // Rings with no surface grouping.
    if (strings.size() > 1) {
        auto out = nlohmann::json::array();
        for (std::size_t s = 0; s < strings.size(); ++s) {
            out.push_back(take_appearance_indices(vertices, c.vertex, strings[s]));
        }
        return out;
    }

    // MultiPoint, or a single ring: a flat index list.
    return flat_appearance_indices(vertices);
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
