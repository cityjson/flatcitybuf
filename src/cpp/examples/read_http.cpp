// Read a remote .fcb over HTTP range requests.
//
//     read_http <url> [minx miny maxx maxy]
//
// Requires -DFCB_WITH_CURL=ON (its own build tree: `cmake -B build-curl -S .
// -DFCB_WITH_CURL=ON`). The point of the format is that only the intersecting
// features are fetched, never the whole file -- the request count printed at
// the end is the evidence.
//
// PASS A BBOX for any real remote file. With no bbox this queries a huge
// default window (see below), which on a national dataset means tens of GB of
// range requests. The default exists only so the example does *something*
// against the tiny Delft fixture; it is not a sensible default for 3dbag.
#include <fcb/reader.hpp>

#include <cstdio>
#include <memory>

#ifdef FCB_WITH_CURL
#    include <fcb/http/curl_range_reader.hpp>
#endif

int main(int argc, char** argv) {
#ifndef FCB_WITH_CURL
    (void)argc;
    (void)argv;
    std::fprintf(stderr, "built without FCB_WITH_CURL; reconfigure with -DFCB_WITH_CURL=ON\n");
    return 2;
#else
    if (argc != 2 && argc != 6) {
        std::fprintf(stderr, "usage: %s <url> [minx miny maxx maxy]\n", argv[0]);
        return 2;
    }
    try {
        auto transport = std::make_shared<fcb::CurlRangeReader>(argv[1]);

        // Opening the file parses only the header plus the top of the R-tree,
        // so this is one small read regardless of how large the file is. If
        // the header fails FlatBuffers verification here, the file predates
        // the struct-alignment fix (540772a) -- re-serialize it.
        fcb::FcbReader reader = fcb::FcbReader::open(transport);
        const auto& info = reader.header().info();
        std::fprintf(stderr, "%llu features, CityJSON %s\n",
                     static_cast<unsigned long long>(info.features_count),
                     info.cityjson_version.c_str());
        std::fprintf(stderr, "opened in %llu HTTP request(s)\n",
                     static_cast<unsigned long long>(transport->request_count()));

        fcb::BBox query;
        if (argc == 6) {
            query = fcb::BBox{std::stod(argv[2]), std::stod(argv[3]), std::stod(argv[4]),
                              std::stod(argv[5])};
        } else {
            // No bbox given: the western half of the declared extent. Fine for
            // the Delft fixture, ruinous for a national file -- pass a bbox.
            query = fcb::BBox{info.geographical_extent[0], info.geographical_extent[1],
                              (info.geographical_extent[0] + info.geographical_extent[3]) / 2.0,
                              info.geographical_extent[4]};
        }

        transport->reset_request_count();
        auto it = reader.select_bbox(query);
        unsigned long long n = 0;
        while (it.next())
            ++n;
        std::fprintf(stderr, "%llu feature(s) in the query bbox, %llu HTTP request(s)\n", n,
                     static_cast<unsigned long long>(transport->request_count()));
        return 0;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
#endif
}
