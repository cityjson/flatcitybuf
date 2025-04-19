use super::tree::Layout;
use crate::entry::{Entry, Offset};
use crate::error::Error;
use crate::key::Key;
use std::io::Write;

/// Builder for a serialized static B+Tree.
/// Collect entries in **sorted order** and call `build()` to obtain a ready‑to‑store byte vector.
///
/// Duplicate keys are allowed.
pub struct StaticBTreeBuilder<K: Key> {
    branching_factor: usize,
    // raw input pairs; duplicates allowed
    // To be grouped in build(): Vec<(K, Vec<Offset>)>
    entries: Vec<(K, Offset)>,
}

impl<K: Key> StaticBTreeBuilder<K> {
    pub fn new(branching_factor: u16) -> Self {
        // Initialize builder with target branching factor
        StaticBTreeBuilder {
            branching_factor: branching_factor as usize,
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, key: K, offset: Offset) {
        // TODO: collect (key, offset) pairs for grouping into payload blocks
        self.entries.push((key, offset));
    }

    /// Consume builder and return serialized byte vector.
    pub fn build(self) -> Result<Vec<u8>, Error> {
        // TODO: Implement builder logic
        // 1. Group self.entries by key into Vec<(K, Vec<Offset>)>
        // 2. For each group, emit chained payload blocks with capacity = branching_factor
        //    - Write u32 count, u64 next_ptr, u64 offsets[M]
        //    - Chain blocks via next_ptr
        //    - Record first block offset as block_ptr for this key
        // 3. Build index region: create Entry { key, block_ptr } per unique key
        //    - Pack leaves to multiple of branching_factor, compute internal layers top-down
        // 4. Serialize index region entries then payload blocks into Vec<u8>
        unimplemented!("StaticBTreeBuilder::build");
    }

    // fn pad_layer(...) – no longer needed; payload blocks handle duplicate distribution
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::StaticBTree;
    use std::io::Cursor;
}
