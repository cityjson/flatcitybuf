#pragma once

#include <cstdint>
#include <string>
#include <vector>

#include <fcb/error.hpp>
#include <fcb/header.hpp>
#include <fcb/span.hpp>

#ifdef FCB_WITH_JSON
#include <nlohmann/json.hpp>
#endif

namespace fcb {

/// One decoded attribute value.
///
/// Note this is the ATTRIBUTE BLOB encoding, which differs from the B+tree
/// KEY encoding for two types: DateTime is a length-prefixed string here but
/// 12 packed bytes as a key, and strings are length-prefixed here but fixed
/// width as keys.
struct AttrValue {
    enum class Type {
        Null,
        Bool,
        Int,     // any signed integer column
        UInt,    // any unsigned integer column
        Double,  // Float or Double
        String,  // String or DateTime
        Json,    // Json column, still as its raw text
    };

    Type type = Type::Null;
    bool b = false;
    std::int64_t i = 0;
    std::uint64_t u = 0;
    double d = 0.0;
    std::string s;
};

/// Decode a feature's attribute blob against the column schema.
///
/// Wire format (reader/deserializer.rs:249-372): repeated records of a
/// `u16` little-endian column index followed by the value, encoded per the
/// column's type. Fixed-width types are packed little-endian; String,
/// DateTime and Json are a `u32` little-endian byte length then UTF-8.
///
/// Throws on a column index absent from the schema, on a truncated record,
/// or on Byte/UByte/Binary -- which the writer emits but the Rust reader
/// rejects at deserializer.rs:372 (`unreachable!()`). Mirroring the
/// rejection is honest: those branches have never been exercised.
std::vector<std::pair<std::string, AttrValue>> decode_attributes(
    bytes_view blob, const std::vector<ColumnInfo>& schema);

#ifdef FCB_WITH_JSON
/// Same decode, rendered as a JSON object for CityJSON emission.
nlohmann::json attributes_to_json(bytes_view blob, const std::vector<ColumnInfo>& schema);
#endif

}  // namespace fcb
