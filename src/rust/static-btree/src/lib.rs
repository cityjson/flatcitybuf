use std::mem;

pub mod builder;
pub mod entry;
pub mod error;
pub mod key;
pub mod query;
pub mod tree;

pub use builder::StaticBTreeBuilder;
pub use entry::Entry;
pub use error::Error;
pub use key::Key;
pub use tree::StaticBTree;

pub use query::Comparison;

// Add basic tests or examples here later if needed
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        // Basic assertion to ensure tests run
    }
}
