#include <fcb/attribute.hpp>
#include <fcb/generated/header_generated.h>

#include <algorithm>
#include <cstring>
#include <unordered_map>

#include "detail/checked.hpp"

namespace fcb {

namespace {

template <typename T> T get_le(bytes_view b, std::size_t at) {
    using U = typename std::make_unsigned<T>::type;
    U u = 0;
    for (std::size_t i = 0; i < sizeof(T); ++i) {
        u |= static_cast<U>(b[at + i]) << (8 * i);
    }
    return static_cast<T>(u);
}

float get_f32(bytes_view b, std::size_t at) {
    const std::uint32_t bits = get_le<std::uint32_t>(b, at);
    float f;
    std::memcpy(&f, &bits, sizeof(f));
    return f;
}

double get_f64(bytes_view b, std::size_t at) {
    const std::uint64_t bits = get_le<std::uint64_t>(b, at);
    double d;
    std::memcpy(&d, &bits, sizeof(d));
    return d;
}

void need(bytes_view b, std::size_t at, std::size_t n, const char* what) {
    if (at > b.size() || b.size() - at < n) {
        throw Error(ErrorCode::InvalidAttributeValue,
                    std::string("truncated attribute blob reading ") + what);
    }
}

}  // namespace

std::vector<std::pair<std::string, AttrValue>>
decode_attributes(bytes_view blob, const std::vector<ColumnInfo>& schema) {
    std::vector<std::pair<std::string, AttrValue>> out;
    if (blob.empty())
        return out;

    std::unordered_map<std::uint16_t, const ColumnInfo*> by_index;
    by_index.reserve(schema.size());
    for (const auto& c : schema)
        by_index.emplace(c.index, &c);

    std::size_t at = 0;
    while (at < blob.size()) {
        need(blob, at, 2, "column index");
        const std::uint16_t col_index = get_le<std::uint16_t>(blob, at);
        at += 2;

        auto found = by_index.find(col_index);
        if (found == by_index.end()) {
            throw Error(ErrorCode::InvalidAttributeValue,
                        "attribute references unknown column index " + std::to_string(col_index));
        }
        const ColumnInfo& col = *found->second;
        const auto type = static_cast<::ColumnType>(col.type);

        AttrValue v{};
        switch (type) {
            case ::ColumnType::Bool:
                need(blob, at, 1, "Bool");
                v.type = AttrValue::Type::Bool;
                v.b = blob[at] != 0;
                at += 1;
                break;
            case ::ColumnType::Short:
                need(blob, at, 2, "Short");
                v.type = AttrValue::Type::Int;
                v.i = get_le<std::int16_t>(blob, at);
                at += 2;
                break;
            case ::ColumnType::UShort:
                need(blob, at, 2, "UShort");
                v.type = AttrValue::Type::UInt;
                v.u = get_le<std::uint16_t>(blob, at);
                at += 2;
                break;
            case ::ColumnType::Int:
                need(blob, at, 4, "Int");
                v.type = AttrValue::Type::Int;
                v.i = get_le<std::int32_t>(blob, at);
                at += 4;
                break;
            case ::ColumnType::UInt:
                need(blob, at, 4, "UInt");
                v.type = AttrValue::Type::UInt;
                v.u = get_le<std::uint32_t>(blob, at);
                at += 4;
                break;
            case ::ColumnType::Long:
                need(blob, at, 8, "Long");
                v.type = AttrValue::Type::Int;
                v.i = get_le<std::int64_t>(blob, at);
                at += 8;
                break;
            case ::ColumnType::ULong:
                need(blob, at, 8, "ULong");
                v.type = AttrValue::Type::UInt;
                v.u = get_le<std::uint64_t>(blob, at);
                at += 8;
                break;
            case ::ColumnType::Float:
                need(blob, at, 4, "Float");
                v.type = AttrValue::Type::Double;
                v.d = static_cast<double>(get_f32(blob, at));
                at += 4;
                break;
            case ::ColumnType::Double:
                need(blob, at, 8, "Double");
                v.type = AttrValue::Type::Double;
                v.d = get_f64(blob, at);
                at += 8;
                break;

            case ::ColumnType::String:
            case ::ColumnType::DateTime:
            case ::ColumnType::Json: {
                // u32 LE byte length, then UTF-8. Note DateTime is a STRING
                // here, unlike its 12-byte packed form as a B+tree key.
                need(blob, at, 4, "string length");
                const std::uint32_t len = get_le<std::uint32_t>(blob, at);
                at += 4;
                need(blob, at, len, "string body");
                v.type =
                    (type == ::ColumnType::Json) ? AttrValue::Type::Json : AttrValue::Type::String;
                v.s.assign(reinterpret_cast<const char*>(blob.data()) + at, len);
                at += len;
                break;
            }

            // Byte is UNSIGNED on the wire: the writer stores it as a raw u8
            // (writer/attribute.rs) and indexes it as u8 (writer/attr_index.rs),
            // so a stored 200 must read back as 200, not -56. The Rust reader
            // agrees on both its value and its index path.
            case ::ColumnType::Byte:
                need(blob, at, 1, "Byte");
                v.type = AttrValue::Type::UInt;
                v.u = get_le<std::uint8_t>(blob, at);
                at += 1;
                break;
            case ::ColumnType::UByte:
                need(blob, at, 1, "UByte");
                v.type = AttrValue::Type::UInt;
                v.u = get_le<std::uint8_t>(blob, at);
                at += 1;
                break;

            case ::ColumnType::Binary: {
                // u32 LE byte length, then that many raw bytes -- the same
                // framing as String, but the payload is not text.
                need(blob, at, 4, "binary length");
                const std::uint32_t len = get_le<std::uint32_t>(blob, at);
                at += 4;
                need(blob, at, len, "binary body");
                v.type = AttrValue::Type::Binary;
                v.s.assign(reinterpret_cast<const char*>(blob.data()) + at, len);
                at += len;
                break;
            }
        }

        out.emplace_back(col.name, std::move(v));
    }
    return out;
}

#ifdef FCB_WITH_JSON
nlohmann::json attributes_to_json(bytes_view blob, const std::vector<ColumnInfo>& schema) {
    nlohmann::json obj = nlohmann::json::object();
    for (auto& [name, v] : decode_attributes(blob, schema)) {
        switch (v.type) {
            case AttrValue::Type::Null:
                obj[name] = nullptr;
                break;
            case AttrValue::Type::Bool:
                obj[name] = v.b;
                break;
            case AttrValue::Type::Int:
                obj[name] = v.i;
                break;
            case AttrValue::Type::UInt:
                obj[name] = v.u;
                break;
            case AttrValue::Type::Double:
                obj[name] = v.d;
                break;
            case AttrValue::Type::String:
                obj[name] = v.s;
                break;
            case AttrValue::Type::Binary:
                // Raw bytes have no faithful JSON form; emit as a byte array.
                obj[name] = std::vector<std::uint8_t>(v.s.begin(), v.s.end());
                break;
            case AttrValue::Type::Json:
                // Stored as text; re-parse so it nests as real JSON, which is
                // what the Rust deserializer does (serde_json::from_str).
                obj[name] = nlohmann::json::parse(v.s, nullptr, /*allow_exceptions=*/false);
                break;
        }
    }
    return obj;
}
#endif

}  // namespace fcb
