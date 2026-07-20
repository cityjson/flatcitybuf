#include <fcb/cityjson.hpp>

#ifdef FCB_WITH_JSON

#include <fcb/attribute.hpp>
#include <fcb/geometry.hpp>

#include "detail/feature_access.hpp"
#include "detail/header_access.hpp"

#include <fcb/generated/feature_generated.h>
#include <fcb/generated/header_generated.h>

#include <array>
#include <cstring>
#include <string>

namespace fcb {

namespace {

/// Names must match CityJSON exactly; the enum order comes from
/// feature.fbs's CityObjectType declaration.
const char* const kCityObjectTypeNames[] = {
    "Bridge", "BridgePart", "BridgeInstallation", "BridgeConstructiveElement",
    "BridgeRoom", "BridgeFurniture",
    "Building", "BuildingPart", "BuildingInstallation",
    "BuildingConstructiveElement", "BuildingFurniture", "BuildingStorey",
    "BuildingRoom", "BuildingUnit",
    "CityFurniture", "CityObjectGroup", "GenericCityObject", "LandUse",
    "OtherConstruction", "PlantCover", "SolitaryVegetationObject", "TINRelief",
    "Road", "Railway", "Waterway", "TransportSquare",
    "Tunnel", "TunnelPart", "TunnelInstallation", "TunnelConstructiveElement",
    "TunnelHollowSpace", "TunnelFurniture",
    "WaterBody",
    "ExtensionObject",
};

UIntView as_uint_view(const flatbuffers::Vector<std::uint32_t>* v) {
    if (v == nullptr) return {};
    return UIntView(v->data(), v->size());
}

const char* const kSemanticSurfaceTypeNames[] = {
    "RoofSurface", "GroundSurface", "WallSurface", "ClosureSurface",
    "OuterCeilingSurface", "OuterFloorSurface", "Window", "Door",
    "InteriorWallSurface", "CeilingSurface", "FloorSurface",
    "WaterSurface", "WaterGroundSurface", "WaterClosureSurface",
    "TrafficArea", "AuxiliaryTrafficArea", "TransportationMarking",
    "TransportationHole",
    "ExtraSemanticSurface",
};

/// u32::MAX marks "no semantic surface for this boundary" and becomes JSON
/// null (geom_decoder.rs:284).
nlohmann::json semantic_index_to_json(std::uint32_t v) {
    if (v == UINT32_MAX) return nullptr;
    return v;
}

/// Nesting depth of `semantics.values`, chosen by geometry type
/// (geom_decoder.rs:348-353): solids nest twice, Solid once, surfaces not
/// at all. The values array is sliced by the shell/solid counts.
nlohmann::json decode_semantics_values(const ::Geometry* g, UIntView values) {
    const auto type = g->type();
    const bool two_deep = (type == ::GeometryType::MultiSolid ||
                           type == ::GeometryType::CompositeSolid);
    const bool one_deep = (type == ::GeometryType::Solid);

    if (!two_deep && !one_deep) {
        auto flat = nlohmann::json::array();
        for (std::size_t i = 0; i < values.size(); ++i) {
            flat.push_back(semantic_index_to_json(values[i]));
        }
        return flat;
    }

    auto shells = as_uint_view(g->shells());
    std::size_t cursor = 0;
    auto per_shell = nlohmann::json::array();
    for (std::size_t i = 0; i < shells.size(); ++i) {
        const std::uint32_t n = shells[i];
        auto grp = nlohmann::json::array();
        for (std::uint32_t k = 0; k < n && cursor < values.size(); ++k) {
            grp.push_back(semantic_index_to_json(values[cursor++]));
        }
        per_shell.push_back(std::move(grp));
    }

    if (one_deep) return per_shell;

    // MultiSolid/CompositeSolid add one more level, grouped by solid.
    auto solids = as_uint_view(g->solids());
    std::size_t shell_cursor = 0;
    auto per_solid = nlohmann::json::array();
    for (std::size_t i = 0; i < solids.size(); ++i) {
        auto grp = nlohmann::json::array();
        for (std::uint32_t k = 0; k < solids[i] && shell_cursor < per_shell.size(); ++k) {
            grp.push_back(per_shell[shell_cursor++]);
        }
        per_solid.push_back(std::move(grp));
    }
    return per_solid;
}

/// `material`: theme -> either a single shared-material index or a nested
/// values array. A mapping with neither a value nor a vertices array is
/// skipped, so a geometry whose every mapping is skipped still emits an
/// empty object; the key itself is omitted only when the mapping vector is
/// absent or empty, which the caller checks (geom_decoder.rs:419).
nlohmann::json materials_to_json(const flatbuffers::Vector<
                                 flatbuffers::Offset<::MaterialMapping>>* mappings) {
    nlohmann::json out = nlohmann::json::object();
    for (const auto* m : *mappings) {
        if (m == nullptr) continue;
        const std::string theme = (m->theme() != nullptr) ? m->theme()->str() : "theme";
        if (const auto v = m->value()) {
            out[theme] = {{"value", *v}};
            continue;
        }
        if (m->vertices() == nullptr) continue;
        out[theme] = {{"values", decode_material_values(as_uint_view(m->solids()),
                                                        as_uint_view(m->shells()),
                                                        as_uint_view(m->vertices()))}};
    }
    return out;
}

/// `texture`: theme -> nested values array. Mappings without vertices are
/// skipped; see materials_to_json for why the object can end up empty.
nlohmann::json textures_to_json(const flatbuffers::Vector<
                                flatbuffers::Offset<::TextureMapping>>* mappings) {
    nlohmann::json out = nlohmann::json::object();
    for (const auto* m : *mappings) {
        if (m == nullptr) continue;
        auto vertices = as_uint_view(m->vertices());
        if (vertices.empty()) continue;
        const std::string theme = (m->theme() != nullptr) ? m->theme()->str() : "theme";
        out[theme] = {{"values", decode_texture_values(as_uint_view(m->solids()),
                                                       as_uint_view(m->shells()),
                                                       as_uint_view(m->surfaces()),
                                                       as_uint_view(m->strings()), vertices)}};
    }
    return out;
}

nlohmann::json geometry_instance_to_json(const ::GeometryInstance* gi) {
    nlohmann::json out = nlohmann::json::object();
    out["type"] = "GeometryInstance";
    out["template"] = gi->template_();

    // The boundaries array holds exactly one vertex index: CityGML's
    // "referencePoint" for the instance.
    auto b = nlohmann::json::array();
    if (gi->boundaries() != nullptr) {
        for (std::uint32_t v : *gi->boundaries()) b.push_back(v);
    }
    out["boundaries"] = std::move(b);

    // 16 doubles in row-major order. Read via memcpy: like Transform in the
    // header, this struct can sit at a misaligned internal offset.
    if (const auto* m = gi->transformation()) {
        auto mat = nlohmann::json::array();
        for (std::size_t i = 0; i < 16; ++i) {
            double d;
            std::memcpy(&d, reinterpret_cast<const std::uint8_t*>(m) + i * sizeof(double),
                        sizeof(double));
            mat.push_back(d);
        }
        out["transformationMatrix"] = std::move(mat);
    }
    return out;
}

nlohmann::json geometry_to_json(const ::Geometry* g,
                                const std::vector<ColumnInfo>& semantic_columns) {
    nlohmann::json out = nlohmann::json::object();
    out["type"] = geometry_type_name(static_cast<std::uint8_t>(g->type()));
    if (g->lod() != nullptr) out["lod"] = g->lod()->str();

    out["boundaries"] = decode_boundaries(
        as_uint_view(g->solids()), as_uint_view(g->shells()),
        as_uint_view(g->surfaces()), as_uint_view(g->strings()),
        as_uint_view(g->boundaries()));

    // Semantics: a surface list plus a values array indexing into it.
    if (g->semantics_objects() != nullptr && g->semantics_objects()->size() > 0) {
        auto surfaces = nlohmann::json::array();
        for (const auto* so : *g->semantics_objects()) {
            if (so == nullptr) continue;
            nlohmann::json s = nlohmann::json::object();
            const auto t = static_cast<std::size_t>(so->type());
            constexpr std::size_t kCount = sizeof(kSemanticSurfaceTypeNames) /
                                           sizeof(kSemanticSurfaceTypeNames[0]);
            s["type"] = (so->extension_type() != nullptr)
                            ? so->extension_type()->str()
                            : (t < kCount ? kSemanticSurfaceTypeNames[t] : "ExtraSemanticSurface");
            // Semantic surfaces carry their own attributes, decoded against
            // Header.semantic_columns -- a schema separate from the feature
            // attribute columns. Merged inline, as the reference does.
            if (so->attributes() != nullptr && so->attributes()->size() > 0) {
                auto attrs = attributes_to_json(
                    bytes_view(so->attributes()->data(), so->attributes()->size()),
                    semantic_columns);
                for (auto& [k, v] : attrs.items()) s[k] = v;
            }
            if (so->children() != nullptr && so->children()->size() > 0) {
                auto kids = nlohmann::json::array();
                for (std::uint32_t c : *so->children()) kids.push_back(c);
                s["children"] = std::move(kids);
            }
            surfaces.push_back(std::move(s));
        }

        nlohmann::json sem = nlohmann::json::object();
        sem["surfaces"] = std::move(surfaces);
        sem["values"] = decode_semantics_values(g, as_uint_view(g->semantics()));
        out["semantics"] = std::move(sem);
    }

    // Appearance: per-geometry mappings only. The header's `appearance`
    // object (the materials/textures/vertices-texture arrays these index
    // into) is deliberately not emitted -- the Rust reader does not emit it
    // either, and CityJSONSeq consumers read it from the source file.
    //
    // An EMPTY mapping vector omits the key entirely: the reference returns
    // None for an empty slice (geom_decoder.rs:419, :598) and serde drops
    // the field. Only a vector whose mappings were all skipped yields `{}`.
    if (g->material() != nullptr && g->material()->size() > 0) {
        out["material"] = materials_to_json(g->material());
    }
    if (g->texture() != nullptr && g->texture()->size() > 0) {
        out["texture"] = textures_to_json(g->texture());
    }

    return out;
}

}  // namespace

std::string city_object_type_name(std::uint8_t type) {
    constexpr std::size_t kCount =
        sizeof(kCityObjectTypeNames) / sizeof(kCityObjectTypeNames[0]);
    if (type >= kCount) {
        throw Error(ErrorCode::InvalidFlatbuffer,
                    "unknown city object type " + std::to_string(type));
    }
    return kCityObjectTypeNames[type];
}

nlohmann::json to_cityjson_metadata(const HeaderView& header) {
    const auto& info = header.info();
    nlohmann::json cj = nlohmann::json::object();

    cj["type"] = "CityJSON";
    cj["version"] = info.cityjson_version;

    if (info.has_transform) {
        cj["transform"] = {{"scale", info.scale}, {"translate", info.translate}};
    }

    nlohmann::json meta = nlohmann::json::object();
    if (info.has_extent) meta["geographicalExtent"] = info.geographical_extent;
    if (!info.crs.empty()) {
        meta["referenceSystem"] =
            "https://www.opengis.net/def/crs/" +
            info.crs.substr(0, info.crs.find(':')) + "/0/" +
            info.crs.substr(info.crs.find(':') + 1);
    }
    if (!info.identifier.empty()) meta["identifier"] = info.identifier;
    if (!info.title.empty()) meta["title"] = info.title;
    if (!meta.empty()) cj["metadata"] = std::move(meta);

    // A CityJSONSeq header line carries no features of its own.
    cj["CityObjects"] = nlohmann::json::object();
    cj["vertices"] = nlohmann::json::array();

    // Geometry templates: shapes shared by every GeometryInstance in the
    // file, with their own vertex list. Emitted only when BOTH arrays are
    // present -- a template without vertices indexes nothing
    // (deserializer.rs:92).
    const ::Header* hdr = detail::HeaderAccess::get(header);
    if (hdr != nullptr && hdr->templates() != nullptr &&
        hdr->templates_vertices() != nullptr) {
        auto templates = nlohmann::json::array();
        for (const auto* t : *hdr->templates()) {
            if (t != nullptr) templates.push_back(geometry_to_json(t, info.semantic_columns));
        }

        // Template vertices are absolute doubles, NOT quantised: the header
        // transform does not apply to them. Read via memcpy for the same
        // reason as Transform -- the struct can sit at a misaligned offset.
        auto verts = nlohmann::json::array();
        for (const auto* v : *hdr->templates_vertices()) {
            std::array<double, 3> xyz{};
            for (std::size_t i = 0; i < 3; ++i) {
                std::memcpy(&xyz[i], reinterpret_cast<const std::uint8_t*>(v) + i * sizeof(double),
                            sizeof(double));
            }
            verts.push_back(nlohmann::json::array({xyz[0], xyz[1], xyz[2]}));
        }

        cj["geometry-templates"] = {{"templates", std::move(templates)},
                                    {"vertices-templates", std::move(verts)}};
    }

    return cj;
}

nlohmann::json to_cityjson_feature(const Feature& feature, const HeaderView& header) {
    const ::CityFeature* cf = detail::FeatureAccess::get(feature);
    if (cf == nullptr) {
        throw Error(ErrorCode::MissingRequiredField, "empty feature");
    }

    nlohmann::json out = nlohmann::json::object();
    out["type"] = "CityJSONFeature";
    out["id"] = feature.id();

    nlohmann::json objects = nlohmann::json::object();
    const std::size_t n = feature.city_object_count();
    for (std::size_t i = 0; i < n; ++i) {
        const auto* obj = cf->objects()->Get(static_cast<flatbuffers::uoffset_t>(i));
        if (obj == nullptr) continue;

        nlohmann::json co = nlohmann::json::object();
        co["type"] = (obj->extension_type() != nullptr)
                         ? obj->extension_type()->str()
                         : city_object_type_name(static_cast<std::uint8_t>(obj->type()));

        // Per-object schema when declared, header schema otherwise.
        // Emitted iff the object DECLARES an attributes vector -- a
        // present-but-empty one becomes `{}`, an absent one is omitted
        // entirely. The reference distinguishes these and consumers compare
        // against it.
        if (feature.object_has_attributes(i)) {
            auto own = feature.object_columns(i);
            auto blob = feature.object_attributes(i);
            const auto& schema =
                feature.object_has_columns(i) ? own : header.info().columns;
            co["attributes"] = blob.empty() ? nlohmann::json::object()
                                            : attributes_to_json(blob, schema);
        }

        std::array<double, 6> extent{};
        if (feature.object_extent(i, extent)) {
            co["geographicalExtent"] = extent;
        }

        auto geoms = nlohmann::json::array();
        if (obj->geometry() != nullptr) {
            for (const auto* g : *obj->geometry()) {
                if (g != nullptr) {
                    geoms.push_back(geometry_to_json(g, header.info().semantic_columns));
                }
            }
        }
        // Geometry templates: the shape lives once in the header and each
        // instance references it by index plus a 4x4 placement matrix and a
        // single reference-point vertex.
        if (obj->geometry_instances() != nullptr) {
            for (const auto* gi : *obj->geometry_instances()) {
                if (gi != nullptr) geoms.push_back(geometry_instance_to_json(gi));
            }
        }
        if (!geoms.empty()) co["geometry"] = std::move(geoms);

        if (obj->children() != nullptr && obj->children()->size() > 0) {
            auto kids = nlohmann::json::array();
            for (const auto* c : *obj->children()) {
                if (c != nullptr) kids.push_back(c->str());
            }
            co["children"] = std::move(kids);
        }
        if (obj->parents() != nullptr && obj->parents()->size() > 0) {
            auto ps = nlohmann::json::array();
            for (const auto* p : *obj->parents()) {
                if (p != nullptr) ps.push_back(p->str());
            }
            co["parents"] = std::move(ps);
        }

        objects[feature.object_id(i)] = std::move(co);
    }
    out["CityObjects"] = std::move(objects);

    // Vertices are quantised integers; the header transform maps them back
    // to world coordinates, so they stay integral here.
    auto verts = nlohmann::json::array();
    if (cf->vertices() != nullptr) {
        for (const auto* v : *cf->vertices()) {
            verts.push_back(nlohmann::json::array({v->x(), v->y(), v->z()}));
        }
    }
    out["vertices"] = std::move(verts);

    return out;
}

}  // namespace fcb

#endif  // FCB_WITH_JSON
