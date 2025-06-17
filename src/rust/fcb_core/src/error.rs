use crate::packed_rtree::Error as PackedRtreeError;
use cjseq::error::CjseqError;
use flatbuffers::InvalidFlatbuffer;
use serde_json;
use thiserror::Error;

/// The main error type for the FCB Core library.
/// This enum represents all possible errors that can occur during FCB operations.
#[derive(Debug, Error)]
pub enum Error {
    // File format errors
    #[error("Missing magic bytes in FCB file header")]
    MissingMagicBytes,

    #[error("Required index is missing")]
    NoIndex,

    #[error("Attribute index not found")]
    AttributeIndexNotFound,

    #[error("Attribute index size overflow")]
    AttributeIndexSizeOverflow,

    #[error("No columns found in header")]
    NoColumnsInHeader,

    #[error("Missing required field of CityJSON: {0}")]
    MissingRequiredField(String),

    #[error("Invalid header size {0}, expected size between 8 and 1MB")]
    IllegalHeaderSize(usize),

    #[error("Invalid FlatBuffer format: {0}")]
    InvalidFlatbuffer(#[from] InvalidFlatbuffer),

    // IO and serialization errors
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("R-tree error: {0}")]
    RtreeError(#[from] PackedRtreeError),

    // Validation errors
    #[error("Unsupported column type: {0}")]
    UnsupportedColumnType(String),

    #[error("Invalid attribute value: {msg}")]
    InvalidAttributeValue { msg: String },

    // Index and query errors
    #[error("Failed to create index: {0}")]
    IndexCreationError(String),

    #[error("Failed to execute query: {0}")]
    QueryExecutionError(String),

    // HTTP errors (when http feature is enabled)
    #[cfg(feature = "http")]
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] http_range_client::HttpError),

    // CityJSON specific errors
    #[error("CityJSON error: {source}")]
    CityJson {
        #[from]
        source: crate::cjerror::CjError,
    },

    #[error("Cjseq error: {source}")]
    CjseqError {
        #[from]
        source: CjseqError,
    },

    #[error("StaticBTree error: {source}")]
    StaticBTree {
        #[from]
        source: crate::static_btree::Error,
    },
}

impl Error {
    /// Returns true if the error is related to IO operations
    pub fn is_io_error(&self) -> bool {
        matches!(self, Error::IoError(_))
    }

    /// Returns true if the error is related to data format
    pub fn is_format_error(&self) -> bool {
        matches!(
            self,
            Error::MissingMagicBytes | Error::InvalidFlatbuffer(_) | Error::IllegalHeaderSize(_)
        )
    }

    /// Returns true if the error is related to validation
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            Error::UnsupportedColumnType(_) | Error::InvalidAttributeValue { .. }
        )
    }

    /// Returns true if the error is related to index or query operations
    pub fn is_index_error(&self) -> bool {
        matches!(
            self,
            Error::IndexCreationError(_) | Error::QueryExecutionError(_)
        )
    }
}

pub type Result<T> = std::result::Result<T, Error>;
