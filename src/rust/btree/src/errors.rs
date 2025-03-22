use std::fmt;
use thiserror::Error;

/// Error types for B-tree operations
#[derive(Error, Debug)]
pub enum BTreeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Key error: {0}")]
    Key(#[from] KeyError),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Block not found at offset {0}")]
    BlockNotFound(u64),

    #[error("Invalid tree structure: {0}")]
    InvalidStructure(String),

    #[error("Invalid node type: expected {expected}, got {actual}")]
    InvalidNodeType {
        expected: &'static str,
        actual: String,
    },

    #[error("Alignment error: offset {0} is not aligned to block size")]
    AlignmentError(u64),

    #[error("B-tree is full")]
    TreeFull,

    #[error("Stream error: {0}")]
    Stream(String),
}

/// Error types for key operations
#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Encoding error: {0}")]
    Encoding(String),

    #[error("Decoding error: {0}")]
    Decoding(String),

    #[error("Key size error: expected {expected}, got {actual}")]
    InvalidSize { expected: usize, actual: usize },
}

pub type Result<T> = std::result::Result<T, BTreeError>;
