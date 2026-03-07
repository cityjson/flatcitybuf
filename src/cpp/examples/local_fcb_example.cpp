/**
 * @file local_fcb_example.cpp
 * @brief Example of reading local FCB files with C++ API
 *
 * Demonstrates:
 * - Opening FCB files from local disk
 * - Reading file metadata
 * - Iterating through all features
 * - Spatial filtering with bounding box queries
 * - Parsing feature attributes and geometry
 *
 * @note This example uses only local file operations.
 *       For HTTP/remote file access, use the CLI tool: `fcb info -i <url>`
 *
 * @author Hidemichi Baba
 * @copyright MIT License
 */

#include "fcb.h"  // FlatCityBuf C++ API header

#include <nlohmann/json.hpp>  // JSON parsing for metadata_json

#include <iostream>
#include <stdexcept>
#include <string>

int main(int argc, char* argv[]) {
    if (argc < 2) {
        std::cerr << "Usage: " << argv[0] << " <fcb_file>" << std::endl;
        std::cerr << "\nExample: Read FCB metadata and iterate features" << std::endl;
        return 1;
    }

    std::string fcb_path = argv[1];

    try {
        // === Open FCB file ===
        auto reader = fcb::fcb_reader_open(fcb_path);

        // === Get metadata ===
        auto meta = fcb::fcb_reader_metadata(*reader);

        std::cout << "\n=== FCB File Metadata ===" << std::endl;
        // --- FCB binary format fields ---
        std::cout << "Format version:     " << static_cast<int>(meta.version) << std::endl;
        std::cout << "Total features:     " << meta.features_count << std::endl;
        std::cout << "Spatial index:      " << (meta.has_spatial_index ? "yes" : "no") << std::endl;
        std::cout << "Attribute index:    " << (meta.has_attribute_index ? "yes" : "no")
                  << std::endl;

        // --- CityJSON metadata fields ---
        std::cout << "CityJSON version:   " << std::string(meta.cityjson_version) << std::endl;

        if (meta.has_transform) {
            std::cout << "Transform scale:    [" << meta.transform.scale_x << ", "
                      << meta.transform.scale_y << ", " << meta.transform.scale_z << "]"
                      << std::endl;
            std::cout << "Transform offset:   [" << meta.transform.translate_x << ", "
                      << meta.transform.translate_y << ", " << meta.transform.translate_z << "]"
                      << std::endl;
        }

        if (meta.has_geographical_extent) {
            std::cout << "Extent min:         [" << meta.geographical_extent.min_x << ", "
                      << meta.geographical_extent.min_y << ", " << meta.geographical_extent.min_z
                      << "]" << std::endl;
            std::cout << "Extent max:         [" << meta.geographical_extent.max_x << ", "
                      << meta.geographical_extent.max_y << ", " << meta.geographical_extent.max_z
                      << "]" << std::endl;
        }

        // Parse metadata_json to access identifier, title, referenceSystem, extensions, etc.
        if (!std::string(meta.metadata_json).empty()) {
            nlohmann::json cj_meta = nlohmann::json::parse(std::string(meta.metadata_json));

            if (cj_meta.contains("metadata")) {
                auto& m = cj_meta["metadata"];
                if (m.contains("datasetTitle"))
                    std::cout << "Title:              " << m["datasetTitle"] << std::endl;
                if (m.contains("datasetIdentifier"))
                    std::cout << "Identifier:         " << m["datasetIdentifier"] << std::endl;
                if (m.contains("datasetReferenceDate"))
                    std::cout << "Reference date:     " << m["datasetReferenceDate"] << std::endl;
                if (m.contains("referenceSystem"))
                    std::cout << "CRS:                " << m["referenceSystem"] << std::endl;
                if (m.contains("pointOfContact")) {
                    auto& poc = m["pointOfContact"];
                    if (poc.contains("contactName"))
                        std::cout << "Contact:            " << poc["contactName"] << std::endl;
                }
            }
            if (cj_meta.contains("extensions")) {
                std::cout << "Extensions:         " << cj_meta["extensions"].dump() << std::endl;
            }
        }
        std::cout << std::endl;

        // === Select all features ===
        auto iter = fcb::fcb_reader_select_all(std::move(reader));

        std::cout << "\nIterating through features..." << std::endl;
        size_t count = 0;
        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);

            std::cout << "Feature #" << count << std::endl;
            std::cout << "  ID: " << std::string(feature.id) << std::endl;

            count++;
            if (count >= 5) {
                std::cout << "  ... (showing first 5 features)" << std::endl;
                break;
            }
        }

        std::cout << "\nTotal features iterated: " << count << std::endl;

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }

    return 0;
}
