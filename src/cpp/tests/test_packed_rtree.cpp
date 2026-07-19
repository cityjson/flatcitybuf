#include <doctest/doctest.h>

#include <fcb/packed_rtree.hpp>
#include <fcb/reader.hpp>

#include <cstring>
#include <set>
#include <string>
#include <vector>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("NodeItem decodes 40 little-endian bytes") {
    std::vector<std::uint8_t> raw(40, 0);
    const std::uint8_t one[8] = {0, 0, 0, 0, 0, 0, 0xF0, 0x3F};  // 1.0
    std::memcpy(raw.data() + 0, one, 8);
    std::memcpy(raw.data() + 8, one, 8);
    std::memcpy(raw.data() + 16, one, 8);
    std::memcpy(raw.data() + 24, one, 8);
    raw[32] = 0x2A;  // offset = 42

    NodeItem n = NodeItem::decode(bytes_view(raw));
    CHECK(n.min_x == doctest::Approx(1.0));
    CHECK(n.max_y == doctest::Approx(1.0));
    CHECK(n.offset == 42U);
    CHECK(NodeItem::kSize == 40);
}

TEST_CASE("intersects matches Rust NodeItem::intersects boundary semantics") {
    // packed_rtree/mod.rs:122-134 uses strict < and >, so TOUCHING edges
    // do intersect. An inclusive/exclusive slip here silently changes every
    // query result, so pin the boundary cases explicitly.
    NodeItem n{0.0, 0.0, 10.0, 10.0, 0};

    CHECK(n.intersects(BBox{5.0, 5.0, 6.0, 6.0}));       // fully inside
    CHECK(n.intersects(BBox{-5.0, -5.0, 5.0, 5.0}));     // overlapping
    CHECK(n.intersects(BBox{-5.0, -5.0, 20.0, 20.0}));   // enclosing
    CHECK(n.intersects(BBox{10.0, 10.0, 20.0, 20.0}));   // corner touch
    CHECK(n.intersects(BBox{-5.0, -5.0, 0.0, 0.0}));     // corner touch

    CHECK_FALSE(n.intersects(BBox{10.1, 0.0, 20.0, 10.0}));   // past max_x
    CHECK_FALSE(n.intersects(BBox{-20.0, 0.0, -0.1, 10.0}));  // before min_x
    CHECK_FALSE(n.intersects(BBox{0.0, 10.1, 10.0, 20.0}));   // past max_y
    CHECK_FALSE(n.intersects(BBox{0.0, -20.0, 10.0, -0.1}));  // before min_y
}

TEST_CASE("rtree_num_nodes matches the Rust level-bounds loop") {
    // Loop breaks at n == 1 (unlike the B+tree, which breaks at
    // n < branching_factor).
    CHECK(rtree_num_nodes(1, 16) == 2);
    CHECK(rtree_num_nodes(16, 16) == 17);
    CHECK(rtree_num_nodes(17, 16) == 20);
    CHECK(rtree_num_nodes(257, 16) == 277);
    CHECK(rtree_num_nodes(1115, 16) == 1191);  // the delft fixture
}

TEST_CASE("a bbox covering the whole extent returns every feature") {
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& info = r.header().info();
    REQUIRE(info.has_extent);

    BBox all{info.geographical_extent[0], info.geographical_extent[1],
             info.geographical_extent[3], info.geographical_extent[4]};

    FeatureIterator it = r.select_bbox(all);
    std::uint64_t seen = 0;
    while (it.next()) ++seen;
    CHECK(seen == info.features_count);
}

TEST_CASE("a bbox far outside the extent returns nothing") {
    FcbReader r = FcbReader::open_file(kFixture);
    BBox none{-1e9, -1e9, -1e9 + 1.0, -1e9 + 1.0};

    FeatureIterator it = r.select_bbox(none);
    std::uint64_t seen = 0;
    while (it.next()) ++seen;
    CHECK(seen == 0);
}

TEST_CASE("a quarter bbox returns a strict non-empty subset") {
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& info = r.header().info();
    const double mid_x = (info.geographical_extent[0] + info.geographical_extent[3]) / 2.0;
    const double mid_y = (info.geographical_extent[1] + info.geographical_extent[4]) / 2.0;

    BBox quarter{info.geographical_extent[0], info.geographical_extent[1], mid_x, mid_y};
    FeatureIterator it = r.select_bbox(quarter);
    std::uint64_t seen = 0;
    while (it.next()) ++seen;

    CHECK(seen > 0);
    CHECK(seen < info.features_count);
}

TEST_CASE("bbox results are a subset of the full scan, by id") {
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& info = r.header().info();

    std::set<std::string> all_ids;
    {
        FeatureIterator it = r.select_all();
        while (it.next()) all_ids.insert(it.current().id());
    }

    const double mid_x = (info.geographical_extent[0] + info.geographical_extent[3]) / 2.0;
    BBox half{info.geographical_extent[0], info.geographical_extent[1], mid_x,
              info.geographical_extent[4]};

    FeatureIterator it = r.select_bbox(half);
    std::uint64_t n = 0;
    while (it.next()) {
        CHECK(all_ids.count(it.current().id()) == 1);
        ++n;
    }
    CHECK(n > 0);
}

TEST_CASE("the last feature in the file is reachable by bbox query") {
    // Guards the final-leaf edge case: the last leaf has no successor
    // offset, so its length must come from its own 4-byte prefix.
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& info = r.header().info();
    BBox all{info.geographical_extent[0], info.geographical_extent[1],
             info.geographical_extent[3], info.geographical_extent[4]};

    FeatureIterator it = r.select_bbox(all);
    std::string last_id;
    while (it.next()) last_id = it.current().id();
    CHECK_FALSE(last_id.empty());
}
