// Error types for the static-btree crate
//
// This module defines the error types used throughout the static-btree crate.
// We use thiserror for defining structured error types.

use thiserror::Error;

/// Main error type for static B+tree operations
#[derive(Error, Debug)]
pub enum Error {
    /// Key-related errors
    #[error("key error: {0}")]
    Key(#[from] KeyError),

    /// Node-related errors
    #[error("node error: {0}")]
    Node(#[from] NodeError),

    /// Tree structure errors
    #[error("tree error: {0}")]
    Tree(#[from] TreeError),

    /// HTTP errors
    #[error("http error: {0}")]
    Http(#[from] HttpError),

    /// I/O errors
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Other errors
    #[error("{0}")]
    Other(String),
}

/// Key-related errors
#[derive(Error, Debug)]
pub enum KeyError {
    /// Encoding error
    #[error("encoding error: {0}")]
    Encoding(String),

    /// Decoding error
    #[error("decoding error: {0}")]
    Decoding(String),

    /// Invalid key size
    #[error("invalid key size: expected {expected}, got {actual}")]
    InvalidSize { expected: usize, actual: usize },

    /// Invalid key type
    #[error("invalid key type: {0}")]
    InvalidType(String),
}

/// Node-related errors
#[derive(Error, Debug)]
pub enum NodeError {
    /// Invalid node type
    #[error("invalid node type: expected {expected}, got {actual}")]
    InvalidType { expected: String, actual: String },

    /// Node overflow
    #[error("node overflow: tried to add entry to full node")]
    Overflow,

    /// Node underflow
    #[error("node underflow: node has too few entries")]
    Underflow,

    /// Serialization error
    #[error("node serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("node deserialization error: {0}")]
    Deserialization(String),

    /// Invalid node index
    #[error("invalid node index: {0}")]
    InvalidIndex(usize),
}

/// Tree structure errors
#[derive(Error, Debug)]
pub enum TreeError {
    /// Invalid tree structure
    #[error("invalid tree structure: {0}")]
    InvalidStructure(String),

    /// Tree is empty
    #[error("tree is empty")]
    EmptyTree,

    /// Invalid branching factor
    #[error("invalid branching factor: {0} (must be at least 2)")]
    InvalidBranchingFactor(usize),

    /// Key not found
    #[error("key not found")]
    KeyNotFound,

    /// Builder error
    #[error("tree builder error: {0}")]
    Builder(String),
}

/// Storage-related errors
#[derive(Error, Debug)]
pub enum StorageError {
    /// Block not found
    #[error("block not found at offset {0}")]
    BlockNotFound(u64),

    /// Invalid offset
    #[error("invalid offset: {0}")]
    InvalidOffset(u64),

    /// Alignment error
    #[error("alignment error: offset {0} is not aligned to block size")]
    AlignmentError(u64),

    /// Write error
    #[error("write error: {0}")]
    Write(String),

    /// Read error
    #[error("read error: {0}")]
    Read(String),

    /// Block size error
    #[error("invalid block size: {0}")]
    InvalidBlockSize(usize),
}

/// HTTP-related errors
#[derive(Error, Debug)]
pub enum HttpError {
    /// Network error
    #[error("network error: {0}")]
    Network(String),

    /// HTTP status error
    #[error("HTTP status error: {0}")]
    Status(String),

    /// URL error
    #[error("URL error: {0}")]
    Url(String),

    /// Request error
    #[error("request error: {0}")]
    Request(String),

    /// Response error
    #[error("response error: {0}")]
    Response(String),
}

/// Result type for static B+tree operations
pub type Result<T> = std::result::Result<T, Error>;
