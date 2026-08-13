// Print everything the header knows about a .fcb file, without decoding a
// single feature.
//
// This is the example to run first against an unfamiliar file: it answers
// "how many features, in what CRS, over what extent, and WHICH attributes can
// I actually query?" -- that last one matters, because `select_attr` only
// works on columns that were given an index at write time. The Rust CLI's
// `fcb inspect --static` covers the same ground; this is the C++ API doing it.
#include <fcb/generated/header_generated.h>
#include <fcb/header.hpp>
#include <fcb/reader.hpp>

#include <cstdio>
#include <set>
#include <string>

namespace {

const char* type_name(std::uint8_t t) { return EnumNameColumnType(static_cast<::ColumnType>(t)); }

}  // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s <file.fcb>\n", argv[0]);
        return 2;
    }
    try {
        fcb::FcbReader reader = fcb::FcbReader::open_file(argv[1]);
        const fcb::HeaderView& header = reader.header();
        const fcb::FileInfo& info = header.info();

        std::printf("file          %s\n", argv[1]);
        std::printf("features      %llu\n", static_cast<unsigned long long>(info.features_count));
        std::printf("CityJSON      %s\n", info.cityjson_version.c_str());
        if (info.title.has_value())
            std::printf("title         %s\n", info.title->c_str());
        if (!info.crs.empty())
            std::printf("CRS           %s\n", info.crs.c_str());

        if (info.has_extent) {
            const auto& e = info.geographical_extent;
            std::printf("extent        [%.3f %.3f %.3f] .. [%.3f %.3f %.3f]\n", e[0], e[1], e[2],
                        e[3], e[4], e[5]);
        } else {
            std::printf("extent        (none declared)\n");
        }

        // Coordinates are stored as scaled integers; every vertex is
        // vertex * scale + translate. A file without a transform stores
        // coordinates directly.
        if (info.has_transform) {
            std::printf("transform     scale [%g %g %g] translate [%.3f %.3f %.3f]\n",
                        info.scale[0], info.scale[1], info.scale[2], info.translate[0],
                        info.translate[1], info.translate[2]);
        }

        // A spatial query needs the R-tree; select_bbox throws NoIndex without
        // one. index_node_size is the tree's branching factor and is read from
        // the header rather than assumed, so files written with a non-default
        // node size still traverse correctly.
        std::printf("R-tree        %s (node size %u)\n",
                    header.layout().rtree_begin != 0 ? "yes" : "no",
                    static_cast<unsigned>(info.index_node_size));

        // Which columns carry a static B+tree, i.e. which ones select_attr can
        // answer. Everything else is readable but not queryable.
        std::set<std::uint16_t> indexed;
        for (const fcb::AttrIndexInfo& ix : header.attr_indices()) {
            indexed.insert(ix.column_index);
        }

        std::printf("\ncolumns (%zu; * = queryable via select_attr)\n", info.columns.size());
        for (const fcb::ColumnInfo& c : info.columns) {
            std::printf("  %c %-34s %-9s%s\n", indexed.count(c.index) != 0 ? '*' : ' ',
                        c.name.c_str(), type_name(c.type), c.nullable ? "" : "  NOT NULL");
        }
        std::printf("\n%zu of %zu columns are queryable\n", indexed.size(), info.columns.size());

        // Per-object schemas override the header's, and that is the normal
        // case rather than the exception -- see read_features.cpp.
        if (!info.semantic_columns.empty()) {
            std::printf("semantic columns: %zu\n", info.semantic_columns.size());
        }
        return 0;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
}
