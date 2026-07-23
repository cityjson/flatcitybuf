// Walk features WITHOUT converting them to CityJSON.
//
//     read_features <file.fcb> [max-features]
//
// read_local.cpp shows the high-level path: to_cityjson_feature() hands back
// a whole nlohmann::json tree. That is the right tool when you want CityJSON.
// When you only need a few attributes, building that tree per feature is
// wasted work -- this shows the lower-level API instead.
//
// It also demonstrates the single easiest thing to get wrong in this format:
// ATTRIBUTE SCHEMAS ARE PER OBJECT. A CityObject may carry its own `columns`,
// which override the header's, and on real data that is the normal case, not
// an edge case. Attribute blobs are not self-delimiting -- each value's width
// comes from its column type -- so decoding with the wrong schema does not
// throw, it silently yields plausible garbage.
#include <fcb/attribute.hpp>
#include <fcb/feature.hpp>
#include <fcb/header.hpp>
#include <fcb/reader.hpp>

#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s <file.fcb> [max-features]\n", argv[0]);
        return 2;
    }
    const unsigned long limit = argc >= 3 ? std::strtoul(argv[2], nullptr, 10) : 3;

    try {
        fcb::FcbReader reader = fcb::FcbReader::open_file(argv[1]);
        const std::vector<fcb::ColumnInfo>& header_columns = reader.header().info().columns;

        fcb::FeatureIterator it = reader.select_all();
        unsigned long seen = 0;
        unsigned long with_own_schema = 0;

        while (seen < limit && it.next()) {
            const fcb::Feature& f = it.current();
            std::printf("feature %s  (%zu CityObject%s)\n", f.id().c_str(), f.city_object_count(),
                        f.city_object_count() == 1 ? "" : "s");

            for (std::size_t i = 0; i < f.city_object_count(); ++i) {
                std::printf("  object %s\n", f.object_id(i).c_str());

                std::array<double, 6> extent{};
                if (f.object_extent(i, extent)) {
                    std::printf("    extent   [%.2f %.2f %.2f] .. [%.2f %.2f %.2f]\n", extent[0],
                                extent[1], extent[2], extent[3], extent[4], extent[5]);
                }

                if (!f.object_has_attributes(i)) {
                    std::printf("    (no attributes)\n");
                    continue;
                }

                // THE important line: the object's own columns win when it
                // declares them; fall back to the header's only when it does
                // not. Using header_columns unconditionally is the bug.
                const bool own = f.object_has_columns(i);
                if (own)
                    ++with_own_schema;
                const std::vector<fcb::ColumnInfo> schema =
                    own ? f.object_columns(i) : header_columns;

                // attributes_to_json is the convenient rendering; decode_attributes
                // returns (name, AttrValue) pairs if you would rather switch on
                // the value type yourself and skip JSON entirely.
                const nlohmann::json attrs =
                    fcb::attributes_to_json(f.object_attributes(i), schema);
                std::printf("    schema   %s (%zu columns)\n", own ? "own" : "header",
                            schema.size());

                if (attrs.empty()) {
                    // The object declared an attribute blob, but it decoded to
                    // nothing -- an empty record, not an error. Parent objects
                    // in a parent/child pair often look like this.
                    std::printf("    (attribute blob present but empty)\n");
                    continue;
                }

                unsigned shown = 0;
                for (auto entry = attrs.begin(); entry != attrs.end() && shown < 5;
                     ++entry, ++shown) {
                    std::printf("    %-28s %s\n", entry.key().c_str(),
                                entry.value().dump().c_str());
                }
                if (attrs.size() > shown) {
                    std::printf("    ... %zu more\n", attrs.size() - shown);
                }
            }
            ++seen;
        }

        std::fprintf(stderr, "%lu feature(s) shown; %lu object(s) carried their own schema\n", seen,
                     with_own_schema);
        return 0;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
}
