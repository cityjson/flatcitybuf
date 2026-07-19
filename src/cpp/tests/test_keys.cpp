#include <doctest/doctest.h>

#include <fcb/key.hpp>

#include <cmath>
#include <limits>
#include <string>

using namespace fcb;

TEST_CASE("serialized sizes match the Rust key encoders") {
    CHECK(key_serialized_size(KeyKind::Int8) == 1);
    CHECK(key_serialized_size(KeyKind::UInt8) == 1);
    CHECK(key_serialized_size(KeyKind::Int16) == 2);
    CHECK(key_serialized_size(KeyKind::UInt16) == 2);
    CHECK(key_serialized_size(KeyKind::Int32) == 4);
    CHECK(key_serialized_size(KeyKind::UInt32) == 4);
    CHECK(key_serialized_size(KeyKind::Int64) == 8);
    CHECK(key_serialized_size(KeyKind::UInt64) == 8);
    CHECK(key_serialized_size(KeyKind::Float32) == 4);
    CHECK(key_serialized_size(KeyKind::Float64) == 8);
    CHECK(key_serialized_size(KeyKind::Bool) == 1);
    CHECK(key_serialized_size(KeyKind::DateTime) == 12);  // i64 secs + u32 nanos
    CHECK(key_serialized_size(KeyKind::String20) == 20);
    CHECK(key_serialized_size(KeyKind::String50) == 50);
    CHECK(key_serialized_size(KeyKind::String100) == 100);
}

TEST_CASE("integers round-trip as little-endian two's complement") {
    KeyValue v = KeyValue::from_i32(-2);
    auto bytes = encode_key(v);
    REQUIRE(bytes.size() == 4);
    CHECK(bytes[0] == 0xFE);
    CHECK(bytes[1] == 0xFF);
    CHECK(bytes[2] == 0xFF);
    CHECK(bytes[3] == 0xFF);
    CHECK(compare_keys(decode_key(KeyKind::Int32, bytes_view(bytes)), v) == 0);
}

TEST_CASE("unsigned integers round-trip") {
    KeyValue v = KeyValue::from_u64(0xDEADBEEFCAFEULL);
    auto bytes = encode_key(v);
    REQUIRE(bytes.size() == 8);
    CHECK(compare_keys(decode_key(KeyKind::UInt64, bytes_view(bytes)), v) == 0);
}

TEST_CASE("floats are stored as raw IEEE-754 LE bits with NO order transform") {
    // key.rs:347-370 writes the plain bit pattern. Do NOT apply the usual
    // "flip the sign bit" total-order trick -- that would disagree with
    // every file the reference implementation has ever written.
    KeyValue v = KeyValue::from_f64(1.0);
    auto bytes = encode_key(v);
    REQUIRE(bytes.size() == 8);
    // 1.0 == 0x3FF0000000000000, little-endian on disk.
    CHECK(bytes[7] == 0x3F);
    CHECK(bytes[6] == 0xF0);
    CHECK(bytes[0] == 0x00);
}

TEST_CASE("float comparison uses ordered_float total order") {
    const double nan = std::numeric_limits<double>::quiet_NaN();
    const double inf = std::numeric_limits<double>::infinity();

    // NaN equals itself and sorts above everything, including +inf.
    CHECK(compare_keys(KeyValue::from_f64(nan), KeyValue::from_f64(nan)) == 0);
    CHECK(compare_keys(KeyValue::from_f64(nan), KeyValue::from_f64(inf)) > 0);
    CHECK(compare_keys(KeyValue::from_f64(inf), KeyValue::from_f64(nan)) < 0);

    // -0.0 == +0.0
    CHECK(compare_keys(KeyValue::from_f64(-0.0), KeyValue::from_f64(0.0)) == 0);

    CHECK(compare_keys(KeyValue::from_f64(-1.0), KeyValue::from_f64(1.0)) < 0);
    CHECK(compare_keys(KeyValue::from_f64(-inf), KeyValue::from_f64(-1.0)) < 0);
    CHECK(compare_keys(KeyValue::from_f64(1.0), KeyValue::from_f64(inf)) < 0);
}

TEST_CASE("fixed strings zero-pad and truncate silently at the byte level") {
    KeyValue short_s = KeyValue::from_string(KeyKind::String50, "abc");
    auto bytes = encode_key(short_s);
    REQUIRE(bytes.size() == 50);
    CHECK(bytes[0] == 'a');
    CHECK(bytes[3] == 0x00);
    CHECK(bytes[49] == 0x00);

    std::string long_s(60, 'x');
    auto tb = encode_key(KeyValue::from_string(KeyKind::String50, long_s));
    REQUIRE(tb.size() == 50);
    CHECK(tb[49] == 'x');
}

TEST_CASE("truncation splits multi-byte UTF-8 without complaint") {
    // key.rs:483-489 copies min(len, N) BYTES. It does not respect UTF-8
    // boundaries, so a 3-byte character straddling the limit is cut apart.
    // 17 'a' + 11 euro signs (3 bytes each) = 17 + 33 = 50 bytes exactly at
    // the boundary; adding one more 'a' first pushes a euro across it.
    std::string s(18, 'a');
    for (int i = 0; i < 11; ++i) s += "\xE2\x82\xAC";  // U+20AC EURO SIGN

    auto b = encode_key(KeyValue::from_string(KeyKind::String50, s));
    REQUIRE(b.size() == 50);
    // Byte 49 is a continuation byte (10xxxxxx), i.e. mid-character.
    CHECK((b[49] & 0xC0) == 0x80);
}

TEST_CASE("strings sharing an N-byte prefix collide in the index") {
    // This is WHY select_attr must post-filter: the index cannot tell these
    // apart, so it yields candidates rather than answers.
    std::string a = std::string(50, 'y') + "AAA";
    std::string b = std::string(50, 'y') + "BBB";
    CHECK(compare_keys(KeyValue::from_string(KeyKind::String50, a),
                       KeyValue::from_string(KeyKind::String50, b)) == 0);
}

TEST_CASE("a short string with an embedded NUL collides with its prefix") {
    // The reason post-filtering cannot be gated on "query length >= N":
    // zero padding makes "a" and "a\0" identical on disk, even though both
    // are far shorter than the key width.
    std::string with_nul("a\0", 2);
    CHECK(compare_keys(KeyValue::from_string(KeyKind::String50, "a"),
                       KeyValue::from_string(KeyKind::String50, with_nul)) == 0);
}

TEST_CASE("string sentinels are all-0xFF and all-0x00") {
    auto mx = encode_key(key_max(KeyKind::String50));
    auto mn = encode_key(key_min(KeyKind::String50));
    CHECK(mx[0] == 0xFF);
    CHECK(mx[49] == 0xFF);
    CHECK(mn[0] == 0x00);
    CHECK(mn[49] == 0x00);
}

TEST_CASE("DateTime is i64 seconds followed by u32 nanos, both LE") {
    KeyValue v = KeyValue::from_datetime(/*secs=*/1, /*nanos=*/2);
    auto bytes = encode_key(v);
    REQUIRE(bytes.size() == 12);
    CHECK(bytes[0] == 1);
    CHECK(bytes[8] == 2);
    CHECK(compare_keys(decode_key(KeyKind::DateTime, bytes_view(bytes)), v) == 0);
}

TEST_CASE("DateTime ordering handles negative seconds") {
    // The wire format stores a signed i64, so pre-1970 timestamps encode
    // fine even though the query sentinel min_value() is epoch 0.
    CHECK(compare_keys(KeyValue::from_datetime(-100, 0),
                       KeyValue::from_datetime(100, 0)) < 0);
}

TEST_CASE("column type maps to the key kind the WRITER produces") {
    // Raw ubyte values, matching header.fbs's ColumnType declaration order,
    // so this test does not depend on the generated API either.
    struct CT { enum : std::uint8_t { Byte=0,UByte=1,Bool=2,Short=3,UShort=4,Int=5,
                UInt=6,Long=7,ULong=8,Float=9,Double=10,String=11,Json=12,
                DateTime=13,Binary=14 }; };
    CHECK(key_kind_for_column(CT::String) == KeyKind::String50);
    CHECK(key_kind_for_column(CT::Json) == KeyKind::String100);
    CHECK(key_kind_for_column(CT::Binary) == KeyKind::String100);
    CHECK(key_kind_for_column(CT::Bool) == KeyKind::Bool);
    CHECK(key_kind_for_column(CT::Double) == KeyKind::Float64);
    CHECK(key_kind_for_column(CT::Float) == KeyKind::Float32);
    CHECK(key_kind_for_column(CT::Long) == KeyKind::Int64);
    CHECK(key_kind_for_column(CT::ULong) == KeyKind::UInt64);

    // Byte maps to UInt8, NOT Int8. The writer stores Byte as u8
    // (writer/attribute.rs:209) and indexes it as MemoryIndex<u8>
    // (writer/attr_index.rs:240); only the Rust READER decodes i8
    // (reader/attr_query.rs:118), which returns negative numbers for stored
    // values above 127. We match the writer so we decode files correctly.
    CHECK(key_kind_for_column(CT::Byte) == KeyKind::UInt8);
    CHECK(key_kind_for_column(CT::UByte) == KeyKind::UInt8);
}

TEST_CASE("a Byte value above 127 decodes as unsigned, not negative") {
    std::vector<std::uint8_t> raw = {200};
    KeyValue v = decode_key(KeyKind::UInt8, bytes_view(raw));
    CHECK(compare_keys(v, KeyValue::from_u8(200)) == 0);
    CHECK(compare_keys(v, KeyValue::from_u8(0)) > 0);
}

TEST_CASE("decode rejects a buffer shorter than the key") {
    std::vector<std::uint8_t> tooshort = {1, 2};
    CHECK_THROWS_AS(decode_key(KeyKind::Int64, bytes_view(tooshort)), Error);
}

TEST_CASE("comparing different kinds is rejected rather than silently wrong") {
    CHECK_THROWS_AS(compare_keys(KeyValue::from_i32(1), KeyValue::from_f64(1.0)), Error);
}
