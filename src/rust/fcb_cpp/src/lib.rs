//! C++ Bindings for FlatCityBuf Core Library
//!
//! This crate provides C++ bindings for fcb_core library using CXX.

mod reader;
mod writer;

use reader::{FcbFileReader, FcbFileReaderIterator};
use writer::FcbFileWriter;

#[cxx::bridge(namespace = "fcb")]
mod ffi {
    /// 3D coordinate transform (scale and translation)
    #[derive(Default)]
    struct FcbTransform {
        /// Scale factor for X axis
        scale_x: f64,
        /// Scale factor for Y axis
        scale_y: f64,
        /// Scale factor for Z axis
        scale_z: f64,
        /// Translation offset for X axis
        translate_x: f64,
        /// Translation offset for Y axis
        translate_y: f64,
        /// Translation offset for Z axis
        translate_z: f64,
    }

    /// 3D geographical extent (bounding box with elevation)
    #[derive(Default)]
    struct FcbGeographicalExtent {
        /// Minimum X coordinate
        min_x: f64,
        /// Minimum Y coordinate
        min_y: f64,
        /// Minimum Z coordinate (elevation)
        min_z: f64,
        /// Maximum X coordinate
        max_x: f64,
        /// Maximum Y coordinate
        max_y: f64,
        /// Maximum Z coordinate (elevation)
        max_z: f64,
    }

    /// Metadata about an FCB file
    struct FcbMetadata {
        /// FCB format version
        version: u8,
        /// Number of features in file
        features_count: u64,
        /// Whether the file has a spatial index
        has_spatial_index: bool,
        /// Whether the file has an attribute index
        has_attribute_index: bool,

        /// CityJSON specification version (e.g. "2.0")
        cityjson_version: String,

        /// Whether a coordinate transform is present
        has_transform: bool,
        /// Coordinate transform (scale + translation); valid only if has_transform is true
        transform: FcbTransform,

        /// Whether a geographical extent is present
        has_geographical_extent: bool,
        /// 3D geographical extent; valid only if has_geographical_extent is true
        geographical_extent: FcbGeographicalExtent,

        /// Full CityJSON header as a JSON string (type, version, transform, metadata,
        /// referenceSystem, extensions). Parse with your preferred JSON library.
        /// geometry_templates are excluded.
        metadata_json: String,
    }

    /// 2D bounding box for spatial queries
    struct BoundingBox {
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    }

    /// City feature data returned from iteration
    struct CityFeatureData {
        /// Feature ID
        id: String,
        /// Serialized CityJSONFeature as JSON string
        json: String,
    }

    extern "Rust" {
        // Opaque types
        type FcbFileReader;
        type FcbFileReaderIterator;
        type FcbFileWriter;

        // ============ Reader API ============

        /// Open an FCB file for reading
        fn fcb_reader_open(path: &str) -> Result<Box<FcbFileReader>>;

        /// Get metadata from an open reader
        fn fcb_reader_metadata(reader: &FcbFileReader) -> FcbMetadata;

        /// Select all features for iteration
        fn fcb_reader_select_all(reader: Box<FcbFileReader>) -> Result<Box<FcbFileReaderIterator>>;

        /// Select features within a bounding box
        fn fcb_reader_select_bbox(
            reader: Box<FcbFileReader>,
            bbox: BoundingBox,
        ) -> Result<Box<FcbFileReaderIterator>>;

        // ============ Iterator API ============

        /// Advance to next feature, returns false when done
        fn fcb_iterator_next(iter: &mut FcbFileReaderIterator) -> Result<bool>;

        /// Get to current feature data
        fn fcb_iterator_current(iter: &FcbFileReaderIterator) -> Result<CityFeatureData>;

        /// Get the total features count (if known)
        fn fcb_iterator_features_count(iter: &FcbFileReaderIterator) -> u64;

        // ============ Writer API ============

        /// Create a new FCB writer with CityJSON metadata
        fn fcb_writer_new(metadata_json: &str) -> Result<Box<FcbFileWriter>>;

        /// Add a feature to writer
        fn fcb_writer_add_feature(writer: &mut FcbFileWriter, feature_json: &str) -> Result<()>;

        /// Write FCB file to disk
        fn fcb_writer_write(writer: Box<FcbFileWriter>, path: &str) -> Result<()>;
    }
}

// ============================================================================
// Re-exports
// ============================================================================

// Re-export the FFI functions from submodules
pub use reader::{
    fcb_iterator_current, fcb_iterator_features_count, fcb_iterator_next, fcb_reader_metadata,
    fcb_reader_open, fcb_reader_select_all, fcb_reader_select_bbox,
};
pub use writer::{fcb_writer_add_feature, fcb_writer_new, fcb_writer_write};

// Re-export bridge types
pub use ffi::{BoundingBox, CityFeatureData, FcbGeographicalExtent, FcbMetadata, FcbTransform};
