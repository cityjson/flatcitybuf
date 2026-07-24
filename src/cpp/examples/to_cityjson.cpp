// Get the CityJSON JSON representation and reach into its fields.
//
//     to_cityjson <file.fcb> [feature-index]
//
// read_local.cpp streams whole CityJSONSeq lines out; read_features.cpp reads
// attributes WITHOUT building JSON. This one sits in between: it shows the
// two conversion entry points --
//
//     to_cityjson_metadata(header)          -> the CityJSON "metadata" object
//     to_cityjson_feature(feature, header)  -> one CityJSONFeature
//
// -- both returning an `nlohmann::json`, and then how to NAVIGATE that tree to
// pull out the fields you actually want: the id, a CityObject's type and
// attributes, its geometry's LoD and boundaries, and real-world vertex
// coordinates. Requires the JSON component (-DFCB_WITH_JSON=ON, the default).
#include <fcb/cityjson.hpp>
#include <fcb/reader.hpp>

#include <cstdio>
#include <cstdlib>
#include <string>

using nlohmann::json;

namespace {

// The shape of a CityJSONFeature, for reference while reading the code below:
//
//   { "type": "CityJSONFeature",
//     "id": "NL.IMBAG.Pand....",
//     "CityObjects": {
//       "<object id>": {
//         "type": "Building" | "BuildingPart" | ...,
//         "attributes": { "b3_h_dak_50p": 11.26, "status": "...", ... },
//         "children" / "parents": [ "<object id>", ... ],
//         "geometry": [ { "type": "Solid", "lod": "1.2",
//                         "boundaries": [...], "semantics": {...} } ] } },
//     "vertices": [ [i, j, k], ... ] }        // QUANTIZED integers
//
// The `transform` that turns those integer vertices into real coordinates is
// NOT on the feature -- it is on the metadata object, which is why this
// example reads both.

void print_metadata_fields(const json& meta) {
    std::printf("== metadata (to_cityjson_metadata) ==\n");

    // `.value(key, default)` reads a field if present, else the default --
    // no exception for an absent optional field.
    std::printf("  version   %s\n", meta.value("version", "?").c_str());

    // `.contains()` guards a nested lookup; `.at()` throws if the key is
    // missing, so guard before using it on anything optional.
    if (meta.contains("metadata") && meta["metadata"].contains("referenceSystem")) {
        std::printf("  CRS       %s\n",
                    meta["metadata"]["referenceSystem"].get<std::string>().c_str());
    }

    // `transform.scale` / `transform.translate` are arrays of three doubles.
    // Every vertex v maps to (v[i] * scale[i] + translate[i]).
    if (meta.contains("transform")) {
        const json& t = meta["transform"];
        const json& s = t["scale"];
        const json& tr = t["translate"];
        std::printf("  scale     [%g, %g, %g]\n", s[0].get<double>(), s[1].get<double>(),
                    s[2].get<double>());
        std::printf("  translate [%.3f, %.3f, %.3f]\n", tr[0].get<double>(), tr[1].get<double>(),
                    tr[2].get<double>());
    }
}

void print_feature_fields(const json& feat, const json& meta) {
    std::printf("\n== feature (to_cityjson_feature) ==\n");
    std::printf("  id            %s\n", feat.value("id", "?").c_str());

    const json& objects = feat.at("CityObjects");  // always present; a JSON object
    std::printf("  CityObjects   %zu\n", objects.size());

    // Iterate the CityObjects map: `.items()` yields (key, value) pairs, the
    // key being the object id.
    const json* first_with_attrs = nullptr;
    std::string first_with_attrs_id;
    for (const auto& [obj_id, obj] : objects.items()) {
        std::printf("  - %-42s type=%s\n", obj_id.c_str(), obj.value("type", "?").c_str());
        if (first_with_attrs == nullptr && obj.contains("attributes") &&
            !obj["attributes"].empty()) {
            first_with_attrs = &obj;
            first_with_attrs_id = obj_id;
        }
    }
    if (first_with_attrs == nullptr) {
        std::printf("  (no object carries attributes)\n");
        return;
    }

    // --- reading typed attribute values -----------------------------------
    const json& obj = *first_with_attrs;
    const json& attrs = obj.at("attributes");
    std::printf("\n  attributes of %s (%zu):\n", first_with_attrs_id.c_str(), attrs.size());

    // A string attribute. `.value` returns the default if the key is absent.
    std::printf("    status               %s\n", attrs.value("status", "(absent)").c_str());

    // A numeric attribute, read as a double -- but only if it is actually a
    // number, so a missing or differently-typed value does not throw.
    if (attrs.contains("b3_h_dak_50p") && attrs["b3_h_dak_50p"].is_number()) {
        std::printf("    b3_h_dak_50p         %.2f\n", attrs["b3_h_dak_50p"].get<double>());
    }
    // A boolean, and an integer year.
    if (attrs.contains("b3_kas_warenhuis")) {
        std::printf("    b3_kas_warenhuis     %s\n",
                    attrs["b3_kas_warenhuis"].get<bool>() ? "true" : "false");
    }
    if (attrs.contains("oorspronkelijkbouwjaar")) {
        std::printf("    oorspronkelijkbouwjaar %lld\n",
                    attrs["oorspronkelijkbouwjaar"].get<long long>());
    }

    // --- geometry ---------------------------------------------------------
    if (obj.contains("geometry") && !obj["geometry"].empty()) {
        const json& g = obj["geometry"][0];
        std::printf("\n  geometry[0]: type=%s lod=%s\n", g.value("type", "?").c_str(),
                    g.value("lod", "?").c_str());
        // `boundaries` is a nested array of integer indices INTO `vertices`.
        // Its nesting depth depends on the geometry type (Solid nests deeper
        // than MultiSurface); here we just report the top-level count.
        if (g.contains("boundaries")) {
            std::printf("  geometry[0]: %zu top-level boundary group(s)\n", g["boundaries"].size());
        }
    }

    // --- vertices: quantized -> real coordinates --------------------------
    const json& verts = feat.at("vertices");
    std::printf("\n  vertices      %zu (quantized integers)\n", verts.size());
    if (!verts.empty() && meta.contains("transform")) {
        const json& v0 = verts[0];  // [i, j, k]
        const json& s = meta["transform"]["scale"];
        const json& tr = meta["transform"]["translate"];
        // The dequantization the whole format hangs on:
        const double x = v0[0].get<double>() * s[0].get<double>() + tr[0].get<double>();
        const double y = v0[1].get<double>() * s[1].get<double>() + tr[1].get<double>();
        const double z = v0[2].get<double>() * s[2].get<double>() + tr[2].get<double>();
        std::printf("    vertices[0]  raw [%lld, %lld, %lld]  ->  real [%.3f, %.3f, %.3f]\n",
                    v0[0].get<long long>(), v0[1].get<long long>(), v0[2].get<long long>(), x, y,
                    z);
    }
}

}  // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s <file.fcb> [feature-index]\n", argv[0]);
        return 2;
    }
    const unsigned long want = argc >= 3 ? std::strtoul(argv[2], nullptr, 10) : 0;

    try {
        fcb::FcbReader reader = fcb::FcbReader::open_file(argv[1]);

        // The metadata object -- and, inside it, the transform we need to
        // dequantize vertices.
        const json meta = fcb::to_cityjson_metadata(reader.header());
        print_metadata_fields(meta);

        // Walk to the requested feature and convert just that one.
        fcb::FeatureIterator it = reader.select_all();
        unsigned long i = 0;
        while (it.next()) {
            if (i == want) {
                const json feat = fcb::to_cityjson_feature(it.current(), reader.header());
                print_feature_fields(feat, meta);

                // The full JSON, so you can see every field the accessors
                // above reached into. `.dump(2)` pretty-prints with 2-space
                // indent; `.dump()` gives the compact CityJSONSeq form.
                std::printf("\n== feature %lu, full JSON ==\n%s\n", want, feat.dump(2).c_str());
                return 0;
            }
            ++i;
        }
        std::fprintf(stderr, "feature index %lu out of range (%lu features)\n", want, i);
        return 1;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
}
