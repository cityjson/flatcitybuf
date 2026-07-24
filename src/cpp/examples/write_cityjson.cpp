// Write a CityJSONSeq (.jsonl) file out as `.fcb`, using this port's own
// writer -- no Rust toolchain involved.
//
//     write_cityjson <input.jsonl> <output.fcb>
//
// A CityJSONSeq is one CityJSON metadata line (`type`/`version`/`transform`/
// `metadata`/...) followed by one `CityJSONFeature` line per feature. This
// example reads the whole thing with a plain `nlohmann::json` parse per
// line (no CityJSON-specific parsing needed on the way IN -- the writer
// takes CityJSON-shaped JSON directly), then:
//
//   1. Scans every feature TWICE to build two SEPARATE schemas -- column
//      numbering is INSERTION order, not alphabetical, so every feature
//      must be scanned before any is encoded (a two-pass shape). This
//      follows the Rust `fcb` CLI's own two passes closely (cli/src/
//      main.rs) but is not a byte-for-byte reproduction of it: the CLI
//      caps its ORDINARY attribute scan at the first 1000 features (a
//      speed optimization, not a correctness requirement -- commented as
//      such in the CLI source), which this example does not replicate, so
//      a >1000-feature input with attributes that only appear later can
//      produce a different schema here than the CLI would.
//        a. Ordinary CityObject attributes (`fcb::add_attributes`) --
//           city objects are visited in ASCENDING ID order within each
//           feature, matching the CLI exactly, since column numbering
//           depends on which object's keys are seen first.
//        b. Semantic surface attributes: every geometry's
//           `semantics.surfaces[i]`'s members OTHER than `type`/`parent`/
//           `children` -- these need their own, separate schema, encoded
//           and indexed independently of ordinary attributes. Passing
//           `std::nullopt` here instead (as an earlier version of this
//           example did) silently drops every such member: the writer
//           has nowhere to encode a semantic attribute without a schema
//           for it.
//   2. Constructs an `fcb::FcbWriter` and calls `add_feature` once per
//      feature -- each call spools that feature's encoded bytes to a
//      private temp file rather than holding every encoded feature in
//      memory at once.
//   3. Calls `write(std::ostream&)` to stream the complete file straight
//      to the output path -- the overload that actually keeps memory
//      bounded during finalization (the vector-returning `write()`
//      overload exists too, for convenience, but always materializes the
//      whole output at once; see fcb_writer.hpp).
//
// Every ordinary column discovered in step 1a gets a B+tree attribute
// index (the `fcb_query_attributes` example can then query any of them),
// at branching factor 256 -- this mirrors the Rust CLI's `-A`/
// --index-all-attributes flag (which defaults to 256, not this library's
// own default of 16) closely enough to produce comparable files, to keep
// this example's argument list to just the two file paths. Semantic
// attributes are encoded but not indexed, matching what `-A` alone does
// in the CLI too.
#include <fcb/error.hpp>
#include <fcb/writer/attribute.hpp>
#include <fcb/writer/fcb_writer.hpp>

#include <algorithm>
#include <cstdio>
#include <fstream>
#include <string>
#include <vector>

using nlohmann::ordered_json;

namespace {

std::vector<ordered_json> read_jsonl(const std::string& path) {
    std::ifstream in(path);
    if (!in.good()) {
        throw fcb::Error(fcb::ErrorCode::IoError, "cannot open " + path);
    }
    std::vector<ordered_json> lines;
    std::string line;
    while (std::getline(in, line)) {
        if (!line.empty()) {
            lines.push_back(ordered_json::parse(line));
        }
    }
    return lines;
}

/// This feature's CityObject ids, ascending -- column numbering depends on
/// visit order, so this must match the Rust CLI's own `ids.sort_unstable()`
/// exactly, not just visit every object in SOME order.
std::vector<std::string> sorted_city_object_ids(const ordered_json& feature) {
    std::vector<std::string> ids;
    auto co_it = feature.find("CityObjects");
    if (co_it == feature.end() || !co_it->is_object()) {
        return ids;
    }
    for (const auto& [id, unused] : co_it->items()) {
        (void)unused;
        ids.push_back(id);
    }
    std::sort(ids.begin(), ids.end());
    return ids;
}

/// A semantic surface's members besides `type`/`parent`/`children` --
/// mirrors feature_serializer.cpp's `to_semantic_object`, which treats
/// exactly those three as known and everything else as an indexable
/// "other" attribute.
ordered_json semantic_surface_other_members(const ordered_json& surface) {
    ordered_json other = ordered_json::object();
    for (const auto& [key, val] : surface.items()) {
        if (key != "type" && key != "parent" && key != "children") {
            other[key] = val;
        }
    }
    return other;
}

}  // namespace

int main(int argc, char** argv) {
    if (argc != 3) {
        std::fprintf(stderr, "usage: %s <input.jsonl> <output.fcb>\n", argv[0]);
        return 2;
    }

    try {
        std::vector<ordered_json> lines = read_jsonl(argv[1]);
        if (lines.empty()) {
            std::fprintf(stderr, "error: %s is empty\n", argv[1]);
            return 1;
        }
        const ordered_json& metadata_line = lines.front();
        const std::vector<ordered_json> features(lines.begin() + 1, lines.end());

        // Step 1a: ordinary CityObject attributes.
        fcb::AttributeSchema attr_schema;
        for (const auto& feature : features) {
            for (const auto& id : sorted_city_object_ids(feature)) {
                const ordered_json& city_object = feature.at("CityObjects").at(id);
                if (auto attr_it = city_object.find("attributes"); attr_it != city_object.end()) {
                    fcb::add_attributes(attr_schema, *attr_it);
                }
            }
        }

        // Step 1b: semantic surface attributes -- a separate schema.
        fcb::AttributeSchema semantic_attr_schema;
        for (const auto& feature : features) {
            for (const auto& id : sorted_city_object_ids(feature)) {
                const ordered_json& city_object = feature.at("CityObjects").at(id);
                auto geom_it = city_object.find("geometry");
                if (geom_it == city_object.end() || !geom_it->is_array()) {
                    continue;
                }
                for (const auto& geometry : *geom_it) {
                    auto sem_it = geometry.find("semantics");
                    if (sem_it == geometry.end() || !sem_it->contains("surfaces")) {
                        continue;
                    }
                    for (const auto& surface : sem_it->at("surfaces")) {
                        ordered_json other = semantic_surface_other_members(surface);
                        if (!other.empty()) {
                            fcb::add_attributes(semantic_attr_schema, other);
                        }
                    }
                }
            }
        }
        const bool has_semantic_attrs = !semantic_attr_schema.empty();

        // Step 2/3: stream every feature through FcbWriter, indexing every
        // ordinary column at the Rust CLI's own `-A` branching factor.
        fcb::FcbWriterOptions options;
        for (const auto& [name, index_and_type] : attr_schema) {
            (void)index_and_type;
            options.attribute_indices.emplace_back(name, static_cast<std::uint16_t>(256));
        }

        fcb::FcbWriter writer(metadata_line, options, attr_schema,
                              has_semantic_attrs ? std::optional(semantic_attr_schema)
                                                 : std::nullopt);
        for (const auto& feature : features) {
            writer.add_feature(feature);
        }

        std::ofstream out(argv[2], std::ios::binary);
        if (!out.good()) {
            std::fprintf(stderr, "error: cannot create %s\n", argv[2]);
            return 1;
        }
        writer.write(out);  // streams straight to `out`; see the file header comment
        out.close();
        if (!out) {
            std::fprintf(stderr, "error: failed writing %s\n", argv[2]);
            return 1;
        }

        std::printf("%s -> %s\n", argv[1], argv[2]);
        std::printf("  %zu feature(s)\n", features.size());
        std::printf("  %zu attribute column(s), each with a B+tree index:\n", attr_schema.size());
        // `attr_schema` is keyed by name (alphabetical iteration), not by
        // column index -- sort by index for a summary that reads in the
        // same order the columns were actually assigned.
        std::vector<std::pair<std::string, std::uint16_t>> columns_by_index;
        for (const auto& [name, index_and_type] : attr_schema) {
            columns_by_index.emplace_back(name, index_and_type.first);
        }
        std::sort(columns_by_index.begin(), columns_by_index.end(),
                  [](const auto& a, const auto& b) { return a.second < b.second; });
        for (const auto& [name, index] : columns_by_index) {
            std::printf("    column %-4u %s\n", index, name.c_str());
        }
        if (has_semantic_attrs) {
            std::printf("  %zu semantic-surface attribute column(s) (not indexed):\n",
                        semantic_attr_schema.size());
            for (const auto& [name, index_and_type] : semantic_attr_schema) {
                (void)index_and_type;
                std::printf("    %s\n", name.c_str());
            }
        }
        return 0;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    } catch (const std::exception& e) {
        std::fprintf(stderr, "error: malformed input -- %s\n", e.what());
        return 1;
    }
}
