#include <doctest/doctest.h>

#include <fcb/cityjson.hpp>
#include <fcb/reader.hpp>

#include <nlohmann/json.hpp>

using namespace fcb;
using nlohmann::json;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

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
