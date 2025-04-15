use crate::entry::{Entry, Offset};
use crate::error::Error;
use crate::key::Key;
use std::io::{Seek, Write};
use std::marker::PhantomData;
use std::mem;

/// Builder structure for creating a StaticBTree file/data structure.
/// Writes to a `Write + Seek` target using a bottom-up approach.
/// Buffers nodes in memory and writes them top-down at the end.
/// Header isn't needed as the metadata will be given in the constructor such as branching factor, number of entries.
pub struct StaticBTreeBuilder<K: Key, W: Write + Seek> {
    /// The output target. Must be seekable to write the header at the end.
    writer: W,
    /// The chosen branching factor for the tree (number of keys/entries per node).
    branching_factor: u16,
    num_entries: u64,
    /// The size of the key in bytes.
    key_size: usize,
    /// The size of the offset in bytes. This will be u64.
    offset_size: usize,
    /// The size of the entry in bytes.
    entry_size: usize,

    _phantom_key: PhantomData<K>,
}

impl<K: Key, W: Write + Seek> StaticBTreeBuilder<K, W> {
    /// Creates a new builder targeting the given writer.
    /// Reserves space for the header but defers node writing until finalization.
    pub fn new(writer: W, branching_factor: u16) -> Result<Self, Error> {
        if branching_factor <= 1 {
            return Err(Error::BuildError(format!(
                "branching factor must be greater than 1, got {}",
                branching_factor
            )));
        }

        let key_size = K::SERIALIZED_SIZE;
        let value_size = mem::size_of::<Offset>();
        let entry_size = key_size + value_size;

        Ok(StaticBTreeBuilder {
            writer,
            branching_factor,
            num_entries: 0,
            key_size,
            offset_size: value_size,
            entry_size,
            _phantom_key: PhantomData,
        })
    }

    /// Builds the entire tree from an iterator providing pre-sorted entries.
    pub fn build_from_sorted<I>(mut self, sorted_entries: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = Result<Entry<K>, Error>>,
    {
        // TODO: Implement
        // build the tree from leaf nodes to the root node. The neccessary information can be defived by using `blocks` and `prev_keys`, and `height` functions.

        // Here is the example of the tree construction in C++:
        //=================
        // memcpy(btree, a, 4 * N);
        // for (int i = N; i < S; i++)
        //     btree[i] = INT_MAX;

        // for (int h = 1; h < H; h++) {
        //     for (int i = 0; i < offset(h + 1) - offset(h); i++) {
        //         // i = k * B + j
        //         int k = i / B,
        //             j = i - k * B;
        //         k = k * (B + 1) + j + 1; // compare to the right of the key
        //         // and then always to the left
        //         for (int l = 0; l < h - 1; l++)
        //             k *= (B + 1);
        //         // pad the rest with infinities if the key doesn't exist
        //         btree[offset(h) + i] = (k * B < N ? btree[k * B] : INT_MAX);
        //     }
        // }

        // for (int i = offset(1); i < S; i += B)
        // permute(btree + i);
        //=================

        // write built tree to the writer. Reverse then so it's top-down. But in the node, it still keep the order of left-to-right of entries.
        Ok(())
    }

    // number of B-element blocks in a layer with n keys
    fn blocks(&self, n: u64) -> u64 {
        n.div_ceil(self.branching_factor as u64)
    }

    // number of keys on the layer previous to one with n keys
    fn prev_keys(&self, n: u64) -> u64 {
        self.blocks(n).div_ceil(self.branching_factor as u64 + 1) * self.branching_factor as u64
    }

    // height of a balanced n-key B+ tree
    fn height(&self, n: u64) -> u64 {
        if n <= self.branching_factor as u64 {
            1
        } else {
            self.height(self.prev_keys(n)) + 1
        }
    }

    // where the layer h starts (layer 0 is the largest)
    fn offset(&self, level: u64) -> u64 {
        let mut level = level;
        let mut k = 0;
        let mut n = self.num_entries;
        while level > 0 {
            k += self.blocks(n) * self.branching_factor as u64;
            n = self.prev_keys(n);
            level -= 1;
        }
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test1: test with a simple example such as u64 key and 3 items.

    // test2: test with more entries such as 20 with branching factor 3.

    // test with other types such as f32 and string
}
