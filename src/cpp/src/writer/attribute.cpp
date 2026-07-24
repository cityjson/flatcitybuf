#include <fcb/writer/attribute.hpp>

#ifdef FCB_WITH_JSON

#    include <algorithm>
#    include <cstring>
#    include <limits>
#    include <optional>

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
    r.ok = true;
    r.seconds = days * 86400 + hour * 3600 + minute * 60 + second - offset_seconds;
    r.nanos = nanos;
    return r;
}

bool looks_like_rfc3339(const std::string& s) { return parse_rfc3339(s).ok; }

std::optional<::ColumnType> guess_type(const nlohmann::json& value) {
    if (value.is_boolean())
        return ::ColumnType::Bool;
    if (value.is_number()) {
        if (value.is_number_float())
            return ::ColumnType::Double;
        if (value.is_number_unsigned())
            return ::ColumnType::ULong;
        if (value.is_number_integer())
            return ::ColumnType::Long;
        return ::ColumnType::ULong;
    }
    if (value.is_string())
        return looks_like_rfc3339(value.get_ref<const std::string&>()) ? ::ColumnType::DateTime
                                                                       : ::ColumnType::String;
    if (value.is_array() || value.is_object())
        return ::ColumnType::Json;
    return std::nullopt;  // null, or anything else
}

}  // namespace

void add_attributes(AttributeSchema& schema, const nlohmann::json& attrs) {
    if (!attrs.is_object()) {
        schema.emplace(
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

}  // namespace fcb

#endif  // FCB_WITH_JSON
