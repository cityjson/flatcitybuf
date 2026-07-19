#ifdef FCB_WITH_CURL

#include <doctest/doctest.h>

#include <fcb/cityjson.hpp>
#include <fcb/http/curl_range_reader.hpp>
#include <fcb/reader.hpp>

#include <cstdlib>
#include <memory>
#include <set>
#include <string>

using namespace fcb;

/// Set by run_http_tests.cmake to a loopback URL serving delft.fcb.
static std::string fixture_url() {
    const char* u = std::getenv("FCB_TEST_HTTP_URL");
    return u != nullptr ? std::string(u) : std::string();
}

#define SKIP_IF_NO_SERVER(url)                                  \
    if ((url).empty()) {                                        \
        MESSAGE("FCB_TEST_HTTP_URL not set; skipping");         \
        return;                                                 \
    }

TEST_CASE("CurlRangeReader reports total size") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    CurlRangeReader r(url);
    CHECK(r.total_size() == 7668160);
}

TEST_CASE("remote reads return the same bytes as the local file") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    FileRangeReader local(FCB_TEST_DATA_DIR "/delft.fcb");
    CurlRangeReader remote(url);

    CHECK(remote.total_size() == local.total_size());
    CHECK(remote.read(0, 64) == local.read(0, 64));
    CHECK(remote.read(1000, 256) == local.read(1000, 256));

    // The tail: exercises a range that ends exactly at EOF.
    const std::uint64_t n = local.total_size();
    CHECK(remote.read(n - 16, 16) == local.read(n - 16, 16));
}

TEST_CASE("a zero-length read never contacts the server") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    CurlRangeReader r(url);
    r.total_size();
    r.reset_request_count();
    CHECK(r.read(100, 0).empty());
    CHECK(r.request_count() == 0);
}

TEST_CASE("reading past the end returns empty rather than throwing") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    CurlRangeReader r(url);
    const std::uint64_t n = r.total_size();
    CHECK(r.read(n + 1000, 16).empty());
}

TEST_CASE("a range crossing EOF returns exactly the bytes that exist") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    CurlRangeReader r(url);
    const std::uint64_t n = r.total_size();
    CHECK(r.read(n - 10, 100).size() == 10);
}

TEST_CASE("a server ignoring Range still yields the correct slice") {
    // The dangerous case: answering 200 with the whole body. Truncating to
    // `length` would return bytes [0,length) instead of [offset,offset+length).
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    FileRangeReader local(FCB_TEST_DATA_DIR "/delft.fcb");
    CurlRangeReader ignoring(url + "?ignore_range=1");

    CHECK(ignoring.read(1000, 32) == local.read(1000, 32));
    CHECK(ignoring.read(0, 8) == local.read(0, 8));
}

TEST_CASE("a malformed Content-Range is rejected") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    CurlRangeReader bad(url + "?bad_range=1");
    CHECK_THROWS_AS(bad.read(100, 16), Error);
}

TEST_CASE("a server answering a different range than requested is rejected") {
    // Returning the wrong offset silently would corrupt every downstream
    // read, so the client must verify Content-Range's start.
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    CurlRangeReader wrong(url + "?wrong_offset=1");
    CHECK_THROWS_AS(wrong.read(100, 64), Error);
}

TEST_CASE("opening a remote file over HTTP parses the same header") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    FcbReader local = FcbReader::open_file(FCB_TEST_DATA_DIR "/delft.fcb");
    FcbReader remote = FcbReader::open(std::make_shared<CurlRangeReader>(url));

    CHECK(remote.header().info().features_count == local.header().info().features_count);
    CHECK(remote.header().info().cityjson_version == local.header().info().cityjson_version);
    CHECK(remote.header().layout().feature_begin == local.header().layout().feature_begin);
}

TEST_CASE("opening a remote file costs a bounded number of requests") {
    // The whole point of the 12944-byte prefetch: header plus the top R-tree
    // levels should arrive in essentially one round trip.
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    auto r = std::make_shared<CurlRangeReader>(url);
    r->total_size();
    r->reset_request_count();

    FcbReader reader = FcbReader::open(r);
    CHECK(reader.header().info().features_count > 0);
    CHECK(r->request_count() <= 2);
}

TEST_CASE("a bbox query over HTTP returns the same ids as over the local file") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    auto ids_from = [](std::shared_ptr<RangeReader> rr) {
        FcbReader r = FcbReader::open(std::move(rr));
        const auto& info = r.header().info();
        const double mid_x =
            (info.geographical_extent[0] + info.geographical_extent[3]) / 2.0;
        BBox half{info.geographical_extent[0], info.geographical_extent[1], mid_x,
                  info.geographical_extent[4]};
        FeatureIterator it = r.select_bbox(half);
        std::set<std::string> ids;
        while (it.next()) ids.insert(it.current().id());
        return ids;
    };

    auto local = ids_from(std::make_shared<FileRangeReader>(FCB_TEST_DATA_DIR "/delft.fcb"));
    auto remote = ids_from(std::make_shared<CurlRangeReader>(url));
    CHECK_FALSE(local.empty());
    CHECK(local == remote);
}

TEST_CASE("CityJSON emitted over HTTP matches the local file exactly") {
    const std::string url = fixture_url();
    SKIP_IF_NO_SERVER(url);

    FcbReader local = FcbReader::open_file(FCB_TEST_DATA_DIR "/delft.fcb");
    FcbReader remote = FcbReader::open(std::make_shared<CurlRangeReader>(url));

    auto lit = local.select_all();
    auto rit = remote.select_all();

    int checked = 0;
    while (checked < 25 && lit.next() && rit.next()) {
        CHECK(to_cityjson_feature(lit.current(), local.header()) ==
              to_cityjson_feature(rit.current(), remote.header()));
        ++checked;
    }
    CHECK(checked == 25);
}

#endif  // FCB_WITH_CURL
