#include <fcb/header.hpp>
#include <fcb/range_reader.hpp>

#include <memory>
#include <vector>

#include <doctest/doctest.h>

#include "fake_range_reader.hpp"

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("read_header parses the committed delft fixture") {
    auto r = std::make_shared<FileRangeReader>(kFixture);
    HeaderView h = read_header(r);

    CHECK(h.info().features_count > 0);
    CHECK_FALSE(h.info().columns.empty());
    CHECK_FALSE(h.info().cityjson_version.empty());
    CHECK(h.layout().header_len > 12);
    CHECK(h.layout().feature_begin >= h.layout().attr_index_begin);
    CHECK(h.layout().feature_begin < r->total_size());
}

TEST_CASE("read_header rejects a file with bad magic") {
    std::vector<std::uint8_t> junk(64, 0xAB);
    auto fake = std::make_shared<testing::FakeRangeReader>(junk);

    try {
        read_header(fake);
        FAIL("expected read_header to throw");
    } catch (const Error& e) {
        CHECK(e.code() == ErrorCode::MissingMagicBytes);
    }
}

TEST_CASE("read_header rejects a truncated file") {
    // Valid magic, but nothing after it.
    std::vector<std::uint8_t> stub = {'f', 'c', 'b', 1, 'f', 'c', 'b', 0};
    auto fake = std::make_shared<testing::FakeRangeReader>(stub);
    CHECK_THROWS_AS(read_header(fake), Error);
}

TEST_CASE("attribute index entries carry absolute begin offsets") {
    auto r = std::make_shared<FileRangeReader>(kFixture);
    HeaderView h = read_header(r);

    std::uint64_t expected = h.layout().attr_index_begin;
    for (const auto& ai : h.attr_indices()) {
        CHECK(ai.begin == expected);
        expected += ai.length;
    }
    // The attribute indexes must exactly fill the gap up to the features.
    CHECK(expected == h.layout().feature_begin);
}

TEST_CASE("feature_begin points at a plausible size-prefixed feature") {
    // An independent check of the section arithmetic: derive feature_begin
    // from the header, then confirm the bytes there actually look like a
    // size-prefixed FlatBuffer that fits inside the file. Comparing the
    // cumulative attribute offsets against each other only proves internal
    // consistency; this proves we landed in the right place.
    auto r = std::make_shared<FileRangeReader>(kFixture);
    HeaderView h = read_header(r);

    const std::uint64_t begin = h.layout().feature_begin;
    auto prefix = r->read(begin, 4);
    REQUIRE(prefix.size() == 4);

    const std::uint32_t len = static_cast<std::uint32_t>(prefix[0]) |
                              (static_cast<std::uint32_t>(prefix[1]) << 8) |
                              (static_cast<std::uint32_t>(prefix[2]) << 16) |
                              (static_cast<std::uint32_t>(prefix[3]) << 24);

    CHECK(len > 0);
    CHECK(len < kMaxFeatureSize);
    CHECK(begin + 4 + len <= r->total_size());
}

TEST_CASE("HeaderView keeps its buffer alive independently of the reader") {
    HeaderView kept = [] {
        auto r = std::make_shared<FileRangeReader>(kFixture);
        return read_header(r);
    }();
    // Reader is gone; the parsed view must still be usable.
    CHECK(kept.info().features_count > 0);
    CHECK_FALSE(kept.info().cityjson_version.empty());
}
