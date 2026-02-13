#include "lib.rs.h"

#include <fstream>
#include <iostream>
#include <string>

int main(int argc, char* argv[]) {
    if (argc < 3) {
        std::cerr << "Usage: " << argv[0] << " <input.city.jsonl> <output.fcb>" << std::endl;
        return 1;
    }

    std::string jsonl_path = argv[1];
    std::string fcb_path = argv[2];

    try {
        // === Step 1: Encode CityJSONSeq -> FCB ===
        std::cout << "=== Encoding CityJSONSeq -> FCB ===" << std::endl;

        std::ifstream infile(jsonl_path);
        if (!infile.is_open()) {
            std::cerr << "Failed to open: " << jsonl_path << std::endl;
            return 1;
        }

        std::string line;
        // First line is CityJSON metadata
        if (!std::getline(infile, line)) {
            std::cerr << "Empty input file" << std::endl;
            return 1;
        }

        auto writer = fcb::fcb_writer_new(line);
        std::cout << "Created writer with metadata" << std::endl;

        size_t write_count = 0;
        while (std::getline(infile, line)) {
            if (line.empty())
                continue;
            fcb::fcb_writer_add_feature(*writer, line);
            write_count++;
        }
        infile.close();

        std::cout << "Added " << write_count << " features" << std::endl;

        fcb::fcb_writer_write(std::move(writer), fcb_path);
        std::cout << "Wrote FCB file: " << fcb_path << std::endl;

        // === Step 2: Decode FCB -> read back ===
        std::cout << "\n=== Decoding FCB ===" << std::endl;

        auto reader = fcb::fcb_reader_open(fcb_path);
        auto meta = fcb::fcb_reader_metadata(*reader);

        std::cout << "Features count: " << meta.features_count << std::endl;
        std::cout << "Has spatial index: " << (meta.has_spatial_index ? "yes" : "no") << std::endl;

        auto iter = fcb::fcb_reader_select_all(std::move(reader));

        size_t read_count = 0;
        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);
            read_count++;

            // Print first 3 features briefly
            if (read_count <= 3) {
                std::cout << "  Feature " << read_count << ": ID=" << std::string(feature.id)
                          << std::endl;
            }
        }

        std::cout << "Total features read back: " << read_count << std::endl;

        // === Step 3: Verify ===
        std::cout << "\n=== Roundtrip Verification ===" << std::endl;
        if (read_count == write_count) {
            std::cout << "PASS: wrote " << write_count << " features, read back " << read_count
                      << std::endl;
        } else {
            std::cerr << "FAIL: wrote " << write_count << " but read back " << read_count
                      << std::endl;
            return 1;
        }

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }

    return 0;
}
