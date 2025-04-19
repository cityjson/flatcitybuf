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
    /// padded entry count per layer (0=leaf up to height-1=root)
    layer_counts: Vec<usize>,
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
            layer_counts,
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
    /// Number of entries in the leaf layer (padded to branching factor).
    pub(crate) fn leaf_count(&self) -> usize {
        self.layer_counts[0]
    }
    /// Total index entries across all layers.
    pub(crate) fn total_entries(&self) -> usize {
        self.layer_counts.iter().sum()
    }
}

/// Represents the static B+Tree structure, providing read access.
/// `K` is the Key type, `R` is the underlying readable and seekable data source.
#[derive(Debug)]
pub struct StaticBTree<K: Key, R: Read + Seek> {
    reader: R,
    layout: Layout,
    entry_size: usize,
    /// File offset where payload region begins
    payload_start: u64,
    _phantom_key: PhantomData<K>,
}

impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
    /// Initialize a StaticBTree over serialized index + payload data.
    pub fn new(mut reader: R, branching_factor: u16, num_entries: u64) -> Result<Self, Error> {
        let b = branching_factor as usize;
        let n = num_entries as usize;
        // Build layout for index region
        let layout = Layout::new(n, b);
        // Fixed entry size
        let entry_size = Entry::<K>::SERIALIZED_SIZE;
        // Compute payload region start offset
        let total = layout.total_entries();
        let payload_start = (total * entry_size) as u64;
        Ok(StaticBTree {
            reader,
            layout,
            entry_size,
            payload_start,
            _phantom_key: PhantomData,
        })
    }

    /// Number of index layers.
    /// Number of index layers in the tree (height).
    pub fn height(&self) -> usize {
        self.layout.height
    }

    /// Number of unique keys indexed.
    /// Number of unique keys indexed in the tree.
    pub fn len(&self) -> usize {
        self.layout.num_entries
    }

    /// Read a fixed-size node of `Entry<K>`s from the index region.
    /// Read a node of `branching_factor` entries at given layer and node index.
    fn read_node(&mut self, layer: usize, node_idx: usize) -> Result<Vec<Entry<K>>, Error> {
        let b = self.layout.branching_factor;
        let abs_start = self.layout.layer_offset(layer) + node_idx * b;
        let mut entries = Vec::with_capacity(b);
        for i in 0..b {
            entries.push(self.read_index_entry(abs_start + i)?);
        }
        Ok(entries)
    }

    /// Find the absolute index entry position for the first key >= `key`.
    /// Leaf-layer index for first key >= `key`.
    pub(crate) fn lower_bound_index(&mut self, key: &K) -> Result<usize, Error> {
        let mut layer = self.layout.height - 1;
        let mut node = 0;
        // descend internal layers
        while layer > 0 {
            let entries = self.read_node(layer, node)?;
            let idx = match entries.binary_search_by(|e| e.key.cmp(key)) {
                Ok(mut i) => {
                    // find first occurrence if duplicates exist
                    while i > 0 && entries[i - 1].key == *key {
                        i -= 1;
                    }
                    i
                }
                Err(i) => i,
            };
            node = idx;
            layer -= 1;
        }
        // at leaf layer
        let entries = self.read_node(0, node)?;
        let idx = match entries.binary_search_by(|e| e.key.cmp(key)) {
            Ok(mut i) => {
                // find first matching key
                while i > 0 && entries[i - 1].key == *key {
                    i -= 1;
                }
                i
            }
            Err(i) => i,
        };
        Ok(node * self.layout.branching_factor + idx)
    }

    /// Find the absolute index entry position for the first key > `key`.
    /// Leaf-layer index for first key > `key`.
    pub(crate) fn upper_bound_index(&mut self, key: &K) -> Result<usize, Error> {
        let mut layer = self.layout.height - 1;
        let mut node = 0;
        while layer > 0 {
            let entries = self.read_node(layer, node)?;
            let idx = match entries.binary_search_by(|e| e.key.cmp(key)) {
                Ok(mut i) => {
                    // move to last matching key
                    while i + 1 < entries.len() && entries[i + 1].key == *key {
                        i += 1;
                    }
                    i + 1
                }
                Err(i) => i,
            };
            node = idx;
            layer -= 1;
        }
        let entries = self.read_node(0, node)?;
        let idx = match entries.binary_search_by(|e| e.key.cmp(key)) {
            Ok(mut i) => {
                // move to last matching key
                while i + 1 < entries.len() && entries[i + 1].key == *key {
                    i += 1;
                }
                i + 1
            }
            Err(i) => i,
        };
        Ok(node * self.layout.branching_factor + idx)
    }

    /// Read a single index entry at `entry_idx`.
    /// Read a single index entry at `entry_idx` (leaf layer coordinate).
    /// Read the index entry at the given leaf-layer position.
    pub(crate) fn read_entry(&mut self, entry_idx: usize) -> Result<Entry<K>, Error> {
        let abs = self.layout.layer_offset(0) + entry_idx;
        self.read_index_entry(abs)
    }

    /// Read a payload block chain starting at `block_ptr`.
    /// Read all record offsets for a key by following its payload block chain.
    pub(crate) fn read_all_offsets(&mut self, mut block_ptr: u64) -> Result<Vec<Offset>, Error> {
        let mut result = Vec::new();
        let b = self.layout.branching_factor;
        while block_ptr != 0 {
            // Seek to block
            self.reader.seek(SeekFrom::Start(block_ptr))?;
            // Read count (u32)
            let mut cnt_buf = [0u8; 4];
            self.reader.read_exact(&mut cnt_buf)?;
            let count = u32::from_le_bytes(cnt_buf) as usize;
            // Read next_ptr (u64)
            let mut nxt_buf = [0u8; 8];
            self.reader.read_exact(&mut nxt_buf)?;
            let next = u64::from_le_bytes(nxt_buf);
            // Read offsets array of length b, but only first `count` are valid
            for i in 0..b {
                self.reader.read_exact(&mut nxt_buf)?;
                if i < count {
                    result.push(u64::from_le_bytes(nxt_buf));
                }
            }
            block_ptr = next;
        }
        Ok(result)
    }
    /// Read an index entry by absolute position in the index region.
    fn read_index_entry(&mut self, abs_idx: usize) -> Result<Entry<K>, Error> {
        let pos = (abs_idx * self.entry_size) as u64;
        self.reader.seek(SeekFrom::Start(pos))?;
        let entry = Entry::read_from(&mut self.reader)?;
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StaticBTreeBuilder;
    use std::io::Cursor;

    #[test]
    fn test_read_entry_and_offsets() {
        // Build a small tree with branching factor 2 and duplicate values
        let mut builder = StaticBTreeBuilder::<u32>::new(2);
        builder.push(1, 10);
        builder.push(2, 20);
        builder.push(2, 21);
        builder.push(2, 22);
        builder.push(3, 30);
        let data = builder.build().expect("build should succeed");
        let cursor = Cursor::new(data);
        let mut tree = StaticBTree::<u32, _>::new(cursor, 2, 3).expect("open tree failed");
        // Leaf entries padded to 4: [1,2,3,3]
        // Test key=1 at index 0
        let e1 = tree.read_entry(0).expect("read entry 0");
        assert_eq!(e1.key, 1);
        let o1 = tree.read_all_offsets(e1.offset).expect("read offsets 1");
        assert_eq!(o1, vec![10]);
        // Test key=2 at index 1
        let e2 = tree.read_entry(1).expect("read entry 1");
        assert_eq!(e2.key, 2);
        let o2 = tree.read_all_offsets(e2.offset).expect("read offsets 2");
        assert_eq!(o2, vec![20, 21, 22]);
        // Test key=3 at index 2
        let e3 = tree.read_entry(2).expect("read entry 2");
        assert_eq!(e3.key, 3);
        let o3 = tree.read_all_offsets(e3.offset).expect("read offsets 3");
        assert_eq!(o3, vec![30]);
    }
}
