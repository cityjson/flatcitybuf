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
#include <fcb/writer/btree_builder.hpp>
#include <fcb/writer/fcb_writer.hpp>
#include <fcb/writer/feature_serializer.hpp>
#include <fcb/writer/header_serializer.hpp>
#include <fcb/writer/rtree_builder.hpp>

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

/// Test-only convenience wrapper: `FcbWriter` (M8) is the streaming public
/// API (add_feature spools to a temp file, mirroring Rust's own memory-
/// scalable writer), but every oracle test here already has every feature
/// as one in-memory vector, so this just loops `add_feature` -- exercising
/// the real streaming implementation underneath, not a separate code path.
std::vector<std::uint8_t> write_fcb(const ordered_json& cj,
                                    const std::vector<ordered_json>& features,
                                    const FcbWriterOptions& options,
                                    const AttributeSchema& attr_schema,
                                    const AttributeSchema* semantic_attr_schema) {
    FcbWriter w(cj, options, attr_schema,
                semantic_attr_schema ? std::optional<AttributeSchema>(*semantic_attr_schema)
                                     : std::nullopt);
    for (const auto& f : features)
        w.add_feature(f);
    return w.write();
}

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

/// Builds the Nth (0-indexed) CityJSONFeature line of
/// `conformance/inputs/<fixture>.city.jsonl` with this writer, returning
/// its size-prefixed bytes alongside the source feature JSON.
std::pair<std::vector<std::uint8_t>, ordered_json>
build_feature_from_fixture(const std::string& fixture, std::size_t feature_index = 0) {
    const std::string input_path =
        std::string(FCB_CONFORMANCE_DIR) + "/inputs/" + fixture + ".city.jsonl";
    std::vector<ordered_json> input_lines = read_jsonl(input_path);
    REQUIRE(input_lines.size() > feature_index + 1);  // metadata line + features
    const ordered_json& feature_json = input_lines[feature_index + 1];

    std::vector<ordered_json> all_features(input_lines.begin() + 1, input_lines.end());
    AttributeSchema attr_schema = build_attr_schema(all_features);

    flatbuffers::FlatBufferBuilder fbb;
    auto [off, bbox] = to_fcb_city_feature(fbb, feature_json.at("id").get<std::string>(),
                                           feature_json, attr_schema, nullptr);
    (void)bbox;
    fbb.FinishSizePrefixed(off);

    return {
        std::vector<std::uint8_t>(fbb.GetBufferPointer(), fbb.GetBufferPointer() + fbb.GetSize()),
        feature_json};
}

std::pair<std::vector<std::uint8_t>, ordered_json> build_single_feature() {
    return build_feature_from_fixture("single_feature");
}

/// Byte-compares this writer's output for `<fixture>.city.jsonl`'s feature
/// `feature_index` against the corresponding slice of the real Rust-written
/// `<fixture>.fcb` (sliced via the header's own computed layout, never a
/// hardcoded offset -- the fixture may carry a spatial index, since it is
/// unaffected by whether one exists).
void check_feature_byte_exact(const std::string& fixture, std::size_t feature_index = 0) {
    CAPTURE(fixture);
    const std::string fcb_path = std::string(FCB_CONFORMANCE_DIR) + "/" + fixture + ".fcb";

    FcbReader r = FcbReader::open_file(fcb_path);
    const auto feature_begin = r.header().layout().feature_begin;

    std::vector<std::uint8_t> whole_file = read_file_bytes(fcb_path);
    REQUIRE(whole_file.size() > feature_begin);
    std::vector<std::uint8_t> expected_feature_bytes(whole_file.begin() + feature_begin,
                                                     whole_file.end());

    auto [actual_feature_bytes, feature_json] = build_feature_from_fixture(fixture, feature_index);
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

}  // namespace

TEST_CASE("oracle: to_fcb_city_feature is byte-identical to the Rust writer's output") {
    check_feature_byte_exact("single_feature");
}

TEST_CASE("oracle: interleaved geometry and GeometryInstance byte-match Rust's two-pass order") {
    // Found during the M3 codex review: C++ originally built each CityObject
    // geometry entry in ONE interleaved pass (as encountered in the source
    // array), while Rust's to_city_object does two full passes -- every
    // non-instance geometry, in order, THEN every instance, in order
    // (writer/serializer.rs:644-670). The two produce identical DECODED
    // content but different FlatBuffer byte layouts whenever a CityObject's
    // "geometry" array interleaves instances with non-instances, which no
    // single-geometry fixture (like single_feature) can catch.
    check_feature_byte_exact("geometry_instance_interleaved");
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

namespace {

/// Byte-compares this writer's `to_fcb_header` output for
/// `<fixture>.city.jsonl`'s metadata line against the corresponding slice of
/// the real Rust-written `<fixture>.fcb` header (sliced via the header's own
/// computed layout, never a hardcoded offset). `feature_count`/
/// `index_node_size` must match whatever the fixture was actually generated
/// with (check via `fcb_inspect_header`) -- neither is recoverable from the
/// fixture's own header bytes in a way that lets this function derive them
/// itself, since the whole point is to independently confirm them.
void check_header_byte_exact(const std::string& fixture, std::uint64_t feature_count,
                             std::uint16_t index_node_size) {
    CAPTURE(fixture);
    const std::string fcb_path = std::string(FCB_CONFORMANCE_DIR) + "/" + fixture + ".fcb";
    FcbReader r = FcbReader::open_file(fcb_path);
    const auto& layout = r.header().layout();
    REQUIRE(r.header().info().features_count == feature_count);
    REQUIRE(r.header().info().index_node_size == index_node_size);
    REQUIRE(r.header().attr_indices().empty());

    std::vector<std::uint8_t> whole_file = read_file_bytes(fcb_path);
    constexpr std::size_t header_begin = 8 + 4;  // magic bytes + size prefix
    REQUIRE(whole_file.size() >= layout.header_len);
    std::vector<std::uint8_t> expected_header_bytes(whole_file.begin() + header_begin,
                                                    whole_file.begin() + layout.header_len);

    const std::string input_path =
        std::string(FCB_CONFORMANCE_DIR) + "/inputs/" + fixture + ".city.jsonl";
    std::vector<ordered_json> input_lines = read_jsonl(input_path);
    const ordered_json& cj = input_lines[0];
    std::vector<ordered_json> all_features(input_lines.begin() + 1, input_lines.end());
    AttributeSchema attr_schema = build_attr_schema(all_features);

    HeaderWriterOptions options;
    options.feature_count = feature_count;
    options.index_node_size = index_node_size;

    flatbuffers::FlatBufferBuilder fbb;
    auto off = to_fcb_header(fbb, cj, options, attr_schema, nullptr, nullptr);
    fbb.FinishSizePrefixed(off);
    std::vector<std::uint8_t> actual_header_bytes(fbb.GetBufferPointer() + 4,
                                                  fbb.GetBufferPointer() + fbb.GetSize());

    if (actual_header_bytes != expected_header_bytes) {
        MESSAGE("actual size: " << actual_header_bytes.size()
                                << " expected size: " << expected_header_bytes.size());
        std::size_t n = std::min(actual_header_bytes.size(), expected_header_bytes.size());
        for (std::size_t i = 0; i < n; ++i) {
            if (actual_header_bytes[i] != expected_header_bytes[i]) {
                MESSAGE("first diff at byte " << i << ": actual=" << (int)actual_header_bytes[i]
                                              << " expected=" << (int)expected_header_bytes[i]);
                break;
            }
        }
    }
    CHECK(actual_header_bytes == expected_header_bytes);
}

}  // namespace

TEST_CASE("oracle: to_fcb_header is byte-identical to the Rust writer's header, including "
          "geometry-templates/templates_vertices") {
    // `single_feature.fcb` turned out to carry an attribute index (M6, not
    // built yet -- its `fcb_inspect_header` "queryable" markers were missed
    // on first read of this fixture, see docs/upstream-findings.md-adjacent
    // note below). `geometry_instance_interleaved.fcb` has none (0 of 0
    // columns queryable) and, as a bonus, exercises `geometry-templates` /
    // `templates_vertices`, which single_feature does not carry at all.
    check_header_byte_exact("geometry_instance_interleaved", /*feature_count=*/1,
                            /*index_node_size=*/16);
}

TEST_CASE("oracle: to_fcb_header is byte-identical to the Rust writer's header, for a fixture "
          "carrying every optional metadata field at once") {
    // Added after the first codex review of this milestone flagged that no
    // byte-exact test exercised referenceSystem, identifier, referenceDate,
    // title, a full pointOfContact (incl. its address sub-object), or
    // `extensions` -- geometry_instance_interleaved's metadata is `{}` and
    // single_feature has none of these either. `header_metadata_full` is a
    // fixture built specifically to cover all of them at once (generated via
    // the real Rust CLI, like geometry_instance_interleaved was for M3).
    check_header_byte_exact("header_metadata_full", /*feature_count=*/1,
                            /*index_node_size=*/16);
}

TEST_CASE("oracle: build_packed_rtree is byte-identical to the Rust writer's spatial index, for "
          "a real multi-feature fixture") {
    // `rtree_multilevel.fcb` (20 features at distinct grid positions, node
    // size 16, generated via the real Rust CLI like geometry_instance_
    // interleaved/header_metadata_full were for M3/M4) forces a real
    // 3-level tree (20 leaves -> ceil(20/16)=2 -> 1) with genuinely
    // different bboxes -- every OTHER multi-feature fixture in the corpus
    // (duplicate_keys, colliding_strings, ...) happens to carry IDENTICAL
    // bboxes for every feature, which can't exercise `hilbert_sort`
    // reordering or `build_packed_rtree`'s bottom-up aggregation at more
    // than one level (found during the M5 codex review). This test
    // reimplements just enough of `FcbWriter::write`'s orchestration
    // (writer/mod.rs:191-225) to build realistic input -- computing each
    // feature's ACTUAL (transform-scaled) bbox, sorting, and reassigning
    // offsets from each feature's OWN encoded byte size -- without
    // exposing any of that as this milestone's own API (that's M7's job).
    const std::string fixture = "rtree_multilevel";
    const std::string fcb_path = std::string(FCB_CONFORMANCE_DIR) + "/" + fixture + ".fcb";
    FcbReader r = FcbReader::open_file(fcb_path);
    const auto& layout = r.header().layout();
    REQUIRE(r.header().info().features_count == 20);
    REQUIRE(r.header().info().index_node_size == 16);

    std::vector<std::uint8_t> whole_file = read_file_bytes(fcb_path);
    REQUIRE(whole_file.size() >= layout.attr_index_begin);
    std::vector<std::uint8_t> expected_rtree_bytes(whole_file.begin() + layout.rtree_begin,
                                                   whole_file.begin() + layout.attr_index_begin);

    const std::string input_path =
        std::string(FCB_CONFORMANCE_DIR) + "/inputs/" + fixture + ".city.jsonl";
    std::vector<ordered_json> input_lines = read_jsonl(input_path);
    const ordered_json& cj = input_lines[0];
    std::vector<ordered_json> all_features(input_lines.begin() + 1, input_lines.end());
    AttributeSchema attr_schema = build_attr_schema(all_features);

    const auto& transform = cj.at("transform");
    const double scale_x = transform.at("scale").at(0).get<double>();
    const double scale_y = transform.at("scale").at(1).get<double>();
    const double translate_x = transform.at("translate").at(0).get<double>();
    const double translate_y = transform.at("translate").at(1).get<double>();

    // Mirrors FeatureWriter::finish_to_feature + FcbWriter::write_feature:
    // each feature gets its own fbb (matching `self.fbb.reset()` between
    // features), its raw bbox is transform-scaled to real-world coordinates
    // (`FcbWriter::actual_bbox`), and `feat_nodes[i].offset` starts as the
    // feature's ORIGINAL (pre-sort) index -- exactly like Rust's
    // `node.offset = self.feat_offsets.len() as u64` before the sort.
    std::vector<std::uint64_t> feat_sizes(all_features.size());
    std::vector<NodeItem> feat_nodes;
    feat_nodes.reserve(all_features.size());
    for (std::size_t i = 0; i < all_features.size(); ++i) {
        flatbuffers::FlatBufferBuilder fbb;
        auto [off, raw_bbox] = to_fcb_city_feature(fbb, all_features[i].at("id").get<std::string>(),
                                                   all_features[i], attr_schema, nullptr);
        fbb.FinishSizePrefixed(off);
        feat_sizes[i] = fbb.GetSize();

        NodeItem node{
            raw_bbox.min_x * scale_x + translate_x, raw_bbox.min_y * scale_y + translate_y,
            raw_bbox.max_x * scale_x + translate_x, raw_bbox.max_y * scale_y + translate_y, i};
        feat_nodes.push_back(node);
    }

    NodeItem extent = calc_extent(feat_nodes);
    hilbert_sort(feat_nodes, extent);

    // Mirrors writer/mod.rs:211-222: reassign each sorted node's `.offset`
    // to its FINAL byte position, derived from the ORIGINAL feature's own
    // encoded size (looked up via the temp index `.offset` still carries at
    // this point, before being overwritten here).
    std::uint64_t running_offset = 0;
    for (auto& node : feat_nodes) {
        const std::uint64_t temp_id = node.offset;
        node.offset = running_offset;
        running_offset += feat_sizes[temp_id];
    }

    std::vector<NodeItem> tree = build_packed_rtree(feat_nodes, extent, /*node_size=*/16);
    std::vector<std::uint8_t> actual_rtree_bytes = encode_packed_rtree(tree);

    if (actual_rtree_bytes != expected_rtree_bytes) {
        MESSAGE("actual size: " << actual_rtree_bytes.size()
                                << " expected size: " << expected_rtree_bytes.size());
        std::size_t n = std::min(actual_rtree_bytes.size(), expected_rtree_bytes.size());
        for (std::size_t i = 0; i < n; ++i) {
            if (actual_rtree_bytes[i] != expected_rtree_bytes[i]) {
                MESSAGE("first diff at byte " << i << ": actual=" << (int)actual_rtree_bytes[i]
                                              << " expected=" << (int)expected_rtree_bytes[i]);
                break;
            }
        }
    }
    CHECK(actual_rtree_bytes == expected_rtree_bytes);
}

namespace {

/// Byte-compares this writer's `build_static_btree` output for every
/// attribute index `<fixture>.fcb` actually carries against the Rust
/// writer's own bytes. Replicates just enough of `FcbWriter::write`'s
/// orchestration (writer/mod.rs:191-247) to get feature offsets right:
/// each feature's ACTUAL (transform-scaled) bbox is hilbert-sorted exactly
/// like M5's R-tree oracle does, and attribute-index entries are tagged
/// with each feature's FINAL sorted byte offset, not its input-order one --
/// required whenever a fixture's features do NOT all share one bbox (this
/// port's own `duplicate_keys.fcb` usage got away without this because
/// every one of its features happens to share an identical bbox, making
/// the sort a no-op; `btree_multilevel.fcb` does not).
void check_attribute_index_byte_exact(const std::string& fixture) {
    CAPTURE(fixture);
    const std::string fcb_path = std::string(FCB_CONFORMANCE_DIR) + "/" + fixture + ".fcb";
    FcbReader r = FcbReader::open_file(fcb_path);
    const auto& attr_indices = r.header().attr_indices();
    REQUIRE_FALSE(attr_indices.empty());

    std::vector<std::uint8_t> whole_file = read_file_bytes(fcb_path);

    const std::string input_path =
        std::string(FCB_CONFORMANCE_DIR) + "/inputs/" + fixture + ".city.jsonl";
    std::vector<ordered_json> input_lines = read_jsonl(input_path);
    const ordered_json& cj = input_lines[0];
    std::vector<ordered_json> all_features(input_lines.begin() + 1, input_lines.end());
    AttributeSchema attr_schema = build_attr_schema(all_features);

    const auto& transform = cj.at("transform");
    const double scale_x = transform.at("scale").at(0).get<double>();
    const double scale_y = transform.at("scale").at(1).get<double>();
    const double translate_x = transform.at("translate").at(0).get<double>();
    const double translate_y = transform.at("translate").at(1).get<double>();

    std::vector<std::string> all_column_names;
    for (const auto& [name, unused] : attr_schema)
        all_column_names.push_back(name);

    // Build every feature once, collecting its encoded size, its
    // transform-scaled bbox (tagged with its ORIGINAL index, matching M5's
    // oracle), and its attribute-index entries (tagged the same way, since
    // the real byte offset isn't known until after sorting).
    std::vector<std::uint64_t> feat_sizes(all_features.size());
    std::vector<NodeItem> feat_nodes;
    feat_nodes.reserve(all_features.size());
    std::vector<std::vector<AttributeIndexEntry>> index_entries_by_feature(all_features.size());
    for (std::size_t i = 0; i < all_features.size(); ++i) {
        flatbuffers::FlatBufferBuilder fbb;
        auto [off, raw_bbox] = to_fcb_city_feature(fbb, all_features[i].at("id").get<std::string>(),
                                                   all_features[i], attr_schema, nullptr);
        fbb.FinishSizePrefixed(off);
        feat_sizes[i] = fbb.GetSize();

        feat_nodes.push_back(NodeItem{
            raw_bbox.min_x * scale_x + translate_x, raw_bbox.min_y * scale_y + translate_y,
            raw_bbox.max_x * scale_x + translate_x, raw_bbox.max_y * scale_y + translate_y, i});
        index_entries_by_feature[i] =
            cityfeature_to_index_entries(all_features[i], attr_schema, all_column_names);
    }

    NodeItem extent = calc_extent(feat_nodes);
    hilbert_sort(feat_nodes, extent);

    // `final_offset_by_temp_id[i]` = feature `i`'s (original input order)
    // byte position in the SORTED features section. Walking `feat_nodes`
    // (sorted order) computes these cumulative offsets, but the entries
    // fed to `build_static_btree` below must NOT be collected in this
    // sorted order: Rust's `build_index_generic` (writer/attr_index.rs)
    // iterates `attribute_index_entries: BTreeMap<usize, _>` -- keyed by
    // each feature's ORIGINAL (pre-sort) temp id, so `.values()` walks in
    // ORIGINAL INPUT order even though each entry's `.offset` field was
    // separately overwritten to its final sorted position. `Stree::build`
    // then does its OWN stable sort by key, so for a group of DUPLICATE
    // keys, which offset ends up FIRST in the payload entry depends on
    // ORIGINAL input order, not sorted order -- collecting entries in
    // sorted order (as this test originally did) reorders duplicate-key
    // payloads incorrectly, a real divergence the M6 codex review's
    // "shallow oracle coverage" finding surfaced by prompting a fixture
    // with actual duplicates AND a real hilbert reorder in the same file.
    std::vector<std::uint64_t> final_offset_by_temp_id(all_features.size());
    std::uint64_t running_offset = 0;
    for (const auto& node : feat_nodes) {
        final_offset_by_temp_id[node.offset] = running_offset;
        running_offset += feat_sizes[node.offset];
    }

    std::vector<std::vector<BtreeEntry>> entries_by_column(attr_schema.size());
    for (std::size_t temp_id = 0; temp_id < all_features.size(); ++temp_id) {
        const std::uint64_t feature_offset = final_offset_by_temp_id[temp_id];
        for (const auto& e : index_entries_by_feature[temp_id])
            entries_by_column.at(e.index).push_back(BtreeEntry{e.value, feature_offset});
    }

    for (const auto& ai : attr_indices) {
        CAPTURE(ai.column_index);
        const KeyKind kind =
            key_kind_for_column(r.header().info().columns.at(ai.column_index).type);
        BuiltBtreeIndex built =
            build_static_btree(entries_by_column.at(ai.column_index), kind, ai.branching_factor);
        CHECK(built.num_unique_items == ai.num_unique_items);

        REQUIRE(whole_file.size() >= ai.begin + ai.length);
        std::vector<std::uint8_t> expected_bytes(whole_file.begin() + ai.begin,
                                                 whole_file.begin() + ai.begin + ai.length);

        if (built.bytes != expected_bytes) {
            MESSAGE("actual size: " << built.bytes.size()
                                    << " expected size: " << expected_bytes.size());
            std::size_t n = std::min(built.bytes.size(), expected_bytes.size());
            for (std::size_t i = 0; i < n; ++i) {
                if (built.bytes[i] != expected_bytes[i]) {
                    MESSAGE("first diff at byte " << i << ": actual=" << (int)built.bytes[i]
                                                  << " expected=" << (int)expected_bytes[i]);
                    break;
                }
            }
        }
        CHECK(built.bytes == expected_bytes);
    }
}

}  // namespace

TEST_CASE("oracle: build_static_btree is byte-identical to the Rust writer's attribute index, for "
          "a real fixture with duplicate key values") {
    // `duplicate_keys.fcb` (5 features, generated with `-A`/index-all-
    // attributes -- confirmed via its own header: 2 attribute indices,
    // branching_factor 256, matching the CLI's `-A` default of 256, NOT
    // the crate's own DEFAULT_BRANCHING_FACTOR=16) has "grp"="same" on
    // every feature (one unique key backed by a 5-entry payload) and
    // "idx"=0..4 (five distinct keys, no payload at all) -- exercising
    // both the duplicate/payload path and the plain path in one fixture,
    // but only ever a 2-level tree (5 and 1 unique keys respectively).
    check_attribute_index_byte_exact("duplicate_keys");
}

TEST_CASE("oracle: build_static_btree is byte-identical to the Rust writer's attribute index, for "
          "a real fixture forcing a 3-level tree") {
    // `duplicate_keys.fcb`'s branching_factor (256) and unique-key counts
    // (1, 5) never build more than a 2-level tree, so `generate_nodes`'s
    // multi-level `parent_min_key` propagation was, until now, only ever
    // checked against this port's OWN reader round-trip
    // (test_writer_btree.cpp), not against real Rust-written bytes --
    // flagged by the M6 codex review. `btree_multilevel.fcb` (20 distinct
    // "idx" values, 15 distinct "grp" values with 6 colliding on "same",
    // `--attr-branching-factor 4`) forces a real 3-level tree for BOTH
    // columns at once. Its features also sit at distinct grid positions
    // (unlike duplicate_keys' identical bboxes), so this is the first
    // attribute-index oracle that actually needs the hilbert-sort/offset-
    // reassignment machinery `check_attribute_index_byte_exact` provides.
    check_attribute_index_byte_exact("btree_multilevel");
}

namespace {

/// Byte-compares `write_fcb`'s ENTIRE output for `<fixture>.city.jsonl`
/// against the real Rust-written `<fixture>.fcb`, byte for byte, start to
/// end -- the strongest check in the whole writer, subsuming every
/// per-section oracle above it. Options are read back from the real
/// file's own header (feature count, node size, which columns carry an
/// attribute index and at what branching factor) rather than hardcoded,
/// so this test can't silently drift from what the fixture actually is.
void check_whole_file_byte_exact(const std::string& fixture) {
    CAPTURE(fixture);
    const std::string fcb_path = std::string(FCB_CONFORMANCE_DIR) + "/" + fixture + ".fcb";
    FcbReader r = FcbReader::open_file(fcb_path);
    const std::vector<std::uint8_t> expected = read_file_bytes(fcb_path);

    const std::string input_path =
        std::string(FCB_CONFORMANCE_DIR) + "/inputs/" + fixture + ".city.jsonl";
    std::vector<ordered_json> input_lines = read_jsonl(input_path);
    const ordered_json& cj = input_lines[0];
    std::vector<ordered_json> features(input_lines.begin() + 1, input_lines.end());
    AttributeSchema attr_schema = build_attr_schema(features);

    FcbWriterOptions options;
    options.write_index = r.header().info().index_node_size > 0;
    options.index_node_size = r.header().info().index_node_size;
    for (const auto& ai : r.header().attr_indices())
        options.attribute_indices.emplace_back(r.header().info().columns.at(ai.column_index).name,
                                               ai.branching_factor);

    std::vector<std::uint8_t> actual = write_fcb(cj, features, options, attr_schema, nullptr);

    if (actual != expected) {
        MESSAGE("actual size: " << actual.size() << " expected size: " << expected.size());
        std::size_t n = std::min(actual.size(), expected.size());
        for (std::size_t i = 0; i < n; ++i) {
            if (actual[i] != expected[i]) {
                MESSAGE("first diff at byte " << i << ": actual=" << (int)actual[i]
                                              << " expected=" << (int)expected[i]);
                break;
            }
        }
    }
    CHECK(actual == expected);

    // The bytes this writer produces must also decode correctly through
    // the existing, already-conformant reader -- independent of the
    // byte-exact check above, and the strongest form of the project
    // owner's explicit cross-reader requirement: a file this writer
    // produces must be readable by both implementations.
    const std::string tmp_path = "test_writer_oracle_whole_file.fcb";
    {
        std::ofstream out(tmp_path, std::ios::binary);
        REQUIRE_MESSAGE(out.good(), "cannot create " << tmp_path);
        out.write(reinterpret_cast<const char*>(actual.data()),
                  static_cast<std::streamsize>(actual.size()));
    }
    FcbReader r2 = FcbReader::open_file(tmp_path);
    CHECK(r2.header().info().features_count == features.size());
    std::size_t decoded_count = 0;
    FeatureIterator it = r2.select_all();
    while (it.next())
        ++decoded_count;
    std::remove(tmp_path.c_str());
    CHECK(decoded_count == features.size());
}

}  // namespace

TEST_CASE("oracle: write_fcb produces a byte-identical whole file for single_feature") {
    // 1 feature, R-tree (node size 16), 2 attribute indices (branching
    // factor 256, matching `-A`'s default) -- the smallest fixture that
    // still exercises both index kinds together in the full pipeline.
    check_whole_file_byte_exact("single_feature");
}

TEST_CASE("oracle: write_fcb produces a byte-identical whole file for "
          "geometry_instance_interleaved") {
    // 1 feature, R-tree, geometry-templates, NO attribute index -- exercises
    // the header's templates/templates_vertices path end to end and the
    // "no attribute index at all" branch (attr_index_info empty -> nullptr).
    check_whole_file_byte_exact("geometry_instance_interleaved");
}

TEST_CASE("oracle: write_fcb produces a byte-identical whole file for header_metadata_full") {
    // 1 feature, full optional header metadata (referenceSystem, poc,
    // extensions, ...), R-tree, no attribute index.
    check_whole_file_byte_exact("header_metadata_full");
}

TEST_CASE("oracle: write_fcb produces a byte-identical whole file for duplicate_keys") {
    // 5 features sharing one bbox (hilbert_sort is a no-op), 2 attribute
    // indices at branching factor 256, one column with a 5-entry payload.
    check_whole_file_byte_exact("duplicate_keys");
}

TEST_CASE("oracle: write_fcb produces a byte-identical whole file for rtree_multilevel") {
    // 20 features at distinct grid positions -- hilbert_sort genuinely
    // reorders them; no attribute index. The strongest end-to-end check of
    // the R-tree path specifically (real 3-level tree, real reordering).
    check_whole_file_byte_exact("rtree_multilevel");
}

TEST_CASE("oracle: write_fcb produces a byte-identical whole file for btree_multilevel") {
    // 20 features at distinct grid positions AND 2 attribute indices at
    // branching factor 4 (real 3-level trees, one column with duplicates)
    // -- the single strongest test in the whole writer: every milestone's
    // hardest case (hilbert reordering, multi-level R-tree, multi-level
    // B+tree with a payload entry, and the original-vs-sorted-order trap
    // this milestone's own oracle test surfaced) all in one real file.
    check_whole_file_byte_exact("btree_multilevel");
}
