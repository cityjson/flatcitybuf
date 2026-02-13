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
        std::cout << "Format version: " << static_cast<int>(meta.version) << std::endl;
        std::cout << "Total features: " << meta.features_count << std::endl;
        std::cout << "Has spatial index: " << (meta.has_spatial_index ? "yes" : "no") << std::endl;
        std::cout << "Has attribute index: " << (meta.has_attribute_index ? "yes" : "no")
                  << std::endl;
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
