#include "lib.rs.h"
#include <iostream>
#include <stdexcept>
#include <string>

int main(int argc, char *argv[]) {
  if (argc < 2) {
    std::cerr << "Usage: " << argv[0] << " <fcb_file>" << std::endl;
    return 1;
  }

  std::string path = argv[1];

  try {
    // Open FCB file
    auto reader = fcb::fcb_reader_open(path);

    // Get metadata
    auto meta = fcb::fcb_reader_metadata(*reader);
    std::cout << "=== FCB File Metadata ===" << std::endl;
    std::cout << "Version: " << static_cast<int>(meta.version) << std::endl;
    std::cout << "Features count: " << meta.features_count << std::endl;
    std::cout << "Has spatial index: "
              << (meta.has_spatial_index ? "yes" : "no") << std::endl;
    std::cout << "Has attribute index: "
              << (meta.has_attribute_index ? "yes" : "no") << std::endl;
    std::cout << std::endl;

    // Select all features
    auto iter = fcb::fcb_reader_select_all(std::move(reader));

    std::cout << "=== Features ===" << std::endl;
    size_t count = 0;
    while (fcb::fcb_iterator_next(*iter)) {
      auto feature = fcb::fcb_iterator_current(*iter);
      std::cout << "Feature " << count << ": ID=" << std::string(feature.id)
                << std::endl;

      // Print first 200 chars of JSON
      std::string json = std::string(feature.json);
      if (json.length() > 200) {
        json = json.substr(0, 200) + "...";
      }
      std::cout << "  JSON: " << json << std::endl;

      count++;
      if (count >= 5) {
        std::cout << "  ... (showing first 5 features)" << std::endl;
        break;
      }
    }

    std::cout << std::endl;
    std::cout << "Total features iterated: " << count << std::endl;

  } catch (const std::exception &e) {
    std::cerr << "Error: " << e.what() << std::endl;
    return 1;
  }

  return 0;
}
