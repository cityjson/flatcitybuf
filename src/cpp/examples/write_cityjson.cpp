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
//   1. Scans every feature once to build the attribute schema
//      (`fcb::add_attributes`) -- column numbering is INSERTION order, not
//      alphabetical, so every feature must be scanned before any is
//      encoded (a two-pass shape, same as the Rust `fcb` CLI's own
//      approach: schema first, then writing).
//   2. Constructs an `fcb::FcbWriter` and calls `add_feature` once per
//      feature -- each call spools that feature's encoded bytes to a
//      private temp file rather than holding every feature in memory, so
//      this scales to a CityJSONSeq larger than available RAM.
//   3. Calls `write()` once to get the complete file bytes, and writes
//      them out.
//
// Every column discovered in step 1 gets a B+tree attribute index (the
// `fcb_query_attributes` example can then query any of them) -- this
// mirrors the Rust CLI's `-A`/--index-all-attributes flag, always on here
// to keep this example's argument list to just the two file paths.
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

        // Step 1: scan every feature once, in document order, to build the
        // attribute schema -- exactly like the Rust `fcb` CLI's own
        // schema-building pass (cli/src/main.rs).
        fcb::AttributeSchema attr_schema;
        for (const auto& feature : features) {
            auto co_it = feature.find("CityObjects");
            if (co_it == feature.end() || !co_it->is_object()) {
                continue;
            }
            for (const auto& [id, city_object] : co_it->items()) {
                (void)id;
                if (auto attr_it = city_object.find("attributes"); attr_it != city_object.end()) {
                    fcb::add_attributes(attr_schema, *attr_it);
                }
            }
        }

        // Step 2/3: stream every feature through FcbWriter, indexing every
        // discovered column.
        fcb::FcbWriterOptions options;
        for (const auto& [name, index_and_type] : attr_schema) {
            (void)index_and_type;
            options.attribute_indices.emplace_back(name, std::nullopt);
        }

        fcb::FcbWriter writer(metadata_line, options, attr_schema, std::nullopt);
        for (const auto& feature : features) {
            writer.add_feature(feature);
        }
        const std::vector<std::uint8_t> bytes = writer.write();

        std::ofstream out(argv[2], std::ios::binary);
        if (!out.good()) {
            std::fprintf(stderr, "error: cannot create %s\n", argv[2]);
            return 1;
        }
        out.write(reinterpret_cast<const char*>(bytes.data()),
                  static_cast<std::streamsize>(bytes.size()));
        out.close();

        std::printf("%s -> %s\n", argv[1], argv[2]);
        std::printf("  %zu feature(s), %zu byte(s)\n", features.size(), bytes.size());
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
        return 0;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
}
