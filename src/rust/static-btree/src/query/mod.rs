// Query module for static-btree crate
//
// This module provides a higher-level interface for working with
// static B+trees, including various index implementations and
// query capabilities.

mod memory;
mod stream;
mod types;

// We'll implement the HTTP module later when needed
// #[cfg(feature = "http")]
// mod http;

#[cfg(test)]
mod tests;

// Re-export public types and traits
pub use memory::{MemoryIndex, MemoryMultiIndex};
pub use stream::{StreamIndex, StreamMultiIndex};
pub use types::{MultiIndex, Operator, Query, QueryCondition, SearchIndex};

// We'll re-export the HTTP types later when implemented
// #[cfg(feature = "http")]
// pub use http::{HttpIndex, HttpMultiIndex};
