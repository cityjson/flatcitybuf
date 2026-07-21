#include <doctest/doctest.h>

#include <fcb/cityjson.hpp>
#include <fcb/generated/header_generated.h>
#include <fcb/header.hpp>
#include <fcb/layout.hpp>
#include <fcb/reader.hpp>

#include "fake_range_reader.hpp"

#include <nlohmann/json.hpp>

#include <memory>
#include <vector>

using namespace fcb;
using nlohmann::json;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

namespace {

/// Builds a minimal, well-formed .fcb byte stream containing nothing but a
/// header -- no features, no R-tree, no attribute index -- so a
/// `pointOfContact` gap can be exercised without a full fixture. `version`
/// is `(required)` in header.fbs, so it must always be set for the
/// FlatBuffers verifier to accept the buffer.
std::vector<std::uint8_t> build_header_only_file(const char* poc_contact_name,
                                                  const char* poc_email) {
    flatbuffers::FlatBufferBuilder fbb;
    auto header = CreateHeaderDirect(
        fbb,
        /*transform=*/nullptr,
        /*appearance=*/0,
        /*columns=*/nullptr,
        /*semantic_columns=*/nullptr,
        /*features_count=*/0,
        /*index_node_size=*/16,
        /*attribute_index=*/nullptr,
        /*geographical_extent=*/nullptr,
        /*reference_system=*/0,
        /*identifier=*/nullptr,
        /*reference_date=*/nullptr,
        /*title=*/nullptr,
        /*templates=*/nullptr,
        /*templates_vertices=*/nullptr,
        /*extensions=*/nullptr,
        /*poc_contact_name=*/poc_contact_name,
        /*poc_contact_type=*/nullptr,
        /*poc_role=*/nullptr,
        /*poc_phone=*/nullptr,
        /*poc_email=*/poc_email,
        /*poc_website=*/nullptr,
        /*poc_address_thoroughfare_number=*/nullptr,
        /*poc_address_thoroughfare_name=*/nullptr,
        /*poc_address_locality=*/nullptr,
        /*poc_address_postcode=*/nullptr,
        /*poc_address_country=*/nullptr,
        /*attributes=*/nullptr,
        /*version=*/"2.0");
    FinishSizePrefixedHeaderBuffer(fbb, header);

    std::vector<std::uint8_t> file_bytes = {'f', 'c', 'b', kVersion, 'f', 'c', 'b', kVersion};
    const auto* body = fbb.GetBufferPointer();
    file_bytes.insert(file_bytes.end(), body, body + fbb.GetSize());
    return file_bytes;
}

}  // namespace

TEST_CASE("metadata emits a valid CityJSON envelope") {
    FcbReader r = FcbReader::open_file(kFixture);
    json cj = to_cityjson_metadata(r.header());

    CHECK(cj["type"] == "CityJSON");
    CHECK(cj["version"] == "2.0");
    REQUIRE(cj.contains("transform"));
    CHECK(cj["transform"]["scale"].size() == 3);
    CHECK(cj["transform"]["translate"].size() == 3);
    REQUIRE(cj.contains("metadata"));
    CHECK(cj["metadata"]["geographicalExtent"].size() == 6);
}

TEST_CASE("metadata from an empty header does not dereference a null buffer") {
    // A default-constructed HeaderView owns no bytes. Template decoding
    // reaches for the raw FlatBuffers header, so this path has to survive.
    json cj = to_cityjson_metadata(HeaderView{});
    CHECK(cj["type"] == "CityJSON");
    CHECK_FALSE(cj.contains("geometry-templates"));
}

TEST_CASE("transform is emitted even when the header carries none") {
    // cjseq2's CityJSON::transform is a non-Option field: to_cj_metadata
    // (deserializer.rs:22-31) starts from CityJSON::new(), whose
    // Transform::new() defaults to scale [1,1,1] / translate [0,0,0]
    // (cjseq2 lib.rs:1053-1064), and only overwrites it when
    // header.transform() is Some. Rust never omits the key. A
    // default-constructed HeaderView has has_transform == false, exactly
    // the "no transform in the file" case.
    json cj = to_cityjson_metadata(HeaderView{});
    REQUIRE(cj.contains("transform"));
    CHECK(cj["transform"]["scale"] == json::array({1.0, 1.0, 1.0}));
    CHECK(cj["transform"]["translate"] == json::array({0.0, 0.0, 0.0}));
}

TEST_CASE("pointOfContact without emailAddress fails like Rust's required field") {
    // cjseq2's PointOfContact::email_address is a required String (no
    // skip_serializing_if), and to_cj_point_of_contact (deserializer.rs:
    // 175-177) does `.ok_or(Error::MissingRequiredField("email_address"))?`,
    // which propagates out of the whole to_cj_metadata call. poc_contact_name
    // and poc_email are independently optional flatbuffer fields
    // (header.fbs:151,155), so a header can legally carry one without the
    // other -- and when it does, Rust fails the entire metadata line rather
    // than silently omitting emailAddress.
    auto bytes = build_header_only_file("Test Team", /*poc_email=*/nullptr);
    auto reader = std::make_shared<testing::FakeRangeReader>(bytes);
    HeaderView header = read_header(reader);

    REQUIRE(header.info().poc_contact_name == "Test Team");
    REQUIRE(header.info().poc_email.empty());

    try {
        to_cityjson_metadata(header);
        FAIL("expected to_cityjson_metadata to throw");
    } catch (const Error& e) {
        CHECK(e.code() == ErrorCode::MissingRequiredField);
    }
}

TEST_CASE("pointOfContact with a present-but-empty emailAddress does not throw") {
    // cjseq2's PointOfContact::email_address is required, but Rust's gate is
    // `header.poc_email().ok_or(...)` -- a flatbuffer string that is present
    // yet empty is `Some("")`, which satisfies `ok_or` and yields
    // `email_address: ""`. Only a genuinely ABSENT poc_email is `None` and
    // throws (covered by the test above). `""` (a non-null, zero-length C
    // string) makes the header builder emit a present-but-empty flatbuffer
    // string, exactly like the Rust oracle would accept.
    auto bytes = build_header_only_file("Test Team", /*poc_email=*/"");
    auto reader = std::make_shared<testing::FakeRangeReader>(bytes);
    HeaderView header = read_header(reader);

    REQUIRE(header.info().poc_contact_name == "Test Team");
    REQUIRE(header.info().has_poc_email);
    REQUIRE(header.info().poc_email.empty());

    json cj = to_cityjson_metadata(header);
    REQUIRE(cj.contains("metadata"));
    REQUIRE(cj["metadata"].contains("pointOfContact"));
    CHECK(cj["metadata"]["pointOfContact"]["contactName"] == "Test Team");
    CHECK(cj["metadata"]["pointOfContact"]["emailAddress"] == "");
}

TEST_CASE("a feature emits a valid CityJSONFeature") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();
    REQUIRE(it.next());

    json f = to_cityjson_feature(it.current(), r.header());

    CHECK(f["type"] == "CityJSONFeature");
    CHECK_FALSE(f["id"].get<std::string>().empty());
    REQUIRE(f.contains("CityObjects"));
    CHECK(f["CityObjects"].is_object());
    CHECK_FALSE(f["CityObjects"].empty());

    REQUIRE(f.contains("vertices"));
    CHECK(f["vertices"].is_array());
    for (const auto& v : f["vertices"]) {
        REQUIRE(v.is_array());
        CHECK(v.size() == 3);
        CHECK(v[0].is_number_integer());  // quantised, not floating point
    }
}

TEST_CASE("every feature in the file emits without error") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();

    std::uint64_t n = 0, with_geom = 0, with_attrs = 0;
    while (it.next()) {
        json f = to_cityjson_feature(it.current(), r.header());
        CHECK(f["type"] == "CityJSONFeature");
        for (const auto& co : f["CityObjects"]) {
            if (co.contains("geometry")) ++with_geom;
            if (co.contains("attributes")) ++with_attrs;
        }
        ++n;
    }
    CHECK(n == r.header().info().features_count);
    CHECK(with_geom > 0);
    CHECK(with_attrs > 0);
}

TEST_CASE("object types are real CityJSON names") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();
    REQUIRE(it.next());

    json f = to_cityjson_feature(it.current(), r.header());
    for (auto& [id, co] : f["CityObjects"].items()) {
        const std::string t = co["type"];
        CHECK((t == "Building" || t == "BuildingPart" || t.rfind("+", 0) == 0));
    }
}

TEST_CASE("geometry boundaries reach vertex indices at some depth") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();

    bool checked = false;
    while (it.next() && !checked) {
        json f = to_cityjson_feature(it.current(), r.header());
        const std::size_t nverts = f["vertices"].size();
        for (const auto& co : f["CityObjects"]) {
            if (!co.contains("geometry")) continue;
            for (const auto& g : co["geometry"]) {
                CHECK(g.contains("boundaries"));
                CHECK(g.contains("type"));
                // Descend to the first integer and check it indexes a vertex.
                const json* cur = &g["boundaries"];
                while (cur->is_array() && !cur->empty() && (*cur)[0].is_array()) {
                    cur = &(*cur)[0];
                }
                if (cur->is_array() && !cur->empty() && (*cur)[0].is_number_integer()) {
                    CHECK((*cur)[0].get<std::size_t>() < nverts);
                    checked = true;
                }
            }
        }
    }
    CHECK(checked);
}

TEST_CASE("city object type names cover the enum and reject nonsense") {
    CHECK(city_object_type_name(6) == "Building");
    CHECK(city_object_type_name(7) == "BuildingPart");
    CHECK_THROWS_AS(city_object_type_name(200), Error);
}
