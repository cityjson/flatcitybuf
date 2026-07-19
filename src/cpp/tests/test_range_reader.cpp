#include <doctest/doctest.h>

#include <fcb/range_reader.hpp>

#include "fake_range_reader.hpp"

#include <cstdio>
#include <fstream>
#include <memory>
#include <vector>

using namespace fcb;

static std::vector<std::uint8_t> iota_bytes(std::size_t n) {
    std::vector<std::uint8_t> v(n);
    for (std::size_t i = 0; i < n; ++i) v[i] = static_cast<std::uint8_t>(i & 0xFF);
    return v;
}

TEST_CASE("FileRangeReader reads exact ranges and reports total size") {
    const std::string path = "test_frr.bin";
    auto data = iota_bytes(1000);
    {
        std::ofstream f(path, std::ios::binary);
        f.write(reinterpret_cast<const char*>(data.data()), 1000);
    }

    FileRangeReader r(path);
    CHECK(r.total_size() == 1000);

    auto chunk = r.read(100, 10);
    REQUIRE(chunk.size() == 10);
    CHECK(chunk[0] == 100);
    CHECK(chunk[9] == 109);

    // A range crossing EOF returns exactly the bytes that exist.
    auto tail = r.read(995, 50);
    CHECK(tail.size() == 5);

    // Past EOF is empty, not an error.
    CHECK(r.read(2000, 10).empty());

    // Zero length never contacts the transport.
    CHECK(r.read(0, 0).empty());

    std::remove(path.c_str());
}

TEST_CASE("FileRangeReader throws on a missing file") {
    CHECK_THROWS_AS(FileRangeReader("definitely_not_a_file.bin"), Error);
}

TEST_CASE("default read_batch fills every request in order") {
    testing::FakeRangeReader fake(iota_bytes(1000));
    std::vector<RangeRequest> reqs = {{10, 4, {}}, {500, 2, {}}};
    fake.read_batch(reqs);

    REQUIRE(reqs[0].data.size() == 4);
    CHECK(reqs[0].data[0] == 10);
    REQUIRE(reqs[1].data.size() == 2);
    CHECK(reqs[1].data[0] == 244);  // 500 & 0xFF
    CHECK(fake.requests.size() == 2);
}

TEST_CASE("BufferedRangeReader over-fetches to min_req_size and serves hits from cache") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(10000));
    BufferedRangeReader buf(fake, /*min_req_size=*/1024);

    auto a = buf.read(0, 8);
    REQUIRE(a.size() == 8);
    REQUIRE(fake->requests.size() == 1);
    CHECK(fake->requests[0].offset == 0);
    CHECK(fake->requests[0].length == 1024);  // over-fetched

    // Inside the cached window -> no new upstream request.
    auto b = buf.read(500, 20);
    REQUIRE(b.size() == 20);
    CHECK(b[0] == 244);
    CHECK(fake->requests.size() == 1);

    // Outside the window -> exactly one more request.
    auto c = buf.read(5000, 4);
    REQUIRE(c.size() == 4);
    CHECK(c[0] == 136);  // 5000 & 0xFF
    CHECK(fake->requests.size() == 2);
}

TEST_CASE("BufferedRangeReader honours reads larger than min_req_size") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(10000));
    BufferedRangeReader buf(fake, /*min_req_size=*/64);

    auto big = buf.read(100, 2000);
    CHECK(big.size() == 2000);
    REQUIRE(fake->requests.size() == 1);
    CHECK(fake->requests[0].length == 2000);
}

TEST_CASE("BufferedRangeReader zero-length read never contacts the transport") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(1000));
    BufferedRangeReader buf(fake, 1024);
    CHECK(buf.read(0, 0).empty());
    CHECK(fake->requests.empty());
}

TEST_CASE("BufferedRangeReader::read_batch serves cache hits without upstream reads") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(10000));
    BufferedRangeReader buf(fake, /*min_req_size=*/1024);

    buf.read(0, 8);  // primes the cache with [0, 1024)
    REQUIRE(fake->requests.size() == 1);

    std::vector<RangeRequest> reqs = {{10, 4, {}}, {900, 4, {}}};
    buf.read_batch(reqs);  // both inside the cached window

    CHECK(fake->requests.size() == 1);  // no new upstream traffic
    REQUIRE(reqs[0].data.size() == 4);
    CHECK(reqs[0].data[0] == 10);
    REQUIRE(reqs[1].data.size() == 4);
    CHECK(reqs[1].data[0] == 132);  // 900 & 0xFF
}

TEST_CASE("BufferedRangeReader::read_batch forwards only misses, preserving order") {
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(10000));
    BufferedRangeReader buf(fake, /*min_req_size=*/1024);

    buf.read(0, 8);  // caches [0, 1024)
    fake->requests.clear();

    std::vector<RangeRequest> reqs = {{10, 4, {}}, {5000, 4, {}}, {20, 4, {}}};
    buf.read_batch(reqs);

    // Only the 5000 request should have gone upstream.
    REQUIRE(fake->requests.size() == 1);
    CHECK(fake->requests[0].offset == 5000);

    // Order preserved, every request filled, each with EXACTLY its own
    // length -- never the over-fetched block.
    REQUIRE(reqs[0].data.size() == 4);
    CHECK(reqs[0].data[0] == 10);
    REQUIRE(reqs[1].data.size() == 4);
    CHECK(reqs[1].data[0] == 136);  // 5000 & 0xFF
    REQUIRE(reqs[2].data.size() == 4);
    CHECK(reqs[2].data[0] == 20);
}

TEST_CASE("BufferedRangeReader cache coverage does not wrap on hostile offsets") {
    // An unchecked offset+length could wrap and falsely report a cache hit,
    // after which slicing builds invalid iterators.
    auto fake = std::make_shared<testing::FakeRangeReader>(iota_bytes(1000));
    BufferedRangeReader buf(fake, 512);
    buf.read(0, 8);  // prime the cache

    CHECK_THROWS_AS(buf.read(UINT64_MAX - 2, 10), Error);
}
