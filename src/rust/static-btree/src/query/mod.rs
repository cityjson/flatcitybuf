// Query module for static-btree crate
//
// This module provides a higher-level interface for working with
// static B+trees, including various index implementations and
// query capabilities.

mod memory;
mod stream;
mod types;

/// HTTP-based query implementation (requires the `http` feature)
#[cfg(feature = "http")]
mod http;

#[cfg(test)]
mod tests;

// Re-export public types and traits
pub use memory::{MemoryIndex, MemoryMultiIndex};
// Re-export KeyType and TypedQueryCondition from types
pub use stream::{StreamIndex, StreamMultiIndex};
pub use types::{KeyType, TypedQueryCondition};
pub use types::{MultiIndex, Operator, Query, SearchIndex};

/// Re-export HTTP query types when the `http` feature is enabled
#[cfg(feature = "http")]
pub use http::*;
