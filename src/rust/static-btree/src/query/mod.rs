// Query module for static-btree crate
//
// This module provides a higher-level interface for working with
// static B+trees, including various index implementations and
// query capabilities.

mod memory;
mod stream;
mod types;

#[cfg(feature = "http")]
mod http;

#[cfg(test)]
mod tests;

pub use memory::*;
pub use stream::*;
pub use types::{MultiIndex, Operator, Query, QueryCondition, SearchIndex};

#[cfg(feature = "http")]
pub use http::*;
