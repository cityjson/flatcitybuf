#include <fcb/cityjson.hpp>
#include <fcb/reader.hpp>

#include <nlohmann/json.hpp>

#include <fstream>
#include <string>
#include <vector>

#include <doctest/doctest.h>

using namespace fcb;
using nlohmann::json;

static std::vector<json> read_jsonl(const std::string& path) {
    std::vector<json> out;
    std::ifstream f(path);
    REQUIRE_MESSAGE(f.good(), "cannot open " << path);
    std::string line;
    while (std::getline(f, line)) {
        if (!line.empty())
            out.push_back(json::parse(line));
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
    while (it.next())
        actual.push_back(to_cityjson_feature(it.current(), r.header()));

    REQUIRE(actual.size() == expected.size());

    // Line 0 is the metadata envelope. Once pointOfContact/referenceDate
    // were wired up (Task 15, defect 2), nothing was left to deliberately
    // exclude here, so this is a full comparison like the feature lines
    // below -- narrowing it back down to individual keys is what let the
    // pointOfContact/referenceDate gap go unnoticed for as long as it did.
    CHECK(actual[0] == expected[0]);

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
// One feature carrying two values of the same indexed attribute (h = 1 and
// h = 9 on two BuildingParts), which no other fixture has.
TEST_CASE("conformance: multi_object_attrs") { check_case("multi_object_attrs"); }
// features_count = 0 in the header, which means "unknown": the reader must
// scan to EOF rather than trust the count. Without this case the count-0
// branch in layout.cpp is never taken by the conformance suite.
TEST_CASE("conformance: no_count") { check_case("no_count"); }
// String values that agree in the first 50 bytes -- the width of a B+tree
// string key -- plus values shorter than 50 bytes, which are zero-padded.
TEST_CASE("conformance: colliding_strings") { check_case("colliding_strings"); }
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
// The only fixture whose expected output contains a semantic surface
// `"parent"` (defect 1) and a full `pointOfContact`/`referenceDate`
// metadata line (defect 2). Its absence from this list is why both defects
// shipped undetected.
TEST_CASE("conformance: geom_decoder_edges") { check_case("geom_decoder_edges"); }
// A CityObject with geometry AND a GeometryInstance interleaved in the
// source order. The reference writer always emits every non-instance
// geometry before every instance (two separate passes over the input,
// filtered), so the decoded order is [MultiSurface, MultiSurface,
// GeometryInstance] even though the source interleaves them as [MultiSurface,
// GeometryInstance, MultiSurface] -- added for the C++ writer's byte-exact
// oracle tests (test_writer_oracle.cpp), which need a fixture exercising
// this reordering; also exercises the reader decoding geometry and
// geometry_instances together on one object, which no prior fixture did.
TEST_CASE("conformance: geometry_instance_interleaved") {
    check_case("geometry_instance_interleaved");
}

// Added for the C++ writer's M4 byte-exact oracle test (test_writer_oracle.cpp),
// which needed a fixture exercising every optional header metadata field at
// once: referenceSystem, identifier, referenceDate, title, a full
// pointOfContact (including a non-string address member and the
// postalCode-vs-postcode key rename), and multiple `extensions` entries --
// none of which single_feature/geometry_instance_interleaved carry.
TEST_CASE("conformance: header_metadata_full") { check_case("header_metadata_full"); }

// Added for the C++ writer's M5 byte-exact oracle test: every prior
// multi-feature fixture (duplicate_keys, colliding_strings, ...) happens to
// carry IDENTICAL bboxes for every feature, so hilbert_sort's reordering
// and the packed R-tree's bottom-up aggregation were never exercised on
// genuinely distinct geometry. This fixture's 20 features sit at distinct
// grid positions and, at node_size 16, force a real 3-level tree
// (20 leaves -> 2 -> 1), unlike every prior fixture's 2-level tree.
TEST_CASE("conformance: rtree_multilevel") { check_case("rtree_multilevel"); }

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
    BBox all{info.geographical_extent[0], info.geographical_extent[1], info.geographical_extent[3],
             info.geographical_extent[4]};
    FeatureIterator it = r.select_bbox(all);
    std::uint64_t n = 0;
    while (it.next())
        ++n;
    CHECK(n == info.features_count);
}
