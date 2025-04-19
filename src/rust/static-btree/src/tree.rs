use crate::entry::{Entry, Offset};
use crate::error::{Error, Error as BTreeError};
use crate::key::Key;
use std::io::{Read, Seek, SeekFrom};
use std::marker::PhantomData;
use std::mem;

/// Helper utilities to compute layer statistics for a static B+Tree.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    branching_factor: usize,
    num_entries: usize,
    height: usize,
    /// starting entry index for each layer (0=leaf, height-1=root) in the serialized entries array
    layer_offsets: Vec<usize>,
}

impl Layout {
    /// Create a layout describing the static B+Tree layers.
    /// Layers are numbered from bottom (leaf = 0) up to root (height - 1).
    /// `layer_offsets[h]` gives the starting entry index for layer `h` in the serialized array.
    pub(crate) fn new(num_entries: usize, branching_factor: usize) -> Self {
        assert!(branching_factor >= 2, "branching factor must be >=2");
        let b = branching_factor;
        let n = num_entries;
        // Compute entry counts per layer (bottom-up), padding each to a multiple of b
        let mut layer_counts: Vec<usize> = Vec::new();
        // Leaf layer: pad total entries to multiple of b
        let mut count = if n == 0 { 0 } else { ((n + b - 1) / b) * b };
        layer_counts.push(count);
        // Build internal layers until a layer fits in one node
        while count > b {
            // number of child nodes in the previous layer
            let raw = (count + b - 1) / b;
            // pad to multiple of b for node-aligned storage
            count = ((raw + b - 1) / b) * b;
            layer_counts.push(count);
        }
        let height = layer_counts.len();
        // Compute starting offsets per layer: sum of counts of all higher layers
        let mut layer_offsets = Vec::with_capacity(height);
        for h in 0..height {
            let mut offset = 0usize;
            // layers > h are above (closer to root)
            for j in (h + 1)..height {
                offset += layer_counts[j];
            }
            layer_offsets.push(offset);
        }
        Layout {
            branching_factor: b,
            num_entries: n,
            height,
            layer_offsets,
        }
    }

    #[inline]
    fn blocks(n: usize, b: usize) -> usize {
        (n + b - 1) / b
    }
    #[inline]
    fn prev_keys(n: usize, b: usize) -> usize {
        // (blocks(n) + b) / (b+1) * b
        let blocks = Self::blocks(n, b);
        ((blocks + b) / (b + 1)) * b
    }
    /// start index (in entries) of layer h (0‑based). h==height‑1 is leaf layer
    pub(crate) fn layer_offset(&self, h: usize) -> usize {
        self.layer_offsets[h]
    }

    pub(crate) fn last_entry_index(&self) -> usize {
        *self.layer_offsets.last().unwrap()
    }
}

/// Represents the static B+Tree structure, providing read access.
/// `K` is the Key type, `R` is the underlying readable and seekable data source.
#[derive(Debug)]
pub struct StaticBTree<K: Key, R: Read + Seek> {
    reader: R,
    layout: Layout,
    entry_size: usize,
    _phantom_key: PhantomData<K>,
}

impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
    /// Initialize a StaticBTree over serialized index + payload data.
    pub fn new(reader: R, branching_factor: u16, num_entries: u64) -> Result<Self, Error> {
        // TODO:
        // 1. Build Layout for index region (unique keys count = num_entries)
        // 2. Determine entry_size and payload_region_start offset
        unimplemented!("StaticBTree::new");
    }

    /// Number of index layers.
    pub fn height(&self) -> usize {
        // TODO: return number of index layers based on Layout
        unimplemented!("StaticBTree::height");
    }

    /// Number of unique keys indexed.
    pub fn len(&self) -> usize {
        // TODO: return total unique key count
        unimplemented!("StaticBTree::len");
    }

    /// Read a fixed-size node of `Entry<K>`s from the index region.
    fn read_node(&mut self, layer: usize, node_idx: usize) -> Result<Vec<Entry<K>>, Error> {
        // TODO: seek and read index entries for layer and node_idx
        unimplemented!("StaticBTree::read_node");
    }

    /// Find the absolute index entry position for the first key >= `key`.
    pub(crate) fn lower_bound_index(&mut self, key: &K) -> Result<usize, Error> {
        // TODO: traverse internal layers top-down (Eytzinger) to leaf index
        unimplemented!("StaticBTree::lower_bound_index");
    }

    /// Find the absolute index entry position for the first key > `key`.
    pub(crate) fn upper_bound_index(&mut self, key: &K) -> Result<usize, Error> {
        // TODO: similar to lower_bound_index but with strict > comparison
        unimplemented!("StaticBTree::upper_bound_index");
    }

    /// Read a single index entry at `entry_idx`.
    pub(crate) fn read_entry(&mut self, entry_idx: usize) -> Result<Entry<K>, Error> {
        // TODO: seek and deserialize Entry<K> (key, block_ptr)
        unimplemented!("StaticBTree::read_entry");
    }

    /// Read a payload block chain starting at `block_ptr`.
    pub(crate) fn read_all_offsets(&mut self, block_ptr: u64) -> Result<Vec<Offset>, Error> {
        // TODO:
        // 1. Seek to block_ptr; read u32 count, u64 next_ptr, u64 offsets[M]
        // 2. Append offsets; if next_ptr != 0, repeat
        unimplemented!("StaticBTree::read_all_offsets");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;
}
