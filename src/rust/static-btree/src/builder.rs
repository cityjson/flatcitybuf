use crate::entry::Entry;
use crate::error::Error;
use crate::key::Key;
use crate::Value; // Assuming Value is always u64
use std::io::{Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::mem;

// Constants for the header structure (adjust size as needed)
pub const MAGIC_BYTES: &[u8; 8] = b"STREE01\0"; // Made public
pub const FORMAT_VERSION: u16 = 1; // Made public
pub const DEFAULT_HEADER_RESERVATION: u64 = 64; // Made public

/// Builder structure for creating a StaticBTree file/data structure.
/// Writes to a `Write + Seek` target using a bottom-up approach.
/// Buffers nodes in memory and writes them top-down at the end.
pub struct StaticBTreeBuilder<K: Key, W: Write + Seek> {
    /// The output target. Must be seekable to write the header at the end.
    writer: W,
    /// The chosen branching factor for the tree (number of keys/entries per node).
    branching_factor: u16,
    /// Size reserved at the beginning for the header.
    header_size: u64,
    /// Counter for the total number of entries added.
    num_entries: u64,
    // --- Internal state for the bottom-up build process ---
    /// Buffer holding keys that need to be promoted to the next level up.
    promoted_keys_buffer: Vec<K>,
    /// Buffer for assembling the current node being written (contains serialized items).
    current_node_buffer: Vec<u8>,
    /// Tracks the number of items (entries or keys) added to the current node buffer.
    items_in_current_node: u16,
    // Removed current_offset as writing happens at the end
    // current_offset: u64,
    /// Stores the first key of each node written at the *current* level being processed.
    first_keys_of_current_level: Vec<K>,
    /// Stores calculated node counts per level during build (leaf level first).
    nodes_per_level_build: Vec<u64>,
    /// Stores serialized node data per level (leaf level first). Vec<LevelData>, LevelData = Vec<NodeData>
    buffered_levels: Vec<Vec<Vec<u8>>>,
    /// Cached size of a key.
    key_size: usize,
    /// Cached size of an entry.
    entry_size: usize,
    /// Cached byte size of a fully packed internal node.
    internal_node_byte_size: usize,
    /// Cached byte size of a fully packed leaf node.
    leaf_node_byte_size: usize,

    _phantom_key: PhantomData<K>,
}

impl<K: Key, W: Write + Seek> StaticBTreeBuilder<K, W> {
    /// Creates a new builder targeting the given writer.
    /// Reserves space for the header but defers node writing until finalization.
    pub fn new(mut writer: W, branching_factor: u16) -> Result<Self, Error> {
        if branching_factor <= 1 {
            return Err(Error::BuildError(format!(
                "branching factor must be greater than 1, got {}",
                branching_factor
            )));
        }

        let key_size = K::SERIALIZED_SIZE;
        let value_size = mem::size_of::<Value>();
        let entry_size = key_size + value_size;
        let internal_node_byte_size = branching_factor as usize * key_size;
        let leaf_node_byte_size = branching_factor as usize * entry_size;
        let header_size = DEFAULT_HEADER_RESERVATION;

        // Reserve Header Space (still needed for final write)
        writer.seek(SeekFrom::Start(0))?;
        let header_placeholder = vec![0u8; header_size as usize];
        writer.write_all(&header_placeholder)?;
        // We don't track current_offset during build anymore

        Ok(StaticBTreeBuilder {
            writer,
            branching_factor,
            header_size,
            num_entries: 0,
            promoted_keys_buffer: Vec::new(),
            current_node_buffer: Vec::with_capacity(
                leaf_node_byte_size.max(internal_node_byte_size) + key_size,
            ),
            items_in_current_node: 0,
            // current_offset: header_size, // Removed
            first_keys_of_current_level: Vec::new(),
            nodes_per_level_build: Vec::new(),
            buffered_levels: Vec::new(), // Initialize buffer for levels
            key_size,
            entry_size,
            internal_node_byte_size,
            leaf_node_byte_size,
            _phantom_key: PhantomData,
        })
    }

    /// Builds the entire tree from an iterator providing pre-sorted entries.
    pub fn build_from_sorted<I>(mut self, sorted_entries: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = Result<Entry<K>, Error>>,
    {
        let mut first_key_current_node: Option<K> = None;
        let mut last_key_processed: Option<K> = None;
        let mut current_level_nodes: Vec<Vec<u8>> = Vec::new(); // Buffer for nodes of the current level

        // --- Phase 1: Process Leaf Nodes & Collect First Keys ---
        for entry_result in sorted_entries {
            let entry = entry_result?;

            if let Some(ref last_key) = last_key_processed {
                if entry.key <= *last_key {
                    return Err(Error::BuildError(format!(
                        "input entries are not strictly sorted. key {:?} <= previous key {:?}",
                        entry.key, last_key
                    )));
                }
            }
            last_key_processed = Some(entry.key.clone());

            if self.items_in_current_node == 0 {
                first_key_current_node = Some(entry.key.clone());
            }

            entry.write_to(&mut self.current_node_buffer)?;
            self.items_in_current_node += 1;
            self.num_entries += 1;

            // If node buffer is full, finalize and buffer it
            if self.items_in_current_node == self.branching_factor {
                // No padding needed for full node
                current_level_nodes.push(self.current_node_buffer.clone()); // Add node data to level buffer
                self.first_keys_of_current_level.push(
                    first_key_current_node.take().ok_or_else(|| {
                        Error::BuildError("internal error: missing first key for full node".to_string())
                    })?,
                );
                self.items_in_current_node = 0;
                self.current_node_buffer.clear();
            }
        }

        // Handle the last potentially partial leaf node
        if self.items_in_current_node > 0 {
             self.pad_current_node_buffer(self.leaf_node_byte_size)?; // Pad before buffering
             current_level_nodes.push(self.current_node_buffer.clone()); // Add padded node data
             self.first_keys_of_current_level
                .push(first_key_current_node.take().ok_or_else(|| {
                    Error::BuildError("internal error: missing first key for partial node".to_string())
                })?);
        } else if self.num_entries == 0 {
             println!("warning: building tree from empty input iterator");
             // No nodes to buffer, finalize will handle empty case
             return self.finalize_build();
        }

        // Record leaf level info
        if !current_level_nodes.is_empty() {
            self.nodes_per_level_build.push(current_level_nodes.len() as u64);
            self.buffered_levels.push(current_level_nodes); // Add leaf level nodes to main buffer
            self.promoted_keys_buffer = std::mem::take(&mut self.first_keys_of_current_level);
        }

        // --- Phase 2..N: Process Internal Nodes (Bottom-Up) ---
        while self.promoted_keys_buffer.len() > 1 {
            let keys_for_this_level = std::mem::take(&mut self.promoted_keys_buffer);
            let mut current_level_nodes: Vec<Vec<u8>> = Vec::new(); // Buffer for this internal level
            let mut first_key_current_node: Option<K> = None;
            self.items_in_current_node = 0;
            self.current_node_buffer.clear();

            println!("debug: buffering internal level with {} keys", keys_for_this_level.len());

            for key in keys_for_this_level {
                if self.items_in_current_node == 0 {
                    first_key_current_node = Some(key.clone());
                }
                key.write_to(&mut self.current_node_buffer)?;
                self.items_in_current_node += 1;

                if self.items_in_current_node == self.branching_factor {
                    // Buffer full internal node (no padding needed)
                    current_level_nodes.push(self.current_node_buffer.clone());
                    self.first_keys_of_current_level.push(
                        first_key_current_node.take().ok_or_else(|| {
                            Error::BuildError("internal error: missing first key for full internal node".to_string())
                        })?,
                    );
                    self.items_in_current_node = 0;
                    self.current_node_buffer.clear();
                }
            }

            if self.items_in_current_node > 0 {
                self.pad_current_node_buffer(self.internal_node_byte_size)?; // Pad before buffering
                current_level_nodes.push(self.current_node_buffer.clone()); // Add padded node
                self.first_keys_of_current_level
                    .push(first_key_current_node.take().ok_or_else(|| {
                        Error::BuildError("internal error: missing first key for partial internal node".to_string())
                    })?);
            }

            if !current_level_nodes.is_empty() {
                self.nodes_per_level_build.push(current_level_nodes.len() as u64);
                self.buffered_levels.push(current_level_nodes); // Add internal level nodes
                self.promoted_keys_buffer = std::mem::take(&mut self.first_keys_of_current_level);
            } else {
                 // Should not happen if promoted_keys > 1
                 return Err(Error::BuildError("internal error: generated empty internal level".to_string()));
            }
        }

        // --- Phase N+1: Finalization ---
        self.finalize_build()
    }

    /// Helper to write the final header and buffered nodes.
    fn finalize_build(mut self) -> Result<(), Error> {
        let height = self.nodes_per_level_build.len() as u8;

        println!("debug: finalizing build. height: {}, num_entries: {}, nodes_per_level (leaf first): {:?}",
                 height, self.num_entries, self.nodes_per_level_build);

        // --- Write Nodes Top-Down ---
        self.writer.seek(SeekFrom::Start(self.header_size))?; // Start writing after header reservation
        let mut current_write_offset = self.header_size;

        // Iterate through buffered levels in reverse order (root first)
        for level_data in self.buffered_levels.iter().rev() {
            println!("debug: writing level with {} nodes at offset {}", level_data.len(), current_write_offset);
            for node_data in level_data {
                self.writer.write_all(node_data)?;
                current_write_offset += node_data.len() as u64; // Use actual node data length
            }
        }

        // --- Write Header ---
        self.writer.seek(SeekFrom::Start(0))?;
        self.writer.write_all(MAGIC_BYTES)?;
        self.writer.write_all(&FORMAT_VERSION.to_le_bytes())?;
        self.writer.write_all(&self.branching_factor.to_le_bytes())?;
        self.writer.write_all(&self.num_entries.to_le_bytes())?;
        self.writer.write_all(&height.to_le_bytes())?;

        let current_pos = self.writer.stream_position()?;
        if current_pos > self.header_size {
            return Err(Error::BuildError(format!(
                "header content size ({}) exceeded reservation ({})",
                current_pos, self.header_size
            )));
        }
        let padding_needed = self.header_size - current_pos;
        if padding_needed > 0 {
            let padding = vec![0u8; padding_needed as usize];
            self.writer.write_all(&padding)?;
        }

        self.writer.flush()?;
        println!("debug: header and nodes written successfully.");
        Ok(())
    }

    /// Helper to pad the current node buffer to the expected size.
    fn pad_current_node_buffer(&mut self, expected_node_size: usize) -> Result<(), Error> {
        if self.current_node_buffer.len() > expected_node_size {
            return Err(Error::BuildError(format!(
                "internal error: buffer size {} exceeds expected node size {}",
                self.current_node_buffer.len(),
                expected_node_size
            )));
        }
        let padding_needed = expected_node_size - self.current_node_buffer.len();
        if padding_needed > 0 {
            self.current_node_buffer
                .extend(std::iter::repeat(0).take(padding_needed));
        }
        // Ensure buffer is exactly the expected size after padding
        if self.current_node_buffer.len() != expected_node_size {
             return Err(Error::BuildError(format!(
                "internal error: padding failed, buffer size {} != expected {}",
                self.current_node_buffer.len(), expected_node_size
            )));
        }
        Ok(())
    }

    // Removed write_current_node and pad_and_write_current_node as writing is deferred
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;
    use crate::key::Key;
    use std::io::{Cursor, Read, SeekFrom};

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

    fn read_test_header(cursor: &mut Cursor<Vec<u8>>) -> (u16, u16, u64, u8) {
        cursor.seek(SeekFrom::Start(MAGIC_BYTES.len() as u64)).unwrap();
        let mut u16_buf = [0u8; 2];
        let mut u64_buf = [0u8; 8];
        let mut u8_buf = [0u8; 1];

        cursor.read_exact(&mut u16_buf).unwrap();
        let version = u16::from_le_bytes(u16_buf);
        cursor.read_exact(&mut u16_buf).unwrap();
        let bfactor = u16::from_le_bytes(u16_buf);
        cursor.read_exact(&mut u64_buf).unwrap();
        let num_entries = u64::from_le_bytes(u64_buf);
        cursor.read_exact(&mut u8_buf).unwrap();
        let height = u8::from_le_bytes(u8_buf);

        (version, bfactor, num_entries, height)
    }

    #[test]
    fn test_builder_new_valid() {
        let cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(cursor, 10).unwrap();
        assert_eq!(builder.branching_factor, 10);
        let buffer = builder.writer.into_inner();
        assert_eq!(buffer.len() as u64, DEFAULT_HEADER_RESERVATION);
    }

    #[test]
    fn test_builder_new_invalid_branching_factor() {
        let cursor = Cursor::new(Vec::new());
        let result = StaticBTreeBuilder::<TestKey, _>::new(cursor, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_single_leaf_node_tree() {
        let b: u16 = 5;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![
            Ok(Entry { key: TestKey(10), value: 1 }),
            Ok(Entry { key: TestKey(20), value: 2 }),
            Ok(Entry { key: TestKey(30), value: 3 }),
        ];
        let num_entries_expected = entries.len() as u64;

        assert!(builder.build_from_sorted(entries).is_ok());

        let mut buffer = cursor.into_inner();
        let header_size = DEFAULT_HEADER_RESERVATION as usize;
        let entry_size = 12;
        let node_size = b as usize * entry_size;

        assert_eq!(buffer.len(), header_size + node_size);
        let (version, bfactor, num_entries_hdr, height) =
            read_test_header(&mut Cursor::new(buffer.clone()));
        assert_eq!(height, 1);
        assert_eq!(num_entries_hdr, num_entries_expected);

        // Check node data (should be written after header)
        let node_data = &buffer[header_size..];
        let mut expected_node_data = Vec::with_capacity(node_size);
        Entry { key: TestKey(10), value: 1 }.write_to(&mut expected_node_data).unwrap();
        Entry { key: TestKey(20), value: 2 }.write_to(&mut expected_node_data).unwrap();
        Entry { key: TestKey(30), value: 3 }.write_to(&mut expected_node_data).unwrap();
        expected_node_data.resize(node_size, 0);
        assert_eq!(node_data, expected_node_data.as_slice());
    }

     #[test]
    fn test_build_two_level_tree() {
        let b: u16 = 2;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![
            Ok(Entry { key: TestKey(10), value: 1 }), Ok(Entry { key: TestKey(20), value: 2 }),
            Ok(Entry { key: TestKey(30), value: 3 }), Ok(Entry { key: TestKey(40), value: 4 }),
            Ok(Entry { key: TestKey(50), value: 5 }),
        ];
        let num_entries_expected = entries.len() as u64;

        assert!(builder.build_from_sorted(entries).is_ok());

        let mut buffer = cursor.into_inner();
        let header_size = DEFAULT_HEADER_RESERVATION as usize;
        let entry_size = 12;
        let key_size = 4;
        let leaf_node_size = b as usize * entry_size; // 24
        let internal_node_size = b as usize * key_size; // 8
        let num_leaf_nodes = 3;
        let num_internal1_nodes = 2; // Level above leaves
        let num_root_nodes = 1; // Level above internal1

        let expected_size = header_size
            + num_root_nodes * internal_node_size // Root written first
            + num_internal1_nodes * internal_node_size // Then Internal Level 1
            + num_leaf_nodes * leaf_node_size; // Then Leaves
        assert_eq!(buffer.len(), expected_size);

        let (version, bfactor, num_entries_hdr, height) =
            read_test_header(&mut Cursor::new(buffer.clone()));
        assert_eq!(version, FORMAT_VERSION);
        assert_eq!(bfactor, b);
        assert_eq!(num_entries_hdr, num_entries_expected);
        assert_eq!(height, 3);

        // --- Verify content based on new WRITE ORDER ---
        // Order: Header -> Root -> Internal Level 1 -> Leaves
        let root_start = header_size;
        let internal1_start = root_start + num_root_nodes * internal_node_size;
        let leaf_start = internal1_start + num_internal1_nodes * internal_node_size;

        // Check Root Node: [Key(10), Key(50)]
        let root_node_data = &buffer[root_start..(root_start + internal_node_size)];
        let mut expected_root_data = Vec::with_capacity(internal_node_size);
        TestKey(10).write_to(&mut expected_root_data).unwrap();
        TestKey(50).write_to(&mut expected_root_data).unwrap();
        assert_eq!(root_node_data, expected_root_data.as_slice());

        // Check Internal Level 1, Node 1: [Key(10), Key(30)]
        let internal1_node1_data = &buffer[internal1_start..(internal1_start + internal_node_size)];
        let mut expected_internal1_node1 = Vec::with_capacity(internal_node_size);
        TestKey(10).write_to(&mut expected_internal1_node1).unwrap();
        TestKey(30).write_to(&mut expected_internal1_node1).unwrap();
        assert_eq!(internal1_node1_data, expected_internal1_node1.as_slice());

        // Check Internal Level 1, Node 2: [Key(50), pad]
        let internal1_node2_data = &buffer
            [(internal1_start + internal_node_size)..(internal1_start + 2 * internal_node_size)];
        let mut expected_internal1_node2 = Vec::with_capacity(internal_node_size);
        TestKey(50).write_to(&mut expected_internal1_node2).unwrap();
        expected_internal1_node2.resize(internal_node_size, 0);
        assert_eq!(internal1_node2_data, expected_internal1_node2.as_slice());

        // Check Leaf 1: [Entry(10,1), Entry(20,2)]
        let leaf1_data = &buffer[leaf_start..(leaf_start + leaf_node_size)];
        let mut expected_leaf1 = Vec::with_capacity(leaf_node_size);
        Entry { key: TestKey(10), value: 1 }.write_to(&mut expected_leaf1).unwrap();
        Entry { key: TestKey(20), value: 2 }.write_to(&mut expected_leaf1).unwrap();
        assert_eq!(leaf1_data, expected_leaf1.as_slice());

        // Check Leaf 2: [Entry(30,3), Entry(40,4)]
        let leaf2_data = &buffer[(leaf_start + leaf_node_size)..(leaf_start + 2 * leaf_node_size)];
        let mut expected_leaf2 = Vec::with_capacity(leaf_node_size);
        Entry { key: TestKey(30), value: 3 }.write_to(&mut expected_leaf2).unwrap();
        Entry { key: TestKey(40), value: 4 }.write_to(&mut expected_leaf2).unwrap();
        assert_eq!(leaf2_data, expected_leaf2.as_slice());

        // Check Leaf 3: [Entry(50,5), pad]
        let leaf3_data =
            &buffer[(leaf_start + 2 * leaf_node_size)..(leaf_start + 3 * leaf_node_size)];
        let mut expected_leaf3 = Vec::with_capacity(leaf_node_size);
        Entry { key: TestKey(50), value: 5 }.write_to(&mut expected_leaf3).unwrap();
        expected_leaf3.resize(leaf_node_size, 0);
        assert_eq!(leaf3_data, expected_leaf3.as_slice());
    }

    #[test]
    fn test_build_leaves_unsorted_input() {
        let b: u16 = 3;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![
            Ok(Entry { key: TestKey(10), value: 1 }),
            Ok(Entry { key: TestKey(30), value: 3 }), // Out of order
            Ok(Entry { key: TestKey(20), value: 2 }),
        ];
        let result = builder.build_from_sorted(entries);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_empty_input() {
        let b: u16 = 3;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![];

        assert!(builder.build_from_sorted(entries).is_ok());

        let mut buffer = cursor.into_inner();
        assert_eq!(buffer.len() as u64, DEFAULT_HEADER_RESERVATION);
        let (_, _, num_entries_hdr, height) = read_test_header(&mut Cursor::new(buffer.clone()));
        assert_eq!(num_entries_hdr, 0);
        assert_eq!(height, 0);
    }
}
