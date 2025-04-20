use std::mem;

pub mod error;
pub mod key;
// Declare other modules later as they are created
pub mod builder;
pub mod entry;
pub mod stree;
pub mod tree;

pub use builder::StaticBTreeBuilder;
pub use entry::Entry;
pub use error::Error;
pub use key::Key;
pub use stree::Stree;

// Add basic tests or examples here later if needed
#[cfg(test)]
mod tests {
    use super::*;
}
