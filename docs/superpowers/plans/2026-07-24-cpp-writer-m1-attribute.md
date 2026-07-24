# C++ writer M1: attribute schema + byte encoding — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port `src/rust/fcb_core/src/writer/attribute.rs` to C++: an `AttributeSchema` type, JSON-to-schema inference, the attribute byte-blob encoder, the `Column` FlatBuffers vector builder, and B+tree index-entry extraction.

**Architecture:** One new header/source pair, `include/fcb/writer/attribute.hpp` + `src/writer/attribute.cpp`, added to `fcb_core_cpp` under the existing `FCB_WITH_JSON` guard. All new symbols live directly in `namespace fcb` (matching the reader's flat-namespace convention; the `writer/` directory is what disambiguates from `fcb/attribute.hpp`, the read side).

**Tech Stack:** C++17, `nlohmann::json` (already a dependency), the generated `header_generated.h` (`::ColumnType`, `::Column`, `CreateColumn`), `doctest` for tests.

## Global Constraints

- Little-endian, always, for every multi-byte value this milestone writes (`CLAUDE.md`).
- `AttributeSchema` must be an *ordered* map keyed by name (mirrors Rust's `BTreeMap`) — never `std::unordered_map` — because column-index assignment order for multiple new keys discovered in the same call depends on iteration order, and both `nlohmann::json`'s default object type (`std::map`) and `serde_json`'s default `Map` (`BTreeMap`, since this workspace does not enable the `preserve_order` feature) iterate alphabetically. This is what makes column-index assignment match Rust's without extra sorting.
- Only these 9 column types produce B+tree index entries at extraction time: `Bool, Int, UInt, Long, ULong, Float, Double, String, DateTime`. `Byte, UByte, Short, UShort, Json, Binary` are silently skipped here — this is a deliberate, documented divergence in the Rust writer itself (`.llm/docs/specification.md`, "known divergences" #1), not a gap to fix.
- Reuse `fcb::KeyValue`/`fcb::KeyKind` (`include/fcb/key.hpp`) for index-entry values rather than inventing a parallel tagged union — they already model exactly this value space.
- No CLI. This is a library-only change.
- **Build every JSON test fixture with `nlohmann::json::parse("...")` on real JSON text, never a C++ initializer-list literal, whenever a numeric value's signedness matters.** Discovered while running Task 1's first test: `nlohmann::json{{"uint", 5}}` stores `5` as `number_integer_t` (signed), because the C++ literal `5`'s static type is `int` — but nlohmann's *parser*, given the text `5` with no leading `-`, stores it as `number_unsigned_t`, exactly matching `serde_json::Number`'s sign-based `PosInt`/`NegInt` split (which is what `guess_type` on the Rust side actually keys off, regardless of the value's original signed/unsigned Rust type). Since every real caller feeds this code parsed JSONL text, `::parse` fixtures are also the more representative test, not just the passing one.

---

### Task 1: `AttributeSchema`, `add_attributes`, `guess_type`, RFC3339 detection

**Files:**
- Create: `src/cpp/include/fcb/writer/attribute.hpp`
- Create: `src/cpp/src/writer/attribute.cpp`
- Test: `src/cpp/tests/test_writer_attribute.cpp`
- Modify: `src/cpp/CMakeLists.txt` (add `src/writer/attribute.cpp` to `fcb_core_cpp` sources, under `FCB_WITH_JSON`)
- Modify: `src/cpp/tests/CMakeLists.txt` (add `test_writer_attribute.cpp`)

**Interfaces:**
- Produces: `fcb::AttributeSchema` (`std::map<std::string, std::pair<std::uint16_t, ::ColumnType>>`), `void fcb::add_attributes(AttributeSchema&, const nlohmann::json&)`.
- Internal (anonymous namespace in the .cpp, not exposed): `std::optional<::ColumnType> guess_type(const nlohmann::json&)`, `bool looks_like_rfc3339(const std::string&)`, `Rfc3339Result parse_rfc3339(const std::string&)` (the latter is reused by Task 4, so it moves to an unnamed namespace visible to the whole .cpp file, not file-local to a single function).

- [ ] **Step 1: Write the failing test**

Create `src/cpp/tests/test_writer_attribute.cpp`:

```cpp
#include <fcb/writer/attribute.hpp>

#include <doctest/doctest.h>

using namespace fcb;

TEST_CASE("add_attributes assigns column indices in first-seen, alphabetical order") {
    AttributeSchema schema;
    add_attributes(schema, nlohmann::json{
        {"int", -10},
        {"uint", 5},
        {"bool", true},
        {"float", 1.0},
        {"string", "hoge"},
        {"array", nlohmann::json::array({1, 2, 3})},
        {"json", nlohmann::json{{"hoge", "fuga"}}},
        {"null", nullptr},
    });

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
    add_attributes(schema, nlohmann::json{{"neg", -1}, {"pos", 1}});
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src/cpp && just test 2>&1 | tail -40` (this will fail at the CMake/compile stage since `fcb/writer/attribute.hpp` does not exist yet and the test file is not yet wired into `tests/CMakeLists.txt`).
Expected: FAIL — configure or compile error naming `fcb/writer/attribute.hpp` or `test_writer_attribute.cpp` as missing.

- [ ] **Step 3: Wire the new files into CMake**

In `src/cpp/CMakeLists.txt`, inside the existing `if(FCB_WITH_JSON)` block (after the `nlohmann_json` link lines), add:

```cmake
    target_sources(fcb_core_cpp PRIVATE src/writer/attribute.cpp)
```

In `src/cpp/tests/CMakeLists.txt`, add `test_writer_attribute.cpp` to the `add_executable(fcb_tests ...)` list (alongside `test_attributes.cpp`).

- [ ] **Step 4: Write the implementation**

Create `src/cpp/include/fcb/writer/attribute.hpp`:

```cpp
#pragma once

#ifdef FCB_WITH_JSON

#    include <fcb/generated/header_generated.h>
#    include <fcb/key.hpp>

#    include <cstdint>
#    include <map>
#    include <string>
#    include <utility>
#    include <vector>

#    include <flatbuffers/flatbuffers.h>
#    include <nlohmann/json.hpp>

namespace fcb {

/// Attribute schema: name -> (column index, column type).
///
/// `std::map`, not `std::unordered_map`, mirrors Rust's `BTreeMap`
/// (writer/attribute.rs): the column INDEX comes from `schema.size()` at
/// insert time, independent of the map's own order, but both nlohmann's
/// default object type and serde_json's default `Map` iterate a JSON
/// object's keys alphabetically -- so when several new attributes appear
/// together in one `add_attributes` call, both languages assign indices in
/// the same (alphabetical) order without any extra sorting.
using AttributeSchema = std::map<std::string, std::pair<std::uint16_t, ::ColumnType>>;

/// Adds every member of a JSON object to `schema`, assigning each new,
/// non-null name the next free column index. A non-object `attrs` becomes a
/// single "json" column, matching the writer's fallback for untyped
/// attribute payloads. Existing names and null values are left alone.
void add_attributes(AttributeSchema& schema, const nlohmann::json& attrs);

}  // namespace fcb

#endif  // FCB_WITH_JSON
```

Create `src/cpp/src/writer/attribute.cpp`:

```cpp
#include <fcb/writer/attribute.hpp>

#ifdef FCB_WITH_JSON

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

    const std::int64_t days = days_from_civil(year, static_cast<unsigned>(month),
                                               static_cast<unsigned>(day));
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
        schema.emplace("json", std::make_pair(static_cast<std::uint16_t>(schema.size()),
                                              ::ColumnType::Json));
        return;
    }
    for (const auto& [key, val] : attrs.items()) {
        if (schema.find(key) != schema.end() || val.is_null())
            continue;
        if (auto coltype = guess_type(val)) {
            schema.emplace(key, std::make_pair(static_cast<std::uint16_t>(schema.size()), *coltype));
        }
    }
}

}  // namespace fcb

#endif  // FCB_WITH_JSON
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src/cpp && just test 2>&1 | tail -60`
Expected: PASS — all `TEST_CASE`s in `test_writer_attribute.cpp` succeed, and no existing test regresses.

- [ ] **Step 6: Commit**

```bash
git add src/cpp/include/fcb/writer/attribute.hpp src/cpp/src/writer/attribute.cpp \
        src/cpp/tests/test_writer_attribute.cpp src/cpp/CMakeLists.txt src/cpp/tests/CMakeLists.txt
git commit -m "feat(cpp): add attribute schema inference for the writer (M1 part 1)"
```

---

### Task 2: `attr_size` and `encode_attributes_with_schema`

**Files:**
- Modify: `src/cpp/include/fcb/writer/attribute.hpp`
- Modify: `src/cpp/src/writer/attribute.cpp`
- Modify: `src/cpp/tests/test_writer_attribute.cpp`

**Interfaces:**
- Consumes: `fcb::AttributeSchema` (Task 1).
- Produces: `std::size_t fcb::attr_size(::ColumnType, const nlohmann::json&)`, `std::vector<std::uint8_t> fcb::encode_attributes_with_schema(const nlohmann::json&, const AttributeSchema&)`.

- [ ] **Step 1: Write the failing test**

Append to `src/cpp/tests/test_writer_attribute.cpp`:

```cpp
#include <fcb/attribute.hpp>  // decode_attributes, the read-side inverse, used to verify round-trips

static std::vector<ColumnInfo> to_column_info(const AttributeSchema& schema) {
    std::vector<ColumnInfo> out;
    for (const auto& [name, idx_type] : schema)
        out.push_back({idx_type.first, name, static_cast<std::uint8_t>(idx_type.second), true});
    return out;
}

TEST_CASE("encode then decode round-trips every basic column type") {
    // Parsed from JSON text -- see the Global Constraints note on why a bare
    // C++ initializer-list literal like `{"uint", 5}` would misclassify.
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src/cpp && just test 2>&1 | tail -40`
Expected: FAIL — compile error, `encode_attributes_with_schema`/`attr_size` not declared.

- [ ] **Step 3: Write the implementation**

Add to `src/cpp/include/fcb/writer/attribute.hpp`, inside `namespace fcb`, after `add_attributes`:

```cpp
/// Byte width one value of `coltype` occupies in the attribute blob,
/// EXCLUDING the 2-byte column-index prefix every record also carries.
std::size_t attr_size(::ColumnType coltype, const nlohmann::json& colval);

/// Encodes `attr` (a CityJSON attributes object) against `schema`: repeated
/// `[u16 LE column index][value]` records, one per schema member present
/// and non-null in `attr`, in ascending column-index order (NOT `attr`'s own
/// JSON key order). A schema member absent from `attr`, or explicitly null,
/// is skipped -- not zero-filled. Returns an empty vector for a non-object
/// or empty `attr`.
std::vector<std::uint8_t> encode_attributes_with_schema(const nlohmann::json& attr,
                                                        const AttributeSchema& schema);
```

Add to `src/cpp/src/writer/attribute.cpp`, inside the anonymous namespace (after `guess_type`):

```cpp
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

std::int64_t as_i64_or0(const nlohmann::json& v) {
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

std::uint64_t as_u64_or0(const nlohmann::json& v) {
    if (v.is_number_unsigned())
        return v.get<std::uint64_t>();
    if (v.is_number_integer()) {
        const std::int64_t i = v.get<std::int64_t>();
        return i >= 0 ? static_cast<std::uint64_t>(i) : 0;
    }
    return 0;
}

double as_f64_or0(const nlohmann::json& v) { return v.is_number() ? v.get<double>() : 0.0; }

bool as_bool_or_false(const nlohmann::json& v) { return v.is_boolean() && v.get<bool>(); }

std::string as_str_or_empty(const nlohmann::json& v) {
    return v.is_string() ? v.get<std::string>() : std::string();
}

}  // namespace  -- NOTE: keep this closing brace where the anonymous
                 // namespace from Task 1 already ends; these functions are
                 // added INSIDE that same anonymous namespace, above it.
```

(The comment inside the code block above is an instruction to the implementer, not something to paste literally: add these functions to the *existing* anonymous namespace opened in Task 1, immediately before its closing `}  // namespace`, rather than opening a second one.)

Then add, after the anonymous namespace closes (alongside `add_attributes`):

```cpp
std::size_t attr_size(::ColumnType coltype, const nlohmann::json& colval) {
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

std::vector<std::uint8_t> encode_attributes_with_schema(const nlohmann::json& attr,
                                                        const AttributeSchema& schema) {
    std::vector<std::uint8_t> out;
    if (!attr.is_object() || attr.empty())
        return out;

    std::vector<std::pair<std::string, std::pair<std::uint16_t, ::ColumnType>>> sorted(schema.begin(),
                                                                                        schema.end());
    std::sort(sorted.begin(), sorted.end(),
              [](const auto& a, const auto& b) { return a.second.first < b.second.first; });

    for (const auto& [name, idx_type] : sorted) {
        const auto [index, coltype] = idx_type;
        auto it = attr.find(name);
        if (it == attr.end() || it->is_null())
            continue;
        const nlohmann::json& val = *it;

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
                put_le<std::uint32_t>(out, value_offset, static_cast<std::uint32_t>(as_u64_or0(val)));
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
                put_le<std::uint16_t>(out, value_offset, static_cast<std::uint16_t>(as_u64_or0(val)));
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
                put_le<std::uint32_t>(out, value_offset, static_cast<std::uint32_t>(json_str.size()));
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
```

Add `#include <algorithm>`, `#include <cstring>`, `#include <limits>` to the top of `src/cpp/src/writer/attribute.cpp`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src/cpp && just test 2>&1 | tail -60`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/cpp/include/fcb/writer/attribute.hpp src/cpp/src/writer/attribute.cpp \
        src/cpp/tests/test_writer_attribute.cpp
git commit -m "feat(cpp): add attribute byte-blob encoder for the writer (M1 part 2)"
```

---

### Task 3: `to_columns`

**Files:**
- Modify: `src/cpp/include/fcb/writer/attribute.hpp`
- Modify: `src/cpp/src/writer/attribute.cpp`
- Modify: `src/cpp/tests/test_writer_attribute.cpp`

**Interfaces:**
- Consumes: `fcb::AttributeSchema` (Task 1).
- Produces: `flatbuffers::Offset<flatbuffers::Vector<flatbuffers::Offset<::Column>>> fcb::to_columns(flatbuffers::FlatBufferBuilder&, const AttributeSchema&)`.

- [ ] **Step 1: Write the failing test**

Append to `src/cpp/tests/test_writer_attribute.cpp`:

```cpp
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
    // Emitted in ascending column-index order: "b_col" is index 0 (first
    // alphabetically among the two new names), "a_col" is index 1.
    CHECK(h->columns()->Get(0)->name()->str() == "b_col");
    CHECK(h->columns()->Get(0)->index() == 0);
    CHECK(h->columns()->Get(0)->type() == ::ColumnType::Bool);
    CHECK(h->columns()->Get(1)->name()->str() == "a_col");
    CHECK(h->columns()->Get(1)->index() == 1);
    CHECK(h->columns()->Get(1)->type() == ::ColumnType::Long);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src/cpp && just test 2>&1 | tail -40`
Expected: FAIL — `to_columns` not declared.

- [ ] **Step 3: Write the implementation**

Add to `src/cpp/include/fcb/writer/attribute.hpp`, after `encode_attributes_with_schema`:

```cpp
/// Builds the `Column` vector for `Header.columns` or `CityObject.columns`,
/// in ascending column-index order.
::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Column>>>
to_columns(::flatbuffers::FlatBufferBuilder& fbb, const AttributeSchema& schema);
```

Add to `src/cpp/src/writer/attribute.cpp` (after `encode_attributes_with_schema`):

```cpp
::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Column>>>
to_columns(::flatbuffers::FlatBufferBuilder& fbb, const AttributeSchema& schema) {
    std::vector<std::pair<std::string, std::pair<std::uint16_t, ::ColumnType>>> sorted(schema.begin(),
                                                                                        schema.end());
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src/cpp && just test 2>&1 | tail -60`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/cpp/include/fcb/writer/attribute.hpp src/cpp/src/writer/attribute.cpp \
        src/cpp/tests/test_writer_attribute.cpp
git commit -m "feat(cpp): add Column vector builder for the writer (M1 part 3)"
```

---

### Task 4: `AttributeIndexEntry`, `attribute_to_index_entries`, `cityfeature_to_index_entries`

**Files:**
- Modify: `src/cpp/include/fcb/writer/attribute.hpp`
- Modify: `src/cpp/src/writer/attribute.cpp`
- Modify: `src/cpp/tests/test_writer_attribute.cpp`

**Interfaces:**
- Consumes: `fcb::AttributeSchema` (Task 1), `fcb::KeyValue`/`fcb::KeyKind` (`include/fcb/key.hpp`, already exists).
- Produces: `struct fcb::AttributeIndexEntry { std::uint16_t index; KeyValue value; }`, `std::vector<AttributeIndexEntry> fcb::attribute_to_index_entries(const nlohmann::json&, const AttributeSchema&, const std::vector<std::string>&)`, `std::vector<AttributeIndexEntry> fcb::cityfeature_to_index_entries(const nlohmann::json&, const AttributeSchema&, const std::vector<std::string>&)`.

- [ ] **Step 1: Write the failing test**

Append to `src/cpp/tests/test_writer_attribute.cpp`:

```cpp
TEST_CASE("attribute_to_index_entries extracts only the requested, indexable columns") {
    AttributeSchema schema;
    nlohmann::json attrs = {
        {"name", "Building A"},
        {"height", 12.5},
        {"count", 3},
        {"flag", true},
        {"raw", nlohmann::json{{"nested", 1}}},  // Json column: never indexed
    };
    add_attributes(schema, attrs);

    auto entries = attribute_to_index_entries(attrs, schema, {"name", "height", "count", "flag", "raw"});
    // "raw" is a Json column -- attribute_to_index_entries never produces an
    // entry for it, matching the Rust writer's own gap (see spec doc).
    REQUIRE(entries.size() == 4);

    auto find = [&](std::uint16_t idx) -> const AttributeIndexEntry& {
        for (auto& e : entries)
            if (e.index == idx)
                return e;
        FAIL("missing entry for column " << idx);
        static AttributeIndexEntry dummy{0, KeyValue()};
        return dummy;
    };
    CHECK(find(schema.at("name").first).value.kind() == KeyKind::String50);
    CHECK(find(schema.at("height").first).value.kind() == KeyKind::Float64);
    CHECK(find(schema.at("flag").first).value.kind() == KeyKind::Bool);
}

TEST_CASE("attribute_to_index_entries parses DateTime strings, falling back to epoch on failure") {
    AttributeSchema schema;
    nlohmann::json attrs = {{"ts", "2010-10-13T12:29:24Z"}, {"bad_ts", "not a date"}};
    schema.emplace("ts", std::make_pair(0, ::ColumnType::DateTime));
    schema.emplace("bad_ts", std::make_pair(1, ::ColumnType::DateTime));

    auto entries = attribute_to_index_entries(attrs, schema, {"ts", "bad_ts"});
    REQUIRE(entries.size() == 2);
    for (auto& e : entries)
        CHECK(e.value.kind() == KeyKind::DateTime);
}

TEST_CASE("cityfeature_to_index_entries visits CityObjects in ascending id order") {
    AttributeSchema schema;
    schema.emplace("n", std::make_pair(0, ::ColumnType::ULong));

    nlohmann::json feature = {
        {"type", "CityJSONFeature"},
        {"id", "f1"},
        {"CityObjects",
         {{"z_obj", {{"type", "Building"}, {"attributes", {{"n", 1}}}}},
          {"a_obj", {{"type", "Building"}, {"attributes", {{"n", 2}}}}}}},
        {"vertices", nlohmann::json::array()},
    };

    auto entries = cityfeature_to_index_entries(feature, schema, {"n"});
    REQUIRE(entries.size() == 2);
    // "a_obj" sorts before "z_obj", so its value (2) comes first.
    CHECK(entries[0].value.kind() == KeyKind::UInt64);
}

TEST_CASE("cityfeature_to_index_entries skips objects with no attributes") {
    AttributeSchema schema;
    schema.emplace("n", std::make_pair(0, ::ColumnType::ULong));
    nlohmann::json feature = {
        {"CityObjects", {{"o1", {{"type", "Building"}}}, {"o2", {{"type", "Building"}}}}},
    };
    CHECK(cityfeature_to_index_entries(feature, schema, {"n"}).empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src/cpp && just test 2>&1 | tail -40`
Expected: FAIL — `AttributeIndexEntry`/`attribute_to_index_entries`/`cityfeature_to_index_entries` not declared.

- [ ] **Step 3: Write the implementation**

Add to `src/cpp/include/fcb/writer/attribute.hpp`, after `to_columns`:

```cpp
/// One indexable (column, value) pair pulled out of a feature for the
/// static B+tree builder (M6). `value.kind()` always matches
/// `key_kind_for_column(schema column type)` for `index`.
struct AttributeIndexEntry {
    std::uint16_t index;
    KeyValue value;
};

/// Extracts index entries for `indexing_attr` from one CityJSON attributes
/// object. Only Bool, Int, UInt, Long, ULong, Float, Double, String and
/// DateTime columns produce entries -- Byte, UByte, Short, UShort, Json and
/// Binary are silently skipped, matching the Rust writer's
/// `attribute_to_index_entries` exactly (a known, deliberate gap: those
/// types ARE supported by the B+tree builder itself, just never reached
/// through this normal extraction path). A name in `indexing_attr` absent
/// from `attr` or from `schema` is skipped.
std::vector<AttributeIndexEntry> attribute_to_index_entries(
    const nlohmann::json& attr, const AttributeSchema& schema,
    const std::vector<std::string>& indexing_attr);

/// Same, over every object in one CityJSONFeature's `CityObjects`, visited
/// in ascending object-id order (not JSON key order, which need not be
/// stable) so that duplicate-key payload ordering in the eventual B+tree is
/// reproducible. An object with no `attributes` member, or an explicit
/// `"attributes": null`, contributes nothing.
std::vector<AttributeIndexEntry> cityfeature_to_index_entries(
    const nlohmann::json& city_feature, const AttributeSchema& schema,
    const std::vector<std::string>& indexing_attr);
```

Add to `src/cpp/src/writer/attribute.cpp`, after `to_columns`:

```cpp
std::vector<AttributeIndexEntry> attribute_to_index_entries(
    const nlohmann::json& attr, const AttributeSchema& schema,
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
        const nlohmann::json& val = *val_it;

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

std::vector<AttributeIndexEntry> cityfeature_to_index_entries(
    const nlohmann::json& city_feature, const AttributeSchema& schema,
    const std::vector<std::string>& indexing_attr) {
    std::vector<AttributeIndexEntry> out;
    auto co_it = city_feature.find("CityObjects");
    if (co_it == city_feature.end() || !co_it->is_object())
        return out;

    std::vector<std::string> object_ids;
    object_ids.reserve(co_it->size());
    for (const auto& [id, _] : co_it->items())
        object_ids.push_back(id);
    std::sort(object_ids.begin(), object_ids.end());

    for (const auto& id : object_ids) {
        const nlohmann::json& co = co_it->at(id);
        auto attr_it = co.find("attributes");
        if (attr_it == co.end() || attr_it->is_null())
            continue;
        auto entries = attribute_to_index_entries(*attr_it, schema, indexing_attr);
        out.insert(out.end(), entries.begin(), entries.end());
    }
    return out;
}
```

`parse_rfc3339` must be visible here: move its definition (and `Rfc3339Result`) so both this function and `guess_type` (Task 1) can see it -- it already lives in the same anonymous namespace in `attribute.cpp`, so no header change is needed, only confirm it was not accidentally made local to `guess_type`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src/cpp && just test 2>&1 | tail -60`
Expected: PASS.

- [ ] **Step 5: Run the full check**

Run: `cd src/cpp && just check 2>&1 | tail -80`
Expected: PASS — lint, type-check, test and build all green, including every pre-existing test (this task must not regress the reader).

- [ ] **Step 6: Commit**

```bash
git add src/cpp/include/fcb/writer/attribute.hpp src/cpp/src/writer/attribute.cpp \
        src/cpp/tests/test_writer_attribute.cpp
git commit -m "feat(cpp): add B+tree index-entry extraction for the writer (M1 part 4)"
```

---

## After all 4 tasks: milestone review

- [ ] Run `codex exec -m gpt-5.6-sol --sandbox read-only` (from the repo root) reviewing the diff since the milestone started (`git diff <commit-before-M1>..HEAD -- src/cpp`), asking it to check: correctness against `src/rust/fcb_core/src/writer/attribute.rs` and `.llm/docs/specification.md`; the deliberate divergence in `attribute_to_index_entries` (documented, not a bug); C++ idioms and project conventions (`src/cpp/CLAUDE.md`-equivalent style already visible in `attribute.cpp`/`attribute.hpp`); and the RFC3339 detector's scope (pragmatic, not a full ISO 8601 grammar -- flag if it should be tightened).
- [ ] Triage findings per `superpowers:receiving-code-review` (verify before applying), fix what's real, note what's deliberately out of scope.
- [ ] Mark milestone M1 complete in the task tracker; move to M2.
