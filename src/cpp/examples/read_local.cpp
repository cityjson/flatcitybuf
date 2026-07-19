// Read a local .fcb file and stream it out as CityJSONSeq.
#include <fcb/cityjson.hpp>
#include <fcb/reader.hpp>

#include <cstdio>
#include <iostream>

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s <file.fcb> [minx miny maxx maxy]\n", argv[0]);
        return 2;
    }
    try {
        fcb::FcbReader reader = fcb::FcbReader::open_file(argv[1]);
        const auto& info = reader.header().info();
        std::fprintf(stderr, "%llu features, CityJSON %s, %s\n",
                     static_cast<unsigned long long>(info.features_count),
                     info.cityjson_version.c_str(), info.crs.c_str());

        std::cout << fcb::to_cityjson_metadata(reader.header()).dump() << "\n";

        auto it = (argc >= 6) ? reader.select_bbox(fcb::BBox{std::stod(argv[2]),
                                                             std::stod(argv[3]),
                                                             std::stod(argv[4]),
                                                             std::stod(argv[5])})
                              : reader.select_all();
        while (it.next()) {
            std::cout << fcb::to_cityjson_feature(it.current(), reader.header()).dump()
                      << "\n";
        }
        return 0;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
}
