#include <doctest/doctest.h>

#include <fcb/reader.hpp>

#include <set>
#include <string>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("select_all visits exactly features_count features") {
    FcbReader r = FcbReader::open_file(kFixture);
    const std::uint64_t expected = r.header().info().features_count;
    REQUIRE(expected > 0);

    FeatureIterator it = r.select_all();
    CHECK(it.features_count() == expected);

    std::uint64_t seen = 0;
    while (it.next()) {
        CHECK_FALSE(it.current().id().empty());
        ++seen;
    }
    CHECK(seen == expected);
}

TEST_CASE("feature ids are non-empty and unique") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();

    std::set<std::string> ids;
    while (it.next()) {
        ids.insert(it.current().id());
    }
    CHECK(ids.size() == r.header().info().features_count);
}

TEST_CASE("every feature carries at least one CityObject") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();
    while (it.next()) {
        CHECK(it.current().city_object_count() > 0);
    }
}

TEST_CASE("a Feature outlives the iterator and reader that produced it") {
    // The backing buffer is shared-owned, so a Feature must stay valid after
    // everything that produced it is gone. Exercised through PUBLIC value
    // accessors only -- raw() is private by design.
    Feature kept;
    std::string expected_id;
    {
        FcbReader r = FcbReader::open_file(kFixture);
        FeatureIterator it = r.select_all();
        REQUIRE(it.next());
        kept = it.current();
        expected_id = kept.id();
    }
    CHECK_FALSE(kept.id().empty());
    CHECK(kept.id() == expected_id);
    CHECK(kept.city_object_count() > 0);
}

TEST_CASE("a default-constructed Feature is empty rather than dangerous") {
    Feature f;
    CHECK(f.empty());
    CHECK(f.id().empty());
    CHECK(f.city_object_count() == 0);
}

TEST_CASE("byte offsets advance monotonically from zero") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();

    REQUIRE(it.next());
    CHECK(it.current().byte_offset() == 0);  // relative to feature_begin

    std::uint64_t prev = it.current().byte_offset();
    while (it.next()) {
        CHECK(it.current().byte_offset() > prev);
        prev = it.current().byte_offset();
    }
}

TEST_CASE("opening a non-FCB file throws rather than misbehaving") {
    CHECK_THROWS_AS(FcbReader::open_file(FCB_TEST_DATA_DIR "/delft.city.jsonl"), Error);
}
