use crate::entry::{Entry, Offset};
use crate::error::{Error, Error as BTreeError};
use crate::key::Key;
use std::io::{Read, Seek, SeekFrom};
use std::marker::PhantomData;
use std::mem;

/// Helper utilities to compute layer statistics for a static B+Tree.
#[derive(Debug, Clone)]
struct Layout {
    branching_factor: usize,
    num_entries: usize,
    height: usize,
    /// starting entry index (not bytes) for each layer 0..height-1 (root=0)
    layer_offsets: Vec<usize>,
}

impl Layout {
    fn new(num_entries: usize, branching_factor: usize) -> Self {
        assert!(branching_factor >= 2, "branching factor must be >=2");
        // compute height and layer offsets top‑down (root layer = 0)
        let mut layer_offsets = Vec::new();
        let mut offset = 0usize;
        let mut n = num_entries;
        let b = branching_factor;
        loop {
            layer_offsets.push(offset);
            if n <= b {
                // leaves will be next layer; stop after pushing internal nodes later
                break;
            }
            let blocks = Self::blocks(n, b);
            offset += blocks * b; // each block has b keys (internal nodes)
            n = Self::prev_keys(n, b);
        }
        // finally, push offset for leaf layer
        if *layer_offsets.last().unwrap() != offset {
            layer_offsets.push(offset);
        } else {
            // the last push already root? ensure height at least 1
        }
        let height = layer_offsets.len();
        Layout {
            branching_factor: b,
            num_entries,
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
    #[inline]
    fn layer_offset(&self, h: usize) -> usize {
        self.layer_offsets[h]
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
    /// Create a new tree given a reader pointing at the beginning of the serialized entries.
    pub fn new(mut reader: R, branching_factor: u16, num_entries: u64) -> Result<Self, Error> {
        let layout = Layout::new(num_entries as usize, branching_factor as usize);

        // quick sanity check: seek to end to see size? optional.
        let entry_size = K::SERIALIZED_SIZE + mem::size_of::<Offset>();

        // Ensure reader is at start
        reader.seek(SeekFrom::Start(0))?;

        Ok(Self {
            reader,
            layout,
            entry_size,
            _phantom_key: PhantomData,
        })
    }

    /// Height of the tree (number of layers). 1 means leaf‑only (root==leaf).
    pub fn height(&self) -> usize {
        self.layout.height
    }

    pub fn len(&self) -> usize {
        self.layout.num_entries
    }

    /// Read a node (array of `B` entries) for given layer `h` and node index `k`.
    fn read_node(&mut self, layer: usize, node_idx: usize) -> Result<Vec<Entry<K>>, Error> {
        let b = self.layout.branching_factor;
        let entry_start_idx = self.layout.layer_offset(layer) + node_idx * b;
        let byte_offset = (entry_start_idx * self.entry_size) as u64;
        self.reader.seek(SeekFrom::Start(byte_offset))?;

        let mut entries = Vec::with_capacity(b);
        for _ in 0..b {
            let e = Entry::<K>::read_from(&mut self.reader)?;
            entries.push(e);
        }
        Ok(entries)
    }

    /// Internal search that returns the *absolute entry index* of lower bound.
    fn lower_bound_index(&mut self, key: &K) -> Result<usize, Error> {
        let b = self.layout.branching_factor;
        let mut node_idx = 0usize; // multiplied by b implicitly per formula
                                   // iterate internal layers (root .. height‑2) if height>1
        for h in 0..(self.layout.height - 1) {
            let node = self.read_node(h, node_idx)?;
            // linear scan
            let mut pos = 0;
            while pos < b && &node[pos].key < key {
                pos += 1;
            }
            node_idx = node_idx * (b + 1) + pos;
        }
        // leaf layer calculations
        let leaf_layer = self.layout.height - 1;
        let leaf_node_start_idx = node_idx * b;
        let leaf_entries = self.read_node(leaf_layer, node_idx)?;
        let mut pos = 0;
        while pos < b && &leaf_entries[pos].key < key {
            pos += 1;
        }
        Ok(self.layout.layer_offset(leaf_layer) + leaf_node_start_idx + pos)
    }

    /// like lower_bound_index but first key > query (upper bound)
    fn upper_bound_index(&mut self, key: &K) -> Result<usize, Error> {
        let b = self.layout.branching_factor;
        let mut node_idx = 0usize;
        for h in 0..(self.layout.height - 1) {
            let node = self.read_node(h, node_idx)?;
            let mut pos = 0;
            while pos < b && &node[pos].key <= key {
                pos += 1;
            }
            node_idx = node_idx * (b + 1) + pos;
        }
        let leaf_layer = self.layout.height - 1;
        let leaf_node_start_idx = node_idx * b;
        let leaf_entries = self.read_node(leaf_layer, node_idx)?;
        let mut pos = 0;
        while pos < b && &leaf_entries[pos].key <= key {
            pos += 1;
        }
        Ok(self.layout.layer_offset(leaf_layer) + leaf_node_start_idx + pos)
    }

    /// Return offsets whose key equals `search_key`.
    /// If the key is **not present**, returns a single‑element vec containing the offset of the
    /// first key greater than `search_key` (the classic lower‑bound semantics).
    pub fn lower_bound(&mut self, search_key: &K) -> Result<Vec<Offset>, Error> {
        let start_idx = self.lower_bound_index(search_key)?;
        let entry = self.read_entry(start_idx)?;

        if &entry.key != search_key {
            // key not present, return offset of first key > search_key
            return Ok(vec![entry.offset]);
        }

        // gather duplicates to right until key!=search_key
        let mut result = Vec::new();
        let mut idx = start_idx;
        let total = self.len();
        let b_key_equal = |e: &Entry<K>| &e.key == search_key;
        while idx < total {
            let ent = self.read_entry(idx)?;
            if b_key_equal(&ent) {
                result.push(ent.offset);
                idx += 1;
            } else {
                break;
            }
        }
        // also gather duplicates to the left
        let mut left_idx = if start_idx == 0 { 0 } else { start_idx - 1 };
        while left_idx < start_idx {
            let ent = self.read_entry(left_idx)?;
            if b_key_equal(&ent) {
                result.insert(0, ent.offset);
                if left_idx == 0 {
                    break;
                }
                left_idx -= 1;
            } else {
                break;
            }
        }
        Ok(result)
    }

    /// Range query inclusive [min, max]
    pub fn range(&mut self, min_key: &K, max_key: &K) -> Result<Vec<Offset>, Error> {
        if min_key > max_key {
            return Err(Error::QueryError("min_key > max_key".into()));
        }
        let start_idx = self.lower_bound_index(min_key)?;
        let end_idx = self.upper_bound_index(max_key)?;
        let mut offsets = Vec::new();
        for idx in start_idx..end_idx {
            let entry = self.read_entry(idx)?;
            offsets.push(entry.offset);
        }
        Ok(offsets)
    }

    /// helper to read a single entry by absolute index
    fn read_entry(&mut self, entry_idx: usize) -> Result<Entry<K>, Error> {
        let byte_offset = (entry_idx * self.entry_size) as u64;
        self.reader.seek(SeekFrom::Start(byte_offset))?;
        Entry::<K>::read_from(&mut self.reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;

    type TestKey = i32;

    fn build_simple_tree(entries: &[(i32, Offset)], b: usize) -> Vec<u8> {
        // naive builder: assume entries.len() == b, so root==leaf
        let mut buf = Vec::new();
        for (k, off) in entries {
            let e = Entry {
                key: *k,
                offset: *off,
            };
            e.write_to(&mut buf).unwrap();
        }
        buf
    }

    #[test]
    fn test_lb_single_leaf() {
        let data = vec![(10, 100), (20, 200), (30, 300), (40, 400)];
        let serialized = build_simple_tree(&data, 4);
        let cursor = std::io::Cursor::new(serialized);
        let mut tree: StaticBTree<i32, _> =
            StaticBTree::new(cursor, 4, data.len() as u64).expect("init");

        let res = tree.lower_bound(&20).unwrap();
        assert_eq!(res, vec![200]);
        let res_none = tree.lower_bound(&25).unwrap();
        assert_eq!(res_none, vec![300]);
    }

    #[test]
    fn test_duplicates_and_range() {
        // branching factor 4
        let data = vec![
            (10, 100),
            (15, 150),
            (20, 201),
            (20, 202),
            (25, 250),
            (30, 300),
            (35, 350),
            (40, 400),
        ];
        let serialized = build_simple_tree(&data, 8); // leaf only, b=8 (root==leaf)
        let cursor = std::io::Cursor::new(serialized);
        let mut tree: StaticBTree<i32, _> =
            StaticBTree::new(cursor, 8, data.len() as u64).expect("init");

        // duplicate lookup
        let dup = tree.lower_bound(&20).unwrap();
        assert_eq!(dup, vec![201, 202]);

        // non‑existing lower bound returns first greater key
        let lb = tree.lower_bound(&18).unwrap();
        assert_eq!(lb, vec![201]);

        // range query [15, 30]
        let mut range = tree.range(&15, &30).unwrap();
        range.sort();
        let expected = vec![150, 201, 202, 250, 300];
        assert_eq!(range, expected);

        // range query [22, 30]. lower bound doesn't exist, so it returns all keys greater than 22 and less than 30
        let mut range = tree.range(&22, &30).unwrap();
        range.sort();
        let expected = vec![250, 300];
        assert_eq!(range, expected);

        // range query [20, 32]. lower bound exists but upper bound doesn't, so it returns all keys greater than 20 and less than 32
        let mut range = tree.range(&20, &32).unwrap();
        range.sort();
        let expected = vec![201, 202, 250, 300];
        assert_eq!(range, expected);

        // range query [20, 20]. When min and max are the same, it returns all keys equal to the min
        let mut range = tree.range(&20, &20).unwrap();
        range.sort();
        let expected = vec![201, 202];
        assert_eq!(range, expected);

        // range query [-100, 100]. When min is less than all keys and max is greater than all keys, it returns all keys
        let mut range = tree.range(&-100, &100).unwrap();
        range.sort();
        let expected = vec![100, 150, 201, 202, 250, 300, 350, 400];
        assert_eq!(range, expected);

        // range query [50, 60]. When min is greater than all keys it shouldn't return any keys
        let mut range = tree.range(&50, &60).unwrap();
        range.sort();
        let expected = vec![];
        assert_eq!(range, expected);

        // range query [32, 38]. When max is less than all keys it shouldn't return any keys
        let mut range = tree.range(&0, &5).unwrap();
        range.sort();
        let expected = vec![];
        assert_eq!(range, expected);

        // range query [38, 32]. When min is greater than max, it should return an error
        let res = tree.range(&38, &32);
        assert!(res.is_err());
    }
}
