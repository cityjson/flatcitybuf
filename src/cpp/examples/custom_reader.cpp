// Bring your own transport by implementing fcb::RangeReader.
//
//     custom_reader <file.fcb> [minx miny maxx maxy]
//
// RangeReader is the library's only IO seam: local files, HTTP and anything
// else all go through it, so the traversal code never knows where bytes come
// from. Implement it to read from an object store, a game engine's VFS, an
// mmap, a decrypting layer -- INSTALL.md calls this out but ships no worked
// example, which is what this is.
//
// The interface is deliberately SYNCHRONOUS. Batching, not asynchrony, is the
// concurrency primitive: override read_batch() to service many ranges at once
// (pipelined HTTP, parallel object-store GETs). A blocking interface is
// trivially wrapped by whatever threading model you already have, whereas an
// imposed async runtime is not.
//
// The reader below is deliberately dumb -- it slurps the file into memory --
// so that the interesting part is the instrumentation: it counts requests and
// bytes, which makes the point of the format visible. Compare a bbox query
// against a full scan and watch how little of the file is touched.
#include <fcb/cityjson.hpp>
#include <fcb/range_reader.hpp>
#include <fcb/reader.hpp>

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <memory>
#include <vector>

namespace {

/// Serves an in-memory buffer, and records what was asked for.
class CountingMemoryReader : public fcb::RangeReader {
  public:
    explicit CountingMemoryReader(std::vector<std::uint8_t> bytes) : bytes_(std::move(bytes)) {}

    std::uint64_t total_size() override { return bytes_.size(); }

    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override {
        ++requests_;

        // The contract (see include/fcb/range_reader.hpp) is a SHORT READ, not
        // an exception, when the range runs past EOF -- the traversal relies
        // on that when it reads the tail of the file.
        if (offset >= bytes_.size())
            return {};
        const std::uint64_t available = bytes_.size() - offset;
        const std::uint64_t n = length < available ? length : available;
        bytes_read_ += n;

        const auto begin = bytes_.begin() + static_cast<std::ptrdiff_t>(offset);
        return std::vector<std::uint8_t>(begin, begin + static_cast<std::ptrdiff_t>(n));
    }

    // read_batch() is not overridden: the base class services requests one by
    // one through read(), which is correct, just not parallel. Override it if
    // your transport can do better -- that is the whole reason it exists.

    std::uint64_t requests() const { return requests_; }
    std::uint64_t bytes_read() const { return bytes_read_; }
    void reset_counters() {
        requests_ = 0;
        bytes_read_ = 0;
    }

  private:
    std::vector<std::uint8_t> bytes_;
    std::uint64_t requests_ = 0;
    std::uint64_t bytes_read_ = 0;
};

std::vector<std::uint8_t> slurp(const char* path) {
    std::ifstream in(path, std::ios::binary);
    if (!in)
        throw fcb::Error(fcb::ErrorCode::IoError, std::string("cannot open ") + path);
    return std::vector<std::uint8_t>((std::istreambuf_iterator<char>(in)),
                                     std::istreambuf_iterator<char>());
}

}  // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s <file.fcb> [minx miny maxx maxy]\n", argv[0]);
        return 2;
    }
    try {
        auto transport = std::make_shared<CountingMemoryReader>(slurp(argv[1]));
        const std::uint64_t file_size = transport->total_size();

        // FcbReader::open takes any RangeReader. Wrapping it in a
        // BufferedRangeReader coalesces small reads into larger ones, which is
        // what makes remote access practical; it is a plain decorator, so it
        // works over a custom transport exactly as it does over HTTP.
        fcb::FcbReader reader = fcb::FcbReader::open(transport);
        const auto& info = reader.header().info();
        std::fprintf(stderr, "opened: %llu features, %llu bytes, %llu request(s) so far\n",
                     static_cast<unsigned long long>(info.features_count),
                     static_cast<unsigned long long>(file_size),
                     static_cast<unsigned long long>(transport->requests()));

        transport->reset_counters();

        fcb::FeatureIterator it =
            argc >= 6 ? reader.select_bbox(fcb::BBox{std::stod(argv[2]), std::stod(argv[3]),
                                                     std::stod(argv[4]), std::stod(argv[5])})
                      : reader.select_all();

        unsigned long long n = 0;
        while (it.next()) {
            std::printf("%s\n", it.current().id().c_str());
            ++n;
        }

        const double pct = file_size == 0 ? 0.0
                                          : 100.0 * static_cast<double>(transport->bytes_read()) /
                                                static_cast<double>(file_size);
        std::fprintf(stderr, "%llu feature(s); %llu read(s), %llu bytes (%.1f%% of the file)\n", n,
                     static_cast<unsigned long long>(transport->requests()),
                     static_cast<unsigned long long>(transport->bytes_read()), pct);
        return 0;
    } catch (const fcb::Error& e) {
        std::fprintf(stderr, "error: %s\n", e.what());
        return 1;
    }
}
