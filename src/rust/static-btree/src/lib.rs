use std::mem;

pub mod builder;
pub mod entry;
pub mod error;
pub mod key;
pub mod node;
pub mod tree;

pub use builder::StaticBTreeBuilder;
pub use entry::Entry;
pub use error::Error;
pub use key::Key;
pub use node::*;
pub use tree::StaticBTree;

// Add basic tests or examples here later if needed
#[cfg(test)]
mod tests {
    use crate::entry::Offset;

    use super::*;
}
