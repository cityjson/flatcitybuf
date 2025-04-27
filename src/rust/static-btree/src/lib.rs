pub mod entry;
pub mod error;
pub mod key;
#[cfg(feature = "http")]
mod mocked_http_range_client;
pub mod payload;
pub mod query;
pub mod stree;

pub use entry::Entry;
pub use error::Error;
pub use key::Key;
pub use payload::PayloadEntry;
pub use query::{MemoryIndex, MemoryMultiIndex};
pub use query::{MultiIndex, Operator, Query, QueryCondition, SearchIndex};
pub use query::{StreamIndex, StreamMultiIndex};
pub use stree::Stree;

// Add basic tests or examples here later if needed
#[cfg(test)]
mod tests {}
