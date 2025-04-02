use crate::entry::Entry;
use crate::error::Error;
use crate::key::Key;
use crate::Value; // Assuming Value is always u64
use std::io::{Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::mem;

// Constants for the header structure (adjust size as needed)
const MAGIC_BYTES: &[u8; 8] = b"STREE01\0";
const FORMAT_VERSION: u16 = 1;
const DEFAULT_HEADER_RESERVATION: u64 = 64; // Placeholder size, can be calculated more precisely

/// Builder structure for creating a StaticBTree file/data structure.
/// Writes to a `Write + Seek` target using a bottom-up approach.
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
    /// Stores the *first key* of each node written at the level below.
    promoted_keys_buffer: Vec<K>,
    /// Buffer for assembling the current node being written (contains serialized items).
    current_node_buffer: Vec<u8>,
    /// Tracks the number of items (entries or keys) added to the current node buffer.
    items_in_current_node: u16,
    /// Tracks the current write position (absolute offset from start of writer).
    current_offset: u64,
    /// Stores the first key of each node written at the *current* level being processed.
    /// This becomes the `promoted_keys_buffer` for the next level up.
    first_keys_of_current_level: Vec<K>,
    /// Stores calculated node counts per level during build (leaf level first).
    nodes_per_level_build: Vec<u64>,
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
    ///
    /// Reserves space for the header at the beginning of the writer.
    ///
    /// # Arguments
    /// * `writer`: The `Write + Seek` target for the tree data.
    /// * `branching_factor`: The desired number of keys/entries per node (must be > 1).
    ///
    /// # Returns
    /// `Ok(Self)` or `Err(Error)` if initialization fails (e.g., invalid branching factor, I/O error).
    pub fn new(mut writer: W, branching_factor: u16) -> Result<Self, Error> {
        // 1. Validate Branching Factor
        if branching_factor <= 1 {
            return Err(Error::BuildError(format!(
                "branching factor must be greater than 1, got {}",
                branching_factor
            )));
        }

        // 2. Calculate Sizes
        let key_size = K::SERIALIZED_SIZE;
        let value_size = mem::size_of::<Value>(); // Value is u64
        let entry_size = key_size + value_size;
        let internal_node_byte_size = branching_factor as usize * key_size;
        let leaf_node_byte_size = branching_factor as usize * entry_size;

        // Determine header size (can be fixed or calculated based on fields)
        let header_size = DEFAULT_HEADER_RESERVATION;

        // 3. Reserve Header Space
        writer.seek(SeekFrom::Start(0))?;
        let header_placeholder = vec![0u8; header_size as usize];
        writer.write_all(&header_placeholder)?;
        let current_offset = writer.stream_position()?; // Should be == header_size

        if current_offset != header_size {
            return Err(Error::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "failed to reserve header space correctly: expected offset {}, got {}",
                    header_size, current_offset
                ),
            )));
        }

        // 4. Initialize State
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
            current_offset,
            first_keys_of_current_level: Vec::new(),
            nodes_per_level_build: Vec::new(),
            key_size,
            entry_size,
            internal_node_byte_size,
            leaf_node_byte_size,
            _phantom_key: PhantomData,
        })
    }

    /// Builds the entire tree from an iterator providing pre-sorted entries.
    /// This is the primary and recommended method for construction.
    ///
    /// # Arguments
    /// * `sorted_entries`: An iterator yielding `Result<Entry<K>, Error>`.
    ///                    Entries **must** be strictly sorted by key.
    ///
    /// # Returns
    /// `Ok(())` on successful build.
    /// `Err(Error)` if input is invalid, I/O fails, or sorting errors detected.
    pub fn build_from_sorted<I>(mut self, sorted_entries: I) -> Result<(), Error>
    where
        I: IntoIterator<Item = Result<Entry<K>, Error>>,
    {
        let mut first_key_current_node: Option<K> = None;
        let mut last_key_processed: Option<K> = None;

        // --- Phase 1: Write Leaf Nodes & Collect First Keys ---
        for entry_result in sorted_entries {
            let entry = entry_result?;

            // Optional: Check sorting
            if let Some(ref last_key) = last_key_processed {
                if entry.key <= *last_key {
                    return Err(Error::BuildError(format!(
                        "input entries are not strictly sorted. key {:?} <= previous key {:?}",
                        entry.key, last_key
                    )));
                }
            }
            last_key_processed = Some(entry.key.clone());

            // Store first key if this is the start of a new node
            if self.items_in_current_node == 0 {
                first_key_current_node = Some(entry.key.clone());
            }

            // Add entry to current node buffer
            entry.write_to(&mut self.current_node_buffer)?;
            self.items_in_current_node += 1;
            self.num_entries += 1;

            // If node is full, write it out
            if self.items_in_current_node == self.branching_factor {
                self.write_current_node(self.leaf_node_byte_size)?; // Use leaf size
                                                                    // Promote the first key of the node we just wrote
                self.first_keys_of_current_level.push(
                    first_key_current_node
                        .take() // take ownership, leaving None
                        .ok_or_else(|| {
                            Error::BuildError(
                                "internal error: missing first key for full node".to_string(),
                            )
                        })?,
                );
                // Reset for next node
                self.items_in_current_node = 0;
                self.current_node_buffer.clear();
            }
        }

        // Handle the last potentially partial leaf node
        if self.items_in_current_node > 0 {
            self.pad_and_write_current_node(self.leaf_node_byte_size)?; // Use leaf size
            self.first_keys_of_current_level
                .push(first_key_current_node.take().ok_or_else(|| {
                    Error::BuildError(
                        "internal error: missing first key for partial node".to_string(),
                    )
                })?);
            // Reset state just in case (though not strictly needed before next phase)
            self.items_in_current_node = 0;
            self.current_node_buffer.clear();
        } else if self.num_entries == 0 {
            // Handle empty input case - write empty tree header later?
            // For now, assume build_from_sorted requires at least one entry or handle in finalization.
            println!("warning: building tree from empty input iterator");
        }

        // Record leaf level info
        self.nodes_per_level_build
            .push(self.first_keys_of_current_level.len() as u64);
        // The keys collected from the leaf level are the ones to be promoted
        self.promoted_keys_buffer = std::mem::take(&mut self.first_keys_of_current_level); // Efficiently move Vec

        // --- Phase 2..N: Write Internal Nodes (Bottom-Up) ---
        // (To be implemented next)

        // --- Phase N+1: Finalization ---
        // (To be implemented last)
        // Calculate Height
        // Seek to Start
        // Write Final Header
        // Flush Writer

        // Placeholder until fully implemented
        if self.num_entries > 0 && self.promoted_keys_buffer.is_empty() {
            // This should not happen if there were entries unless branching factor is huge
            return Err(Error::BuildError(
                "internal error: no keys promoted from leaf level".to_string(),
            ));
        }

        // For now, just flush what we have (leaf nodes)
        self.writer.flush()?;
        println!(
            "debug: finished writing leaf nodes. count: {}, promoted keys: {}",
            self.nodes_per_level_build.last().unwrap_or(&0),
            self.promoted_keys_buffer.len()
        );

        // Return NotImplemented until the rest is done
        Err(Error::NotImplemented(
            "build_from_sorted (internal nodes and finalization)".to_string(),
        ))
        // Ok(()) // Final return when complete
    }

    /// Helper to write the current node buffer and update offset.
    /// Assumes the buffer is exactly the correct size (e.g., already padded or full).
    fn write_current_node(&mut self, expected_node_size: usize) -> Result<(), Error> {
        if self.current_node_buffer.len() != expected_node_size {
            // This indicates an internal logic error if called incorrectly
            return Err(Error::BuildError(format!(
                "internal error: buffer size {} does not match expected node size {}",
                self.current_node_buffer.len(),
                expected_node_size
            )));
        }
        self.writer.write_all(&self.current_node_buffer)?;
        self.current_offset += expected_node_size as u64;
        Ok(())
    }

    /// Helper to pad the current node buffer to the expected size and write it.
    fn pad_and_write_current_node(&mut self, expected_node_size: usize) -> Result<(), Error> {
        if self.current_node_buffer.len() > expected_node_size {
            return Err(Error::BuildError(format!(
                "internal error: buffer size {} exceeds expected node size {}",
                self.current_node_buffer.len(),
                expected_node_size
            )));
        }
        // Pad with zeros if needed
        let padding_needed = expected_node_size - self.current_node_buffer.len();
        if padding_needed > 0 {
            self.current_node_buffer
                .extend(std::iter::repeat(0).take(padding_needed));
        }
        self.write_current_node(expected_node_size) // Now buffer has the correct size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry; // Import Entry for testing build
    use crate::key::Key;
    use std::io::{Cursor, Read};

    // Re-use TestKey from entry.rs tests
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

    #[test]
    fn test_builder_new_valid() {
        let cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(cursor, 10);
        assert!(builder.is_ok());
        let builder = builder.unwrap();
        assert_eq!(builder.branching_factor, 10);
        assert_eq!(builder.num_entries, 0);
        assert_eq!(builder.current_offset, DEFAULT_HEADER_RESERVATION);
        assert_eq!(builder.header_size, DEFAULT_HEADER_RESERVATION);
        assert_eq!(builder.key_size, 4);
        assert_eq!(builder.entry_size, 4 + 8);
        assert_eq!(builder.internal_node_byte_size, 10 * 4);
        assert_eq!(builder.leaf_node_byte_size, 10 * (4 + 8));

        let writer = builder.writer;
        let buffer = writer.into_inner();
        assert_eq!(buffer.len() as u64, DEFAULT_HEADER_RESERVATION);
        assert!(buffer.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_builder_new_invalid_branching_factor() {
        let cursor = Cursor::new(Vec::new());
        let result = StaticBTreeBuilder::<TestKey, _>::new(cursor, 0);
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::BuildError(msg) => {
                assert!(msg.contains("branching factor must be greater than 1"))
            }
            _ => panic!("Expected BuildError"),
        }

        let cursor = Cursor::new(Vec::new());
        let result = StaticBTreeBuilder::<TestKey, _>::new(cursor, 1);
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::BuildError(msg) => {
                assert!(msg.contains("branching factor must be greater than 1"))
            }
            _ => panic!("Expected BuildError"),
        }
    }

    // --- Tests for build_from_sorted (Leaf Node Phase) ---

    #[test]
    fn test_build_leaves_single_full_node() {
        let b: u16 = 3;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![
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

        // Call build (expect NotImplemented error for now, but check state after leaves)
        let result = builder.build_from_sorted(entries);
        assert!(result.is_err());
        // We need access to the builder state *after* the leaf phase, which is tricky
        // because build_from_sorted consumes self. Let's rethink testing this incrementally.

        // Alternative: Test helper methods or refactor build_from_sorted later for testability.
        // For now, let's manually check the expected output buffer content after leaves.

        let buffer = cursor.into_inner();
        let header_size = DEFAULT_HEADER_RESERVATION as usize;
        let entry_size = TestKey::SERIALIZED_SIZE + mem::size_of::<Value>(); // 4 + 8 = 12
        let node_size = b as usize * entry_size; // 3 * 12 = 36

        // Check total size (header + one leaf node)
        assert_eq!(buffer.len(), header_size + node_size);

        // Check leaf node content (after header)
        let node_data = &buffer[header_size..];
        let mut expected_node_data = Vec::with_capacity(node_size);
        Entry {
            key: TestKey(10),
            value: 1,
        }
        .write_to(&mut expected_node_data)
        .unwrap();
        Entry {
            key: TestKey(20),
            value: 2,
        }
        .write_to(&mut expected_node_data)
        .unwrap();
        Entry {
            key: TestKey(30),
            value: 3,
        }
        .write_to(&mut expected_node_data)
        .unwrap();
        assert_eq!(node_data, expected_node_data.as_slice());

        // How to check promoted keys? Need to modify builder or test structure.
    }

    #[test]
    fn test_build_leaves_partial_last_node() {
        let b: u16 = 4;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![
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
            }), // Start of partial node
        ];

        let result = builder.build_from_sorted(entries);
        assert!(result.is_err()); // Still expect NotImplemented

        let buffer = cursor.into_inner();
        let header_size = DEFAULT_HEADER_RESERVATION as usize;
        let entry_size = 12;
        let node_size = b as usize * entry_size; // 4 * 12 = 48

        // Check total size (header + one full node + one padded partial node)
        assert_eq!(buffer.len(), header_size + node_size + node_size);

        // Check first node content
        let node1_data = &buffer[header_size..(header_size + node_size)];
        let mut expected_node1_data = Vec::with_capacity(node_size);
        Entry {
            key: TestKey(10),
            value: 1,
        }
        .write_to(&mut expected_node1_data)
        .unwrap();
        Entry {
            key: TestKey(20),
            value: 2,
        }
        .write_to(&mut expected_node1_data)
        .unwrap();
        Entry {
            key: TestKey(30),
            value: 3,
        }
        .write_to(&mut expected_node1_data)
        .unwrap();
        Entry {
            key: TestKey(40),
            value: 4,
        }
        .write_to(&mut expected_node1_data)
        .unwrap();
        assert_eq!(node1_data, expected_node1_data.as_slice());

        // Check second (padded) node content
        let node2_data = &buffer[(header_size + node_size)..];
        let mut expected_node2_data = Vec::with_capacity(node_size);
        Entry {
            key: TestKey(50),
            value: 5,
        }
        .write_to(&mut expected_node2_data)
        .unwrap();
        expected_node2_data.resize(node_size, 0); // Pad with zeros
        assert_eq!(node2_data, expected_node2_data.as_slice());
    }

    #[test]
    fn test_build_leaves_multiple_nodes() {
        let b: u16 = 2;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![
            Ok(Entry {
                key: TestKey(10),
                value: 1,
            }), // Node 1
            Ok(Entry {
                key: TestKey(20),
                value: 2,
            }),
            Ok(Entry {
                key: TestKey(30),
                value: 3,
            }), // Node 2
            Ok(Entry {
                key: TestKey(40),
                value: 4,
            }),
            Ok(Entry {
                key: TestKey(50),
                value: 5,
            }), // Node 3 (partial)
        ];

        let result = builder.build_from_sorted(entries);
        assert!(result.is_err()); // Still expect NotImplemented

        let buffer = cursor.into_inner();
        let header_size = DEFAULT_HEADER_RESERVATION as usize;
        let entry_size = 12;
        let node_size = b as usize * entry_size; // 2 * 12 = 24

        // Check total size (header + 3 nodes)
        assert_eq!(buffer.len(), header_size + 3 * node_size);

        // Check node 1
        let node1_data = &buffer[header_size..(header_size + node_size)];
        let mut expected_node1_data = Vec::with_capacity(node_size);
        Entry {
            key: TestKey(10),
            value: 1,
        }
        .write_to(&mut expected_node1_data)
        .unwrap();
        Entry {
            key: TestKey(20),
            value: 2,
        }
        .write_to(&mut expected_node1_data)
        .unwrap();
        assert_eq!(node1_data, expected_node1_data.as_slice());

        // Check node 2
        let node2_data = &buffer[(header_size + node_size)..(header_size + 2 * node_size)];
        let mut expected_node2_data = Vec::with_capacity(node_size);
        Entry {
            key: TestKey(30),
            value: 3,
        }
        .write_to(&mut expected_node2_data)
        .unwrap();
        Entry {
            key: TestKey(40),
            value: 4,
        }
        .write_to(&mut expected_node2_data)
        .unwrap();
        assert_eq!(node2_data, expected_node2_data.as_slice());

        // Check node 3 (padded)
        let node3_data = &buffer[(header_size + 2 * node_size)..];
        let mut expected_node3_data = Vec::with_capacity(node_size);
        Entry {
            key: TestKey(50),
            value: 5,
        }
        .write_to(&mut expected_node3_data)
        .unwrap();
        expected_node3_data.resize(node_size, 0); // Pad
        assert_eq!(node3_data, expected_node3_data.as_slice());
    }

    #[test]
    fn test_build_leaves_unsorted_input() {
        let b: u16 = 3;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![
            Ok(Entry {
                key: TestKey(10),
                value: 1,
            }),
            Ok(Entry {
                key: TestKey(30),
                value: 3,
            }), // Out of order
            Ok(Entry {
                key: TestKey(20),
                value: 2,
            }),
        ];

        let result = builder.build_from_sorted(entries);
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::BuildError(msg) => assert!(msg.contains("not strictly sorted")),
            _ => panic!("Expected BuildError for unsorted input"),
        }
    }

    #[test]
    fn test_build_leaves_empty_input() {
        let b: u16 = 3;
        let mut cursor = Cursor::new(Vec::new());
        let builder = StaticBTreeBuilder::<TestKey, _>::new(&mut cursor, b).unwrap();
        let entries: Vec<Result<Entry<TestKey>, Error>> = vec![];

        // Currently returns NotImplemented, but should ideally handle empty input gracefully
        let result = builder.build_from_sorted(entries);
        assert!(result.is_err()); // Expect NotImplemented for now
                                  // TODO: When fully implemented, this should likely succeed and write a header for an empty tree.

        let buffer = cursor.into_inner();
        let header_size = DEFAULT_HEADER_RESERVATION as usize;
        // Buffer should only contain the reserved header
        assert_eq!(buffer.len(), header_size);
    }
}
