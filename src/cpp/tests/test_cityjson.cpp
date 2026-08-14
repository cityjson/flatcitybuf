#include <fcb/cityjson.hpp>
#include <fcb/generated/header_generated.h>
#include <fcb/header.hpp>
#include <fcb/layout.hpp>
#include <fcb/reader.hpp>

#include <nlohmann/json.hpp>

#include <memory>
#include <vector>

#include <doctest/doctest.h>

#include "fake_range_reader.hpp"

using namespace fcb;
using nlohmann::json;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

namespace {

/// The schema-optional header strings a test may want to vary. A nullptr
/// member means the FlatBuffers field is genuinely ABSENT; `""` means it is
/// PRESENT but empty. Rust distinguishes the two -- every one of these is read
/// through an `Option<&str>` accessor -- so the tests below must be able to
/// express both.
struct HeaderStrings {
    const char* identifier = nullptr;
    const char* reference_date = nullptr;
    const char* title = nullptr;
    const char* poc_contact_name = nullptr;
    const char* poc_contact_type = nullptr;
    const char* poc_role = nullptr;
    const char* poc_phone = nullptr;
    const char* poc_email = nullptr;
    const char* poc_website = nullptr;
};

/// Builds a minimal, well-formed .fcb byte stream containing nothing but a
/// header -- no features, no R-tree, no attribute index -- so header-metadata
/// gaps can be exercised without a full fixture. `version` is `(required)` in
/// header.fbs, so it must always be set for the FlatBuffers verifier to accept
/// the buffer.
std::vector<std::uint8_t> build_header_only_file(const HeaderStrings& s) {
    flatbuffers::FlatBufferBuilder fbb;
    auto header = CreateHeaderDirect(fbb,
                                     /*transform=*/nullptr,
                                     /*appearance=*/0,
                                     /*columns=*/nullptr,
                                     /*semantic_columns=*/nullptr,
                                     /*features_count=*/0,
                                     /*index_node_size=*/16,
                                     /*attribute_index=*/nullptr,
                                     /*geographical_extent=*/nullptr,
                                     /*reference_system=*/0,
                                     /*identifier=*/s.identifier,
                                     /*reference_date=*/s.reference_date,
                                     /*title=*/s.title,
                                     /*templates=*/nullptr,
                                     /*templates_vertices=*/nullptr,
                                     /*extensions=*/nullptr,
                                     /*poc_contact_name=*/s.poc_contact_name,
                                     /*poc_contact_type=*/s.poc_contact_type,
                                     /*poc_role=*/s.poc_role,
                                     /*poc_phone=*/s.poc_phone,
                                     /*poc_email=*/s.poc_email,
                                     /*poc_website=*/s.poc_website,
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
    HeaderStrings hs;
    hs.poc_contact_name = "Test Team";
    hs.poc_email = nullptr;  // genuinely ABSENT
    auto bytes = build_header_only_file(hs);
    auto reader = std::make_shared<testing::FakeRangeReader>(bytes);
    HeaderView header = read_header(reader);

    REQUIRE(header.info().poc_contact_name == "Test Team");
    REQUIRE_FALSE(header.info().poc_email.has_value());

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
    HeaderStrings hs;
    hs.poc_contact_name = "Test Team";
    hs.poc_email = "";  // PRESENT but empty
    auto bytes = build_header_only_file(hs);
    auto reader = std::make_shared<testing::FakeRangeReader>(bytes);
    HeaderView header = read_header(reader);

    REQUIRE(header.info().poc_contact_name == "Test Team");
    REQUIRE(header.info().poc_email.has_value());
    REQUIRE(header.info().poc_email->empty());

    json cj = to_cityjson_metadata(header);
    REQUIRE(cj.contains("metadata"));
    REQUIRE(cj["metadata"].contains("pointOfContact"));
    CHECK(cj["metadata"]["pointOfContact"]["contactName"] == "Test Team");
    CHECK(cj["metadata"]["pointOfContact"]["emailAddress"] == "");
}

TEST_CASE("present-but-empty identifier/referenceDate/title are EMITTED as \"\"") {
    // Rust gates every one of these on PRESENCE, not on emptiness:
    // `identifier: header.identifier().map(|i| i.to_string())` and the same
    // shape for reference_date/title (deserializer.rs:86-93). A flatbuffer
    // string that is present yet empty is `Some("")`, so `.map` yields
    // `Some("".to_string())` and serde emits `"identifier": ""`. Gating on
    // `.empty()` silently drops a key the oracle keeps -- upstream finding
    // #20.11.
    HeaderStrings hs;
    hs.identifier = "";
    hs.reference_date = "";
    hs.title = "";
    auto bytes = build_header_only_file(hs);
    auto reader = std::make_shared<testing::FakeRangeReader>(bytes);
    HeaderView header = read_header(reader);

    REQUIRE(header.info().identifier.has_value());
    REQUIRE(header.info().reference_date.has_value());
    REQUIRE(header.info().title.has_value());

    json cj = to_cityjson_metadata(header);
    REQUIRE(cj.contains("metadata"));
    const auto& meta = cj["metadata"];
    REQUIRE(meta.contains("identifier"));
    CHECK(meta["identifier"] == "");
    REQUIRE(meta.contains("referenceDate"));
    CHECK(meta["referenceDate"] == "");
    REQUIRE(meta.contains("title"));
    CHECK(meta["title"] == "");
}

TEST_CASE("absent identifier/referenceDate/title are OMITTED") {
    // The other half of the distinction above: `None` from the accessor makes
    // `.map` yield `None`, and cjseq2's `skip_serializing_if = "Option::
    // is_none"` drops the key entirely. Without this case the fix above could
    // be "emit unconditionally", which is equally wrong.
    auto bytes = build_header_only_file(HeaderStrings{});
    auto reader = std::make_shared<testing::FakeRangeReader>(bytes);
    HeaderView header = read_header(reader);

    REQUIRE_FALSE(header.info().identifier.has_value());
    REQUIRE_FALSE(header.info().reference_date.has_value());
    REQUIRE_FALSE(header.info().title.has_value());

    json cj = to_cityjson_metadata(header);
    REQUIRE(cj.contains("metadata"));
    const auto& meta = cj["metadata"];
    CHECK_FALSE(meta.contains("identifier"));
    CHECK_FALSE(meta.contains("referenceDate"));
    CHECK_FALSE(meta.contains("title"));
}

TEST_CASE("a present-but-empty poc_contact_name still produces a pointOfContact") {
    // `match header.poc_contact_name() { Some(_) => Some(to_cj_point_of_
    // contact(header)?), None => None }` (deserializer.rs:81-84) branches on
    // the Option, not on the string's content, and `contact_name` is then a
    // plain required String -- so `""` present yields a pointOfContact whose
    // contactName is `""`. The remaining optional members follow the same
    // `.map()` rule (deserializer.rs:184-192): present-but-empty is emitted.
    HeaderStrings hs;
    hs.poc_contact_name = "";
    hs.poc_contact_type = "";
    hs.poc_role = "";
    hs.poc_phone = "";
    hs.poc_email = "a@b.c";
    hs.poc_website = "";
    auto bytes = build_header_only_file(hs);
    auto reader = std::make_shared<testing::FakeRangeReader>(bytes);
    HeaderView header = read_header(reader);

    REQUIRE(header.info().poc_contact_name.has_value());
    REQUIRE(header.info().poc_contact_name->empty());

    json cj = to_cityjson_metadata(header);
    REQUIRE(cj["metadata"].contains("pointOfContact"));
    const auto& poc = cj["metadata"]["pointOfContact"];
    CHECK(poc["contactName"] == "");
    CHECK(poc["emailAddress"] == "a@b.c");
    REQUIRE(poc.contains("contactType"));
    CHECK(poc["contactType"] == "");
    REQUIRE(poc.contains("role"));
    CHECK(poc["role"] == "");
    REQUIRE(poc.contains("phone"));
    CHECK(poc["phone"] == "");
    REQUIRE(poc.contains("website"));
    CHECK(poc["website"] == "");
    // The address model is unchanged: Rust's `to_cj_address` emits each member
    // iff non-empty and the whole sub-object only when at least one survives,
    // so all-absent means no `address` key.
    CHECK_FALSE(poc.contains("address"));
}

TEST_CASE("an absent poc_contact_name suppresses pointOfContact entirely") {
    HeaderStrings hs;
    hs.poc_email = "a@b.c";
    auto bytes = build_header_only_file(hs);
    auto reader = std::make_shared<testing::FakeRangeReader>(bytes);
    HeaderView header = read_header(reader);

    REQUIRE_FALSE(header.info().poc_contact_name.has_value());
    json cj = to_cityjson_metadata(header);
    CHECK_FALSE(cj["metadata"].contains("pointOfContact"));
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
            if (co.contains("geometry"))
                ++with_geom;
            if (co.contains("attributes"))
                ++with_attrs;
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
            if (!co.contains("geometry"))
                continue;
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

TEST_CASE("city object type names cover the enum") {
    CHECK(city_object_type_name(6) == "Building");
    CHECK(city_object_type_name(7) == "BuildingPart");
    CHECK(city_object_type_name(32) == "WaterBody");
}

// UNKNOWN-TAG POLICY. Tag 33 is CityObjectType::ExtensionObject, reached only
// when the object's `extension_type` string is absent; anything above it is a
// tag a newer encoder added. Both used to throw here, and both used to be
// spelled "ExtensionObject" by the table -- a string that is not a CityJSON
// City Object type and carries no '+', so a document containing it fails
// validation. Emitting a schema-invalid document is as much a defect as
// rejecting a valid one, so the reader now emits the same '+'-prefixed
// placeholder the Rust reader does (deserializer.rs::to_cj_co_type).
TEST_CASE("an unnameable city object tag becomes a schema-valid Extension name") {
    for (std::uint8_t tag : {std::uint8_t{33}, std::uint8_t{34}, std::uint8_t{200}}) {
        const auto name = city_object_type_name(tag);
        CHECK(name == "+UnknownCityObject");
        // the property that matters: an Extension type must start with '+'
        REQUIRE(!name.empty());
        CHECK(name[0] == '+');
    }
    // and it is never the FlatBuffers enumerator name
    CHECK(city_object_type_name(33) != "ExtensionObject");
}

TEST_CASE("semantic surface type names cover the enum") {
    CHECK(semantic_surface_type_name(0) == "RoofSurface");
    CHECK(semantic_surface_type_name(6) == "Window");
    CHECK(semantic_surface_type_name(17) == "TransportationHole");
}

// UNKNOWN-TAG POLICY, the semantic-surface half. Tag 18 is
// SemanticSurfaceType::ExtraSemanticSurface. CityJSON section 3.3 says
// "it is possible to define and use other semantics, but these have to start
// with a '+'", so the placeholder must carry one; "ExtraSemanticSurface",
// which this used to emit, does not and is not a CityJSON surface type.
TEST_CASE("an unnameable semantic surface tag becomes a schema-valid Extension name") {
    for (std::uint8_t tag : {std::uint8_t{18}, std::uint8_t{19}, std::uint8_t{200}}) {
        const auto name = semantic_surface_type_name(tag);
        CHECK(name == "+GenericSurface");
        REQUIRE(!name.empty());
        CHECK(name[0] == '+');
    }
    CHECK(semantic_surface_type_name(18) != "ExtraSemanticSurface");
}
