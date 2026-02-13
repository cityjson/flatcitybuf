/**
 * @file fcb.h
 * @brief FlatCityBuf C++ API - High-level header with documentation
 *
 * This header provides the C++ interface for reading and writing FlatCityBuf
 * (FCB) files. FCB is a cloud-optimized binary format for 3D city models based
 * on CityJSON.
 *
 * @example Reading an FCB file
 * @code
 * #include "lib.rs.h"
 *
 * auto reader = fcb::fcb_reader_open("buildings.fcb");
 * auto meta = fcb::fcb_reader_metadata(*reader);
 *
 * auto iter = fcb::fcb_reader_select_all(std::move(reader));
 * while (fcb::fcb_iterator_next(*iter)) {
 *     auto feature = fcb::fcb_iterator_current(*iter);
 *     std::cout << "ID: " << std::string(feature.id) << std::endl;
 * }
 * @endcode
 *
 * @author Hidemichi Baba
 * @version 0.1.0
 * @copyright MIT License
 */

#pragma once

// Include the CXX-generated bindings
#include "lib.rs.h"

/**
 * @namespace fcb
 * @brief FlatCityBuf C++ API namespace
 *
 * All FlatCityBuf C++ bindings are contained within this namespace.
 * The API provides three main components:
 *
 * - **Reader API**: Open and iterate over features in FCB files
 * - **Iterator API**: Traverse features with optional spatial filtering
 * - **Writer API**: Create new FCB files from CityJSON data
 */

/**
 * @defgroup reader Reader API
 * @brief Functions for opening and reading FCB files
 * @{
 */

/**
 * @struct fcb::FcbMetadata
 * @brief Metadata about an FCB file
 *
 * Contains information about the FCB file structure and capabilities.
 *
 * @var fcb::FcbMetadata::version
 * FCB format version number
 *
 * @var fcb::FcbMetadata::features_count
 * Total number of CityJSON features in the file
 *
 * @var fcb::FcbMetadata::has_spatial_index
 * Whether the file contains an R-tree spatial index for bbox queries
 *
 * @var fcb::FcbMetadata::has_attribute_index
 * Whether the file contains attribute indexes for property queries
 */

/**
 * @struct fcb::BoundingBox
 * @brief 2D bounding box for spatial queries
 *
 * Defines a rectangular region for filtering features by location.
 * Coordinates should be in the same CRS as the FCB file.
 *
 * @var fcb::BoundingBox::min_x
 * Minimum X coordinate (west boundary)
 *
 * @var fcb::BoundingBox::min_y
 * Minimum Y coordinate (south boundary)
 *
 * @var fcb::BoundingBox::max_x
 * Maximum X coordinate (east boundary)
 *
 * @var fcb::BoundingBox::max_y
 * Maximum Y coordinate (north boundary)
 */

/**
 * @struct fcb::CityFeatureData
 * @brief Data for a single CityJSON feature
 *
 * Contains the feature ID and full JSON representation.
 *
 * @var fcb::CityFeatureData::id
 * CityObject identifier (e.g., "NL.IMBAG.Pand.0503100000031902")
 *
 * @var fcb::CityFeatureData::json
 * Complete CityJSONFeature as a JSON string, ready for parsing
 */

/** @} */  // end of reader group

/**
 * @defgroup iterator Iterator API
 * @brief Functions for iterating over features
 * @{
 */

/**
 * @fn bool fcb::fcb_iterator_next(FcbFileReaderIterator& iter)
 * @brief Advance to the next feature
 * @param iter The iterator to advance
 * @return true if a feature is available, false when iteration is complete
 * @throws std::exception on read errors
 *
 * @code
 * while (fcb::fcb_iterator_next(*iter)) {
 *     auto feature = fcb::fcb_iterator_current(*iter);
 *     // process feature
 * }
 * @endcode
 */

/**
 * @fn CityFeatureData fcb::fcb_iterator_current(const FcbFileReaderIterator&
 * iter)
 * @brief Get the current feature data
 * @param iter The iterator positioned on a feature
 * @return CityFeatureData containing id and json
 * @throws std::exception if called before next() or after iteration complete
 */

/**
 * @fn uint64_t fcb::fcb_iterator_features_count(const FcbFileReaderIterator&
 * iter)
 * @brief Get the total number of features
 * @param iter The iterator
 * @return Total feature count, or 0 if unknown
 */

/** @} */  // end of iterator group

/**
 * @defgroup writer Writer API
 * @brief Functions for creating FCB files
 * @{
 */

/**
 * @fn rust::Box<FcbFileWriter> fcb::fcb_writer_new(rust::Str metadata_json)
 * @brief Create a new FCB writer
 * @param metadata_json CityJSON metadata as JSON string (type, version,
 * transform, etc.)
 * @return Boxed writer instance
 * @throws std::exception on invalid JSON
 *
 * @code
 * std::string meta =
 * R"({"type":"CityJSON","version":"2.0","transform":{...}})"; auto writer =
 * fcb::fcb_writer_new(meta);
 * @endcode
 */

/**
 * @fn void fcb::fcb_writer_add_feature(FcbFileWriter& writer, rust::Str
 * feature_json)
 * @brief Add a CityJSON feature to the writer
 * @param writer The writer instance
 * @param feature_json CityJSONFeature as JSON string
 * @throws std::exception on invalid feature JSON
 */

/**
 * @fn void fcb::fcb_writer_write(rust::Box<FcbFileWriter> writer, rust::Str
 * path)
 * @brief Write the FCB file to disk
 * @param writer The writer instance (consumed)
 * @param path Output file path
 * @throws std::exception on I/O errors
 *
 * @note This consumes the writer - it cannot be used after calling write()
 */

/** @} */  // end of writer group
