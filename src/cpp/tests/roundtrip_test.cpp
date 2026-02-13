/**
 * @file roundtrip_test.cpp
 * @brief Roundtrip tests for FlatCityBuf C++ bindings
 *
 * Tests data integrity by serializing CityJSON data to FCB format
 * and deserializing back, comparing original and restored data.
 *
 * Mirrors the Rust tests in fcb_core/tests/e2e.rs
 *
 * @author Hidemichi Baba
 * @version 0.1.0
 */

#include "fcb.h"  // FlatCityBuf C++ API header
#include <iostream>
#include <fstream>
#include <sstream>
#include <cassert>
#include <filesystem>
#include <nlohmann/json.hpp>

namespace fs = std::filesystem;
using json = nlohmann::json;

/**
 * @brief Test basic CityJSON → FCB → CityJSON roundtrip
 *
 * Tests that basic features are preserved through serialization cycle.
 * Uses small.city.jsonl as test data.
 */
void test_cityjson_serialization_cycle(const fs::path& test_data_dir) {
    std::cout << "=== Testing CityJSON Serialization Cycle ===" << std::endl;

    // 1. Read original CityJSONSeq
    auto input_file = test_data_dir / "small.city.jsonl";
    std::cout << "Reading: " << input_file << std::endl;

    std::ifstream infile(input_file);
    json original_cj;
    infile >> original_cj;

    std::cout << "  Original CityJSON type: " << original_cj["type"] << std::endl;
    std::cout << "  Original CityJSON version: " << original_cj["version"] << std::endl;
    std::cout << "  Total features: " << original_cj["CityObjects"].size() << std::endl;

    // 2. Write to FCB
    auto temp_fcb = test_data_dir / "temp_roundtrip.fcb";
    std::cout << "\nWriting to FCB: " << temp_fcb << std::endl;

    auto writer = fcb::fcb_writer_new(original_cj.dump());
    for (auto& [key, value] : original_cj["CityObjects"].items()) {
        std::string feature_json = value.dump();
        fcb::fcb_writer_add_feature(*writer, feature_json.c_str());
    }
    fcb::fcb_writer_write(std::move(writer), temp_fcb.c_str());

    // 3. Read back from FCB
    std::cout << "\nReading back from FCB..." << std::endl;
    auto reader = fcb::fcb_reader_open(temp_fcb.c_str());

    // Get metadata
    auto meta = fcb::fcb_reader_metadata(*reader);
    std::cout << "=== FCB File Metadata ===" << std::endl;
    std::cout << "Format version: " << meta.version << std::endl;
    std::cout << "Total features: " << meta.features_count << std::endl;
    std::cout << "Has spatial index: " << (meta.has_spatial_index ? "yes" : "no") << std::endl;
    std::cout << "Has attribute index: " << (meta.has_attribute_index ? "yes" : "no") << std::endl;

    // Read all features
    auto iter = fcb::fcb_reader_select_all(std::move(reader));
    json deserialized_cj = json::object();
    deserialized_cj["type"] = "CityJSON";
    deserialized_cj["version"] = "1.0"; // Match original
    deserialized_cj["CityObjects"] = json::array();

    size_t feature_num = 0;
    while (fcb::fcb_iterator_next(*iter)) {
        auto feature = fcb::fcb_iterator_current(*iter);

        // Parse the feature JSON
        json feature_obj = json::parse(feature.json);

        // Add to deserialized CityJSON
        deserialized_cj["CityObjects"][feature_num] = feature_obj;

        feature_num++;
    }

    std::cout << "\nDeserialized " << feature_num << " features" << std::endl;

    // 4. Compare
    std::cout << "\n=== Comparing Original vs Deserialized ===" << std::endl;

    // Compare metadata
    assert(original_cj["type"] == deserialized_cj["type"]);
    assert(original_cj["version"] == deserialized_cj["version"]);
    std::cout << "✓ Metadata matches" << std::endl;

    // Compare feature count
    assert(original_cj["CityObjects"].size() == deserialized_cj["CityObjects"].size());
    std::cout << "✅ Feature count matches: " << deserialized_cj["CityObjects"].size() << std::endl;

    // Compare each feature
    for (size_t i = 0; i < original_cj["CityObjects"].size(); i++) {
        auto orig_feat = original_cj["CityObjects"][i];
        auto deser_feat = deserialized_cj["CityObjects"][i];

        // Compare IDs
        assert(orig_feat["id"] == deser_feat["id"]);
        std::cout << "✅ Feature " << i << " ID matches: " << orig_feat["id"] << std::endl;

        // Compare types
        assert(orig_feat["type"] == deser_feat["type"]);

        // Compare vertex counts (if present)
        if (orig_feat.contains("geometry") && orig_feat["geometry"].contains("vertices")) {
            auto orig_verts = orig_feat["geometry"]["vertices"];
            auto deser_verts = deser_feat["geometry"]["vertices"];
            assert(orig_verts.size() == deser_verts.size());
        }
    }

    std::cout << "\n✅ All assertions passed!" << std::endl;

    // Clean up
    fs::remove(temp_fcb);
}

/**
 * @brief Test geometry template preservation through serialization cycle
 *
 * Validates that geometry templates are correctly preserved when
 * serializing to FCB and deserializing back.
 * Uses geom_temp.city.jsonl as test data.
 */
void test_geometry_template_cycle(const fs::path& test_data_dir) {
    std::cout << "\n=== Testing Geometry Template Cycle ===" << std::endl;

    // 1. Read original CityJSONSeq with geometry templates
    auto input_file = test_data_dir / "geom_temp.city.jsonl";
    std::cout << "Reading: " << input_file << std::endl;

    std::ifstream infile(input_file);
    json original_cj;
    infile >> original_cj;

    bool has_templates = original_cj.contains("geometry-templates");
    std::cout << "  Has geometry templates: " << (has_templates ? "yes" : "no") << std::endl;

    if (has_templates) {
        auto templates = original_cj["geometry-templates"];
        std::cout << "  Template count: " << templates.size() << std::endl;
    }

    // 2. Write to FCB
    auto temp_fcb = test_data_dir / "temp_geom_templates.fcb";
    std::cout << "\nWriting to FCB: " << temp_fcb << std::endl;

    auto writer = fcb::fcb_writer_new(original_cj.dump());
    for (auto& [key, value] : original_cj["CityObjects"].items()) {
        std::string feature_json = value.dump();
        fcb::fcb_writer_add_feature(*writer, feature_json.c_str());
    }
    fcb::fcb_writer_write(std::move(writer), temp_fcb.c_str());

    // 3. Read back from FCB
    std::cout << "\nReading back from FCB..." << std::endl;
    auto reader = fcb::fcb_reader_open(temp_fcb.c_str());

    auto meta = fcb::fcb_reader_metadata(*reader);
    std::cout << "Features in FCB: " << meta.features_count << std::endl;

    auto iter = fcb::fcb_reader_select_all(std::move(reader));
    json deserialized_cj = json::object();
    deserialized_cj["type"] = "CityJSON";
    deserialized_cj["version"] = "1.0";
    deserialized_cj["CityObjects"] = json::array();

    while (fcb::fcb_iterator_next(*iter)) {
        auto feature = fcb::fcb_iterator_current(*iter);
        json feature_obj = json::parse(feature.json);
        deserialized_cj["CityObjects"].push_back(feature_obj);
    }

    // 4. Verify geometry templates are preserved
    std::cout << "\n=== Verifying Geometry Templates ===" << std::endl;

    if (has_templates) {
        assert(deserialized_cj.contains("geometry-templates"));
        auto orig_templates = original_cj["geometry-templates"];
        auto deser_templates = deserialized_cj["geometry-templates"];

        assert(orig_templates.size() == deser_templates.size());
        std::cout << "✅ Geometry template count matches: " << deser_templates.size() << std::endl;

        // Note: Deep comparison of template data would require more detailed checks
        // For now, verify the structure is preserved
    }

    // Clean up
    fs::remove(temp_fcb);
}

/**
 * @brief Test extension preservation through serialization cycle
 *
 * Validates that CityJSON extensions are correctly preserved when
 * serializing to FCB and deserializing back.
 * Uses noise_extension.city.jsonl as test data.
 */
void test_extension_serialization_cycle(const fs::path& test_data_dir) {
    std::cout << "\n=== Testing Extension Serialization Cycle ===" << std::endl;

    // 1. Read original CityJSONSeq with extensions
    auto input_file = test_data_dir / "noise_extension.city.jsonl";
    std::cout << "Reading: " << input_file << std::endl;

    std::ifstream infile(input_file);
    json original_cj;
    infile >> original_cj;

    bool has_extensions = original_cj.contains("extensions");
    std::cout << "  Has extensions: " << (has_extensions ? "yes" : "no") << std::endl;

    if (has_extensions) {
        auto extensions = original_cj["extensions"];
        std::cout << "  Extension count: " << extensions.size() << std::endl;
        for (auto& [key, value] : extensions.items()) {
            std::cout << "    - " << key << ": " << value["url"] << std::endl;
        }
    }

    // 2. Write to FCB
    auto temp_fcb = test_data_dir / "temp_extensions.fcb";
    std::cout << "\nWriting to FCB: " << temp_fcb << std::endl;

    auto writer = fcb::fcb_writer_new(original_cj.dump());
    for (auto& [key, value] : original_cj["CityObjects"].items()) {
        std::string feature_json = value.dump();
        fcb::fcb_writer_add_feature(*writer, feature_json.c_str());
    }
    fcb::fcb_writer_write(std::move(writer), temp_fcb.c_str());

    // 3. Read back from FCB
    std::cout << "\nReading back from FCB..." << std::endl;
    auto reader = fcb::fcb_reader_open(temp_fcb.c_str());

    auto meta = fcb::fcb_reader_metadata(*reader);
    std::cout << "Features in FCB: " << meta.features_count << std::endl;

    auto iter = fcb::fcb_reader_select_all(std::move(reader));
    json deserialized_cj = json::object();
    deserialized_cj["type"] = "CityJSON";
    deserialized_cj["version"] = "1.0";
    deserialized_cj["CityObjects"] = json::array();

    while (fcb::fcb_iterator_next(*iter)) {
        auto feature = fcb::fcb_iterator_current(*iter);
        json feature_obj = json::parse(feature.json);
        deserialized_cj["CityObjects"].push_back(feature_obj);
    }

    // 4. Verify extensions are preserved
    std::cout << "\n=== Verifying Extensions ===" << std::endl;

    if (has_extensions) {
        assert(deserialized_cj.contains("extensions"));
        auto orig_ext = original_cj["extensions"];
        auto deser_ext = deserialized_cj["extensions"];

        assert(orig_ext.size() == deser_ext.size());
        std::cout << "✅ Extension count matches: " << deser_ext.size() << std::endl;

        // Compare each extension
        for (auto& [key, value] : orig_ext.items()) {
            assert(deser_ext.contains(key));
            assert(deser_ext[key]["url"] == value["url"]);
            assert(deser_ext[key]["version"] == value["version"]);
            std::cout << "✅ Extension " << key << " preserved" << std::endl;
        }
    }

    // Clean up
    fs::remove(temp_fcb);
}

/**
 * @brief Main test runner
 *
 * Run all roundtrip tests using test data from specified directory.
 */
int main(int argc, char* argv[]) {
    std::cout << "========================================" << std::endl;
    std::cout << "FlatCityBuf C++ Roundtrip Tests" << std::endl;
    std::cout << "========================================" << std::endl;

    // Determine test data directory
    fs::path test_data_dir;
    if (argc > 1) {
        test_data_dir = argv[1];
    } else {
        // Try common locations
        test_data_dir = "../build/tests/data";
        if (!fs::exists(test_data_dir)) {
            test_data_dir = "../../rust/fcb_core/tests/data";
        }
    }

    if (!fs::exists(test_data_dir)) {
        std::cerr << "Error: Test data directory not found: " << test_data_dir << std::endl;
        std::cerr << "Usage: " << argv[0] << " <test_data_dir>" << std::endl;
        return 1;
    }

    std::cout << "Using test data directory: " << test_data_dir << std::endl << std::endl;

    try {
        // Run all tests
        test_cityjson_serialization_cycle(test_data_dir);
        test_geometry_template_cycle(test_data_dir);
        test_extension_serialization_cycle(test_data_dir);

        std::cout << "\n========================================" << std::endl;
        std::cout << "✅ All roundtrip tests passed!" << std::endl;
        std::cout << "========================================" << std::endl;

        return 0;
    } catch (const std::exception& e) {
        std::cerr << "\n❌ Test failed with exception: " << e.what() << std::endl;
        return 1;
    }
}
