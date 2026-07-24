#include <fcb/attribute.hpp>  // decode_attributes, the read-side inverse, used to verify round-trips
#include <fcb/writer/attribute.hpp>

#include <doctest/doctest.h>

using namespace fcb;

static std::vector<ColumnInfo> to_column_info(const AttributeSchema& schema) {
    std::vector<ColumnInfo> out;
    for (const auto& [name, idx_type] : schema)
        out.push_back({idx_type.first, name, static_cast<std::uint8_t>(idx_type.second), true});
    return out;
}

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

TEST_CASE("encode then decode round-trips every basic column type") {
    // Parsed from JSON text -- see the note in the first test case on why a
    // bare C++ initializer-list literal like `{"uint", 5}` would misclassify.
    nlohmann::json attrs = nlohmann::json::parse(R"({
        "int": -10,
        "uint": 5,
        "bool": true,
        "float": 1.5,
        "string": "hoge"
    })");
    AttributeSchema schema;
    add_attributes(schema, attrs);

    auto encoded = encode_attributes_with_schema(attrs, schema);
    CHECK_FALSE(encoded.empty());

    auto decoded = decode_attributes(bytes_view(encoded), to_column_info(schema));
    REQUIRE(decoded.size() == 5);

    auto find = [&](const std::string& name) -> const AttrValue& {
        for (auto& [n, v] : decoded)
            if (n == name)
                return v;
        FAIL("missing decoded attribute: " << name);
        static AttrValue dummy;
        return dummy;
    };
    CHECK(find("int").i == -10);
    CHECK(find("uint").u == 5);
    CHECK(find("bool").b == true);
    CHECK(find("float").d == doctest::Approx(1.5));
    CHECK(find("string").s == "hoge");
}

TEST_CASE("an empty or non-object attrs value encodes to zero bytes") {
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json{{"a", 1}});
    CHECK(encode_attributes_with_schema(nlohmann::json::object(), schema).empty());
    CHECK(encode_attributes_with_schema(nlohmann::json::array(), schema).empty());
}

TEST_CASE("a schema member absent from attrs is skipped, not zero-filled") {
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json{{"a", 1}, {"b", 2}});
    auto encoded = encode_attributes_with_schema(nlohmann::json{{"a", 7}}, schema);
    auto decoded = decode_attributes(bytes_view(encoded), to_column_info(schema));
    REQUIRE(decoded.size() == 1);
    CHECK(decoded[0].first == "a");
}

TEST_CASE("to_columns builds a Header whose columns round-trip through the reader") {
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json{{"b_col", true}, {"a_col", 5}});

    flatbuffers::FlatBufferBuilder fbb;
    auto columns = to_columns(fbb, schema);
    auto version = fbb.CreateString("1.0");
    HeaderBuilder hb(fbb);
    hb.add_version(version);
    hb.add_columns(columns);
    auto header = hb.Finish();
    fbb.Finish(header);

    const ::Header* h = flatbuffers::GetRoot<::Header>(fbb.GetBufferPointer());
    REQUIRE(h->columns() != nullptr);
    REQUIRE(h->columns()->size() == 2);
    // Emitted in ascending column-index order: "a_col" is index 0 (first
    // alphabetically among the two new names, regardless of the order
    // written in the initializer list above), "b_col" is index 1.
    CHECK(h->columns()->Get(0)->name()->str() == "a_col");
    CHECK(h->columns()->Get(0)->index() == 0);
    CHECK(h->columns()->Get(1)->name()->str() == "b_col");
    CHECK(h->columns()->Get(1)->index() == 1);
    CHECK(h->columns()->Get(1)->type() == ::ColumnType::Bool);
}

TEST_CASE("record layout is [u16 LE column index][value], schema-index order") {
    AttributeSchema schema;
    // Force known indices: "b" first (index 0), "a" second (index 1).
    add_attributes(schema, nlohmann::json{{"b", true}});
    add_attributes(schema, nlohmann::json{{"a", true}});
    REQUIRE(schema.at("b").first == 0);
    REQUIRE(schema.at("a").first == 1);

    auto encoded = encode_attributes_with_schema(nlohmann::json{{"a", true}, {"b", false}}, schema);
    // "b" (index 0) is written before "a" (index 1), regardless of attrs's
    // own JSON key order.
    REQUIRE(encoded.size() == 6);  // two records of (u16 index + 1 byte bool)
    CHECK(encoded[0] == 0);
    CHECK(encoded[1] == 0);
    CHECK(encoded[2] == 0);  // value for "b": false
    CHECK(encoded[3] == 1);
    CHECK(encoded[4] == 0);
    CHECK(encoded[5] == 1);  // value for "a": true
}
