#include <fcb/reader.hpp>
#include <fcb/writer/attribute.hpp>
#include <fcb/writer/btree_builder.hpp>
#include <fcb/writer/fcb_writer.hpp>

#include <cstdio>
#include <fstream>
#include <iterator>

#include <doctest/doctest.h>

using namespace fcb;
using nlohmann::ordered_json;

namespace {

ordered_json make_metadata() {
    return ordered_json::parse(R"({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [0.001, 0.001, 0.001], "translate": [0.0, 0.0, 0.0]}
    })");
}

/// `grid_pos` offsets this feature's vertices so distinct features get
/// distinct, non-overlapping bboxes (0,0 by default -- fine whenever a
/// test doesn't care about bbox-driven reordering; several tests DO, so
/// they pass distinct positions to actually exercise hilbert_sort).
ordered_json make_feature(const std::string& id, const ordered_json& attributes, int grid_pos = 0) {
    const int x0 = grid_pos * 10000;
    ordered_json f = ordered_json::parse(R"({"type": "CityJSONFeature", "CityObjects": {}})");
    f["id"] = id;
    f["vertices"] = ordered_json::array(
        {ordered_json::array({x0, 0, 0}), ordered_json::array({x0 + 1000, 0, 0}),
         ordered_json::array({x0 + 1000, 1000, 0}), ordered_json::array({x0, 1000, 0})});
    ordered_json co;
    co["type"] = "Building";
    co["attributes"] = attributes;
    co["geometry"] = ordered_json::array({ordered_json::parse(
        R"({"type": "MultiSurface", "lod": "1", "boundaries": [[[0,1,2,3]]]})")});
    f["CityObjects"]["o_" + id] = co;
    return f;
}

/// Writes `bytes` to a temp file, opens it via FcbReader, and returns the
/// reader (test-only round-trip helper).
FcbReader open_written(const std::vector<std::uint8_t>& bytes, const std::string& tmp_path) {
    std::ofstream out(tmp_path, std::ios::binary);
    REQUIRE_MESSAGE(out.good(), "cannot create " << tmp_path);
    out.write(reinterpret_cast<const char*>(bytes.data()),
              static_cast<std::streamsize>(bytes.size()));
    out.close();
    return FcbReader::open_file(tmp_path);
}

/// Convenience wrapper matching the other writer test files: loops
/// `add_feature` over an in-memory vector, then `write()`s -- exercising
/// the real streaming `FcbWriter` implementation, not a separate path.
std::vector<std::uint8_t> write_fcb(const ordered_json& cj,
                                    const std::vector<ordered_json>& features,
                                    const FcbWriterOptions& options,
                                    const AttributeSchema& attr_schema,
                                    const AttributeSchema* semantic_attr_schema) {
    FcbWriter w(cj, options, attr_schema,
                semantic_attr_schema ? std::optional<AttributeSchema>(*semantic_attr_schema)
                                     : std::nullopt);
    for (const auto& f : features)
        w.add_feature(f);
    return w.write();
}

}  // namespace

TEST_CASE("write_fcb with write_index=false writes index_node_size 0, no R-tree bytes, and leaves "
          "features in original (unsorted) order") {
    // Distinct grid positions (not a shared bbox) so a bug that hilbert-
    // sorted anyway -- despite write_index=false -- would actually reorder
    // features and be caught below, rather than passing by coincidence.
    // Flagged as a coverage gap by the M7 codex review: the previous
    // version of this test used identical bboxes and only checked feature
    // COUNT, not order.
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features{
        make_feature("f_c", ordered_json::parse(R"({"n": 1})"), /*grid_pos=*/2),
        make_feature("f_a", ordered_json::parse(R"({"n": 2})"), /*grid_pos=*/0),
        make_feature("f_b", ordered_json::parse(R"({"n": 3})"), /*grid_pos=*/1),
    };
    AttributeSchema schema;
    for (const auto& f : features)
        add_attributes(schema, f.at("CityObjects").begin().value().at("attributes"));

    FcbWriterOptions options;
    options.write_index = false;
    options.index_node_size = 16;  // must be ignored/zeroed since write_index is false

    std::vector<std::uint8_t> bytes = write_fcb(cj, features, options, schema, nullptr);
    const std::string tmp = "test_writer_fcb_no_index.fcb";
    FcbReader r = open_written(bytes, tmp);
    CHECK(r.header().info().index_node_size == 0);
    CHECK(r.header().layout().rtree_size == 0);
    CHECK(r.header().info().features_count == 3);

    std::vector<std::string> ids;
    FeatureIterator it = r.select_all();
    while (it.next())
        ids.push_back(it.current().id());
    REQUIRE(ids.size() == 3);
    CHECK(ids[0] == "f_c");
    CHECK(ids[1] == "f_a");
    CHECK(ids[2] == "f_b");
    std::remove(tmp.c_str());
}

TEST_CASE("write_fcb with no requested attribute indices writes none") {
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features{make_feature("f1", ordered_json::parse(R"({"n": 1})"))};
    AttributeSchema schema;
    add_attributes(schema, features[0].at("CityObjects").begin().value().at("attributes"));

    FcbWriterOptions options;  // attribute_indices left empty

    std::vector<std::uint8_t> bytes = write_fcb(cj, features, options, schema, nullptr);
    const std::string tmp = "test_writer_fcb_no_attr_index.fcb";
    FcbReader r = open_written(bytes, tmp);
    CHECK(r.header().attr_indices().empty());
    CHECK(r.header().layout().attr_index_size == 0);
    std::remove(tmp.c_str());
}

TEST_CASE("write_fcb silently skips a requested attribute index for a column absent from the "
          "schema") {
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features{make_feature("f1", ordered_json::parse(R"({"n": 1})"))};
    AttributeSchema schema;
    add_attributes(schema, features[0].at("CityObjects").begin().value().at("attributes"));

    FcbWriterOptions options;
    options.attribute_indices.emplace_back("does_not_exist", std::nullopt);
    options.attribute_indices.emplace_back("n",
                                           std::nullopt);  // a real column, alongside the bogus one

    std::vector<std::uint8_t> bytes = write_fcb(cj, features, options, schema, nullptr);
    const std::string tmp = "test_writer_fcb_skip_missing_column.fcb";
    FcbReader r = open_written(bytes, tmp);
    REQUIRE(r.header().attr_indices().size() == 1);
    CHECK(r.header().attr_indices()[0].branching_factor == kDefaultBranchingFactor);
    std::remove(tmp.c_str());
}

TEST_CASE("write_fcb silently skips a requested attribute index for a column with zero indexable "
          "entries") {
    // A Json-typed column (inferred whenever the value is itself an object)
    // is one of the types `cityfeature_to_index_entries` never produces
    // entries for (a known, deliberate gap ported from Rust's own
    // `attribute_to_index_entries`, per writer/attribute.hpp) -- so
    // requesting an index for it must be silently skipped, the same as a
    // genuinely empty/all-null column, not throw.
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features{
        make_feature("f1", ordered_json::parse(R"({"blob": {"nested": true}})"))};
    AttributeSchema schema;
    add_attributes(schema, features[0].at("CityObjects").begin().value().at("attributes"));

    FcbWriterOptions options;
    options.attribute_indices.emplace_back("blob", std::nullopt);

    std::vector<std::uint8_t> bytes = write_fcb(cj, features, options, schema, nullptr);
    const std::string tmp = "test_writer_fcb_skip_empty_column.fcb";
    FcbReader r = open_written(bytes, tmp);
    CHECK(r.header().attr_indices().empty());
    std::remove(tmp.c_str());
}

TEST_CASE("write_fcb requests a non-default branching factor and it round-trips through the "
          "header") {
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features;
    for (int i = 0; i < 5; ++i)
        features.push_back(make_feature("f" + std::to_string(i), ordered_json{{"n", i}}));
    AttributeSchema schema;
    for (const auto& f : features)
        add_attributes(schema, f.at("CityObjects").begin().value().at("attributes"));

    FcbWriterOptions options;
    options.attribute_indices.emplace_back("n", static_cast<std::uint16_t>(4));

    std::vector<std::uint8_t> bytes = write_fcb(cj, features, options, schema, nullptr);
    const std::string tmp = "test_writer_fcb_branching_factor.fcb";
    FcbReader r = open_written(bytes, tmp);
    REQUIRE(r.header().attr_indices().size() == 1);
    CHECK(r.header().attr_indices()[0].branching_factor == 4);
    CHECK(r.header().attr_indices()[0].num_unique_items == 5);
    std::remove(tmp.c_str());
}

TEST_CASE("write_fcb handles an empty feature list") {
    // Flagged as an unpinned path by the M7 codex review: every oracle
    // fixture has 1+ features, so `feat_nodes.empty()` (which gates both
    // hilbert_sort and the R-tree build) was never actually exercised.
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features;
    AttributeSchema schema;

    FcbWriterOptions options;
    options.write_index = true;
    options.index_node_size = 16;

    std::vector<std::uint8_t> bytes = write_fcb(cj, features, options, schema, nullptr);
    const std::string tmp = "test_writer_fcb_empty.fcb";
    FcbReader r = open_written(bytes, tmp);
    CHECK(r.header().info().features_count == 0);
    // No leaves means no R-tree bytes either, regardless of the requested
    // node size -- matches layout.cpp's own `features_count == 0` short
    // circuit (compute_layout treats an empty file as having no spatial
    // index section, whatever index_node_size the header claims).
    CHECK(r.header().layout().rtree_size == 0);

    std::size_t count = 0;
    FeatureIterator it = r.select_all();
    while (it.next())
        ++count;
    CHECK(count == 0);
    std::remove(tmp.c_str());
}

TEST_CASE("write_fcb normalizes attribute_indices request order to schema column order") {
    // Flagged as an unpinned branch by the M7 codex review: the whole-file
    // oracle tests reconstruct options FROM an already schema-sorted real
    // header, so they never actually exercise `write_fcb`'s own reordering
    // of a REVERSED request. Two columns "a" (schema index 0) and "b"
    // (schema index 1), requested in reverse ("b" then "a"), must still
    // produce IDENTICAL output to requesting them in schema order.
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features;
    for (int i = 0; i < 3; ++i)
        features.push_back(
            make_feature("f" + std::to_string(i), ordered_json{{"a", i}, {"b", i * 10}}, i));
    AttributeSchema schema;
    for (const auto& f : features)
        add_attributes(schema, f.at("CityObjects").begin().value().at("attributes"));
    REQUIRE(schema.at("a").first == 0);
    REQUIRE(schema.at("b").first == 1);

    FcbWriterOptions forward;
    forward.attribute_indices.emplace_back("a", std::nullopt);
    forward.attribute_indices.emplace_back("b", std::nullopt);

    FcbWriterOptions reversed;
    reversed.attribute_indices.emplace_back("b", std::nullopt);
    reversed.attribute_indices.emplace_back("a", std::nullopt);

    std::vector<std::uint8_t> forward_bytes = write_fcb(cj, features, forward, schema, nullptr);
    std::vector<std::uint8_t> reversed_bytes = write_fcb(cj, features, reversed, schema, nullptr);
    CHECK(forward_bytes == reversed_bytes);

    const std::string tmp = "test_writer_fcb_reversed_indices.fcb";
    FcbReader r = open_written(reversed_bytes, tmp);
    REQUIRE(r.header().attr_indices().size() == 2);
    CHECK(r.header().attr_indices()[0].column_index == 0);
    CHECK(r.header().attr_indices()[1].column_index == 1);
    std::remove(tmp.c_str());
}

TEST_CASE("FcbWriter used directly (not through the test wrapper) round-trips a feature") {
    // Exercises the actual public API shape end to end -- constructing the
    // class, calling add_feature per feature, then write() once -- rather
    // than through this file's `write_fcb` convenience wrapper.
    ordered_json cj = make_metadata();
    AttributeSchema schema;
    ordered_json f = make_feature("f1", ordered_json::parse(R"({"n": 7})"));
    add_attributes(schema, f.at("CityObjects").begin().value().at("attributes"));

    FcbWriterOptions options;
    FcbWriter w(cj, options, schema, std::nullopt);
    w.add_feature(f);
    std::vector<std::uint8_t> bytes = w.write();

    const std::string tmp = "test_writer_fcb_direct_class_usage.fcb";
    FcbReader r = open_written(bytes, tmp);
    CHECK(r.header().info().features_count == 1);
    FeatureIterator it = r.select_all();
    REQUIRE(it.next());
    CHECK(it.current().id() == "f1");
    CHECK_FALSE(it.next());
    std::remove(tmp.c_str());
}

TEST_CASE("FcbWriter throws if add_feature is called after write()") {
    ordered_json cj = make_metadata();
    AttributeSchema schema;
    FcbWriterOptions options;
    FcbWriter w(cj, options, schema, std::nullopt);
    w.add_feature(make_feature("f1", ordered_json::object()));
    (void)w.write();
    CHECK_THROWS_AS(w.add_feature(make_feature("f2", ordered_json::object())), Error);
}

TEST_CASE("FcbWriter throws if write() is called more than once") {
    ordered_json cj = make_metadata();
    AttributeSchema schema;
    FcbWriterOptions options;
    FcbWriter w(cj, options, schema, std::nullopt);
    w.add_feature(make_feature("f1", ordered_json::object()));
    (void)w.write();
    CHECK_THROWS_AS(w.write(), Error);
}

TEST_CASE("FcbWriter streams many features through the temp file without holding them all at "
          "once") {
    // Not a memory-usage test (impractical to assert in-process), but a
    // functional one: enough features that, if `add_feature` accidentally
    // buffered everything in memory instead of spooling to disk, would
    // still behave identically from the CALLER's side -- so this at least
    // pins that the streaming path produces a correct, fully-decodable
    // file at a size large enough to matter.
    ordered_json cj = make_metadata();
    AttributeSchema schema;
    std::vector<ordered_json> features;
    for (int i = 0; i < 200; ++i)
        features.push_back(make_feature("f" + std::to_string(i), ordered_json{{"n", i}}, i % 20));
    for (const auto& f : features)
        add_attributes(schema, f.at("CityObjects").begin().value().at("attributes"));

    FcbWriterOptions options;
    options.attribute_indices.emplace_back("n", std::nullopt);
    FcbWriter w(cj, options, schema, std::nullopt);
    for (const auto& f : features)
        w.add_feature(f);
    std::vector<std::uint8_t> bytes = w.write();

    const std::string tmp = "test_writer_fcb_many_features.fcb";
    FcbReader r = open_written(bytes, tmp);
    CHECK(r.header().info().features_count == 200);
    REQUIRE(r.header().attr_indices().size() == 1);
    CHECK(r.header().attr_indices()[0].num_unique_items == 200);

    std::size_t count = 0;
    FeatureIterator it = r.select_all();
    while (it.next())
        ++count;
    CHECK(count == 200);
    std::remove(tmp.c_str());
}

TEST_CASE("FcbWriter::write(ostream&) produces byte-identical output to write(), and can write "
          "straight to a real file") {
    // The `ostream` overload is the one that actually delivers the
    // streaming/bounded-memory property (write() the vector-returning
    // convenience wrapper does not, and never claims to) -- flagged during
    // the M8 codex review, since the original version of this milestone
    // only ever exercised the vector-returning overload.
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features;
    for (int i = 0; i < 5; ++i)
        features.push_back(make_feature("f" + std::to_string(i), ordered_json{{"n", i}}, i));
    AttributeSchema schema;
    for (const auto& f : features)
        add_attributes(schema, f.at("CityObjects").begin().value().at("attributes"));

    FcbWriterOptions options;
    options.attribute_indices.emplace_back("n", std::nullopt);

    FcbWriter w1(cj, options, schema, std::nullopt);
    for (const auto& f : features)
        w1.add_feature(f);
    std::vector<std::uint8_t> via_vector = w1.write();

    FcbWriter w2(cj, options, schema, std::nullopt);
    for (const auto& f : features)
        w2.add_feature(f);
    const std::string tmp = "test_writer_fcb_ostream_overload.fcb";
    {
        std::ofstream out(tmp, std::ios::binary);
        REQUIRE_MESSAGE(out.good(), "cannot create " << tmp);
        w2.write(out);
    }

    std::ifstream in(tmp, std::ios::binary);
    std::vector<std::uint8_t> via_ostream((std::istreambuf_iterator<char>(in)),
                                          std::istreambuf_iterator<char>());
    CHECK(via_vector == via_ostream);

    FcbReader r = FcbReader::open_file(tmp);
    CHECK(r.header().info().features_count == 5);
    std::remove(tmp.c_str());
}
