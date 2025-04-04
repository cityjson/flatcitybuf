use std::mem;

pub mod error;
pub mod key;
// Declare other modules later as they are created
pub mod builder;
pub mod entry;
pub mod tree;

pub use builder::StaticBTreeBuilder;
pub use entry::Entry;
pub use error::Error;
pub use key::Key;
pub use tree::StaticBTree;

/// The type associated with each key in the tree.
/// Currently fixed to u64, assuming byte offsets as values.
pub type Value = u64;

/// Constant for the size of the Value type in bytes.
pub const VALUE_SIZE: usize = mem::size_of::<Value>();

// Add basic tests or examples here later if needed
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        // Basic assertion to ensure tests run
        assert_eq!(VALUE_SIZE, 8);
    }
}
