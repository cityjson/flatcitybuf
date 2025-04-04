use crate::builder::{FORMAT_VERSION, MAGIC_BYTES}; // Use constants from builder
use crate::entry::Entry;
use crate::error::Error;
use crate::key::Key;
use crate::Value;
use std::io::{Cursor, Read, Seek, SeekFrom}; // Added Cursor
use std::marker::PhantomData;
use std::mem;

/// Represents the static B+Tree structure, providing read access.
/// `K` is the Key type, `R` is the underlying readable and seekable data source.
#[derive(Debug)]
pub struct StaticBTree<K: Key, R: Read + Seek> {
    /// The underlying data source (e.g., file, memory buffer).
    reader: R,
    /// The branching factor B (number of keys/entries per node). Fixed at creation.
    branching_factor: u16,
    /// Total number of key-value entries stored in the tree.
    num_entries: u64,
    /// Height of the tree. 0 for empty, 1 for root-only leaf, etc.
    height: u8,
    /// The size of the header section in bytes at the beginning of the data source.
    header_size: u64, // Should match builder's reservation
    // --- Pre-calculated sizes for efficiency ---
    /// Cached size of the key type.
    key_size: usize,
    /// Cached size of the value type.
    value_size: usize,
    /// Cached size of an entry.
    entry_size: usize,
    /// Cached byte size of a fully packed internal node.
    internal_node_byte_size: usize,
    /// Cached byte size of a fully packed leaf node.
    leaf_node_byte_size: usize,
    // --- Layout information derived from header/calculation ---
    /// Stores the number of nodes present at each level, from root (index 0) to leaves.
    num_nodes_per_level: Vec<u64>,
    /// Stores the absolute byte offset (from the start of the reader) where each level begins.
    /// Level 0 (Root) starts immediately after the header.
    level_start_offsets: Vec<u64>,
    /// Marker for the generic Key type.
    _phantom_key: PhantomData<K>,
}

impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
    /// Opens an existing StaticBTree from a reader.
    /// Reads the header to validate format and calculate layout parameters.
    pub fn open(mut reader: R) -> Result<Self, Error> {
        // --- Read Header ---
        reader.seek(SeekFrom::Start(0))?;

        // 1. Read and Validate Magic Bytes & Version
        let mut magic_buf = [0u8; MAGIC_BYTES.len()];
        reader.read_exact(&mut magic_buf)?;
        if magic_buf != *MAGIC_BYTES {
            return Err(Error::InvalidFormat("invalid magic bytes".to_string()));
        }

        let mut u16_buf = [0u8; 2];
        reader.read_exact(&mut u16_buf)?;
        let version = u16::from_le_bytes(u16_buf);
        if version != FORMAT_VERSION {
            return Err(Error::InvalidFormat(format!(
                "unsupported format version: expected {}, got {}",
                FORMAT_VERSION, version
            )));
        }

        // 2. Read Core Metadata
        reader.read_exact(&mut u16_buf)?;
        let branching_factor = u16::from_le_bytes(u16_buf);
        if branching_factor <= 1 {
            return Err(Error::InvalidFormat(format!(
                "invalid branching factor in header: {}",
                branching_factor
            )));
        }

        let mut u64_buf = [0u8; 8];
        reader.read_exact(&mut u64_buf)?;
        let num_entries = u64::from_le_bytes(u64_buf);

        let mut u8_buf = [0u8; 1];
        reader.read_exact(&mut u8_buf)?;
        let height = u8::from_le_bytes(u8_buf);

        // 3. Calculate Sizes
        let key_size = K::SERIALIZED_SIZE;
        let value_size = mem::size_of::<Value>();
        let entry_size = Entry::<K>::SERIALIZED_SIZE;
        let internal_node_byte_size = branching_factor as usize * key_size;
        let leaf_node_byte_size = branching_factor as usize * entry_size;

        // Assume header size is fixed for now, matching builder
        let header_size = crate::builder::DEFAULT_HEADER_RESERVATION; // Use constant from builder

        // --- Calculate Eytzinger Layout ---
        let (num_nodes_per_level, level_start_offsets) = Self::calculate_layout(
            height,
            num_entries,
            branching_factor,
            header_size,
            internal_node_byte_size,
            leaf_node_byte_size,
        )?;

        // Construct Self
        Ok(StaticBTree {
            reader,
            branching_factor,
            num_entries,
            height,
            header_size,
            key_size,
            value_size,
            entry_size,
            internal_node_byte_size,
            leaf_node_byte_size,
            num_nodes_per_level,
            level_start_offsets,
            _phantom_key: PhantomData,
        })
    }

    /// Calculates the number of nodes per level and their start offsets.
    fn calculate_layout(
        height: u8,
        num_entries: u64,
        branching_factor: u16,
        header_size: u64,
        internal_node_byte_size: usize,
        leaf_node_byte_size: usize,
    ) -> Result<(Vec<u64>, Vec<u64>), Error> {
        if height == 0 {
            if num_entries == 0 {
                return Ok((Vec::new(), Vec::new())); // Empty tree
            } else {
                return Err(Error::InvalidFormat(
                    "height is 0 but num_entries is non-zero".to_string(),
                ));
            }
        }

        let mut num_nodes_per_level = Vec::with_capacity(height as usize);
        let mut level_start_offsets = Vec::with_capacity(height as usize);
        let b = branching_factor as u64;

        // --- Calculate node counts bottom-up ---
        let num_leaf_nodes = if num_entries == 0 {
            0
        } else {
            (num_entries + b - 1) / b
        };
        num_nodes_per_level.push(num_leaf_nodes);

        let mut num_children_in_level_below = num_leaf_nodes; // Start with leaf node count
        for _level in (0..height - 1).rev() {
            if num_children_in_level_below == 0 {
                return Err(Error::InvalidFormat(format!(
                    "invalid tree structure: level {} has 0 nodes but height is {}",
                    _level + 1,
                    height
                )));
            }
            // Number of nodes in current level = ceil(children_in_level_below / B)
            let num_nodes_current_level = (num_children_in_level_below + b - 1) / b;
            num_nodes_per_level.push(num_nodes_current_level);
            num_children_in_level_below = num_nodes_current_level; // Update for next iteration up
        }

        // Reverse to get root-first order
        num_nodes_per_level.reverse();

        // --- Calculate level start offsets top-down ---
        let mut current_offset = header_size;
        for level in 0..height as usize {
            level_start_offsets.push(current_offset);
            let num_nodes = num_nodes_per_level[level];
            let node_size = if level == height as usize - 1 {
                leaf_node_byte_size // Leaf level
            } else {
                internal_node_byte_size // Internal level
            };
            current_offset += num_nodes * node_size as u64;
        }

        // Basic validation
        if height > 0 && (num_nodes_per_level.is_empty() || num_nodes_per_level[0] != 1) {
            return Err(Error::InvalidFormat(format!(
                "invalid root level node count: expected 1, got {:?}",
                num_nodes_per_level.first()
            )));
        }
        if level_start_offsets.len() != height as usize
            || num_nodes_per_level.len() != height as usize
        {
            return Err(Error::InvalidFormat(
                "layout calculation resulted in incorrect number of levels".to_string(),
            ));
        }

        Ok((num_nodes_per_level, level_start_offsets))
    }

    /// Finds the value associated with a given key.
    pub fn find(&mut self, search_key: &K) -> Result<Option<Value>, Error> {
        if self.height == 0 {
            println!("find: empty tree");
            return Ok(None);
        }

        let mut current_node_absolute_index: u64 = 0; // Root node index
        let b = self.branching_factor as u64;

        println!("find: starting search for key {:?}", search_key);

        // Descend internal nodes
        for current_level in 0..(self.height - 1) as usize {
            println!(
                "find: level {}, node_idx {}",
                current_level, current_node_absolute_index
            );
            let keys = self.read_internal_node_keys(current_node_absolute_index)?;
            if keys.is_empty() && b > 0 {
                // Empty internal node is invalid if branching factor > 0
                return Err(Error::InvalidFormat(format!(
                    "internal node {} at level {} is empty",
                    current_node_absolute_index, current_level
                )));
            }
            println!(
                "find: level {}, node {}, keys {:?}",
                current_level, current_node_absolute_index, keys
            );

            // Corrected logic: Find the index `i` of the first key strictly greater than search_key.
            // The pointer/branch index to follow is `i`.
            let branch_index = keys.partition_point(|k| k <= search_key);

            println!(
                "find: level {}, node {}, search_key {:?}, partition_point index (branch_index) {}",
                current_level, current_node_absolute_index, search_key, branch_index
            );

            // Calculate absolute index of the child node
            let level_start_node_index: u64 =
                self.num_nodes_per_level.iter().take(current_level).sum();
            let node_index_in_level = current_node_absolute_index - level_start_node_index;
            let child_level_start_node_index: u64 = self
                .num_nodes_per_level
                .iter()
                .take(current_level + 1)
                .sum();

            let child_index_within_next_level = node_index_in_level * b + branch_index as u64;
            current_node_absolute_index =
                child_level_start_node_index + child_index_within_next_level;
            println!(
                "find: level {}, descending to child node_idx {}",
                current_level, current_node_absolute_index
            );

            // Bounds check (ensure child exists)
            let total_nodes_in_next_level = self.num_nodes_per_level[current_level + 1];
            let next_level_start_index = child_level_start_node_index;
            if current_node_absolute_index >= next_level_start_index + total_nodes_in_next_level {
                println!(
                    "find: error! calculated child index {} out of bounds for level {} (max {})",
                    current_node_absolute_index,
                    current_level + 1,
                    next_level_start_index + total_nodes_in_next_level - 1
                );
                // If the branch index points past the last valid child for this node, the key is not present.
                return Ok(None);
            }
        }

        // Search Leaf Node
        println!("find: searching leaf node {}", current_node_absolute_index);
        let entries = self.read_leaf_node_entries(current_node_absolute_index)?;
        println!(
            "find: leaf node {}, entries {:?}",
            current_node_absolute_index, entries
        );
        match entries.binary_search_by(|entry| entry.key.cmp(search_key)) {
            Ok(index) => {
                println!("find: key found at index {} in leaf", index);
                Ok(Some(entries[index].value))
            }
            Err(_) => {
                println!("find: key not found in leaf");
                Ok(None)
            }
        }
    }

    // --- range ---
    // (To be implemented)

    // --- Internal Helper Methods ---

    /// Calculates the absolute byte offset for a given node index.
    fn calculate_node_offset(&self, node_index: u64) -> Result<u64, Error> {
        if self.height == 0 {
            return Err(Error::QueryError(
                "cannot calculate offset in empty tree".to_string(),
            ));
        }

        let mut current_level = 0;
        let mut level_start_node_idx: u64 = 0; // Start index of the current level being checked
        let mut found_level = false;

        // Check bounds first
        let total_nodes: u64 = self.num_nodes_per_level.iter().sum();
        if node_index >= total_nodes {
            return Err(Error::QueryError(format!(
                "node index {} is out of bounds for tree with {} total nodes and height {}",
                node_index, total_nodes, self.height
            )));
        }

        // Find the level containing node_index
        for (level_idx, &level_node_count) in self.num_nodes_per_level.iter().enumerate() {
            if node_index < level_start_node_idx + level_node_count {
                current_level = level_idx;
                found_level = true;
                break; // Found the level
            }
            level_start_node_idx += level_node_count;
        }

        // This check should be unreachable due to the initial bounds check
        if !found_level {
            return Err(Error::QueryError(format!(
                "internal error: could not find level for node index {}",
                node_index
            )));
        }

        // Calculate relative index within the found level
        let relative_index_in_level = node_index - level_start_node_idx;

        // Get level start offset and node size for this level
        let level_start_offset = self.level_start_offsets[current_level];
        let node_size = if current_level == self.height as usize - 1 {
            self.leaf_node_byte_size
        } else {
            self.internal_node_byte_size
        };

        // Calculate final offset
        let absolute_offset = level_start_offset + relative_index_in_level * node_size as u64;
        println!("debug: calculate_node_offset(idx={}) -> level={}, rel_idx={}, level_start_idx={}, level_offset={}, node_size={}, abs_offset={}",
                 node_index, current_level, relative_index_in_level, level_start_node_idx, level_start_offset, node_size, absolute_offset);
        Ok(absolute_offset)
    }

    /// Reads and deserializes keys from a specified internal node.
    fn read_internal_node_keys(&mut self, node_index: u64) -> Result<Vec<K>, Error> {
        let current_level = self.find_level_of_node(node_index)?;
        if current_level == self.height as usize - 1 {
            return Err(Error::QueryError(
                "read_internal_node_keys called on a leaf node index".to_string(),
            ));
        }

        let offset = self.calculate_node_offset(node_index)?;
        println!(
            "debug: read_internal_node_keys(idx={}) seeking to offset {}",
            node_index, offset
        );
        self.reader.seek(SeekFrom::Start(offset))?;

        // Determine actual number of keys based on children count (nodes in level below)
        let num_children_nodes = self
            .num_nodes_per_level
            .get(current_level + 1)
            .copied()
            .unwrap_or(0);
        let total_nodes_in_current_level = self.num_nodes_per_level[current_level];
        let level_start_node_index: u64 = self.num_nodes_per_level.iter().take(current_level).sum();
        let is_last_node_in_level =
            node_index == level_start_node_index + total_nodes_in_current_level - 1;
        let b = self.branching_factor as u64;

        // Number of keys in an internal node = number of children it points to.
        let keys_in_this_node = if is_last_node_in_level {
            let children_in_full_nodes = (total_nodes_in_current_level - 1) * b;
            let remaining_children = num_children_nodes.saturating_sub(children_in_full_nodes);
            if remaining_children > b {
                println!("warning: calculated remaining children {} exceeds branching factor {} for last internal node {}", remaining_children, b, node_index);
                b as usize
            } else {
                if remaining_children == 0 && num_children_nodes > 0 {
                    return Err(Error::InvalidFormat(format!(
                        "internal node {} is last in level but has 0 remaining children calculated",
                        node_index
                    )));
                }
                remaining_children as usize
            }
        } else {
            self.branching_factor as usize
        };
        println!(
            "debug: read_internal_node_keys(idx={}) determined keys_in_this_node={}",
            node_index, keys_in_this_node
        );

        // Read only the necessary bytes for the valid keys
        let bytes_to_read = keys_in_this_node * self.key_size;
        if bytes_to_read == 0 {
            println!(
                "debug: read_internal_node_keys(idx={}) reading 0 keys",
                node_index
            );
            return Ok(Vec::new());
        }
        println!(
            "debug: read_internal_node_keys(idx={}) reading {} bytes",
            node_index, bytes_to_read
        );
        let mut node_buffer = vec![0u8; bytes_to_read];
        self.reader.read_exact(&mut node_buffer)?;

        let mut cursor = Cursor::new(node_buffer);
        let mut keys = Vec::with_capacity(keys_in_this_node);
        for i in 0..keys_in_this_node {
            match K::read_from(&mut cursor) {
                Ok(key) => keys.push(key),
                Err(e) => {
                    println!(
                        "debug: read_internal_node_keys(idx={}) error reading key {}: {:?}",
                        node_index, i, e
                    );
                    return Err(e);
                }
            }
        }
        Ok(keys)
    }

    /// Reads and deserializes entries from a specified leaf node.
    fn read_leaf_node_entries(&mut self, node_index: u64) -> Result<Vec<Entry<K>>, Error> {
        let offset = self.calculate_node_offset(node_index)?;
        println!(
            "debug: read_leaf_node_entries(idx={}) seeking to offset {}",
            node_index, offset
        );
        self.reader.seek(SeekFrom::Start(offset))?;

        // Determine how many entries are actually in this node
        let num_nodes_at_leaf_level = *self.num_nodes_per_level.last().unwrap_or(&0);
        let first_node_index_of_leaf_level: u64 =
            self.num_nodes_per_level.iter().rev().skip(1).sum();

        let entries_in_this_node = if node_index
            == first_node_index_of_leaf_level + num_nodes_at_leaf_level - 1
        {
            let entries_in_full_leaves =
                (num_nodes_at_leaf_level - 1) * self.branching_factor as u64;
            let remaining_entries = self.num_entries - entries_in_full_leaves;
            if remaining_entries > self.branching_factor as u64 {
                return Err(Error::InvalidFormat(format!("calculated remaining entries {} exceeds branching factor {} for last leaf node {}", remaining_entries, self.branching_factor, node_index)));
            }
            remaining_entries as usize
        } else {
            self.branching_factor as usize
        };
        println!(
            "debug: read_leaf_node_entries(idx={}) determined entries_in_this_node={}",
            node_index, entries_in_this_node
        );

        // Read only the necessary bytes for the valid entries
        let bytes_to_read = entries_in_this_node * self.entry_size;
        if bytes_to_read == 0 {
            println!(
                "debug: read_leaf_node_entries(idx={}) reading 0 entries",
                node_index
            );
            return Ok(Vec::new());
        }
        println!(
            "debug: read_leaf_node_entries(idx={}) reading {} bytes",
            node_index, bytes_to_read
        );
        let mut node_buffer = vec![0u8; bytes_to_read];
        self.reader.read_exact(&mut node_buffer)?;

        let mut cursor = Cursor::new(node_buffer);
        let mut entries = Vec::with_capacity(entries_in_this_node);
        for i in 0..entries_in_this_node {
            match Entry::<K>::read_from(&mut cursor) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    println!(
                        "debug: read_leaf_node_entries(idx={}) error reading entry {}: {:?}",
                        node_index, i, e
                    );
                    return Err(e);
                }
            }
        }

        Ok(entries)
    }

    /// Helper to find the level index (0=root) for a given absolute node index.
    fn find_level_of_node(&self, node_index: u64) -> Result<usize, Error> {
        let mut nodes_processed: u64 = 0;
        for (level_idx, count) in self.num_nodes_per_level.iter().enumerate() {
            if node_index < nodes_processed + count {
                return Ok(level_idx);
            }
            nodes_processed += count;
        }
        Err(Error::QueryError(format!(
            "node index {} out of bounds",
            node_index
        )))
    }

    // --- Accessors ---
    pub fn branching_factor(&self) -> u16 {
        self.branching_factor
    }
    pub fn len(&self) -> u64 {
        self.num_entries
    }
    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }
    pub fn height(&self) -> u8 {
        self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::StaticBTreeBuilder;
    use crate::entry::Entry;
    use crate::key::Key;
    use std::io::{Cursor, Read, Write};

    // Re-use TestKey
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct TestKey(i32);
    impl Key for TestKey {
        const SERIALIZED_SIZE: usize = 4;
        fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
            writer.write_all(&self.0.to_le_bytes()).map_err(Error::from)
        }
        fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error> {
            let mut bytes = [0u8; Self::SERIALIZED_SIZE];
            reader.read_exact(&mut bytes)?;
            Ok(TestKey(i32::from_le_bytes(bytes)))
        }
    }

    // Helper to build a test tree in memory
    fn build_test_tree(
        entries: Vec<Result<Entry<TestKey>, Error>>,
        b: u16,
    ) -> Result<Cursor<Vec<u8>>, Error> {
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b)?;
        builder.build_from_sorted(entries)?;
        cursor.seek(SeekFrom::Start(0))?;
        Ok(cursor)
    }

    #[test]
    fn test_open_empty_tree() {
        let cursor = build_test_tree(vec![], 3).unwrap();
        let tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();
        assert_eq!(tree.height(), 0);
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn test_open_single_leaf_node_tree() {
        let b = 5;
        let entries = vec![
            Ok(Entry {
                key: TestKey(10),
                value: 1,
            }),
            Ok(Entry {
                key: TestKey(20),
                value: 2,
            }),
            Ok(Entry {
                key: TestKey(30),
                value: 3,
            }),
        ];
        let cursor = build_test_tree(entries, b).unwrap();
        let tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();
        assert_eq!(tree.height(), 1);
        assert_eq!(tree.len(), 3);
        assert_eq!(tree.num_nodes_per_level, vec![1]);
        assert_eq!(tree.level_start_offsets, vec![tree.header_size]);
    }

    #[test]
    fn test_open_two_level_tree() {
        let b = 2;
        let entries = vec![
            Ok(Entry {
                key: TestKey(10),
                value: 1,
            }),
            Ok(Entry {
                key: TestKey(20),
                value: 2,
            }),
            Ok(Entry {
                key: TestKey(30),
                value: 3,
            }),
            Ok(Entry {
                key: TestKey(40),
                value: 4,
            }),
            Ok(Entry {
                key: TestKey(50),
                value: 5,
            }),
        ];
        let cursor = build_test_tree(entries, b).unwrap();
        let tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();
        assert_eq!(tree.height(), 3);
        assert_eq!(tree.len(), 5);
        assert_eq!(tree.num_nodes_per_level, vec![1, 2, 3]);
    }

    #[test]
    fn test_open_invalid_magic() {
        let mut data = vec![0u8; 100];
        data[0..8].copy_from_slice(b"WRONGSIG");
        let cursor = Cursor::new(data);
        assert!(matches!(
            StaticBTree::<TestKey, _>::open(cursor),
            Err(Error::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_open_invalid_version() {
        let mut data = vec![0u8; 100];
        data[0..8].copy_from_slice(MAGIC_BYTES);
        data[8..10].copy_from_slice(&9999u16.to_le_bytes());
        let cursor = Cursor::new(data);
        assert!(matches!(
            StaticBTree::<TestKey, _>::open(cursor),
            Err(Error::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_open_invalid_bfactor() {
        let mut data = vec![0u8; 100];
        data[0..8].copy_from_slice(MAGIC_BYTES);
        data[8..10].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        data[10..12].copy_from_slice(&1u16.to_le_bytes());
        let cursor = Cursor::new(data);
        assert!(matches!(
            StaticBTree::<TestKey, _>::open(cursor),
            Err(Error::InvalidFormat(_))
        ));
    }

    // --- Tests for Helper Methods ---

    #[test]
    fn test_calculate_node_offset() {
        let b = 2;
        let entries = vec![
            Ok(Entry {
                key: TestKey(10),
                value: 1,
            }),
            Ok(Entry {
                key: TestKey(20),
                value: 2,
            }),
            Ok(Entry {
                key: TestKey(30),
                value: 3,
            }),
            Ok(Entry {
                key: TestKey(40),
                value: 4,
            }),
            Ok(Entry {
                key: TestKey(50),
                value: 5,
            }),
        ];
        let cursor = build_test_tree(entries, b).unwrap();
        let tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();

        let leaf_node_size = tree.leaf_node_byte_size as u64;
        let internal_node_size = tree.internal_node_byte_size as u64;

        // Check Root (Index 0, Level 0)
        assert_eq!(
            tree.calculate_node_offset(0).unwrap(),
            tree.level_start_offsets[0]
        );
        // Check Internal Nodes (Indices 1, 2, Level 1)
        assert_eq!(
            tree.calculate_node_offset(1).unwrap(),
            tree.level_start_offsets[1]
        );
        assert_eq!(
            tree.calculate_node_offset(2).unwrap(),
            tree.level_start_offsets[1] + internal_node_size
        );
        // Check Leaf Nodes (Indices 3, 4, 5, Level 2)
        assert_eq!(
            tree.calculate_node_offset(3).unwrap(),
            tree.level_start_offsets[2]
        );
        assert_eq!(
            tree.calculate_node_offset(4).unwrap(),
            tree.level_start_offsets[2] + leaf_node_size
        );
        assert_eq!(
            tree.calculate_node_offset(5).unwrap(),
            tree.level_start_offsets[2] + 2 * leaf_node_size
        );
        // Check out of bounds
        assert!(tree.calculate_node_offset(6).is_err());
    }

    #[test]
    fn test_read_leaf_node_entries() {
        let b = 2;
        let entries_vec = vec![
            Ok(Entry {
                key: TestKey(10),
                value: 1,
            }),
            Ok(Entry {
                key: TestKey(20),
                value: 2,
            }), // Node 3
            Ok(Entry {
                key: TestKey(30),
                value: 3,
            }),
            Ok(Entry {
                key: TestKey(40),
                value: 4,
            }), // Node 4
            Ok(Entry {
                key: TestKey(50),
                value: 5,
            }), // Node 5 (partial)
        ];
        let cursor = build_test_tree(entries_vec, b).unwrap();
        let mut tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();

        // Leaf nodes are indices 3, 4, 5
        let entries3 = tree.read_leaf_node_entries(3).unwrap();
        assert_eq!(entries3.len(), 2);
        assert_eq!(
            entries3[0],
            Entry {
                key: TestKey(10),
                value: 1
            }
        );
        assert_eq!(
            entries3[1],
            Entry {
                key: TestKey(20),
                value: 2
            }
        );
        let entries4 = tree.read_leaf_node_entries(4).unwrap();
        assert_eq!(entries4.len(), 2);
        assert_eq!(
            entries4[0],
            Entry {
                key: TestKey(30),
                value: 3
            }
        );
        assert_eq!(
            entries4[1],
            Entry {
                key: TestKey(40),
                value: 4
            }
        );
        let entries5 = tree.read_leaf_node_entries(5).unwrap();
        assert_eq!(entries5.len(), 1);
        assert_eq!(
            entries5[0],
            Entry {
                key: TestKey(50),
                value: 5
            }
        );
    }

    #[test]
    fn test_read_internal_node_keys() {
        let b = 2;
        let entries_vec = vec![
            Ok(Entry {
                key: TestKey(10),
                value: 1,
            }),
            Ok(Entry {
                key: TestKey(20),
                value: 2,
            }),
            Ok(Entry {
                key: TestKey(30),
                value: 3,
            }),
            Ok(Entry {
                key: TestKey(40),
                value: 4,
            }),
            Ok(Entry {
                key: TestKey(50),
                value: 5,
            }),
        ];
        let cursor = build_test_tree(entries_vec, b).unwrap();
        let mut tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();

        // Node 1 (Internal Level 1) - Keys [10, 30]
        let keys1 = tree.read_internal_node_keys(1).unwrap();
        assert_eq!(keys1.len(), 2);
        assert_eq!(keys1[0], TestKey(10));
        assert_eq!(keys1[1], TestKey(30));

        // Node 2 (Internal Level 1) - Keys [50] (last node, points to 1 child)
        let keys2 = tree.read_internal_node_keys(2).unwrap();
        assert_eq!(keys2.len(), 1); // Corrected expectation
        assert_eq!(keys2[0], TestKey(50));

        // Node 0 (Root) - Keys [10, 50]
        let keys0 = tree.read_internal_node_keys(0).unwrap();
        assert_eq!(keys0.len(), 2);
        assert_eq!(keys0[0], TestKey(10));
        assert_eq!(keys0[1], TestKey(50));
    }

    // --- Tests for find ---
    #[test]
    fn test_find() {
        let b = 3;
        let entries = (1..=20)
            .map(|i| {
                Ok(Entry {
                    key: TestKey(i * 10),
                    value: i as u64,
                })
            })
            .collect();
        let cursor = build_test_tree(entries, b).unwrap();
        let mut tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();

        // Test keys present
        assert_eq!(tree.find(&TestKey(10)).unwrap(), Some(1));
        assert_eq!(tree.find(&TestKey(50)).unwrap(), Some(5));
        assert_eq!(tree.find(&TestKey(100)).unwrap(), Some(10));
        assert_eq!(tree.find(&TestKey(200)).unwrap(), Some(20));

        // Test keys absent (between existing keys)
        assert_eq!(tree.find(&TestKey(15)).unwrap(), None);
        assert_eq!(tree.find(&TestKey(95)).unwrap(), None);
        assert_eq!(tree.find(&TestKey(155)).unwrap(), None);

        // Test keys absent (outside range)
        assert_eq!(tree.find(&TestKey(5)).unwrap(), None);
        assert_eq!(tree.find(&TestKey(210)).unwrap(), None);
        assert_eq!(tree.find(&TestKey(0)).unwrap(), None);
        assert_eq!(tree.find(&TestKey(-10)).unwrap(), None);
    }

    #[test]
    fn test_find_in_empty_tree() {
        let cursor = build_test_tree(vec![], 3).unwrap();
        let mut tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();
        assert_eq!(tree.find(&TestKey(10)).unwrap(), None);
    }

    #[test]
    fn test_find_in_single_leaf_tree() {
        let b = 5;
        let entries = vec![
            Ok(Entry {
                key: TestKey(10),
                value: 1,
            }),
            Ok(Entry {
                key: TestKey(20),
                value: 2,
            }),
            Ok(Entry {
                key: TestKey(30),
                value: 3,
            }),
        ];
        let cursor = build_test_tree(entries, b).unwrap();
        let mut tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();
        assert_eq!(tree.find(&TestKey(20)).unwrap(), Some(2));
        assert_eq!(tree.find(&TestKey(15)).unwrap(), None);
        assert_eq!(tree.find(&TestKey(30)).unwrap(), Some(3));
        assert_eq!(tree.find(&TestKey(40)).unwrap(), None);
    }
}
