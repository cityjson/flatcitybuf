#include <fcb/attribute.hpp>
#include <fcb/generated/header_generated.h>
#include <fcb/reader.hpp>

#include <set>
#include <string>

#include <doctest/doctest.h>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("an empty blob decodes to no attributes") {
    std::vector<ColumnInfo> schema;
    CHECK(decode_attributes(bytes_view(), schema).empty());
}

/// Decode one object using ITS schema: the object's own columns when it
/// declares them, otherwise the header's.
static std::vector<std::pair<std::string, AttrValue>>
decode_object(const Feature& f, std::size_t i, const std::vector<ColumnInfo>& header_cols) {
    auto own = f.object_columns(i);
    return decode_attributes(f.object_attributes(i), own.empty() ? header_cols : own);
}

TEST_CASE("decoding consumes the blob exactly, for every object of every feature") {
    // The records are not self-delimiting: each value's width comes from the
    // column type, so a wrong width desynchronises everything after it. If
    // decoding runs off the end or stops short, this throws or mismatches.
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& schema = r.header().info().columns;
    REQUIRE(schema.size() == 44);

    FeatureIterator it = r.select_all();
    std::uint64_t with_attrs = 0;
    std::uint64_t own_schema = 0;
    while (it.next()) {
        const Feature& f = it.current();
        for (std::size_t i = 0; i < f.city_object_count(); ++i) {
            if (!f.object_columns(i).empty())
                ++own_schema;
            if (f.object_attributes(i).empty())
                continue;
            CHECK_FALSE(decode_object(f, i, schema).empty());
            ++with_attrs;
        }
    }
    CHECK(with_attrs > 0);
    MESSAGE("objects with attributes: " << with_attrs
                                        << ", objects with their own schema: " << own_schema);
}

TEST_CASE("decoded names are real columns and types are plausible") {
    FcbReader r = FcbReader::open_file(kFixture);
    const auto& schema = r.header().info().columns;

    std::set<std::string> known;
    for (const auto& c : schema)
        known.insert(c.name);

    // Find the first object that actually has attributes -- in this data a
    // Building parent carries none and its BuildingPart child carries them.
    FeatureIterator it = r.select_all();
    std::vector<std::pair<std::string, AttrValue>> decoded;
    while (it.next() && decoded.empty()) {
        const Feature& f = it.current();
        for (std::size_t i = 0; i < f.city_object_count(); ++i) {
            if (f.object_attributes(i).empty())
                continue;
            decoded = decode_object(f, i, schema);
            break;
        }
    }
    REQUIRE_FALSE(decoded.empty());

    for (const auto& [name, v] : decoded) {
        CHECK(known.count(name) == 1);
        CHECK(v.type != AttrValue::Type::Null);
    }
}

TEST_CASE("a truncated blob throws instead of reading past the end") {
    std::vector<ColumnInfo> schema = {
        {0, "n", static_cast<std::uint8_t>(::ColumnType::Long), true}};
    // Column index 0 then only 3 of the 8 bytes a Long needs.
    std::vector<std::uint8_t> blob = {0, 0, 1, 2, 3};
    CHECK_THROWS_AS(decode_attributes(bytes_view(blob), schema), Error);
}

TEST_CASE("an unknown column index throws") {
    std::vector<ColumnInfo> schema = {
        {0, "n", static_cast<std::uint8_t>(::ColumnType::Bool), true}};
    std::vector<std::uint8_t> blob = {9, 0, 1};  // column 9 is not in the schema
    CHECK_THROWS_AS(decode_attributes(bytes_view(blob), schema), Error);
}

TEST_CASE("Byte/UByte/Binary decode, matching the reference reader") {
    // Byte is stored UNSIGNED by the writer, so 200 must come back as 200,
    // not -56; Binary is a u32 LE length then that many raw bytes. Mirrors
    // the Rust reader's own test (deserializer.rs,
    // test_decode_attributes_byte_ubyte_binary).
    //
    // The Binary record is deliberately NOT last: a fixed-width Int follows
    // it, so the walk has to land exactly on that record's column index. A
    // length read that is off by even one byte desynchronises the rest of the
    // blob -- with Binary at the end there is nothing left to desynchronise
    // and a wrong length goes unnoticed.
    std::vector<ColumnInfo> schema = {
        {0, "b", static_cast<std::uint8_t>(::ColumnType::Byte), true},
        {1, "ub", static_cast<std::uint8_t>(::ColumnType::UByte), true},
        {2, "bin", static_cast<std::uint8_t>(::ColumnType::Binary), true},
        {3, "i", static_cast<std::uint8_t>(::ColumnType::Int), true}};
    std::vector<std::uint8_t> blob = {
        0, 0, 200,                        // col 0, Byte:   200
        1, 0, 200,                        // col 1, UByte:  200
        2, 0, 2,   0,   0,   0,  1, 255,  // col 2, Binary: u32 len 2, then {1, 255}
        3, 0, 249, 255, 255, 255          // col 3, Int:    -7, LE two's complement
    };

    auto decoded = decode_attributes(bytes_view(blob), schema);
    REQUIRE(decoded.size() == 4);

    CHECK(decoded[0].first == "b");
    CHECK(decoded[0].second.type == AttrValue::Type::UInt);
    CHECK(decoded[0].second.u == 200);

    CHECK(decoded[1].first == "ub");
    CHECK(decoded[1].second.type == AttrValue::Type::UInt);
    CHECK(decoded[1].second.u == 200);

    CHECK(decoded[2].first == "bin");
    CHECK(decoded[2].second.type == AttrValue::Type::Binary);
    REQUIRE(decoded[2].second.s.size() == 2);
    CHECK(static_cast<std::uint8_t>(decoded[2].second.s[0]) == 1);
    CHECK(static_cast<std::uint8_t>(decoded[2].second.s[1]) == 255);

    // The record after the Binary payload: reached only if the u32 length was
    // read correctly and the walk resumed on the right byte.
    CHECK(decoded[3].first == "i");
    CHECK(decoded[3].second.type == AttrValue::Type::Int);
    CHECK(decoded[3].second.i == -7);

#ifdef FCB_WITH_JSON
    // Rust emits {"b":200,"ub":200,"bin":[1,255],"i":-7} -- Binary as an array
    // of numbers, so nlohmann must not turn it into its own binary value type.
    // nlohmann orders object keys alphabetically on dump.
    const nlohmann::json j = attributes_to_json(bytes_view(blob), schema);
    CHECK(j.dump() == "{\"b\":200,\"bin\":[1,255],\"i\":-7,\"ub\":200}");
#endif
}

#ifdef FCB_WITH_JSON
TEST_CASE("attributes render as a JSON object") {
    FcbReader r = FcbReader::open_file(kFixture);
    FeatureIterator it = r.select_all();
    REQUIRE(it.next());

    nlohmann::json j;
    while (it.next() && (j.is_null() || j.empty())) {
        const Feature& f = it.current();
        for (std::size_t i = 0; i < f.city_object_count(); ++i) {
            if (f.object_attributes(i).empty())
                continue;
            auto own = f.object_columns(i);
            j = attributes_to_json(f.object_attributes(i),
                                   own.empty() ? r.header().info().columns : own);
            break;
        }
    }
    CHECK(j.is_object());
    CHECK_FALSE(j.empty());
}
#endif
