/**
 * @file comprehensive_example.cpp
 * @brief Comprehensive examples of FlatCityBuf C++ API usage
 *
 * This file demonstrates various features of the FlatCityBuf C++ API including:
 * - Reading FCB files from local storage
 * - Accessing file metadata
 * - Iterating through all features
 * - Spatial filtering with bounding box queries
 * - Parsing feature attributes and geometry
 * - Writing FCB files from CityJSONSeq (.city.jsonl)
 * - Converting FCB files back to CityJSONSeq (.city.jsonl)
 *
 * @note HTTP/remote reading is currently only available in Rust,
 *       not exposed through C++ bindings yet.
 *
 * @author Hidemichi Baba
 * @note HTTP/Remote Reading: Currently not available through C++ API.
 *       Use CLI tool: `fcb info -i <url>` or download files first.
 * @copyright MIT License
 */

#include "fcb.h"  // FlatCityBuf C++ API header

#include <nlohmann/json.hpp>  // For JSON parsing: https://github.com/nlohmann/json

#include <fstream>
#include <iostream>
#include <stdexcept>
#include <string>

// ============================================================================
// Example 1: Reading FCB File and Accessing Metadata
// ============================================================================

void example_read_metadata(const std::string& fcb_path) {
    std::cout << "\n=== Example 1: Reading FCB Metadata ===" << std::endl;

    try {
        // Open FCB file
        auto reader = fcb::fcb_reader_open(fcb_path);

        // Get metadata
        auto meta = fcb::fcb_reader_metadata(*reader);

        // --- FCB binary format fields ---
        std::cout << "Format version:    " << static_cast<int>(meta.version) << std::endl;
        std::cout << "Total features:    " << meta.features_count << std::endl;
        std::cout << "Spatial index:     " << (meta.has_spatial_index ? "yes" : "no") << std::endl;
        std::cout << "Attribute index:   " << (meta.has_attribute_index ? "yes" : "no")
                  << std::endl;

        // --- CityJSON metadata: typed convenience fields ---
        std::cout << "CityJSON version:  " << std::string(meta.cityjson_version) << std::endl;

        if (meta.has_transform) {
            std::cout << "Transform scale:   [" << meta.transform.scale_x << ", "
                      << meta.transform.scale_y << ", " << meta.transform.scale_z << "]"
                      << std::endl;
            std::cout << "Transform offset:  [" << meta.transform.translate_x << ", "
                      << meta.transform.translate_y << ", " << meta.transform.translate_z << "]"
                      << std::endl;
        } else {
            std::cout << "Transform:         (not present)" << std::endl;
        }

        if (meta.has_geographical_extent) {
            std::cout << "Extent min:        [" << meta.geographical_extent.min_x << ", "
                      << meta.geographical_extent.min_y << ", " << meta.geographical_extent.min_z
                      << "]" << std::endl;
            std::cout << "Extent max:        [" << meta.geographical_extent.max_x << ", "
                      << meta.geographical_extent.max_y << ", " << meta.geographical_extent.max_z
                      << "]" << std::endl;
        } else {
            std::cout << "Extent:            (not present)" << std::endl;
        }

        // --- CityJSON metadata: full JSON string ---
        // metadata_json contains the complete CityJSON header (type, version, transform,
        // metadata, referenceSystem, extensions). Parse it to access any field.
        if (!std::string(meta.metadata_json).empty()) {
            nlohmann::json cj = nlohmann::json::parse(std::string(meta.metadata_json));

            std::cout << "\n--- CityJSON Metadata (from metadata_json) ---" << std::endl;

            if (cj.contains("metadata")) {
                auto& m = cj["metadata"];
                if (m.contains("datasetTitle"))
                    std::cout << "Title:             " << m["datasetTitle"] << std::endl;
                if (m.contains("datasetIdentifier"))
                    std::cout << "Identifier:        " << m["datasetIdentifier"] << std::endl;
                if (m.contains("datasetReferenceDate"))
                    std::cout << "Reference date:    " << m["datasetReferenceDate"] << std::endl;
                if (m.contains("referenceSystem"))
                    std::cout << "CRS:               " << m["referenceSystem"] << std::endl;

                if (m.contains("pointOfContact")) {
                    auto& poc = m["pointOfContact"];
                    std::cout << "Point of contact:" << std::endl;
                    if (poc.contains("contactName"))
                        std::cout << "  Name:            " << poc["contactName"] << std::endl;
                    if (poc.contains("emailAddress"))
                        std::cout << "  Email:           " << poc["emailAddress"] << std::endl;
                    if (poc.contains("contactType"))
                        std::cout << "  Type:            " << poc["contactType"] << std::endl;
                    if (poc.contains("website"))
                        std::cout << "  Website:         " << poc["website"] << std::endl;
                    if (poc.contains("address")) {
                        auto& addr = poc["address"];
                        std::cout << "  Address:         " << addr.dump() << std::endl;
                    }
                }
            }

            if (cj.contains("extensions") && !cj["extensions"].empty()) {
                std::cout << "Extensions:" << std::endl;
                for (auto& [name, ext] : cj["extensions"].items()) {
                    std::cout << "  " << name << " -> url: " << ext.value("url", "")
                              << ", version: " << ext.value("version", "") << std::endl;
                }
            }
        }

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

// ============================================================================
// Example 2: Iterating All Features
// ============================================================================

void example_iterate_all_features(const std::string& fcb_path) {
    std::cout << "\n=== Example 2: Iterating All Features ===" << std::endl;

    try {
        auto reader = fcb::fcb_reader_open(fcb_path);
        auto meta = fcb::fcb_reader_metadata(*reader);

        std::cout << "Iterating through " << meta.features_count << " features..." << std::endl;

        // Select all features for iteration
        auto iter = fcb::fcb_reader_select_all(std::move(reader));

        size_t count = 0;
        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);

            std::cout << "Feature #" << count << std::endl;
            std::cout << "  ID: " << std::string(feature.id) << std::endl;

            // Parse the JSON to access specific fields
            nlohmann::json cj_feature = nlohmann::json::parse(std::string(feature.json));

            // Access type attribute
            if (cj_feature.contains("type") && cj_feature["type"] == "CityJSONFeature") {
                // Vertices — integer coordinates stored in FCB, converted to real-world
                // using the transform: real = integer * scale + translate
                if (cj_feature.contains("vertices")) {
                    auto& verts = cj_feature["vertices"];
                    std::cout << "  Vertices (" << verts.size() << " total):" << std::endl;

                    size_t show = std::min(verts.size(), size_t(3));
                    for (size_t v = 0; v < show; v++) {
                        auto ix = verts[v][0].get<int64_t>();
                        auto iy = verts[v][1].get<int64_t>();
                        auto iz = verts[v][2].get<int64_t>();
                        std::cout << "    [" << v << "] int: [" << ix << ", " << iy << ", " << iz
                                  << "]";
                        if (meta.has_transform) {
                            double rx = ix * meta.transform.scale_x + meta.transform.translate_x;
                            double ry = iy * meta.transform.scale_y + meta.transform.translate_y;
                            double rz = iz * meta.transform.scale_z + meta.transform.translate_z;
                            std::cout << "  real: [" << rx << ", " << ry << ", " << rz << "]";
                        }
                        std::cout << std::endl;
                    }
                    if (verts.size() > show)
                        std::cout << "    ... (" << verts.size() - show << " more)" << std::endl;
                }
                if (cj_feature.contains("CityObjects")) {
                    auto& city_objects = cj_feature["CityObjects"];

                    // Iterate through CityObjects in this feature
                    for (auto& [obj_id, city_obj] : city_objects.items()) {
                        std::cout << "  CityObject: " << obj_id << std::endl;

                        // Get object type (Building, BuildingPart, etc.)
                        if (city_obj.contains("type")) {
                            std::cout << "    Type: " << city_obj["type"] << std::endl;
                        }

                        // Access attributes if present
                        if (city_obj.contains("attributes")) {
                            auto& attrs = city_obj["attributes"];

                            // Print all attributes
                            std::cout << "    Attributes:" << std::endl;
                            for (auto& [key, value] : attrs.items()) {
                                std::cout << "      " << key << ": "
                                          << value.dump(-1, ' ', false,
                                                        nlohmann::json::error_handler_t::replace)
                                          << std::endl;
                            }
                        }

                        // Access geometry information
                        if (city_obj.contains("geometry")) {
                            auto& geom = city_obj["geometry"];

                            if (geom.is_array() && geom.size() > 0) {
                                auto& first_geom = geom[0];

                                // Get geometry type
                                if (first_geom.contains("type")) {
                                    std::cout << "    Geometry type: " << first_geom["type"]
                                              << std::endl;
                                }

                                // Get LoD (Level of Detail)
                                if (first_geom.contains("lod")) {
                                    std::cout << "    LoD: " << first_geom["lod"] << std::endl;
                                }

                                // Get boundaries (geometry vertices)
                                if (first_geom.contains("boundaries")) {
                                    std::cout << "    Has geometry boundaries" << std::endl;
                                }
                            }
                        }
                    }
                }
            }

            count++;
            if (count >= 3) {
                std::cout << "  ... (showing first 3 features)" << std::endl;
                break;
            }
        }

        std::cout << "Total features iterated: " << count << std::endl;

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

// ============================================================================
// Example 3: Spatial Query with Bounding Box
// ============================================================================

void example_spatial_query(const std::string& fcb_path) {
    std::cout << "\n=== Example 3: Spatial Query (Bounding Box) ===" << std::endl;

    try {
        auto reader = fcb::fcb_reader_open(fcb_path);
        auto meta = fcb::fcb_reader_metadata(*reader);

        if (!meta.has_spatial_index) {
            std::cout << "Note: This file has no spatial index. "
                      << "Bbox query will still work but may be slower." << std::endl;
        }

        // Define a bounding box
        // Coordinates must be in the same CRS as the FCB file
        fcb::BoundingBox bbox;
        bbox.min_x = 85000.0;
        bbox.min_y = 446000.0;
        bbox.max_x = 85100.0;
        bbox.max_y = 446100.0;

        std::cout << "Querying bbox: [" << bbox.min_x << ", " << bbox.min_y << ", " << bbox.max_x
                  << ", " << bbox.max_y << "]" << std::endl;

        // Select features within bounding box
        auto iter = fcb::fcb_reader_select_bbox(std::move(reader), bbox);

        size_t count = 0;
        size_t matched_count = 0;
        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);

            // Parse JSON to access feature data
            nlohmann::json cj_feature = nlohmann::json::parse(std::string(feature.json));

            // Check if this feature has CityObjects
            if (cj_feature.contains("CityObjects")) {
                auto& city_objects = cj_feature["CityObjects"];

                for (auto& [obj_id, city_obj] : city_objects.items()) {
                    // You can access specific attributes
                    if (city_obj.contains("attributes")) {
                        auto& attrs = city_obj["attributes"];

                        // Example: Access specific attribute
                        if (attrs.contains("b3_h_dak_50p")) {
                            auto& value = attrs["b3_h_dak_50p"];
                            std::cout << "Feature " << std::string(feature.id)
                                      << " has b3_h_dak_50p: " << value << std::endl;
                            matched_count++;
                        }
                    }
                }
            }

            count++;
            if (count >= 10) {
                std::cout << "  ... (showing first 10 matches)" << std::endl;
                break;
            }
        }

        std::cout << "Total features in bbox: " << count << std::endl;
        std::cout << "Features with matching attribute: " << matched_count << std::endl;

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

// ============================================================================
// Example 4: Writing FCB from CityJSON/CityJSONSeq
// ============================================================================

void example_write_fcb() {
    std::cout << "\n=== Example 4: Writing FCB File ===" << std::endl;

    try {
        // Step 1: Create CityJSON metadata
        // This defines the header structure of the FCB file
        nlohmann::json metadata;
        metadata["type"] = "CityJSON";
        metadata["version"] = "2.0";
        metadata["transform"]["scale"] = {0.001, 0.001, 0.001};
        metadata["transform"]["translate"] = {85088.0, 446394.0, 0.0};
        metadata["metadata"]["datasetTitle"] = "Example Buildings";
        metadata["metadata"]["datasetReferenceDate"] = "2025-01-01";
        metadata["CityObjects"] = nlohmann::json::object();
        metadata["vertices"] = nlohmann::json::array();

        std::string metadata_str = metadata.dump();

        // Step 2: Create FCB writer
        auto writer = fcb::fcb_writer_new(metadata_str);

        std::cout << "Created FCB writer with metadata:" << std::endl;
        std::cout << metadata.dump(2) << std::endl;

        // Step 3: Add CityJSON features

        // Feature 1: A simple building
        nlohmann::json feature1;
        feature1["type"] = "CityJSONFeature";
        feature1["id"] = "BLD001";
        feature1["CityObjects"]["BLD001-0"]["type"] = "Building";
        feature1["CityObjects"]["BLD001-0"]["attributes"]["height"] = 15.5;
        feature1["CityObjects"]["BLD001-0"]["attributes"]["year"] = 2020;
        feature1["CityObjects"]["BLD001-0"]["attributes"]["function"] = "residential";
        feature1["vertices"] = nlohmann::json::array();
        // CityJSON Solid geometry boundaries have complex nesting (6 levels deep)
        // This is a minimal placeholder - real data would have actual vertex indices
        nlohmann::json geom1;
        geom1["type"] = "Solid";
        geom1["lod"] = "1.2";
        // Build boundaries step by step to avoid brace counting issues
        std::vector<int> verts = {0, 1, 2, 3};
        geom1["boundaries"] = {{{{{verts}}}}};
        feature1["CityObjects"]["BLD001-0"]["geometry"].push_back(geom1);

        std::string feature1_str = feature1.dump();
        fcb::fcb_writer_add_feature(*writer, feature1_str);
        std::cout << "Added feature: " << feature1["id"] << std::endl;

        // Feature 2: Another building
        nlohmann::json feature2;
        feature2["type"] = "CityJSONFeature";
        feature2["id"] = "BLD002";
        feature2["CityObjects"]["BLD002-0"]["type"] = "Building";
        feature2["CityObjects"]["BLD002-0"]["attributes"]["height"] = 12.0;
        feature2["CityObjects"]["BLD002-0"]["attributes"]["year"] = 2018;
        feature2["CityObjects"]["BLD002-0"]["attributes"]["function"] = "commercial";
        feature2["vertices"] = nlohmann::json::array();
        nlohmann::json geom2;
        geom2["type"] = "Solid";
        geom2["lod"] = "1.2";
        geom2["boundaries"] = {{{{{verts}}}}};
        feature2["CityObjects"]["BLD002-0"]["geometry"].push_back(geom2);

        std::string feature2_str = feature2.dump();
        fcb::fcb_writer_add_feature(*writer, feature2_str);
        std::cout << "Added feature: " << feature2["id"] << std::endl;

        // Feature 3: Building with parts
        nlohmann::json feature3;
        feature3["type"] = "CityJSONFeature";
        feature3["id"] = "BLD003";
        feature3["CityObjects"]["BLD003-0"]["type"] = "Building";
        feature3["CityObjects"]["BLD003-0"]["attributes"]["height"] = 8.5;
        feature3["CityObjects"]["BLD003-0"]["attributes"]["year"] = 1995;
        feature3["vertices"] = nlohmann::json::array();
        nlohmann::json geom3a;
        geom3a["type"] = "Solid";
        geom3a["lod"] = "1.0";
        geom3a["boundaries"] = {{{{{verts}}}}};
        feature3["CityObjects"]["BLD003-0"]["geometry"].push_back(geom3a);
        feature3["CityObjects"]["BLD003-1"]["type"] = "BuildingPart";
        feature3["CityObjects"]["BLD003-1"]["attributes"]["measure"] = "roof";
        nlohmann::json geom3b;
        geom3b["type"] = "Solid";
        geom3b["lod"] = "1.2";
        geom3b["boundaries"] = {{{{{verts}}}}};
        feature3["CityObjects"]["BLD003-1"]["geometry"].push_back(geom3b);

        std::string feature3_str = feature3.dump();
        fcb::fcb_writer_add_feature(*writer, feature3_str);
        std::cout << "Added feature: " << feature3["id"] << std::endl;

        // Step 4: Write FCB file
        std::string output_path = "example_output.fcb";
        fcb::fcb_writer_write(std::move(writer), output_path);

        std::cout << "Successfully wrote FCB file to: " << output_path << std::endl;

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

// ============================================================================
// Example 5: Reading CityJSONSeq and Converting to FCB
// ============================================================================

/**
 * @brief Example of reading CityJSONSeq and converting to FCB
 *
 * CityJSONSeq is a sequence format where each line is a complete CityJSON
 * object. The first line contains the header (metadata), and subsequent lines
 * contain individual features.
 *
 * File structure:
 * Line 1: {"type":"CityJSON","version":"2.0",...,"CityObjects":{}, "vertices":[]}
 * Line 2: {"type":"CityJSONFeature","id":"...", "CityObjects":{...}}
 * Line 3: {"type":"CityJSONFeature","id":"...", "CityObjects":{...}}
 * ...
 */
void example_cityjsonseq_to_fcb(const std::string& cjseq_path,
                                 const std::string& output_fcb_path) {
    std::cout << "\n=== Example 5: CityJSONSeq to FCB ===" << std::endl;

    if (cjseq_path.empty()) {
        std::cout << "  (skipped — no .city.jsonl path provided)" << std::endl;
        std::cout << "  Usage: pass a .city.jsonl file as the second argument" << std::endl;
        return;
    }

    try {
        std::ifstream infile(cjseq_path);
        if (!infile.is_open())
            throw std::runtime_error("Cannot open: " + cjseq_path);

        // Line 1: CityJSON header (metadata + empty CityObjects/vertices)
        std::string header_line;
        while (std::getline(infile, header_line))
            if (!header_line.empty()) break;

        if (header_line.empty())
            throw std::runtime_error("Empty CityJSONSeq file");

        auto writer = fcb::fcb_writer_new(header_line);
        size_t feature_count = 0;

        // Subsequent lines: one CityJSONFeature per line
        std::string line;
        while (std::getline(infile, line)) {
            if (line.empty()) continue;
            fcb::fcb_writer_add_feature(*writer, line);
            ++feature_count;
        }

        fcb::fcb_writer_write(std::move(writer), output_fcb_path);

        std::cout << "  Features written: " << feature_count << std::endl;
        std::cout << "  Output FCB:       " << output_fcb_path << std::endl;

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

// ============================================================================
// Example 6b: FCB to CityJSONSeq
// ============================================================================

/**
 * @brief Convert an FCB file back to CityJSONSeq (.city.jsonl) format.
 *
 * The CityJSONSeq format is a newline-delimited JSON sequence:
 *   Line 1:  CityJSON header (type, version, transform, metadata, extensions, ...)
 *   Line N+: One CityJSONFeature object per line
 *
 * The FCB metadata_json field provides the header line verbatim, and
 * each feature's json field provides the corresponding CityJSONFeature line.
 */
void example_fcb_to_cityjsonseq(const std::string& fcb_path,
                                 const std::string& output_cjseq_path) {
    std::cout << "\n=== Example 6b: FCB to CityJSONSeq ===" << std::endl;

    try {
        auto reader = fcb::fcb_reader_open(fcb_path);
        auto meta   = fcb::fcb_reader_metadata(*reader);

        if (std::string(meta.metadata_json).empty())
            throw std::runtime_error("FCB file has no metadata_json; cannot write CityJSONSeq header");

        std::ofstream outfile(output_cjseq_path);
        if (!outfile.is_open())
            throw std::runtime_error("Cannot open output file: " + output_cjseq_path);

        // Line 1: CityJSON header derived from FCB metadata
        outfile << std::string(meta.metadata_json) << "\n";

        // Subsequent lines: one CityJSONFeature JSON per feature
        auto iter = fcb::fcb_reader_select_all(std::move(reader));
        size_t feature_count = 0;
        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);
            outfile << std::string(feature.json) << "\n";
            ++feature_count;
        }

        std::cout << "  Features written: " << feature_count << std::endl;
        std::cout << "  Output CityJSONSeq: " << output_cjseq_path << std::endl;

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

// ============================================================================
// Example 6: Accessing Specific Attribute Types
// ============================================================================

void example_access_attributes(const std::string& fcb_path) {
    std::cout << "\n=== Example 6: Accessing Attribute Types ===" << std::endl;

    try {
        auto reader = fcb::fcb_reader_open(fcb_path);
        auto iter = fcb::fcb_reader_select_all(std::move(reader));

        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);
            nlohmann::json cj_feature = nlohmann::json::parse(std::string(feature.json));

            if (cj_feature.contains("CityObjects")) {
                auto& city_objects = cj_feature["CityObjects"];

                for (auto& [obj_id, city_obj] : city_objects.items()) {
                    if (!city_obj.contains("attributes")) {
                        continue;
                    }

                    auto& attrs = city_obj["attributes"];

                    std::cout << "\nObject ID: " << obj_id << std::endl;
                    std::cout << "  Attributes:" << std::endl;

                    // String attribute
                    if (attrs.contains("identificatie")) {
                        auto& val = attrs["identificatie"];
                        std::cout << "    identificatie (string): " << val << std::endl;
                    }

                    // Numeric attribute (float/double)
                    if (attrs.contains("b3_h_dak_50p")) {
                        auto& val = attrs["b3_h_dak_50p"];
                        if (val.is_number()) {
                            double num_val = val.get<double>();
                            std::cout << "    b3_h_dak_50p (number): " << num_val << std::endl;
                        }
                    }

                    // Integer attribute
                    if (attrs.contains("bouwjaar")) {
                        auto& val = attrs["bouwjaar"];
                        if (val.is_number_integer()) {
                            int int_val = val.get<int>();
                            std::cout << "    bouwjaar (integer): " << int_val << std::endl;
                        }
                    }

                    // Date/time attribute
                    if (attrs.contains("tijdstipregistratie")) {
                        auto& val = attrs["tijdstipregistratie"];
                        std::cout << "    tijdstipregistratie (datetime): " << val << std::endl;
                    }

                    // Boolean attribute
                    if (attrs.contains("is_gemeentelijk")) {
                        auto& val = attrs["is_gemeentelijk"];
                        if (val.is_boolean()) {
                            bool bool_val = val.get<bool>();
                            std::cout << "    is_gemeentelijk (boolean): "
                                      << (bool_val ? "true" : "false") << std::endl;
                        }
                    }

                    // Array attribute
                    if (attrs.contains("tags")) {
                        auto& val = attrs["tags"];
                        if (val.is_array()) {
                            std::cout << "    tags (array): [";
                            for (auto& tag : val) {
                                std::cout << tag << " ";
                            }
                            std::cout << "]" << std::endl;
                        }
                    }
                }
            }

            // Show only first feature
            break;
        }

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

// ============================================================================
// Example 7: Accessing Geometry Details
// ============================================================================

void example_access_geometry(const std::string& fcb_path) {
    std::cout << "\n=== Example 7: Accessing Geometry ===" << std::endl;

    try {
        auto reader = fcb::fcb_reader_open(fcb_path);
        auto iter = fcb::fcb_reader_select_all(std::move(reader));

        while (fcb::fcb_iterator_next(*iter)) {
            auto feature = fcb::fcb_iterator_current(*iter);
            nlohmann::json cj_feature = nlohmann::json::parse(std::string(feature.json));

            if (cj_feature.contains("CityObjects")) {
                auto& city_objects = cj_feature["CityObjects"];

                for (auto& [obj_id, city_obj] : city_objects.items()) {
                    std::cout << "\nObject: " << obj_id << std::endl;

                    // Get object type
                    if (city_obj.contains("type")) {
                        std::string type = city_obj["type"];
                        std::cout << "  Type: " << type << std::endl;
                    }

                    if (city_obj.contains("geometry")) {
                        auto& geom_list = city_obj["geometry"];

                        if (geom_list.is_array() && geom_list.size() > 0) {
                            for (size_t i = 0; i < geom_list.size(); i++) {
                                auto& geom = geom_list[i];

                                // Geometry type (Solid, MultiSurface, etc.)
                                if (geom.contains("type")) {
                                    std::cout << "  Geometry #" << i << " type: " << geom["type"]
                                              << std::endl;
                                }

                                // Level of Detail
                                if (geom.contains("lod")) {
                                    std::cout << "    LoD: " << geom["lod"] << std::endl;
                                }

                                // Boundaries (vertices and faces)
                                if (geom.contains("boundaries")) {
                                    auto& boundaries = geom["boundaries"];
                                    std::cout << "    Boundaries: " << boundaries.dump()
                                              << std::endl;
                                }

                                // For MultiSurface and CompositeSurface
                                if (geom.contains("semantics")) {
                                    auto& sem = geom["semantics"];
                                    std::cout << "    Semantics: " << sem.dump() << std::endl;
                                }
                            }
                        }
                    }
                }
            }

            // Show only first feature
            break;
        }

    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

// ============================================================================
// Main - Run all examples
// ============================================================================

int main(int argc, char* argv[]) {
    if (argc < 2) {
        std::cerr << "Usage: " << argv[0] << " <fcb_file> [input.city.jsonl]" << std::endl;
        std::cerr << "\nExamples will use the provided FCB file." << std::endl;
        std::cerr << "Optional second argument: a .city.jsonl file to test CityJSONSeq→FCB conversion."
                  << std::endl;
        return 1;
    }

    std::string fcb_path  = argv[1];
    std::string cjseq_in  = (argc >= 3) ? argv[2] : "";

    std::cout << "==================================================" << std::endl;
    std::cout << "FlatCityBuf C++ API Comprehensive Examples" << std::endl;
    std::cout << "==================================================" << std::endl;

    // Run all examples
    example_read_metadata(fcb_path);
    example_iterate_all_features(fcb_path);
    example_spatial_query(fcb_path);
    example_access_attributes(fcb_path);
    example_access_geometry(fcb_path);

    // Writing examples
    example_write_fcb();
    example_cityjsonseq_to_fcb(cjseq_in, "example_from_cjseq.fcb");

    // Convert FCB back to CityJSONSeq
    example_fcb_to_cityjsonseq(fcb_path, "example_output.city.jsonl");

    std::cout << "\n==================================================" << std::endl;
    std::cout << "All examples completed!" << std::endl;
    std::cout << "==================================================" << std::endl;

    return 0;
}

/**
 * ============================================================================
 * BUILD INSTRUCTIONS
 * ============================================================================
 *
 * To compile this example, you need:
 *
 * 1. The FCB C++ library (libfcb.a) from the Rust build
 * 2. The generated C++ header (lib.rs.h)
 * 3. nlohmann/json library for JSON parsing
 *
 * Example CMakeLists.txt:
 *
 *   cmake_minimum_required(VERSION 3.15)
 *   project(fcb_examples)
 *
 *   find_package(PkgConfig REQUIRED)  # or use find_package for nlohmann_json
 *   find_package(nlohmann_json 3.2.0 REQUIRED)
 *
 *   add_executable(comprehensive_example comprehensive_example.cpp)
 *   target_include_directories(comprehensive_example PRIVATE ${CMAKE_SOURCE_DIR}/../include)
 *   target_link_libraries(comprehensive_example PRIVATE fcb nlohmann_json::nlohmann_json)
 *
 * ============================================================================
 *
 * NOTE ON HTTP/REMOTE READING:
 * ---------------------------
 * HTTP reading is currently only available in the Rust API through the
 * `http` feature flag. It is not yet exposed through the C++ bindings.
 *
 * To read FCB files from remote servers, you can:
 * 1. Use the Rust library directly
 * 2. Use the CLI tool: fcb info -i https://example.com/data.fcb
 * 3. Download the file first, then use C++ API
 *
 * ============================================================================
 */
