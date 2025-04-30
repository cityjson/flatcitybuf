pub mod entry;
pub mod error;
pub mod key;
#[cfg(feature = "http")]
#[cfg(test)]
mod mocked_http_range_client;
pub mod payload;
pub mod query;
pub mod stree;

pub use entry::*;
pub use error::*;
pub use key::*;
pub use ordered_float::OrderedFloat as Float;
pub use payload::*;
pub use query::*;
pub use stree::*;

pub const DEFAULT_BRANCHING_FACTOR: u16 = 16;

// Add basic tests or examples here later if needed
#[cfg(test)]
mod tests {}
