#include <fcb/generated/feature_generated.h>
#include <fcb/generated/header_generated.h>

#include <cstdint>

#include <doctest/doctest.h>

// The generated types live in the GLOBAL namespace: every `namespace
// FlatCityBuf;` declaration in src/fbs/*.fbs is commented out, so flatc
// emits none. Do not add a namespace qualifier.

TEST_CASE("AttributeIndex has the padded 16-byte wire layout") {
    // Field order (ushort, uint, ushort, uint) forces 2 bytes of padding
    // after each ushort. Rust's generated type is [u8; 16]; flatc's C++
    // struct carries explicit padding members. If this ever fails the
    // SCHEMA changed -- fix the Format Reference in the plan and work out
    // what else the layout change breaks. Do not just edit the number.
    CHECK(sizeof(AttributeIndex) == 16);
    CHECK(alignof(AttributeIndex) == 4);
}

TEST_CASE("ColumnType enumerators match the schema's declaration order") {
    // header.fbs:9-26 declares `enum ColumnType: ubyte` in this exact
    // order. The B+tree key mapping depends on these values, so pin them.
    CHECK(static_cast<std::uint8_t>(ColumnType::Byte) == 0);
    CHECK(static_cast<std::uint8_t>(ColumnType::UByte) == 1);
    CHECK(static_cast<std::uint8_t>(ColumnType::Bool) == 2);
    CHECK(static_cast<std::uint8_t>(ColumnType::Short) == 3);
    CHECK(static_cast<std::uint8_t>(ColumnType::UShort) == 4);
    CHECK(static_cast<std::uint8_t>(ColumnType::Int) == 5);
    CHECK(static_cast<std::uint8_t>(ColumnType::UInt) == 6);
    CHECK(static_cast<std::uint8_t>(ColumnType::Long) == 7);
    CHECK(static_cast<std::uint8_t>(ColumnType::ULong) == 8);
    CHECK(static_cast<std::uint8_t>(ColumnType::Float) == 9);
    CHECK(static_cast<std::uint8_t>(ColumnType::Double) == 10);
    CHECK(static_cast<std::uint8_t>(ColumnType::String) == 11);
    CHECK(static_cast<std::uint8_t>(ColumnType::Json) == 12);
    CHECK(static_cast<std::uint8_t>(ColumnType::DateTime) == 13);
    CHECK(static_cast<std::uint8_t>(ColumnType::Binary) == 14);
}

TEST_CASE("the size-prefixed root accessors the reader needs exist and are global") {
    const ::Header* (*get_header)(const void*) = &GetSizePrefixedHeader;
    const ::CityFeature* (*get_feature)(const void*) = &GetSizePrefixedCityFeature;
    CHECK(get_header != nullptr);
    CHECK(get_feature != nullptr);
}

TEST_CASE("verification rejects a buffer too short to hold a root") {
    const std::uint8_t stub[4] = {0, 0, 0, 0};
    flatbuffers::Verifier v(stub, sizeof(stub));
    CHECK_FALSE(VerifySizePrefixedHeaderBuffer(v));
}
