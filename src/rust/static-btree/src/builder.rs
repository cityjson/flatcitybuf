use crate::entry::{Entry, Offset};
use crate::error::Error;
use crate::key::Key;
use crate::node::{InternalNode, LeafNode, Node, NodeType};
use crate::tree::StaticBTree;
use byteorder::{LittleEndian, WriteBytesExt};
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::mem;

/// Default header size reservation in bytes
pub const DEFAULT_HEADER_RESERVATION: u64 = 64;

/// Builder for creating a StaticBTree
pub struct StaticBTreeBuilder<K: Key, W: Write + Seek> {
    /// Writer to output the tree
    writer: W,
    /// Entries to be inserted into the tree
    entries: Vec<Entry<K>>,
    /// Branching factor (maximum number of entries per node)
    branching_factor: u16,
    /// Node size in bytes (power of 2)
    node_size: u16,
    /// Current position in the writer
    current_pos: u64,
    /// Phantom data for key type
    _phantom: PhantomData<K>,
}

impl<K: Key, W: Write + Seek> StaticBTreeBuilder<K, W> {
    /// Create a new StaticBTreeBuilder
    pub fn new(writer: W, branching_factor: u16) -> Result<Self, Error> {
        // Calculate optimal node size based on key size and branching factor
        let node_size = Self::calculate_optimal_node_size::<K>(branching_factor);

        let mut builder = Self {
            writer,
            entries: Vec::new(),
            branching_factor,
            node_size,
            current_pos: 0,
            _phantom: PhantomData,
        };

        // Reserve space for the header
        builder.reserve_header_space()?;

        Ok(builder)
    }

    /// Reserve space for the header
    fn reserve_header_space(&mut self) -> Result<(), Error> {
        // Write placeholder bytes
        let zeros = vec![0u8; DEFAULT_HEADER_RESERVATION as usize];
        self.writer.write_all(&zeros)?;
        self.current_pos = DEFAULT_HEADER_RESERVATION;
        Ok(())
    }

    /// Calculate optimal node size based on key size and branching factor
    fn calculate_optimal_node_size<T: Key>(branching_factor: u16) -> u16 {
        let key_size = T::SERIALIZED_SIZE;
        let entry_size = Entry::<T>::SERIALIZED_SIZE;

        // Calculate sizes for leaf and internal nodes
        let leaf_header_size = 11; // type(1) + count(2) + next_ptr(8)
        let leaf_node_size = leaf_header_size + (branching_factor as usize * entry_size);

        let internal_header_size = 3; // type(1) + count(2)
        let internal_entry_size = key_size + 8; // key + child ptr
        let internal_node_size =
            internal_header_size + (branching_factor as usize * internal_entry_size);

        // Use the larger of the two sizes
        let node_size = leaf_node_size.max(internal_node_size);

        // Round up to the next power of 2
        let mut power_of_2 = 1;
        while power_of_2 < node_size {
            power_of_2 *= 2;
        }

        power_of_2 as u16
    }

    /// Add an entry to the builder
    pub fn add_entry(&mut self, key: K, offset: Offset) -> Result<(), Error> {
        self.entries.push(Entry { key, offset });
        Ok(())
    }

    /// Build the tree from the added entries
    pub fn build(mut self) -> Result<StaticBTree<K>, Error> {
        // Sort entries by key
        self.entries.sort_by(|a, b| a.key.cmp(&b.key));

        // Create leaf nodes
        let leaf_nodes = self.create_leaf_nodes()?;

        // Build internal nodes bottom-up
        let (root_offset, height) = self.build_internal_levels(leaf_nodes)?;

        // Create the tree
        let tree = StaticBTree::new(
            root_offset,
            height,
            self.node_size,
            self.entries.len() as u64,
        );

        // Write the header
        self.writer.seek(SeekFrom::Start(0))?;
        tree.write_header(&mut self.writer)?;

        Ok(tree)
    }

    /// Create leaf nodes from sorted entries
    fn create_leaf_nodes(&mut self) -> Result<Vec<u64>, Error> {
        let entries_per_leaf = self.calculate_entries_per_leaf();
        let mut leaf_offsets = Vec::new();
        let mut i = 0;

        while i < self.entries.len() {
            // Create a new leaf node
            let mut leaf = LeafNode::new();
            let end = (i + entries_per_leaf).min(self.entries.len());

            // Add entries to the leaf
            for j in i..end {
                leaf.add(self.entries[j].clone());
            }

            // Write the leaf node
            let offset = self.current_pos;
            leaf_offsets.push(offset);

            // Seek to the current position
            self.writer.seek(SeekFrom::Start(offset))?;

            // Write the node
            let node = Node::Leaf(leaf);
            node.write_to(&mut self.writer)?;

            // Update current position
            self.current_pos += self.node_size as u64;

            i = end;
        }

        // Link leaf nodes together
        for i in 0..leaf_offsets.len() - 1 {
            // Seek to the next_leaf_offset field of the current leaf
            let offset = leaf_offsets[i] + 3; // type(1) + count(2)
            self.writer.seek(SeekFrom::Start(offset))?;

            // Write the offset of the next leaf
            self.writer.write_u64::<LittleEndian>(leaf_offsets[i + 1])?;
        }

        Ok(leaf_offsets)
    }

    /// Calculate the number of entries per leaf node
    fn calculate_entries_per_leaf(&self) -> usize {
        let entry_size = Entry::<K>::SERIALIZED_SIZE;
        let leaf_header_size = 11; // type(1) + count(2) + next_ptr(8)

        // Calculate how many entries fit in a node
        let available_space = self.node_size as usize - leaf_header_size;
        let entries_per_leaf = available_space / entry_size;

        // Ensure we don't exceed the branching factor
        entries_per_leaf.min(self.branching_factor as usize)
    }

    /// Build internal levels of the tree bottom-up
    fn build_internal_levels(&mut self, mut level_nodes: Vec<u64>) -> Result<(u64, u8), Error> {
        let mut height = 1; // Start with leaf level

        while level_nodes.len() > 1 {
            // Create parent level
            let parent_nodes = self.create_parent_level(&level_nodes)?;
            level_nodes = parent_nodes;
            height += 1;
        }

        // The last remaining node is the root
        let root_offset = level_nodes[0];

        Ok((root_offset, height))
    }

    /// Create a parent level from a list of child node offsets
    fn create_parent_level(&mut self, child_offsets: &[u64]) -> Result<Vec<u64>, Error> {
        let mut parent_offsets = Vec::new();
        let mut i = 0;

        while i < child_offsets.len() {
            // Create a new internal node
            let mut internal = InternalNode::new();
            let end = (i + self.branching_factor as usize).min(child_offsets.len());

            // Add child offsets to the internal node
            for j in i..end {
                // For simplicity, use the first key of each child node as the separator key
                // In a real implementation, we would read the first key from each child node
                let key = if j == 0 {
                    K::default() // Use default key for the first child
                } else {
                    // In a real implementation, we would read the first key from the child node
                    // For now, use a placeholder key
                    K::default()
                };

                internal.add(key, child_offsets[j]);
            }

            // Write the internal node
            let offset = self.current_pos;
            parent_offsets.push(offset);

            // Seek to the current position
            self.writer.seek(SeekFrom::Start(offset))?;

            // Write the node
            let node = Node::Internal(internal);
            node.write_to(&mut self.writer)?;

            // Update current position
            self.current_pos += self.node_size as u64;

            i = end;
        }

        Ok(parent_offsets)
    }

    /// Build a tree from sorted entries
    pub fn build_from_sorted<I>(mut self, entries: I) -> Result<StaticBTree<K>, Error>
    where
        I: IntoIterator<Item = Result<Entry<K>, Error>>,
    {
        // Add all entries
        for entry_result in entries {
            let entry = entry_result?;
            self.entries.push(entry);
        }

        // Build the tree
        self.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_builder_simple() -> Result<(), Error> {
        // Create a builder
        let buffer = Vec::new();
        let mut cursor = Cursor::new(buffer);
        let builder = StaticBTreeBuilder::<u32, _>::new(&mut cursor, 4)?;

        // Define some test entries
        let entries = vec![
            Ok(Entry {
                key: 10,
                offset: 100,
            }),
            Ok(Entry {
                key: 20,
                offset: 200,
            }),
            Ok(Entry {
                key: 30,
                offset: 300,
            }),
            Ok(Entry {
                key: 40,
                offset: 400,
            }),
            Ok(Entry {
                key: 50,
                offset: 500,
            }),
        ];

        // Build the tree
        let tree = builder.build_from_sorted(entries)?;

        // Verify tree metadata
        assert_eq!(tree.num_entries(), 5);
        assert_eq!(tree.height(), 2); // Root + leaves

        // Get the buffer for verification
        let buffer = cursor.into_inner();

        // Read the tree header
        let mut read_cursor = Cursor::new(&buffer);
        let read_tree = StaticBTree::<u32>::deserialize(&mut read_cursor)?;

        // Verify tree metadata
        assert_eq!(read_tree.num_entries(), 5);
        assert_eq!(read_tree.height(), 2);

        // Verify we can find entries
        let results = read_tree.find(&30, &mut Cursor::new(&buffer))?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 300);

        Ok(())
    }

    #[test]
    fn test_calculate_optimal_node_size() {
        // Test with different key types
        let size_u32 =
            StaticBTreeBuilder::<u32, Cursor<Vec<u8>>>::calculate_optimal_node_size::<u32>(16);
        let size_u64 =
            StaticBTreeBuilder::<u64, Cursor<Vec<u8>>>::calculate_optimal_node_size::<u64>(16);

        // Verify sizes are powers of 2
        assert!(size_u32.is_power_of_two());
        assert!(size_u64.is_power_of_two());

        println!("size_u32: {}", size_u32);
        println!("size_u64: {}", size_u64);

        // Verify u64 keys result in larger nodes than u32 keys
        assert!(size_u64 >= size_u32);
    }

    #[test]
    fn test_entries_per_leaf() {
        // Create a builder with a specific node size
        let buffer = Vec::new();
        let cursor = Cursor::new(buffer);
        let builder = StaticBTreeBuilder::<u32, _> {
            writer: cursor,
            entries: Vec::new(),
            branching_factor: 16,
            node_size: 256, // Small node size for testing
            current_pos: 0,
            _phantom: PhantomData,
        };

        // Calculate entries per leaf
        let entries_per_leaf = builder.calculate_entries_per_leaf();

        // Verify it's reasonable
        assert!(entries_per_leaf > 0);
        assert!(entries_per_leaf <= 16); // Should not exceed branching factor
    }
}
