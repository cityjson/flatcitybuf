#include <fcb/reader.hpp>
#include <fcb/writer/attribute.hpp>
#include <fcb/writer/btree_builder.hpp>
#include <fcb/writer/fcb_writer.hpp>

#include <cstdio>
#include <fstream>

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

ordered_json make_feature(const std::string& id, const ordered_json& attributes) {
    ordered_json f = ordered_json::parse(R"({
        "type": "CityJSONFeature",
        "CityObjects": {},
        "vertices": [[0, 0, 0], [1000, 0, 0], [1000, 1000, 0], [0, 1000, 0]]
    })");
    f["id"] = id;
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

}  // namespace

TEST_CASE("write_fcb with write_index=false writes index_node_size 0 and no R-tree bytes") {
    ordered_json cj = make_metadata();
    std::vector<ordered_json> features{make_feature("f1", ordered_json::parse(R"({"n": 1})")),
                                       make_feature("f2", ordered_json::parse(R"({"n": 2})"))};
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
    CHECK(r.header().info().features_count == 2);

    std::size_t count = 0;
    FeatureIterator it = r.select_all();
    while (it.next())
        ++count;
    CHECK(count == 2);
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
