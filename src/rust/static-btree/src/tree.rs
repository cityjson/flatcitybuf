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
        let entry_size = key_size + value_size;
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

        let mut num_items_in_level_above = num_leaf_nodes;
        for _level in (0..height - 1).rev() {
            if num_items_in_level_above == 0 {
                // This can happen legitimately if height=1 and num_entries=0 (though handled above)
                // Or if height > 1 and num_leaf_nodes = 0 (which means num_entries=0, also handled above)
                // So this check might indicate an invalid height value in the header if reached.
                return Err(Error::InvalidFormat(format!(
                    "invalid tree structure: level {} has 0 nodes but height is {}",
                    _level + 1,
                    height
                )));
            }
            let num_nodes_current_level = (num_items_in_level_above + b - 1) / b;
            num_nodes_per_level.push(num_nodes_current_level);
            num_items_in_level_above = num_nodes_current_level;
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

    // --- find ---
    // (To be implemented)

    // --- range ---
    // (To be implemented)

    // --- Internal Helper Methods ---

    /// Calculates the absolute byte offset for a given node index based on the pre-calculated layout.
    /// Node indices are absolute across the entire tree structure (like Eytzinger).
    fn calculate_node_offset(&self, node_index: u64) -> Result<u64, Error> {
        if self.height == 0 {
            return Err(Error::QueryError(
                "cannot calculate offset in empty tree".to_string(),
            ));
        }

        let mut current_level = 0;
        let mut nodes_processed: u64 = 0;
        let mut level_start_node_index: u64 = 0;

        // Find the level of the node_index
        for level_node_count in &self.num_nodes_per_level {
            if node_index < nodes_processed + level_node_count {
                break; // Found the level
            }
            nodes_processed += level_node_count;
            level_start_node_index = nodes_processed; // Start index of the *next* level
            current_level += 1;
        }

        if current_level >= self.height as usize {
            return Err(Error::QueryError(format!(
                "node index {} is out of bounds for tree height {}",
                node_index, self.height
            )));
        }

        // Calculate relative index within the level
        let relative_index_in_level = node_index - level_start_node_index;

        // Get level start offset and node size for this level
        let level_start_offset = self.level_start_offsets[current_level];
        let node_size = if current_level == self.height as usize - 1 {
            self.leaf_node_byte_size // Leaf level
        } else {
            self.internal_node_byte_size // Internal level
        };

        // Calculate final offset
        let absolute_offset = level_start_offset + relative_index_in_level * node_size as u64;
        Ok(absolute_offset)
    }

    /// Reads and deserializes all keys from a specified internal node.
    /// Assumes the node at `node_index` is an internal node.
    fn read_internal_node_keys(&mut self, node_index: u64) -> Result<Vec<K>, Error> {
        let offset = self.calculate_node_offset(node_index)?;
        self.reader.seek(SeekFrom::Start(offset))?;

        // Read the exact node size into a buffer
        let mut node_buffer = vec![0u8; self.internal_node_byte_size];
        self.reader.read_exact(&mut node_buffer)?;

        // Deserialize keys from the buffer
        let mut cursor = Cursor::new(node_buffer);
        let mut keys = Vec::with_capacity(self.branching_factor as usize);
        // TODO: Handle potentially partially filled *last* internal node?
        // The S+Tree paper often assumes full internal nodes except maybe the root.
        // If the last internal node *can* be partial, we need metadata to know how many keys are valid.
        // For now, assume full internal nodes.
        for _ in 0..self.branching_factor {
            match K::read_from(&mut cursor) {
                Ok(key) => keys.push(key),
                Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // This might happen if the node was padded and we try to read past valid keys.
                    // If partial internal nodes are allowed, this might not be an error,
                    // but we need a way to know the actual key count. Assuming full for now.
                    return Err(Error::InvalidFormat(format!(
                        "unexpected end of internal node {} while reading keys (expected {} keys)",
                        node_index, self.branching_factor
                    )));
                }
                Err(e) => return Err(e), // Propagate other errors
            }
        }
        Ok(keys)
    }

    /// Reads and deserializes all entries from a specified leaf node.
    /// Assumes the node at `node_index` is a leaf node. Handles partially filled last leaf.
    fn read_leaf_node_entries(&mut self, node_index: u64) -> Result<Vec<Entry<K>>, Error> {
        let offset = self.calculate_node_offset(node_index)?;
        self.reader.seek(SeekFrom::Start(offset))?;

        // Read the exact node size into a buffer
        let mut node_buffer = vec![0u8; self.leaf_node_byte_size];
        self.reader.read_exact(&mut node_buffer)?;

        // Determine how many entries are actually in this node
        let num_nodes_at_leaf_level = *self.num_nodes_per_level.last().unwrap_or(&0); // Should exist if height > 0
        let first_node_index_of_leaf_level: u64 =
            self.num_nodes_per_level.iter().rev().skip(1).sum(); // Sum counts of levels above leaf

        let entries_in_this_node = if node_index
            == first_node_index_of_leaf_level + num_nodes_at_leaf_level - 1
        {
            // This is the last leaf node, might be partial
            let entries_in_full_leaves =
                (num_nodes_at_leaf_level - 1) * self.branching_factor as u64;
            let remaining_entries = self.num_entries - entries_in_full_leaves;
            if remaining_entries > self.branching_factor as u64 {
                // This indicates an inconsistency
                return Err(Error::InvalidFormat(format!("calculated remaining entries {} exceeds branching factor {} for last leaf node {}", remaining_entries, self.branching_factor, node_index)));
            }
            remaining_entries as usize
        } else {
            // Not the last node, must be full
            self.branching_factor as usize
        };

        // Deserialize the valid number of entries
        let mut cursor = Cursor::new(node_buffer);
        let mut entries = Vec::with_capacity(entries_in_this_node);
        for _ in 0..entries_in_this_node {
            match Entry::<K>::read_from(&mut cursor) {
                Ok(entry) => entries.push(entry),
                Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Should not happen if entries_in_this_node calculation is correct
                    return Err(Error::InvalidFormat(format!(
                        "unexpected end of leaf node {} while reading entry {}/{}",
                        node_index,
                        entries.len() + 1,
                        entries_in_this_node
                    )));
                }
                Err(e) => return Err(e),
            }
        }

        Ok(entries)
    }

    // --- Accessors ---
    /// Returns the branching factor B used by this tree.
    pub fn branching_factor(&self) -> u16 {
        self.branching_factor
    }
    /// Returns the total number of key-value entries stored in the tree.
    pub fn len(&self) -> u64 {
        self.num_entries
    }
    /// Returns true if the tree contains no entries.
    pub fn is_empty(&self) -> bool {
        self.num_entries == 0
    }
    /// Returns the height of the tree.
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
    use std::io::{Cursor, Read, Write}; // Added Read, Write

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
        cursor.seek(SeekFrom::Start(0))?; // Rewind cursor for reading
        Ok(cursor)
    }

    #[test]
    fn test_open_empty_tree() {
        let cursor = build_test_tree(vec![], 3).unwrap();
        let tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();
        assert_eq!(tree.height(), 0);
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
        assert_eq!(tree.branching_factor(), 3);
        assert!(tree.num_nodes_per_level.is_empty());
        assert!(tree.level_start_offsets.is_empty());
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
        let num_entries = entries.len() as u64;
        let cursor = build_test_tree(entries, b).unwrap();
        let tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();

        assert_eq!(tree.height(), 1);
        assert_eq!(tree.len(), num_entries);
        assert!(!tree.is_empty());
        assert_eq!(tree.branching_factor(), b);
        assert_eq!(tree.num_nodes_per_level, vec![1]); // One leaf node
        assert_eq!(tree.level_start_offsets, vec![tree.header_size]); // Starts after header
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
        let num_entries = entries.len() as u64;
        let cursor = build_test_tree(entries, b).unwrap();
        let tree = StaticBTree::<TestKey, _>::open(cursor).unwrap();

        assert_eq!(tree.height(), 3);
        assert_eq!(tree.len(), num_entries);
        assert_eq!(tree.branching_factor(), b);
        assert_eq!(tree.num_nodes_per_level, vec![1, 2, 3]); // Root, Internal1, Leaves

        let header_size = tree.header_size;
        let internal_node_size = tree.internal_node_byte_size as u64;
        let leaf_node_size = tree.leaf_node_byte_size as u64; // Not needed for offset calc here

        let expected_offsets = vec![
            header_size,                                                   // Level 0 (Root) offset
            header_size + 1 * internal_node_size, // Level 1 (Internal) offset (1 root node before)
            header_size + 1 * internal_node_size + 2 * internal_node_size, // Level 2 (Leaf) offset (1 root + 2 internal nodes before)
        ];
        assert_eq!(tree.level_start_offsets, expected_offsets);
    }

    #[test]
    fn test_open_invalid_magic() {
        let mut data = vec![0u8; 100];
        data[0..8].copy_from_slice(b"WRONGSIG");
        let cursor = Cursor::new(data);
        let result = StaticBTree::<TestKey, _>::open(cursor);
        assert!(matches!(result, Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn test_open_invalid_version() {
        let mut data = vec![0u8; 100];
        data[0..8].copy_from_slice(MAGIC_BYTES);
        data[8..10].copy_from_slice(&9999u16.to_le_bytes()); // Invalid version
        let cursor = Cursor::new(data);
        let result = StaticBTree::<TestKey, _>::open(cursor);
        assert!(matches!(result, Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn test_open_invalid_bfactor() {
        let mut data = vec![0u8; 100];
        data[0..8].copy_from_slice(MAGIC_BYTES);
        data[8..10].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        data[10..12].copy_from_slice(&1u16.to_le_bytes()); // Invalid bfactor=1
        let cursor = Cursor::new(data);
        let result = StaticBTree::<TestKey, _>::open(cursor);
        assert!(matches!(result, Err(Error::InvalidFormat(_))));
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

        // Expected offsets (calculated manually based on write order)
        let header_size = tree.header_size;
        let leaf_node_size = tree.leaf_node_byte_size as u64;
        let internal_node_size = tree.internal_node_byte_size as u64;

        // Node indices (absolute, Eytzinger-like, but based on level counts)
        // Level 0 (Root): Index 0
        // Level 1 (Internal): Indices 1, 2
        // Level 2 (Leaf): Indices 3, 4, 5

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
        // Node 3
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

        // Node 4
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

        // Node 5 (last leaf, partial)
        let entries5 = tree.read_leaf_node_entries(5).unwrap();
        assert_eq!(entries5.len(), 1); // Only one entry
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

        // Internal nodes are indices 1, 2 (Level 1) and 0 (Root, Level 0)
        // Node 1 (Internal Level 1) - Keys [10, 30]
        let keys1 = tree.read_internal_node_keys(1).unwrap();
        assert_eq!(keys1.len(), 2);
        assert_eq!(keys1[0], TestKey(10));
        assert_eq!(keys1[1], TestKey(30));

        // Node 2 (Internal Level 1) - Keys [50, pad] -> should read [50] assuming full nodes for now
        // TODO: Revisit if partial internal nodes are allowed and how to detect padding.
        // Current implementation assumes full internal nodes and might err or read garbage if padded.
        // Let's assume the builder *does* pad internal nodes correctly for now.
        let keys2 = tree.read_internal_node_keys(2).unwrap();
        assert_eq!(keys2.len(), 2); // Reads full node size
        assert_eq!(keys2[0], TestKey(50));
        // The second key would be whatever padding was written (likely 0 if padded with zeros)
        // assert_eq!(keys2[1], TestKey(0)); // This depends on padding value

        // Node 0 (Root) - Keys [10, 50]
        let keys0 = tree.read_internal_node_keys(0).unwrap();
        assert_eq!(keys0.len(), 2);
        assert_eq!(keys0[0], TestKey(10));
        assert_eq!(keys0[1], TestKey(50));
    }
}
