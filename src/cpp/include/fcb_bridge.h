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
 * @struct fcb::FcbTransform
 * @brief 3D coordinate transform (scale and translation)
 *
 * Stores the CityJSON coordinate transform that converts integer vertex
 * coordinates to real-world coordinates:
 *   real = integer * scale + translate
 *
 * @var fcb::FcbTransform::scale_x
 * Scale factor for X axis
 * @var fcb::FcbTransform::scale_y
 * Scale factor for Y axis
 * @var fcb::FcbTransform::scale_z
 * Scale factor for Z axis
 * @var fcb::FcbTransform::translate_x
 * Translation offset for X axis
 * @var fcb::FcbTransform::translate_y
 * Translation offset for Y axis
 * @var fcb::FcbTransform::translate_z
 * Translation offset for Z axis
 */

/**
 * @struct fcb::FcbGeographicalExtent
 * @brief 3D geographical bounding box
 *
 * The geographical extent of all features in the file, in the file's
 * coordinate reference system. Includes the Z (elevation) dimension.
 *
 * @var fcb::FcbGeographicalExtent::min_x
 * Minimum X coordinate (west boundary)
 * @var fcb::FcbGeographicalExtent::min_y
 * Minimum Y coordinate (south boundary)
 * @var fcb::FcbGeographicalExtent::min_z
 * Minimum Z coordinate (lowest elevation)
 * @var fcb::FcbGeographicalExtent::max_x
 * Maximum X coordinate (east boundary)
 * @var fcb::FcbGeographicalExtent::max_y
 * Maximum Y coordinate (north boundary)
 * @var fcb::FcbGeographicalExtent::max_z
 * Maximum Z coordinate (highest elevation)
 */

/**
 * @struct fcb::FcbMetadata
 * @brief Metadata about an FCB file and its CityJSON content
 *
 * Contains both FCB format information and CityJSON metadata fields.
 * Check @c has_transform and @c has_geographical_extent before accessing
 * the corresponding typed fields. All CityJSON metadata is also available
 * in @c metadata_json as a complete JSON string.
 *
 * @var fcb::FcbMetadata::version
 * FCB binary format version number
 *
 * @var fcb::FcbMetadata::features_count
 * Total number of CityJSON features in the file
 *
 * @var fcb::FcbMetadata::has_spatial_index
 * Whether the file contains an R-tree spatial index for bbox queries
 *
 * @var fcb::FcbMetadata::has_attribute_index
 * Whether the file contains attribute indexes for property queries
 *
 * @var fcb::FcbMetadata::cityjson_version
 * CityJSON specification version (e.g. "2.0")
 *
 * @var fcb::FcbMetadata::has_transform
 * Whether a coordinate transform is stored in the file
 *
 * @var fcb::FcbMetadata::transform
 * Coordinate transform (scale + translation); only valid if @c has_transform is true
 *
 * @var fcb::FcbMetadata::has_geographical_extent
 * Whether a geographical extent is stored in the file
 *
 * @var fcb::FcbMetadata::geographical_extent
 * 3D geographical extent; only valid if @c has_geographical_extent is true
 *
 * @var fcb::FcbMetadata::metadata_json
 * Full CityJSON header as a JSON string. Contains: type, version, transform,
 * metadata (identifier, title, referenceDate, referenceSystem, geographicalExtent,
 * pointOfContact), extensions. geometry_templates are excluded.
 * Parse with nlohmann::json or any JSON library.
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
