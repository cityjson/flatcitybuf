// Byte-exact oracle tests for the writer (M3, task 5): the requirement,
// stated explicitly by the project owner, is that this C++ writer's output
// must be checked against what the Rust writer actually produces, and that
// files it writes must be readable by both readers. `conformance/
// single_feature.fcb` is produced by the Rust `fcb` CLI and already
// committed (alongside its `.expected.jsonl`) for the reader suite; reused
// here as the writer's oracle too, so this needs no Rust toolchain to run.
//
// Its header may carry a spatial index (M5 isn't built yet), but the
// FEATURE bytes -- what this milestone produces -- are unaffected by
// whether one exists, so the feature section is sliced out via the
// header's own computed layout (never a hardcoded offset) and compared.

#include <fcb/attribute.hpp>
#include <fcb/cityjson.hpp>
#include <fcb/reader.hpp>
#include <fcb/writer/attribute.hpp>
#include <fcb/writer/feature_serializer.hpp>

#include <nlohmann/json.hpp>

#include <algorithm>
#include <cstdio>
#include <fstream>
#include <string>
#include <vector>

#include <doctest/doctest.h>

using namespace fcb;
using nlohmann::ordered_json;

namespace {

std::vector<ordered_json> read_jsonl(const std::string& path) {
    std::vector<ordered_json> out;
    std::ifstream f(path);
    REQUIRE_MESSAGE(f.good(), "cannot open " << path);
    std::string line;
    while (std::getline(f, line))
        if (!line.empty())
            out.push_back(ordered_json::parse(line));
    return out;
}

std::vector<std::uint8_t> read_file_bytes(const std::string& path) {
    std::ifstream f(path, std::ios::binary);
    REQUIRE_MESSAGE(f.good(), "cannot open " << path);
    return std::vector<std::uint8_t>((std::istreambuf_iterator<char>(f)),
                                     std::istreambuf_iterator<char>());
}

/// Mirrors the `fcb` CLI's schema-building exactly (cli/src/main.rs:357-380):
/// city objects visited in ascending id order, `add_attributes` called once
/// per object's attributes -- so the resulting column indices match.
AttributeSchema build_attr_schema(const std::vector<ordered_json>& features) {
    AttributeSchema schema;
    for (const auto& feature : features) {
        auto co_it = feature.find("CityObjects");
        if (co_it == feature.end() || !co_it->is_object())
            continue;
        std::vector<std::string> ids;
        for (const auto& [id, unused] : co_it->items())
            ids.push_back(id);
        std::sort(ids.begin(), ids.end());
        for (const auto& id : ids) {
            const ordered_json& co = co_it->at(id);
            if (auto attr_it = co.find("attributes"); attr_it != co.end())
                add_attributes(schema, *attr_it);
        }
    }
    return schema;
}

std::pair<std::vector<std::uint8_t>, ordered_json> build_single_feature() {
    const std::string input_path =
        std::string(FCB_CONFORMANCE_DIR) + "/inputs/single_feature.city.jsonl";
    std::vector<ordered_json> input_lines = read_jsonl(input_path);
    REQUIRE(input_lines.size() == 2);  // metadata line + one feature line
    const ordered_json& feature_json = input_lines[1];

    AttributeSchema attr_schema = build_attr_schema({feature_json});

    flatbuffers::FlatBufferBuilder fbb;
    auto [off, bbox] = to_fcb_city_feature(fbb, feature_json.at("id").get<std::string>(),
                                           feature_json, attr_schema, nullptr);
    (void)bbox;
    fbb.FinishSizePrefixed(off);

    return {
        std::vector<std::uint8_t>(fbb.GetBufferPointer(), fbb.GetBufferPointer() + fbb.GetSize()),
        feature_json};
}

}  // namespace

TEST_CASE("oracle: to_fcb_city_feature is byte-identical to the Rust writer's output") {
    const std::string fcb_path = std::string(FCB_CONFORMANCE_DIR) + "/single_feature.fcb";

    FcbReader r = FcbReader::open_file(fcb_path);
    REQUIRE(r.header().info().features_count == 1);
    const auto feature_begin = r.header().layout().feature_begin;

    std::vector<std::uint8_t> whole_file = read_file_bytes(fcb_path);
    REQUIRE(whole_file.size() > feature_begin);
    std::vector<std::uint8_t> expected_feature_bytes(whole_file.begin() + feature_begin,
                                                     whole_file.end());

    auto [actual_feature_bytes, feature_json] = build_single_feature();
    (void)feature_json;
    if (actual_feature_bytes != expected_feature_bytes) {
        MESSAGE("actual size: " << actual_feature_bytes.size()
                                << " expected size: " << expected_feature_bytes.size());
        std::size_t n = std::min(actual_feature_bytes.size(), expected_feature_bytes.size());
        for (std::size_t i = 0; i < n; ++i) {
            if (actual_feature_bytes[i] != expected_feature_bytes[i]) {
                MESSAGE("first diff at byte " << i << ": actual=" << (int)actual_feature_bytes[i]
                                              << " expected=" << (int)expected_feature_bytes[i]);
                break;
            }
        }
    }
    CHECK(actual_feature_bytes == expected_feature_bytes);
}

TEST_CASE("oracle: the bytes this writer produces decode correctly through the existing reader") {
    // Splices this writer's OWN feature bytes into a copy of the Rust file
    // (replacing only the feature section -- the header/index sections are
    // not built by this milestone) and decodes through the existing,
    // already-conformant reader -- independent of the byte-exact check
    // above, so a bug that produced wrong-but-plausible bytes would still
    // be caught here even if it somehow passed that one.
    const std::string fcb_path = std::string(FCB_CONFORMANCE_DIR) + "/single_feature.fcb";
    auto [feature_bytes, feature_json] = build_single_feature();

    FcbReader r0 = FcbReader::open_file(fcb_path);
    const auto feature_begin = r0.header().layout().feature_begin;
    std::vector<std::uint8_t> whole_file = read_file_bytes(fcb_path);
    whole_file.resize(feature_begin);
    whole_file.insert(whole_file.end(), feature_bytes.begin(), feature_bytes.end());

    const std::string tmp_path = "test_writer_oracle_spliced.fcb";
    {
        std::ofstream out(tmp_path, std::ios::binary);
        REQUIRE_MESSAGE(out.good(), "cannot create " << tmp_path);
        out.write(reinterpret_cast<const char*>(whole_file.data()),
                  static_cast<std::streamsize>(whole_file.size()));
    }

    FcbReader r = FcbReader::open_file(tmp_path);
    FeatureIterator it = r.select_all();
    REQUIRE(it.next());
    ordered_json decoded = to_cityjson_feature(it.current(), r.header());
    CHECK_FALSE(it.next());
    std::remove(tmp_path.c_str());

    // `decoded` is nlohmann::json (the reader's own type); `feature_json` is
    // nlohmann::ordered_json (this writer's, matching serde_json's actual
    // insertion-order behavior -- see the ordered_json note in
    // writer/attribute.hpp). The two `basic_json` specializations compare
    // ambiguously via a bare `==`, so one side is explicitly converted;
    // JSON equality is order-independent for objects regardless.
    const nlohmann::json expected = feature_json;
    CHECK(decoded["id"] == expected["id"]);
    CHECK(decoded["vertices"] == expected["vertices"]);
    CHECK(decoded["CityObjects"] == expected["CityObjects"]);
    CHECK(decoded == expected);
}

TEST_CASE("oracle: column order is insertion order, not alphabetical, confirmed independently") {
    // `inferable_types.city.jsonl`'s one object declares its attributes as
    // {a_bool, a_double, a_long, a_ulong, a_string, a_json} -- NOT
    // alphabetical order (a_json/a_string/a_ulong are out of place) -- so
    // this is a second, independent confirmation (beyond single_feature's
    // two-attribute case) of the insertion-order finding that drove the
    // ordered_json switch throughout M1-M3.
    FcbReader r = FcbReader::open_file(std::string(FCB_CONFORMANCE_DIR) + "/inferable_types.fcb");
    const auto& columns = r.header().info().columns;
    REQUIRE(columns.size() == 6);
    CHECK(columns[0].name == "a_bool");
    CHECK(columns[1].name == "a_double");
    CHECK(columns[2].name == "a_long");
    CHECK(columns[3].name == "a_ulong");
    CHECK(columns[4].name == "a_string");
    CHECK(columns[5].name == "a_json");
}
