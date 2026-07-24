#pragma once

#include <fcb/error.hpp>
#include <fcb/span.hpp>

#include <cstdint>
#include <string>
#include <vector>

namespace fcb {

/// The concrete key types the B+tree index can hold.
/// StringKey20 exists in the format but the writer never emits it.
enum class KeyKind {
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float32,
    Float64,
    Bool,
    DateTime,
    String20,
    String50,
    String100,
};

/// Serialized width in bytes. DateTime is 12: i64 seconds + u32 nanos.
std::size_t key_serialized_size(KeyKind kind);

/// A decoded index key.
///
/// Ordering is NOT bytewise for floats: the on-disk bytes are the plain
/// IEEE-754 bit pattern, and ordered_float semantics are applied after
/// decoding (see compare_keys).
class KeyValue {
  public:
    KeyValue() = default;

    static KeyValue from_i8(std::int8_t v);
    static KeyValue from_u8(std::uint8_t v);
    static KeyValue from_i16(std::int16_t v);
    static KeyValue from_u16(std::uint16_t v);
    static KeyValue from_i32(std::int32_t v);
    static KeyValue from_u32(std::uint32_t v);
    static KeyValue from_i64(std::int64_t v);
    static KeyValue from_u64(std::uint64_t v);
    static KeyValue from_f32(float v);
    static KeyValue from_f64(double v);
    static KeyValue from_bool(bool v);
    static KeyValue from_datetime(std::int64_t seconds, std::uint32_t nanos);
    static KeyValue from_string(KeyKind kind, const std::string& v);

    KeyKind kind() const { return kind_; }

    /// The original, untruncated string this key was built from. Needed for
    /// post-filtering, since the encoded key keeps only the first N bytes.
    const std::string& original_string() const { return str_; }

  private:
    friend std::vector<std::uint8_t> encode_key(const KeyValue&);
    friend KeyValue decode_key(KeyKind, bytes_view);
    friend int compare_keys(const KeyValue&, const KeyValue&);

    KeyKind kind_ = KeyKind::Int32;
    std::int64_t i_ = 0;   // signed integers, and DateTime seconds
    std::uint64_t u_ = 0;  // unsigned integers, bool, and DateTime nanos
    double f_ = 0.0;       // Float32/Float64
    std::string str_;      // fixed-string kinds, untruncated
};

std::vector<std::uint8_t> encode_key(const KeyValue& v);
KeyValue decode_key(KeyKind kind, bytes_view b);

/// Three-way comparison. Throws if the kinds differ, rather than inventing
/// an ordering between unrelated types.
int compare_keys(const KeyValue& a, const KeyValue& b);

/// Sentinels used to lower open-ended range queries.
///
/// These reproduce the reference's quirks deliberately: the float maximum
/// is +inf even though NaN sorts above it (so NaN-keyed features are
/// invisible to range queries), and the DateTime minimum is epoch 0 even
/// though the wire format allows negative seconds.
KeyValue key_min(KeyKind kind);
KeyValue key_max(KeyKind kind);

/// Column type to key kind, following what the WRITER emits.
/// Byte maps to UInt8, not Int8 -- see the Byte note in key.cpp.
///
/// Takes the raw ubyte rather than the generated ::ColumnType so that
/// consumers of this header never see the generated FlatBuffers API.
/// ColumnInfo::type carries exactly this value.
KeyKind key_kind_for_column(std::uint8_t column_type);

}  // namespace fcb
