#include <fcb/writer/attribute.hpp>

#ifdef FCB_WITH_JSON

#    include <algorithm>
#    include <cstring>
#    include <limits>
#    include <optional>
#    include <type_traits>

namespace fcb {

namespace {

/// Days since 1970-01-01 for a proleptic-Gregorian civil date. Howard
/// Hinnant's algorithm (http://howardhinnant.github.io/date_algorithms.html),
/// the same one std::chrono's civil calendar utilities implement.
std::int64_t days_from_civil(int y, unsigned m, unsigned d) {
    y -= (m <= 2) ? 1 : 0;
    const std::int64_t era = (y >= 0 ? y : y - 399) / 400;
    const unsigned yoe = static_cast<unsigned>(y - era * 400);
    const unsigned doy = (153 * (m + (m > 2 ? static_cast<unsigned>(-3) : 9)) + 2) / 5 + d - 1;
    const unsigned doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + static_cast<std::int64_t>(doe) - 719468;
}

struct Rfc3339Result {
    bool ok = false;
    std::int64_t seconds = 0;
    std::uint32_t nanos = 0;
};

/// A pragmatic RFC3339 acceptor: full-date "T" full-time, matching what
/// `chrono::DateTime::parse_from_rfc3339` accepts closely enough for every
/// string that appears in this project's fixtures (the conformance corpus
/// has exactly one date-like string, a bare date, which this correctly
/// rejects). Not a complete ISO 8601 grammar -- e.g. it does not reject
/// every historically-invalid leap second -- since nothing here depends on
/// those finer points.
Rfc3339Result parse_rfc3339(const std::string& s) {
    Rfc3339Result r;
    if (s.size() < 20)
        return r;  // shortest valid form: "YYYY-MM-DDTHH:MM:SSZ"

    auto is_digit = [](char c) { return c >= '0' && c <= '9'; };
    auto two = [&](std::size_t at) -> int { return (s[at] - '0') * 10 + (s[at + 1] - '0'); };

    for (std::size_t i : {0u, 1u, 2u, 3u, 5u, 6u, 8u, 9u})
        if (!is_digit(s[i]))
            return r;
    if (s[4] != '-' || s[7] != '-')
        return r;
    if (s[10] != 'T' && s[10] != 't' && s[10] != ' ')
        return r;
    for (std::size_t i : {11u, 12u, 14u, 15u, 17u, 18u})
        if (!is_digit(s[i]))
            return r;
    if (s[13] != ':' || s[16] != ':')
        return r;

    const int year = (s[0] - '0') * 1000 + (s[1] - '0') * 100 + (s[2] - '0') * 10 + (s[3] - '0');
    const int month = two(5);
    const int day = two(8);
    const int hour = two(11);
    const int minute = two(14);
    const int second = two(17);

    if (month < 1 || month > 12)
        return r;
    static const int days_in_month[] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
    const bool leap = (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
    const int max_day = (month == 2 && leap) ? 29 : days_in_month[month - 1];
    if (day < 1 || day > max_day)
        return r;
    if (hour > 23 || minute > 59 || second > 60)  // 60 == leap second
        return r;

    std::size_t pos = 19;
    std::uint32_t nanos = 0;
    if (pos < s.size() && s[pos] == '.') {
        const std::size_t start = ++pos;
        while (pos < s.size() && is_digit(s[pos]))
            ++pos;
        if (pos == start)
            return r;  // '.' with no digits after it
        std::string frac = s.substr(start, pos - start);
        if (frac.size() > 9)
            frac.resize(9);
        else
            frac.resize(9, '0');
        nanos = static_cast<std::uint32_t>(std::stoul(frac));
    }
    if (pos >= s.size())
        return r;  // no timezone marker at all -- not RFC3339

    std::int64_t offset_seconds = 0;
    if (s[pos] == 'Z' || s[pos] == 'z') {
        ++pos;
    } else if (s[pos] == '+' || s[pos] == '-') {
        const bool neg = s[pos] == '-';
        ++pos;
        if (pos + 5 > s.size() || !is_digit(s[pos]) || !is_digit(s[pos + 1]) || s[pos + 2] != ':' ||
            !is_digit(s[pos + 3]) || !is_digit(s[pos + 4]))
            return r;
        const int oh = two(pos);
        const int om = two(pos + 3);
        if (oh > 23 || om > 59)
            return r;
        offset_seconds = (oh * 3600 + om * 60) * (neg ? -1 : 1);
        pos += 5;
    } else {
        return r;
    }
    if (pos != s.size())
        return r;  // trailing garbage after the offset

    const std::int64_t days =
        days_from_civil(year, static_cast<unsigned>(month), static_cast<unsigned>(day));
    // A leap second (:60) is chrono's `NaiveTime` convention: it occupies the
    // SAME epoch second as :59, flagged by adding 1_000_000_000 to the
    // nanosecond field, rather than rolling over into the next minute. Rolling
    // over (as a naive `+ second` would) both produces the wrong epoch second
    // and can wrap a leap second at day's end onto the NEXT day's midnight.
    const int second_for_epoch = (second == 60) ? 59 : second;
    if (second == 60)
        nanos += 1'000'000'000u;
    r.ok = true;
    r.seconds = days * 86400 + hour * 3600 + minute * 60 + second_for_epoch - offset_seconds;
    r.nanos = nanos;
    return r;
}

bool looks_like_rfc3339(const std::string& s) { return parse_rfc3339(s).ok; }

std::optional<::ColumnType> guess_type(const nlohmann::ordered_json& value) {
    if (value.is_boolean())
        return ::ColumnType::Bool;
    if (value.is_number()) {
        if (value.is_number_float())
            return ::ColumnType::Double;
        if (value.is_number_unsigned())
            return ::ColumnType::ULong;
        if (value.is_number_integer()) {
            // serde_json::Number splits PosInt/NegInt by the VALUE's sign,
            // regardless of the Rust literal's own signed/unsigned type. A
            // C++ value built via `nlohmann::ordered_json(5)` (as opposed to parsed
            // from JSON text with no leading '-') lands in nlohmann's signed
            // `number_integer_t` bucket even though 5 is non-negative, so
            // this must also decide by value, not by which nlohmann bucket
            // it landed in, to stay oracle-compatible for callers that
            // construct JSON directly rather than parsing it.
            const std::int64_t i = value.get<std::int64_t>();
            return i < 0 ? ::ColumnType::Long : ::ColumnType::ULong;
        }
        return ::ColumnType::ULong;
    }
    if (value.is_string())
        return looks_like_rfc3339(value.get_ref<const std::string&>()) ? ::ColumnType::DateTime
                                                                       : ::ColumnType::String;
    if (value.is_array() || value.is_object())
        return ::ColumnType::Json;
    return std::nullopt;  // null, or anything else
}

template <typename T> void put_le(std::vector<std::uint8_t>& out, std::size_t at, T v) {
    using U = typename std::make_unsigned<T>::type;
    const U u = static_cast<U>(v);
    for (std::size_t i = 0; i < sizeof(T); ++i)
        out[at + i] = static_cast<std::uint8_t>(u >> (8 * i));
}

void put_f32(std::vector<std::uint8_t>& out, std::size_t at, float f) {
    std::uint32_t bits;
    std::memcpy(&bits, &f, sizeof(bits));
    put_le<std::uint32_t>(out, at, bits);
}

void put_f64(std::vector<std::uint8_t>& out, std::size_t at, double d) {
    std::uint64_t bits;
    std::memcpy(&bits, &d, sizeof(bits));
    put_le<std::uint64_t>(out, at, bits);
}

// The following mirror serde_json::Value::as_i64/as_u64/as_f64/as_bool/as_str:
// a type/range mismatch yields the fallback rather than throwing, because the
// Rust writer being ported never throws on this path either -- a value whose
// JSON shape drifted from the schema's remembered column type is written as
// the type's zero value, not rejected.

std::int64_t as_i64_or0(const nlohmann::ordered_json& v) {
    if (v.is_number_unsigned()) {
        const std::uint64_t u = v.get<std::uint64_t>();
        return u <= static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max())
                   ? static_cast<std::int64_t>(u)
                   : 0;
    }
    if (v.is_number_integer())
        return v.get<std::int64_t>();
    return 0;
}

std::uint64_t as_u64_or0(const nlohmann::ordered_json& v) {
    if (v.is_number_unsigned())
        return v.get<std::uint64_t>();
    if (v.is_number_integer()) {
        const std::int64_t i = v.get<std::int64_t>();
        return i >= 0 ? static_cast<std::uint64_t>(i) : 0;
    }
    return 0;
}

double as_f64_or0(const nlohmann::ordered_json& v) { return v.is_number() ? v.get<double>() : 0.0; }

bool as_bool_or_false(const nlohmann::ordered_json& v) { return v.is_boolean() && v.get<bool>(); }

std::string as_str_or_empty(const nlohmann::ordered_json& v) {
    return v.is_string() ? v.get<std::string>() : std::string();
}

}  // namespace

void add_attributes(AttributeSchema& schema, const nlohmann::ordered_json& attrs) {
    if (!attrs.is_object()) {
        // Rust's `BTreeMap::insert` here ALWAYS overwrites -- even a "json"
        // column that already exists gets reassigned a new index equal to
        // the map's current size, which can orphan whatever previously held
        // that index. That is a real quirk of the oracle, not a bug to
        // paper over: `insert_or_assign` (not the no-op-on-existing-key
        // `emplace`) is what reproduces it exactly.
        schema.insert_or_assign(
            "json", std::make_pair(static_cast<std::uint16_t>(schema.size()), ::ColumnType::Json));
        return;
    }
    for (const auto& [key, val] : attrs.items()) {
        if (schema.find(key) != schema.end() || val.is_null())
            continue;
        if (auto coltype = guess_type(val)) {
            schema.emplace(key,
                           std::make_pair(static_cast<std::uint16_t>(schema.size()), *coltype));
        }
    }
}

std::size_t attr_size(::ColumnType coltype, const nlohmann::ordered_json& colval) {
    switch (coltype) {
        case ::ColumnType::Byte:
            return sizeof(std::int8_t);
        case ::ColumnType::UByte:
            return sizeof(std::uint8_t);
        case ::ColumnType::Bool:
            return sizeof(std::uint8_t);
        case ::ColumnType::Short:
            return sizeof(std::int16_t);
        case ::ColumnType::UShort:
            return sizeof(std::uint16_t);
        case ::ColumnType::Int:
            return sizeof(std::int32_t);
        case ::ColumnType::UInt:
            return sizeof(std::uint32_t);
        case ::ColumnType::Long:
            return sizeof(std::int64_t);
        case ::ColumnType::ULong:
            return sizeof(std::uint64_t);
        case ::ColumnType::Float:
            return sizeof(float);
        case ::ColumnType::Double:
            return sizeof(double);
        case ::ColumnType::String:
        case ::ColumnType::DateTime:
            return sizeof(std::uint32_t) + as_str_or_empty(colval).size();
        case ::ColumnType::Json:
            return sizeof(std::uint32_t) + colval.dump().size();
        case ::ColumnType::Binary:
            return sizeof(std::uint32_t) + as_str_or_empty(colval).size();
    }
    throw Error(ErrorCode::UnsupportedColumnType, "attr_size: unknown column type");
}

std::vector<std::uint8_t> encode_attributes_with_schema(const nlohmann::ordered_json& attr,
                                                        const AttributeSchema& schema) {
    std::vector<std::uint8_t> out;
    if (!attr.is_object() || attr.empty())
        return out;

    std::vector<std::pair<std::string, std::pair<std::uint16_t, ::ColumnType>>> sorted(
        schema.begin(), schema.end());
    std::sort(sorted.begin(), sorted.end(),
              [](const auto& a, const auto& b) { return a.second.first < b.second.first; });

    for (const auto& [name, idx_type] : sorted) {
        const auto [index, coltype] = idx_type;
        auto it = attr.find(name);
        if (it == attr.end() || it->is_null())
            continue;
        const nlohmann::ordered_json& val = *it;

        const std::size_t offset = out.size();
        const std::size_t size = attr_size(coltype, val);
        out.resize(offset + sizeof(std::uint16_t) + size, 0);
        put_le<std::uint16_t>(out, offset, index);
        const std::size_t value_offset = offset + sizeof(std::uint16_t);

        switch (coltype) {
            case ::ColumnType::Bool:
                out[value_offset] = as_bool_or_false(val) ? 1 : 0;
                break;
            case ::ColumnType::Int:
                put_le<std::int32_t>(out, value_offset, static_cast<std::int32_t>(as_i64_or0(val)));
                break;
            case ::ColumnType::UInt:
                put_le<std::uint32_t>(out, value_offset,
                                      static_cast<std::uint32_t>(as_u64_or0(val)));
                break;
            case ::ColumnType::Byte:
                out[value_offset] = static_cast<std::uint8_t>(as_i64_or0(val));
                break;
            case ::ColumnType::UByte:
                out[value_offset] = static_cast<std::uint8_t>(as_u64_or0(val));
                break;
            case ::ColumnType::Short:
                put_le<std::int16_t>(out, value_offset, static_cast<std::int16_t>(as_i64_or0(val)));
                break;
            case ::ColumnType::UShort:
                put_le<std::uint16_t>(out, value_offset,
                                      static_cast<std::uint16_t>(as_u64_or0(val)));
                break;
            case ::ColumnType::Long:
                put_le<std::int64_t>(out, value_offset, as_i64_or0(val));
                break;
            case ::ColumnType::ULong:
                put_le<std::uint64_t>(out, value_offset, as_u64_or0(val));
                break;
            case ::ColumnType::Float:
                put_f32(out, value_offset, static_cast<float>(as_f64_or0(val)));
                break;
            case ::ColumnType::Double:
                put_f64(out, value_offset, as_f64_or0(val));
                break;
            case ::ColumnType::String:
            case ::ColumnType::DateTime: {
                const std::string s = as_str_or_empty(val);
                put_le<std::uint32_t>(out, value_offset, static_cast<std::uint32_t>(s.size()));
                std::memcpy(out.data() + value_offset + sizeof(std::uint32_t), s.data(), s.size());
                break;
            }
            case ::ColumnType::Json: {
                const std::string json_str = val.dump();
                put_le<std::uint32_t>(out, value_offset,
                                      static_cast<std::uint32_t>(json_str.size()));
                std::memcpy(out.data() + value_offset + sizeof(std::uint32_t), json_str.data(),
                            json_str.size());
                break;
            }
            case ::ColumnType::Binary: {
                const std::string s = as_str_or_empty(val);
                put_le<std::uint32_t>(out, value_offset, static_cast<std::uint32_t>(s.size()));
                std::memcpy(out.data() + value_offset + sizeof(std::uint32_t), s.data(), s.size());
                break;
            }
        }
    }
    return out;
}

::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Column>>>
to_columns(::flatbuffers::FlatBufferBuilder& fbb, const AttributeSchema& schema) {
    std::vector<std::pair<std::string, std::pair<std::uint16_t, ::ColumnType>>> sorted(
        schema.begin(), schema.end());
    std::sort(sorted.begin(), sorted.end(),
              [](const auto& a, const auto& b) { return a.second.first < b.second.first; });

    std::vector<::flatbuffers::Offset<::Column>> columns;
    columns.reserve(sorted.size());
    for (const auto& [name, idx_type] : sorted) {
        auto name_off = fbb.CreateString(name);
        columns.push_back(CreateColumn(fbb, idx_type.first, name_off, idx_type.second));
    }
    return fbb.CreateVector(columns);
}

std::vector<AttributeIndexEntry>
attribute_to_index_entries(const nlohmann::ordered_json& attr, const AttributeSchema& schema,
                           const std::vector<std::string>& indexing_attr) {
    std::vector<AttributeIndexEntry> out;
    if (!attr.is_object() || attr.empty())
        return out;

    for (const auto& name : indexing_attr) {
        auto val_it = attr.find(name);
        if (val_it == attr.end())
            continue;
        auto schema_it = schema.find(name);
        if (schema_it == schema.end())
            continue;
        const auto [index, coltype] = schema_it->second;
        const nlohmann::ordered_json& val = *val_it;

        switch (coltype) {
            case ::ColumnType::Bool:
                out.push_back({index, KeyValue::from_bool(as_bool_or_false(val))});
                break;
            case ::ColumnType::Int:
                out.push_back(
                    {index, KeyValue::from_i32(static_cast<std::int32_t>(as_i64_or0(val)))});
                break;
            case ::ColumnType::UInt:
                out.push_back(
                    {index, KeyValue::from_u32(static_cast<std::uint32_t>(as_u64_or0(val)))});
                break;
            case ::ColumnType::Long:
                out.push_back({index, KeyValue::from_i64(as_i64_or0(val))});
                break;
            case ::ColumnType::ULong:
                out.push_back({index, KeyValue::from_u64(as_u64_or0(val))});
                break;
            case ::ColumnType::Float:
                out.push_back({index, KeyValue::from_f32(static_cast<float>(as_f64_or0(val)))});
                break;
            case ::ColumnType::Double:
                out.push_back({index, KeyValue::from_f64(as_f64_or0(val))});
                break;
            case ::ColumnType::String:
                out.push_back(
                    {index, KeyValue::from_string(KeyKind::String50, as_str_or_empty(val))});
                break;
            case ::ColumnType::DateTime: {
                const Rfc3339Result parsed = parse_rfc3339(as_str_or_empty(val));
                out.push_back({index, KeyValue::from_datetime(parsed.ok ? parsed.seconds : 0,
                                                              parsed.ok ? parsed.nanos : 0)});
                break;
            }
            case ::ColumnType::Byte:
            case ::ColumnType::UByte:
            case ::ColumnType::Short:
            case ::ColumnType::UShort:
            case ::ColumnType::Json:
            case ::ColumnType::Binary:
                // Not supported for indexing at extraction time -- matches
                // writer/attribute.rs's `attribute_to_index_entries`.
                break;
        }
    }
    return out;
}

std::vector<AttributeIndexEntry>
cityfeature_to_index_entries(const nlohmann::ordered_json& city_feature,
                             const AttributeSchema& schema,
                             const std::vector<std::string>& indexing_attr) {
    std::vector<AttributeIndexEntry> out;
    auto co_it = city_feature.find("CityObjects");
    if (co_it == city_feature.end() || !co_it->is_object())
        return out;

    std::vector<std::string> object_ids;
    object_ids.reserve(co_it->size());
    for (const auto& [id, unused] : co_it->items())
        object_ids.push_back(id);
    std::sort(object_ids.begin(), object_ids.end());

    for (const auto& id : object_ids) {
        const nlohmann::ordered_json& co = co_it->at(id);
        auto attr_it = co.find("attributes");
        if (attr_it == co.end() || attr_it->is_null())
            continue;
        auto entries = attribute_to_index_entries(*attr_it, schema, indexing_attr);
        out.insert(out.end(), entries.begin(), entries.end());
    }
    return out;
}

}  // namespace fcb

#endif  // FCB_WITH_JSON
