//! A static B+tree implementation optimized for read-only access.
//!
//! This crate provides an implementation of a static B+tree data structure
//! that is optimized for memory-efficient storage and fast lookups. The tree
//! uses an implicit layout that allows for efficient navigation without explicit
//! pointers between nodes.
//!
//! # Features
//!
//! - Memory-efficient storage of keys and values
//! - Configurable branching factor
//! - Fast lookups with O(log n) complexity
//! - Support for range queries
//! - Type-safe key encoding

// Core modules
pub mod entry;
pub mod errors;
pub mod key;
pub mod node;
pub mod tree;
pub mod utils;

// Re-export key types and tree implementation
pub use crate::key::{KeyEncoder, KeyType};
pub use crate::tree::{StaticBTree, StaticBTreeBuilder};

/// Interface for static B+tree index operations
pub trait StaticBTreeIndex {
    /// Execute an exact match query
    fn exact_match(&self, key: &[u8]) -> Result<Option<u64>, errors::Error>;

    /// Execute a range query
    fn range_query(&self, start: &[u8], end: &[u8]) -> Result<Vec<u64>, errors::Error>;

    /// Get encoded size of keys in this index
    fn key_size(&self) -> usize;
}

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Feature flags information
#[cfg(feature = "simd")]
pub const HAS_SIMD: bool = true;

#[cfg(not(feature = "simd"))]
pub const HAS_SIMD: bool = false;
