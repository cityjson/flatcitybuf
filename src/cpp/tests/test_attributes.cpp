#include <doctest/doctest.h>

#include <fcb/attribute.hpp>
#include <fcb/reader.hpp>

#include <fcb/generated/header_generated.h>

#include <set>
#include <string>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("an empty blob decodes to no attributes") {
    std::vector<ColumnInfo> schema;
    CHECK(decode_attributes(bytes_view(), schema).empty());
}

/// Decode one object using ITS schema: the object's own columns when it
/// declares them, otherwise the header's.
static std::vector<std::pair<std::string, AttrValue>> decode_object(
    const Feature& f, std::size_t i, const std::vector<ColumnInfo>& header_cols) {
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
            if (!f.object_columns(i).empty()) ++own_schema;
            if (f.object_attributes(i).empty()) continue;
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
    for (const auto& c : schema) known.insert(c.name);

    // Find the first object that actually has attributes -- in this data a
    // Building parent carries none and its BuildingPart child carries them.
    FeatureIterator it = r.select_all();
    std::vector<std::pair<std::string, AttrValue>> decoded;
    while (it.next() && decoded.empty()) {
        const Feature& f = it.current();
        for (std::size_t i = 0; i < f.city_object_count(); ++i) {
            if (f.object_attributes(i).empty()) continue;
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
    std::vector<ColumnInfo> schema = {{0, "n", static_cast<std::uint8_t>(::ColumnType::Long), true}};
    // Column index 0 then only 3 of the 8 bytes a Long needs.
    std::vector<std::uint8_t> blob = {0, 0, 1, 2, 3};
    CHECK_THROWS_AS(decode_attributes(bytes_view(blob), schema), Error);
}

TEST_CASE("an unknown column index throws") {
    std::vector<ColumnInfo> schema = {{0, "n", static_cast<std::uint8_t>(::ColumnType::Bool), true}};
    std::vector<std::uint8_t> blob = {9, 0, 1};  // column 9 is not in the schema
    CHECK_THROWS_AS(decode_attributes(bytes_view(blob), schema), Error);
}

TEST_CASE("Byte/UByte/Binary are rejected, matching the reference reader") {
    // The writer emits these but deserializer.rs:372 is unreachable!() for
    // them, so no such file has ever been read back successfully.
    std::vector<ColumnInfo> schema = {{0, "b", static_cast<std::uint8_t>(::ColumnType::Byte), true}};
    std::vector<std::uint8_t> blob = {0, 0, 200};
    CHECK_THROWS_AS(decode_attributes(bytes_view(blob), schema), Error);
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
            if (f.object_attributes(i).empty()) continue;
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
