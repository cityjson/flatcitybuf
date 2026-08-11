// Walking the FlatBuffers geometry directly, for analysis.
//
//     geometry_analysis <file.fcb> [max-features] [lod]
//
// Every other example converts to CityJSON first. This one does not: it reads
// the format's OWN representation -- five flat count arrays plus a flat
// vertex-index list -- and computes over them. That is the representation to
// use for analysis, because nothing has to be nested, allocated, or turned
// into JSON to get a number out of it.
//
// The arrays, per Geometry:
//
//     solids()[i]      shell count of solid i
//     shells()[i]      surface count of shell i
//     surfaces()[i]    ring count of surface i
//     strings()[i]     vertex count of ring i
//     boundaries()     the flat vertex-index list
//     semantics()[i]   semantic-object index of surface i (UINT32_MAX = none)
//
// THE NESTING DEPTH COMES FROM Geometry::type(), NEVER FROM THE ARRAYS. A
// Solid with one shell and a MultiSolid with one solid flatten to
// byte-identical arrays -- only the type tells them apart. Inferring depth
// from which array is populated is upstream finding #8. This example never
// needs the depth: surface areas sum the same however the surfaces are
// grouped, so it walks surfaces()/strings() straight through. Anything that
// DOES care about grouping (per-shell volume, say) must switch on the type.
//
// Vertices are quantised integers shared by the whole feature: multiply by
// transform.scale and add transform.translate for real-world coordinates. For
// area the translate cancels, but the scale does not.
//
// The flat walk is checked twice: against to_cityjson_feature's nested output,
// and against the dataset's own published ground area.
#include <fcb/attribute.hpp>
#include <fcb/cityjson.hpp>
#include <fcb/feature.hpp>
#include <fcb/generated/feature_generated.h>
#include <fcb/geometry.hpp>
#include <fcb/header.hpp>
#include <fcb/reader.hpp>

#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <map>
#include <string>
#include <vector>

namespace {

struct Pt {
    double x, y, z;
};

/// Area of one planar polygon in 3D, by Newell's method: the magnitude of the
/// summed edge cross-products, halved. Works for any simple polygon and needs
/// no projection or triangulation.
double ring_area(const std::vector<Pt>& pts) {
    double nx = 0, ny = 0, nz = 0;
    const std::size_t n = pts.size();
    for (std::size_t i = 0; i < n; ++i) {
        const Pt& a = pts[i];
        const Pt& b = pts[(i + 1) % n];
        nx += a.y * b.z - a.z * b.y;
        ny += a.z * b.x - a.x * b.z;
        nz += a.x * b.y - a.y * b.x;
    }
    return std::sqrt(nx * nx + ny * ny + nz * nz) / 2.0;
}

/// Area per semantic surface type, from the flat arrays alone.
///
/// Only geometries at `lod` are counted. A City Object carries ONE geometry
/// per level of detail -- a 3DBAG BuildingPart has lod 1.2, 1.3 and 2.2, and
/// its parent Building an lod 0 footprint -- so summing every geometry would
/// count each building three or four times over.
void area_by_surface_type(const fcb::Feature& feature, const fcb::HeaderView& header,
                          const std::string& lod, std::map<std::string, double>& out) {
    const fcb::FileInfo& info = header.info();
    const std::array<double, 3> scale =
        info.has_transform ? info.scale : std::array<double, 3>{1, 1, 1};
    const std::array<double, 3> translate =
        info.has_transform ? info.translate : std::array<double, 3>{0, 0, 0};

    const ::CityFeature* cf = feature.raw();
    if (cf == nullptr || cf->objects() == nullptr)
        return;
    // One flat vector of Vertex structs, shared by every geometry in this
    // feature. Indices in boundaries() point into it.
    const auto* verts = cf->vertices();
    if (verts == nullptr)
        return;

    for (const auto* obj : *cf->objects()) {
        if (obj == nullptr || obj->geometry() == nullptr)
            continue;
        for (const auto* geom : *obj->geometry()) {
            if (geom == nullptr)
                continue;
            if (geom->lod() == nullptr || geom->lod()->str() != lod)
                continue;

            const auto* surfaces = geom->surfaces();
            const auto* strings = geom->strings();
            const auto* boundaries = geom->boundaries();
            const auto* semantics = geom->semantics();
            if (surfaces == nullptr || strings == nullptr || boundaries == nullptr)
                continue;

            std::uint32_t ring = 0;    // index into strings
            std::uint32_t vertex = 0;  // index into boundaries

            for (std::uint32_t s = 0; s < surfaces->size(); ++s) {
                double area = 0.0;
                for (std::uint32_t r = 0; r < surfaces->Get(s); ++r) {
                    const std::uint32_t n = strings->Get(ring);
                    std::vector<Pt> pts;
                    pts.reserve(n);
                    for (std::uint32_t k = 0; k < n; ++k) {
                        const auto* v = verts->Get(boundaries->Get(vertex + k));
                        pts.push_back(Pt{v->x() * scale[0] + translate[0],
                                         v->y() * scale[1] + translate[1],
                                         v->z() * scale[2] + translate[2]});
                    }
                    // Ring 0 is the outer boundary; the rest are holes, which
                    // subtract (CityJSON 2.0 section 6).
                    area += (r == 0 ? 1.0 : -1.0) * ring_area(pts);
                    vertex += n;
                    ++ring;
                }

                // semantics() is one entry per surface, in surface order, so it
                // indexes directly -- no regrouping needed for a per-surface
                // question. UINT32_MAX means "no semantic surface".
                std::string label = "unassigned";
                if (semantics != nullptr && s < semantics->size() &&
                    semantics->Get(s) != UINT32_MAX && geom->semantics_objects() != nullptr) {
                    const auto* so = geom->semantics_objects()->Get(semantics->Get(s));
                    if (so != nullptr) {
                        label = so->extension_type() != nullptr
                                    ? so->extension_type()->str()
                                    : fcb::semantic_surface_type_name(
                                          static_cast<std::uint8_t>(so->type()));
                    }
                }
                out[label] += area;
            }
        }
    }
}

/// The same totals via the nested CityJSON, used only to check the walk above.
/// The slow path: it allocates the whole nested structure and the semantics
/// arrays for every feature.
void area_by_surface_type_via_json(const fcb::Feature& feature, const fcb::HeaderView& header,
                                   const std::string& lod, std::map<std::string, double>& out) {
    const nlohmann::json cj = fcb::to_cityjson_feature(feature, header);
    const fcb::FileInfo& info = header.info();
    const std::array<double, 3> scale =
        info.has_transform ? info.scale : std::array<double, 3>{1, 1, 1};
    const std::array<double, 3> translate =
        info.has_transform ? info.translate : std::array<double, 3>{0, 0, 0};

    const auto at = [&](std::size_t i) {
        const auto& v = cj.at("vertices").at(i);
        return Pt{v.at(0).get<double>() * scale[0] + translate[0],
                  v.at(1).get<double>() * scale[1] + translate[1],
                  v.at(2).get<double>() * scale[2] + translate[2]};
    };

    for (const auto& obj : cj.at("CityObjects")) {
        if (!obj.contains("geometry"))
            continue;
        for (const auto& geom : obj.at("geometry")) {
            if (!geom.contains("lod") || geom.at("lod").get<std::string>() != lod)
                continue;

            std::vector<nlohmann::json> surfaces;
            std::function<void(const nlohmann::json&)> collect = [&](const nlohmann::json& node) {
                if (!node.is_array() || node.empty())
                    return;
                if (node[0].is_array() && !node[0].empty() && node[0][0].is_number()) {
                    surfaces.push_back(node);
                    return;
                }
                for (const auto& child : node)
                    collect(child);
            };
            collect(geom.at("boundaries"));

            std::vector<nlohmann::json> values;
            std::function<void(const nlohmann::json&)> flatten = [&](const nlohmann::json& node) {
                if (node.is_array()) {
                    for (const auto& child : node)
                        flatten(child);
                } else {
                    values.push_back(node);
                }
            };
            nlohmann::json sem_surfaces = nlohmann::json::array();
            if (geom.contains("semantics")) {
                flatten(geom.at("semantics").at("values"));
                sem_surfaces = geom.at("semantics").at("surfaces");
            }

            for (std::size_t s = 0; s < surfaces.size(); ++s) {
                double area = 0.0;
                for (std::size_t r = 0; r < surfaces[s].size(); ++r) {
                    std::vector<Pt> pts;
                    for (const auto& idx : surfaces[s][r])
                        pts.push_back(at(idx.get<std::size_t>()));
                    area += (r == 0 ? 1.0 : -1.0) * ring_area(pts);
                }
                std::string label = "unassigned";
                if (s < values.size() && !values[s].is_null()) {
                    const auto& surf = sem_surfaces.at(values[s].get<std::size_t>());
                    if (surf.contains("type"))
                        label = surf.at("type").get<std::string>();
                }
                out[label] += area;
            }
        }
    }
}

}  // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s <file.fcb> [max-features] [lod]\n", argv[0]);
        return 2;
    }
    const unsigned long limit = argc >= 3 ? std::strtoul(argv[2], nullptr, 10) : 20;
    const std::string lod = argc >= 4 ? argv[3] : "2.2";

    try {
        fcb::FcbReader reader = fcb::FcbReader::open_file(argv[1]);
        std::printf("analysing the first %lu feature(s) of %s, lod %s\n", limit, argv[1],
                    lod.c_str());
        std::printf("walking the flat FlatBuffers arrays -- no CityJSON, no nesting\n\n");

        std::map<std::string, double> totals;
        unsigned long features = 0;
        unsigned long mismatches = 0;
        // 3DBAG publishes its own computed ground area per building, so the
        // walk can be checked against the dataset and not only against this
        // library's other code path.
        double published_ground = 0.0;
        bool have_published = false;

        const auto t0 = std::chrono::steady_clock::now();
        fcb::FeatureIterator it = reader.select_all();
        while (features < limit && it.next()) {
            const fcb::Feature& f = it.current();

            std::map<std::string, double> flat;
            area_by_surface_type(f, reader.header(), lod, flat);
            for (const auto& [k, v] : flat)
                totals[k] += v;

            for (std::size_t i = 0; i < f.city_object_count(); ++i) {
                if (!f.object_has_attributes(i))
                    continue;
                const std::vector<fcb::ColumnInfo> schema =
                    f.object_has_columns(i) ? f.object_columns(i) : reader.header().info().columns;
                const nlohmann::json attrs =
                    fcb::attributes_to_json(f.object_attributes(i), schema);
                if (attrs.contains("b3_opp_grond") && attrs.at("b3_opp_grond").is_number()) {
                    published_ground += attrs.at("b3_opp_grond").get<double>();
                    have_published = true;
                }
            }

            std::map<std::string, double> via_json;
            area_by_surface_type_via_json(f, reader.header(), lod, via_json);
            for (const auto& [k, v] : flat) {
                if (std::fabs(v - via_json[k]) > 1e-6 * std::fmax(1.0, std::fabs(v)))
                    ++mismatches;
            }
            ++features;
        }
        const auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(
                            std::chrono::steady_clock::now() - t0)
                            .count();

        std::printf("surface area at lod %s over %lu feature(s), m^2\n", lod.c_str(), features);
        const char* order[] = {"RoofSurface", "GroundSurface", "WallSurface"};
        double total = 0.0;
        for (const char* label : order) {
            const auto hit = totals.find(label);
            if (hit == totals.end())
                continue;
            total += hit->second;
            std::printf("  %-16s %12.2f\n", label, hit->second);
        }
        for (const auto& [label, v] : totals) {
            if (label == "RoofSurface" || label == "GroundSurface" || label == "WallSurface")
                continue;
            total += v;
            std::printf("  %-16s %12.2f\n", label.c_str(), v);
        }
        std::printf("  %-16s %12.2f\n", "TOTAL", total);

        std::printf("\nflat walk vs nested CityJSON: %s\n", mismatches == 0 ? "AGREE" : "MISMATCH");
        if (have_published) {
            const double ground =
                totals.count("GroundSurface") != 0 ? totals["GroundSurface"] : 0.0;
            const double pct =
                100.0 * std::fabs(ground - published_ground) / std::fmax(1.0, published_ground);
            std::printf("GroundSurface vs the dataset's own b3_opp_grond: %.2f vs %.2f m^2 (%.3f%% "
                        "apart)\n",
                        ground, published_ground, pct);
            std::printf("  a sanity check against a number this library did not produce. "
                        "Ordinary\n  buildings agree to well under 1%% (the 1 mm coordinate "
                        "grid); a few large\n  multi-part ones differ more, because "
                        "b3_opp_grond came from the source\n  geometry by a different "
                        "pipeline. The READER check is the line above.\n");
        }
        std::printf("%lu feature(s) in %lld ms\n", features, static_cast<long long>(ms));
        return 0;
    } catch (const std::exception& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
}
