// Read a remote .fcb over HTTP range requests.
// Requires -DFCB_WITH_CURL=ON.
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
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s <url>\n", argv[0]);
        return 2;
    }
    try {
        auto transport = std::make_shared<fcb::CurlRangeReader>(argv[1]);
        fcb::FcbReader reader = fcb::FcbReader::open(transport);
        const auto& info = reader.header().info();
        std::fprintf(stderr, "%llu features, CityJSON %s\n",
                     static_cast<unsigned long long>(info.features_count),
                     info.cityjson_version.c_str());

        // Only the intersecting features are fetched, not the whole file.
        fcb::BBox half{info.geographical_extent[0], info.geographical_extent[1],
                       (info.geographical_extent[0] + info.geographical_extent[3]) / 2.0,
                       info.geographical_extent[4]};
        auto it = reader.select_bbox(half);
        unsigned long long n = 0;
        while (it.next())
            ++n;
        std::fprintf(stderr, "%llu features in the western half, %llu HTTP requests\n", n,
                     static_cast<unsigned long long>(transport->request_count()));
        return 0;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
#endif
}
