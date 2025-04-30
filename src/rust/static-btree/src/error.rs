#[cfg(feature = "http")]
use http_range_client::HttpError;
use std::io;
use thiserror::Error; // Import the Error derive macro

/// Custom error type for StaticBTree operations.
#[derive(Error, Debug)] // Use thiserror::Error derive
pub enum Error {
    /// Errors originating from the underlying Read/Seek/Write operations.
    #[error("io error: {0}")]
    IoError(#[from] io::Error), // Automatically implements From<io::Error>

    /// Errors indicating the data format is incorrect (e.g., bad magic bytes, wrong version).
    #[error("invalid format: {0}")]
    InvalidFormat(String),

    /// Errors during the serialization of a key.
    #[error("key serialization error: {0}")]
    KeySerializationError(String),

    /// Errors during the deserialization of a key.
    #[error("key deserialization error: {0}")]
    KeyDeserializationError(String),

    /// Errors specific to the tree building process (e.g., unsorted input).
    #[error("build error: {0}")]
    BuildError(String),

    /// Errors specific to querying (e.g., trying to access invalid node index).
    #[error("query error: {0}")]
    QueryError(String),

    /// Used when an operation requires a feature not yet implemented.
    #[error("not implemented: {0}")]
    NotImplemented(String),

    /// Used when an operation fails due to an unexpected condition.
    #[error("other error: {0}")]
    Other(String),

    /// Used when an operation fails due to an HTTP error.
    #[cfg(feature = "http")]
    #[error("http error: {0}")]
    HttpError(#[from] HttpError),

    /// Requested payload offset is not in the cache
    #[error("payload offset not in cache")]
    PayloadOffsetNotInCache,
}

pub type Result<T> = std::result::Result<T, Error>;
