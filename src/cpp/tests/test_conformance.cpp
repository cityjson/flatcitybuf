#include <doctest/doctest.h>

#include <fcb/cityjson.hpp>
#include <fcb/reader.hpp>

#include <nlohmann/json.hpp>

#include <fstream>
#include <string>
#include <vector>

using namespace fcb;
using nlohmann::json;

static std::vector<json> read_jsonl(const std::string& path) {
    std::vector<json> out;
    std::ifstream f(path);
    REQUIRE_MESSAGE(f.good(), "cannot open " << path);
    std::string line;
    while (std::getline(f, line)) {
        if (!line.empty()) out.push_back(json::parse(line));
    }
    return out;
}

/// Compare C++ output against the RUST READER's output for the same file.
///
/// Comparing parsed trees, never text: key order and float formatting
/// legitimately differ between implementations and are not defects.
static void check_case(const std::string& name) {
    CAPTURE(name);
    const std::string base = std::string(FCB_CONFORMANCE_DIR) + "/" + name;

    std::vector<json> expected = read_jsonl(base + ".expected.jsonl");
    REQUIRE_FALSE(expected.empty());

    FcbReader r = FcbReader::open_file(base + ".fcb");

    std::vector<json> actual;
    actual.push_back(to_cityjson_metadata(r.header()));
    FeatureIterator it = r.select_all();
    while (it.next()) actual.push_back(to_cityjson_feature(it.current(), r.header()));

    REQUIRE(actual.size() == expected.size());

    // Line 0 is the metadata envelope. We deliberately do not reproduce
    // every optional field the Rust writer round-trips (point-of-contact
    // and similar), so compare the parts a reader must get right.
    CHECK(actual[0]["type"] == expected[0]["type"]);
    CHECK(actual[0]["version"] == expected[0]["version"]);
    if (expected[0].contains("transform")) {
        CHECK(actual[0]["transform"] == expected[0]["transform"]);
    }
    // Geometry templates live only on line 0; the features reference them by
    // index, so a mismatch here silently corrupts every GeometryInstance.
    CHECK(actual[0].contains("geometry-templates") ==
          expected[0].contains("geometry-templates"));
    if (expected[0].contains("geometry-templates")) {
        CHECK(actual[0]["geometry-templates"] == expected[0]["geometry-templates"]);
    }

    for (std::size_t i = 1; i < actual.size(); ++i) {
        CAPTURE(i);
        CHECK(actual[i]["id"] == expected[i]["id"]);
        CHECK(actual[i]["vertices"] == expected[i]["vertices"]);

        // Compared in full: ids, types, attributes, boundaries, semantics,
        // extents, parents/children, and the texture/material mappings.
        CHECK(actual[i]["CityObjects"] == expected[i]["CityObjects"]);

        // And the WHOLE line, so a key we never thought to emit fails here
        // instead of passing silently. Checking only the keys above is what
        // hid the missing per-feature `appearance` object.
        CHECK(actual[i] == expected[i]);
    }
}

TEST_CASE("conformance: small") { check_case("small"); }
TEST_CASE("conformance: geom_temp") { check_case("geom_temp"); }
TEST_CASE("conformance: noise_extension") { check_case("noise_extension"); }
TEST_CASE("conformance: single_feature") { check_case("single_feature"); }
TEST_CASE("conformance: long_strings") { check_case("long_strings"); }
TEST_CASE("conformance: duplicate_keys") { check_case("duplicate_keys"); }
TEST_CASE("conformance: degenerate_extent") { check_case("degenerate_extent"); }
TEST_CASE("conformance: inferable_types") { check_case("inferable_types"); }
// `"material": {}` in the source is written as a PRESENT, EMPTY mapping
// vector (verified against the .fcb), which the reference reports as no
// appearance at all rather than an empty object. Emitting `"material": {}`
// here instead fails this case.
TEST_CASE("conformance: empty_appearance") { check_case("empty_appearance"); }
// Every geometry type at its schema depth, plus the nullable and
// absent-vs-empty cases. A one-solid MultiSolid and a one-shell Solid flatten
// to byte-identical count arrays, so ONLY the geometry type separates them;
// this case fails for any reader that infers depth from the arrays.
TEST_CASE("conformance: appearance_depths") { check_case("appearance_depths"); }

TEST_CASE("conformance: a single-feature file iterates exactly once") {
    FcbReader r = FcbReader::open_file(FCB_CONFORMANCE_DIR "/single_feature.fcb");
    CHECK(r.header().info().features_count == 1);
    FeatureIterator it = r.select_all();
    CHECK(it.next());
    CHECK_FALSE(it.next());
}

TEST_CASE("conformance: a zero-area extent does not break bbox queries") {
    FcbReader r = FcbReader::open_file(FCB_CONFORMANCE_DIR "/degenerate_extent.fcb");
    const auto& info = r.header().info();
    BBox all{info.geographical_extent[0], info.geographical_extent[1],
             info.geographical_extent[3], info.geographical_extent[4]};
    FeatureIterator it = r.select_bbox(all);
    std::uint64_t n = 0;
    while (it.next()) ++n;
    CHECK(n == info.features_count);
}
