/**
 * @file roundtrip_test.cpp
 * @brief Roundtrip tests for FlatCityBuf C++ bindings
 *
 * Tests data integrity by serializing CityJSON data to FCB format
 * and deserializing back, comparing original and restored data including
 * vertices, CityObjects, attributes, and geometry types.
 *
 * Mirrors the Rust tests in fcb_core/tests/e2e.rs
 *
 * @author Hidemichi Baba
 * @version 0.1.0
 */

#include "fcb.h"  // FlatCityBuf C++ API header

#include <nlohmann/json.hpp>

#include <cassert>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <string>
#include <vector>

namespace fs = std::filesystem;
using json = nlohmann::json;

// ---------------------------------------------------------------------------
// Helper: read a CityJSONSeq (.city.jsonl) file line by line.
//   First non-empty line  -> header_out  (CityJSON object)
//   Subsequent lines      -> feature_lines_out  (CityJSONFeature objects)
// ---------------------------------------------------------------------------
static void read_cityjsonseq(const fs::path& path, std::string& header_out,
                             std::vector<std::string>& feature_lines_out) {
    std::ifstream infile(path);
    if (!infile.is_open())
        throw std::runtime_error("Cannot open " + path.string());

    std::string line;
    bool first = true;
    while (std::getline(infile, line)) {
        if (line.empty())
            continue;
        if (first) {
            header_out = line;
            first = false;
        } else {
            feature_lines_out.push_back(line);
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: assert two vertex arrays carry identical integer triples
// ---------------------------------------------------------------------------
static void assert_vertices_equal(const json& orig, const json& deser, const std::string& ctx) {
    if (orig.size() != deser.size()) {
        std::cerr << "[FAIL] " << ctx << ": vertex count " << orig.size() << " vs " << deser.size()
                  << std::endl;
        assert(false);
    }
    for (size_t v = 0; v < orig.size(); v++) {
        for (size_t ax = 0; ax < 3; ax++) {
            if (orig[v][ax] != deser[v][ax]) {
                std::cerr << "[FAIL] " << ctx << ": vertex[" << v << "][" << ax
                          << "] orig=" << orig[v][ax] << " deser=" << deser[v][ax] << std::endl;
                assert(false);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: deep-compare two CityJSONFeature JSON objects
//   Checks: id, vertices (exact values), CityObjects (presence + type +
//           attributes + geometry count/type/lod)
// ---------------------------------------------------------------------------
static void assert_feature_equal(const json& orig, const json& deser) {
    const std::string feat_id = orig["id"].get<std::string>();

    // id
    assert(deser.contains("id"));
    if (orig["id"] != deser["id"]) {
        std::cerr << "[FAIL] id: " << orig["id"] << " vs " << deser["id"] << std::endl;
        assert(false);
    }

    // vertices
    assert(orig.contains("vertices") && deser.contains("vertices"));
    assert_vertices_equal(orig["vertices"], deser["vertices"], "feature " + feat_id);

    // CityObjects
    assert(orig.contains("CityObjects") && deser.contains("CityObjects"));
    const auto& orig_cos = orig["CityObjects"];
    const auto& deser_cos = deser["CityObjects"];
    if (orig_cos.size() != deser_cos.size()) {
        std::cerr << "[FAIL] feature '" << feat_id << "': CityObjects count " << orig_cos.size()
                  << " vs " << deser_cos.size() << std::endl;
        assert(false);
    }
    for (auto& [co_id, orig_co] : orig_cos.items()) {
        if (!deser_cos.contains(co_id)) {
            std::cerr << "[FAIL] CityObject '" << co_id << "' missing" << std::endl;
            assert(false);
        }
        const auto& deser_co = deser_cos[co_id];

        // type
        if (orig_co["type"] != deser_co["type"]) {
            std::cerr << "[FAIL] '" << co_id << "' type: " << orig_co["type"] << " vs "
                      << deser_co["type"] << std::endl;
            assert(false);
        }

        // attributes — every non-null original key must be present with equal value.
        // FCB encodes null attributes as absent (by design), so null values are skipped.
        if (orig_co.contains("attributes")) {
            assert(deser_co.contains("attributes"));
            for (auto& [k, v] : orig_co["attributes"].items()) {
                if (v.is_null())
                    continue;  // null → absent after FCB roundtrip, skip
                if (!deser_co["attributes"].contains(k)) {
                    std::cerr << "[FAIL] attribute '" << k << "' missing in '" << co_id << "'"
                              << std::endl;
                    assert(false);
                }
                if (orig_co["attributes"][k] != deser_co["attributes"][k]) {
                    std::cerr << "[FAIL] attribute '" << k << "' in '" << co_id
                              << "': orig=" << orig_co["attributes"][k]
                              << " deser=" << deser_co["attributes"][k] << std::endl;
                    assert(false);
                }
            }
        }

        // geometry — count, type and lod must match
        if (orig_co.contains("geometry") && orig_co["geometry"].is_array()) {
            assert(deser_co.contains("geometry") && deser_co["geometry"].is_array());
            if (orig_co["geometry"].size() != deser_co["geometry"].size()) {
                std::cerr << "[FAIL] '" << co_id
                          << "' geometry count: " << orig_co["geometry"].size() << " vs "
                          << deser_co["geometry"].size() << std::endl;
                assert(false);
            }
            for (size_t g = 0; g < orig_co["geometry"].size(); g++) {
                const auto& og = orig_co["geometry"][g];
                const auto& dg = deser_co["geometry"][g];
                if (og["type"] != dg["type"]) {
                    std::cerr << "[FAIL] '" << co_id << "' geom[" << g << "] type: " << og["type"]
                              << " vs " << dg["type"] << std::endl;
                    assert(false);
                }
                if (og.contains("lod") && og["lod"] != dg["lod"]) {
                    std::cerr << "[FAIL] '" << co_id << "' geom[" << g << "] lod: " << og["lod"]
                              << " vs " << dg["lod"] << std::endl;
                    assert(false);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: build a map from feature id -> json for order-independent lookup
// ---------------------------------------------------------------------------
static std::map<std::string, json> index_by_id(const std::vector<json>& features) {
    std::map<std::string, json> m;
    for (const auto& f : features)
        m[f["id"].get<std::string>()] = f;
    return m;
}

// ---------------------------------------------------------------------------
// Test 1: basic CityJSON → FCB → CityJSON roundtrip
// ---------------------------------------------------------------------------
void test_cityjson_serialization_cycle(const fs::path& test_data_dir) {
    std::cout << "=== Test 1: CityJSON Serialization Cycle ===" << std::endl;

    std::string header_line;
    std::vector<std::string> feature_lines;
    read_cityjsonseq(test_data_dir / "small.city.jsonl", header_line, feature_lines);

    json header_cj = json::parse(header_line);
    std::cout << "  CityJSON version: " << header_cj["version"] << std::endl;
    std::cout << "  Features in file: " << feature_lines.size() << std::endl;
    assert(!feature_lines.empty());

    std::vector<json> original_features;
    for (auto& fl : feature_lines)
        original_features.push_back(json::parse(fl));

    // Write to FCB
    auto temp_fcb = test_data_dir / "temp_roundtrip.fcb";
    {
        auto writer = fcb::fcb_writer_new(header_line);
        for (auto& fl : feature_lines)
            fcb::fcb_writer_add_feature(*writer, fl);
        fcb::fcb_writer_write(std::move(writer), temp_fcb.string());
    }

    // Read back
    auto reader = fcb::fcb_reader_open(temp_fcb.string());
    auto meta = fcb::fcb_reader_metadata(*reader);
    std::cout << "  FCB features stored: " << meta.features_count << std::endl;
    std::cout << "  CityJSON version (from FCB header): " << std::string(meta.cityjson_version)
              << std::endl;

    auto iter = fcb::fcb_reader_select_all(std::move(reader));
    std::vector<json> deserialized_features;
    while (fcb::fcb_iterator_next(*iter)) {
        auto f = fcb::fcb_iterator_current(*iter);
        deserialized_features.push_back(json::parse(std::string(f.json)));
    }

    // Compare
    assert(header_cj["version"] == json(std::string(meta.cityjson_version)));
    std::cout << "  ✓ CityJSON version matches" << std::endl;

    assert(meta.features_count == feature_lines.size());
    assert(original_features.size() == deserialized_features.size());
    std::cout << "  ✓ Feature count matches: " << deserialized_features.size() << std::endl;

    auto deser_map = index_by_id(deserialized_features);
    for (const auto& orig : original_features) {
        const std::string id = orig["id"].get<std::string>();
        if (deser_map.find(id) == deser_map.end()) {
            std::cerr << "[FAIL] feature '" << id << "' missing in deserialized output"
                      << std::endl;
            assert(false);
        }
        assert_feature_equal(orig, deser_map.at(id));
        std::cout << "  ✓ Feature '" << id << "' — vertices=" << orig["vertices"].size()
                  << " CityObjects=" << orig["CityObjects"].size() << std::endl;
    }

    fs::remove(temp_fcb);
    std::cout << "  ✅ Passed" << std::endl;
}

// ---------------------------------------------------------------------------
// Test 2: geometry template cycle
//   Features (incl. GeometryInstance geometry types) are fully compared.
//   geometry-templates content is skipped — not yet exposed in C++ API.
// ---------------------------------------------------------------------------
void test_geometry_template_cycle(const fs::path& test_data_dir) {
    std::cout << "\n=== Test 2: Geometry Template Cycle ===" << std::endl;

    std::string header_line;
    std::vector<std::string> feature_lines;
    read_cityjsonseq(test_data_dir / "geom_temp.city.jsonl", header_line, feature_lines);

    json header_cj = json::parse(header_line);
    bool has_templates = header_cj.contains("geometry-templates");
    std::cout << "  Has geometry-templates: " << (has_templates ? "yes" : "no") << std::endl;
    std::cout << "  Features in file: " << feature_lines.size() << std::endl;
    assert(!feature_lines.empty());

    std::vector<json> original_features;
    for (auto& fl : feature_lines)
        original_features.push_back(json::parse(fl));

    // Write to FCB
    auto temp_fcb = test_data_dir / "temp_geom_templates.fcb";
    {
        auto writer = fcb::fcb_writer_new(header_line);
        for (auto& fl : feature_lines)
            fcb::fcb_writer_add_feature(*writer, fl);
        fcb::fcb_writer_write(std::move(writer), temp_fcb.string());
    }

    // Read back
    auto reader = fcb::fcb_reader_open(temp_fcb.string());
    auto meta = fcb::fcb_reader_metadata(*reader);
    auto iter = fcb::fcb_reader_select_all(std::move(reader));

    std::vector<json> deserialized_features;
    while (fcb::fcb_iterator_next(*iter)) {
        auto f = fcb::fcb_iterator_current(*iter);
        deserialized_features.push_back(json::parse(std::string(f.json)));
    }

    assert(original_features.size() == deserialized_features.size());
    std::cout << "  ✓ Feature count matches: " << deserialized_features.size() << std::endl;

    {
        auto deser_map = index_by_id(deserialized_features);
        for (const auto& orig : original_features) {
            const std::string id = orig["id"].get<std::string>();
            if (deser_map.find(id) == deser_map.end()) {
                std::cerr << "[FAIL] feature '" << id << "' missing in deserialized output"
                          << std::endl;
                assert(false);
            }
            assert_feature_equal(orig, deser_map.at(id));
            std::cout << "  ✓ Feature '" << id << "' matches" << std::endl;
        }
    }

    if (has_templates) {
        std::cout << "  ⚠ geometry-templates content check skipped (not yet in C++ API)"
                  << std::endl;
    }

    fs::remove(temp_fcb);
    std::cout << "  ✅ Passed" << std::endl;
}

// ---------------------------------------------------------------------------
// Test 3: extension preservation
// ---------------------------------------------------------------------------
void test_extension_serialization_cycle(const fs::path& test_data_dir) {
    std::cout << "\n=== Test 3: Extension Serialization Cycle ===" << std::endl;

    std::string header_line;
    std::vector<std::string> feature_lines;
    read_cityjsonseq(test_data_dir / "noise_extension.city.jsonl", header_line, feature_lines);

    json header_cj = json::parse(header_line);
    bool has_extensions = header_cj.contains("extensions");
    std::cout << "  Has extensions: " << (has_extensions ? "yes" : "no") << std::endl;
    if (has_extensions) {
        for (auto& [k, v] : header_cj["extensions"].items())
            std::cout << "    - " << k << ": " << v["url"] << std::endl;
    }
    std::cout << "  Features in file: " << feature_lines.size() << std::endl;
    assert(!feature_lines.empty());

    std::vector<json> original_features;
    for (auto& fl : feature_lines)
        original_features.push_back(json::parse(fl));

    // Write to FCB
    auto temp_fcb = test_data_dir / "temp_extensions.fcb";
    {
        auto writer = fcb::fcb_writer_new(header_line);
        for (auto& fl : feature_lines)
            fcb::fcb_writer_add_feature(*writer, fl);
        fcb::fcb_writer_write(std::move(writer), temp_fcb.string());
    }

    // Read back
    auto reader = fcb::fcb_reader_open(temp_fcb.string());
    auto meta = fcb::fcb_reader_metadata(*reader);
    auto iter = fcb::fcb_reader_select_all(std::move(reader));

    std::vector<json> deserialized_features;
    while (fcb::fcb_iterator_next(*iter)) {
        auto f = fcb::fcb_iterator_current(*iter);
        deserialized_features.push_back(json::parse(std::string(f.json)));
    }

    // Compare features
    assert(original_features.size() == deserialized_features.size());
    std::cout << "  ✓ Feature count matches: " << deserialized_features.size() << std::endl;

    {
        auto deser_map = index_by_id(deserialized_features);
        for (const auto& orig : original_features) {
            const std::string id = orig["id"].get<std::string>();
            if (deser_map.find(id) == deser_map.end()) {
                std::cerr << "[FAIL] feature '" << id << "' missing in deserialized output"
                          << std::endl;
                assert(false);
            }
            assert_feature_equal(orig, deser_map.at(id));
            std::cout << "  ✓ Feature '" << id << "' matches" << std::endl;
        }
    }

    // Extensions round-trip through metadata_json
    if (has_extensions) {
        assert(!std::string(meta.metadata_json).empty());
        json cj_header = json::parse(std::string(meta.metadata_json));
        assert(cj_header.contains("extensions"));

        const auto& orig_ext = header_cj["extensions"];
        const auto& deser_ext = cj_header["extensions"];
        assert(orig_ext.size() == deser_ext.size());
        std::cout << "  ✓ Extension count matches: " << deser_ext.size() << std::endl;

        for (auto& [key, value] : orig_ext.items()) {
            assert(deser_ext.contains(key));
            assert(deser_ext[key]["url"] == value["url"]);
            assert(deser_ext[key]["version"] == value["version"]);
            std::cout << "  ✓ Extension '" << key << "' preserved" << std::endl;
        }
    }

    fs::remove(temp_fcb);
    std::cout << "  ✅ Passed" << std::endl;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
int main(int argc, char* argv[]) {
    std::cout << "========================================" << std::endl;
    std::cout << "FlatCityBuf C++ Roundtrip Tests" << std::endl;
    std::cout << "========================================" << std::endl;

    fs::path test_data_dir;
    if (argc > 1) {
        test_data_dir = argv[1];
    } else {
        test_data_dir = "../build/tests/data";
        if (!fs::exists(test_data_dir))
            test_data_dir = "../../rust/fcb_core/tests/data";
    }

    if (!fs::exists(test_data_dir)) {
        std::cerr << "Error: test data directory not found: " << test_data_dir << std::endl;
        std::cerr << "Usage: " << argv[0] << " <test_data_dir>" << std::endl;
        return 1;
    }
    std::cout << "Test data: " << test_data_dir << "\n" << std::endl;

    try {
        test_cityjson_serialization_cycle(test_data_dir);
        test_geometry_template_cycle(test_data_dir);
        test_extension_serialization_cycle(test_data_dir);

        std::cout << "\n========================================" << std::endl;
        std::cout << "✅ All roundtrip tests passed!" << std::endl;
        std::cout << "========================================" << std::endl;
        return 0;
    } catch (const std::exception& e) {
        std::cerr << "\n❌ Test failed: " << e.what() << std::endl;
        return 1;
    }
}
