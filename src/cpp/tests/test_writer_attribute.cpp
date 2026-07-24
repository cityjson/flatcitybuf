#include <fcb/writer/attribute.hpp>

#include <doctest/doctest.h>

using namespace fcb;

TEST_CASE("add_attributes assigns column indices in first-seen, alphabetical order") {
    // Parsed from JSON TEXT, not built via C++ initializer-list literals:
    // nlohmann classifies a parsed integer as number_unsigned_t unless its
    // text has a leading '-', exactly mirroring serde_json's Number (whose
    // sign-based PosInt/NegInt split is what `guess_type` on the Rust side
    // actually keys off). A C++ literal like `5` is a signed `int` and would
    // parse to number_integer_t instead, giving a false mismatch here.
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json::parse(R"({
        "int": -10,
        "uint": 5,
        "bool": true,
        "float": 1.0,
        "string": "hoge",
        "array": [1, 2, 3],
        "json": {"hoge": "fuga"},
        "null": null
    })"));

    CHECK(schema.at("int").second == ::ColumnType::Long);
    CHECK(schema.at("uint").second == ::ColumnType::ULong);
    CHECK(schema.at("bool").second == ::ColumnType::Bool);
    CHECK(schema.at("float").second == ::ColumnType::Double);
    CHECK(schema.at("string").second == ::ColumnType::String);
    CHECK(schema.at("array").second == ::ColumnType::Json);
    CHECK(schema.at("json").second == ::ColumnType::Json);
    CHECK(schema.find("null") == schema.end());

    // Alphabetical order of first appearance: array, bool, float, int, json,
    // string, uint -- so THAT is the index assignment order, not insertion
    // order into the nlohmann::json literal above.
    CHECK(schema.at("array").first == 0);
    CHECK(schema.at("bool").first == 1);
    CHECK(schema.at("float").first == 2);
    CHECK(schema.at("int").first == 3);
    CHECK(schema.at("json").first == 4);
    CHECK(schema.at("string").first == 5);
    CHECK(schema.at("uint").first == 6);
}

TEST_CASE("add_attributes does not reassign an already-known column") {
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json{{"a", 1}});
    add_attributes(schema, nlohmann::json{{"a", 2}, {"b", 3}});
    CHECK(schema.at("a").first == 0);
    CHECK(schema.at("b").first == 1);
    CHECK(schema.size() == 2);
}

TEST_CASE("a non-object attrs value becomes a single json column") {
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json::array({1, 2}));
    CHECK(schema.size() == 1);
    CHECK(schema.at("json").second == ::ColumnType::Json);
}

TEST_CASE("integer type guessing follows sign and magnitude") {
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json::parse(R"({"neg": -1, "pos": 1})"));
    CHECK(schema.at("neg").second == ::ColumnType::Long);
    CHECK(schema.at("pos").second == ::ColumnType::ULong);
}

TEST_CASE("a full RFC3339 datetime string is detected, a bare date is not") {
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json{
                               {"dt", "2010-10-13T12:29:24Z"},
                               {"date_only", "2024-01-15"},
                               {"dt_offset", "2010-10-13T12:29:24+02:00"},
                               {"dt_frac", "2010-10-13T12:29:24.5Z"},
                           });
    CHECK(schema.at("dt").second == ::ColumnType::DateTime);
    CHECK(schema.at("date_only").second == ::ColumnType::String);
    CHECK(schema.at("dt_offset").second == ::ColumnType::DateTime);
    CHECK(schema.at("dt_frac").second == ::ColumnType::DateTime);
}
