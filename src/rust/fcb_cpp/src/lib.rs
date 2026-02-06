//! C++ Bindings for FlatCityBuf Core Library
//!
//! This crate provides C++ bindings for the fcb_core library using CXX.

mod reader;
mod writer;

use reader::{FcbFileReader, FcbFileReaderIterator};
use writer::FcbFileWriter;

#[cxx::bridge(namespace = "fcb")]
mod ffi {
    /// Metadata about an FCB file
    struct FcbMetadata {
        /// FCB format version
        version: u8,
        /// Number of features in the file
        features_count: u64,
        /// Whether the file has a spatial index
        has_spatial_index: bool,
        /// Whether the file has an attribute index
        has_attribute_index: bool,
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

        /// Advance to the next feature, returns false when done
        fn fcb_iterator_next(iter: &mut FcbFileReaderIterator) -> Result<bool>;

        /// Get the current feature data
        fn fcb_iterator_current(iter: &FcbFileReaderIterator) -> Result<CityFeatureData>;

        /// Get the total features count (if known)
        fn fcb_iterator_features_count(iter: &FcbFileReaderIterator) -> u64;

        // ============ Writer API ============

        /// Create a new FCB writer with CityJSON metadata
        fn fcb_writer_new(metadata_json: &str) -> Result<Box<FcbFileWriter>>;

        /// Add a feature to the writer
        fn fcb_writer_add_feature(writer: &mut FcbFileWriter, feature_json: &str) -> Result<()>;

        /// Write the FCB file to disk
        fn fcb_writer_write(writer: Box<FcbFileWriter>, path: &str) -> Result<()>;
    }
}

// Re-export the FFI functions from submodules
pub use reader::{
    fcb_iterator_current, fcb_iterator_features_count, fcb_iterator_next, fcb_reader_metadata,
    fcb_reader_open, fcb_reader_select_all, fcb_reader_select_bbox,
};
pub use writer::{fcb_writer_add_feature, fcb_writer_new, fcb_writer_write};

// Re-export bridge types
pub use ffi::{BoundingBox, CityFeatureData, FcbMetadata};
