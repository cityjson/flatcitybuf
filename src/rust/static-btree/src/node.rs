use crate::entry::{Entry, Offset};
use crate::error::Error;
use crate::key::Key;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::cmp::Ordering;
use std::io::{Cursor, Read, Write};
use std::marker::PhantomData;

/// Node type identifier (1 byte)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    /// Internal node (contains keys and child pointers)
    Internal = 0,
    /// Leaf node (contains entries)
    Leaf = 1,
}

impl NodeType {
    /// Convert from u8 to NodeType
    pub fn from_u8(value: u8) -> Result<Self, Error> {
        match value {
            0 => Ok(NodeType::Internal),
            1 => Ok(NodeType::Leaf),
            _ => Err(Error::InvalidFormat(format!(
                "Invalid node type: {}",
                value
            ))),
        }
    }
}

/// Internal node containing keys and pointers to child nodes
#[derive(Debug, Clone)]
pub struct InternalNode<K: Key> {
    /// Keys for guiding the search
    pub keys: Vec<K>,
    /// Offsets to child nodes
    pub child_offsets: Vec<Offset>,
    /// Phantom data for key type
    _phantom: PhantomData<K>,
}

impl<K: Key> InternalNode<K> {
    /// Create a new empty internal node
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            child_offsets: Vec::new(),
            _phantom: PhantomData,
        }
    }

    /// Create a new internal node with the given capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            keys: Vec::with_capacity(capacity),
            child_offsets: Vec::with_capacity(capacity),
            _phantom: PhantomData,
        }
    }

    /// Add a key and child offset to the node
    pub fn add(&mut self, key: K, child_offset: Offset) {
        self.keys.push(key);
        self.child_offsets.push(child_offset);
    }

    /// Binary search to find the appropriate child for a key
    pub fn binary_search(&self, key: &K) -> usize {
        let mut low = 0;
        let mut high = self.keys.len();

        while low < high {
            let mid = low + (high - low) / 2;
            match self.keys[mid].cmp(key) {
                Ordering::Less => low = mid + 1,
                _ => high = mid,
            }
        }

        // If we're at the end, use the last child
        if low == self.keys.len() && !self.keys.is_empty() {
            low = self.keys.len() - 1;
        }

        low
    }

    /// Write the node to a writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        // Write node type
        writer.write_u8(NodeType::Internal as u8)?;

        // Write count
        writer.write_u16::<LittleEndian>(self.keys.len() as u16)?;

        // Write interleaved keys and pointers for better cache locality
        for i in 0..self.keys.len() {
            self.keys[i].write_to(writer)?;
            writer.write_u64::<LittleEndian>(self.child_offsets[i])?;
        }

        Ok(())
    }

    /// Read an internal node from a reader
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error> {
        // Read count
        let count = reader.read_u16::<LittleEndian>()?;

        // Create node with capacity
        let mut node = InternalNode::with_capacity(count as usize);

        // Read interleaved keys and pointers
        for _ in 0..count {
            let key = K::read_from(reader)?;
            let offset = reader.read_u64::<LittleEndian>()?;
            node.add(key, offset);
        }

        Ok(node)
    }
}

/// Leaf node containing entries (key-value pairs)
#[derive(Debug, Clone)]
pub struct LeafNode<K: Key> {
    /// Entries (key-value pairs)
    pub entries: Vec<Entry<K>>,
    /// Pointer to the next leaf node (for range queries)
    pub next_leaf_offset: Option<Offset>,
    /// Phantom data for key type
    _phantom: PhantomData<K>,
}

impl<K: Key> LeafNode<K> {
    /// Create a new empty leaf node
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_leaf_offset: None,
            _phantom: PhantomData,
        }
    }

    /// Create a new leaf node with the given capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            next_leaf_offset: None,
            _phantom: PhantomData,
        }
    }

    /// Add an entry to the node
    pub fn add(&mut self, entry: Entry<K>) {
        self.entries.push(entry);
    }

    /// Binary search to find an entry with the given key
    pub fn binary_search(&self, key: &K) -> Result<usize, usize> {
        self.entries.binary_search_by(|entry| entry.key.cmp(key))
    }

    /// Find all entries with the given key
    pub fn find_all(&self, key: &K) -> Vec<Offset> {
        let mut results = Vec::new();

        // Binary search to find any match
        match self.binary_search(key) {
            Ok(idx) => {
                // Found a match, now collect all duplicates

                // Scan backward
                let mut i = idx;
                while i > 0 && self.entries[i - 1].key == *key {
                    i -= 1;
                }

                // Scan forward and collect all matches
                while i < self.entries.len() && self.entries[i].key == *key {
                    results.push(self.entries[i].offset);
                    i += 1;
                }
            }
            Err(_) => {} // No match found
        }

        results
    }

    /// Collect all entries in the given range
    pub fn collect_in_range(
        &self,
        start: &K,
        end: &K,
        include_start: bool,
        include_end: bool,
    ) -> Vec<Offset> {
        let mut results = Vec::new();

        for entry in &self.entries {
            let in_range = match (include_start, include_end) {
                (true, true) => entry.key >= *start && entry.key <= *end,
                (true, false) => entry.key >= *start && entry.key < *end,
                (false, true) => entry.key > *start && entry.key <= *end,
                (false, false) => entry.key > *start && entry.key < *end,
            };

            if in_range {
                results.push(entry.offset);
            }

            // Early exit if we've passed the end
            if entry.key > *end {
                break;
            }
        }

        results
    }

    /// Write the node to a writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        // Write node type
        writer.write_u8(NodeType::Leaf as u8)?;

        // Write count
        writer.write_u16::<LittleEndian>(self.entries.len() as u16)?;

        // Write next leaf pointer
        match self.next_leaf_offset {
            Some(offset) => writer.write_u64::<LittleEndian>(offset)?,
            None => writer.write_u64::<LittleEndian>(0)?, // 0 means no next leaf
        }

        // Write entries
        for entry in &self.entries {
            entry.write_to(writer)?;
        }

        Ok(())
    }

    /// Read a leaf node from a reader
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error> {
        // Read count
        let count = reader.read_u16::<LittleEndian>()?;

        // Read next leaf pointer
        let next_offset = reader.read_u64::<LittleEndian>()?;
        let next_leaf_offset = if next_offset == 0 {
            None
        } else {
            Some(next_offset)
        };

        // Create node with capacity
        let mut node = LeafNode::with_capacity(count as usize);
        node.next_leaf_offset = next_leaf_offset;

        // Read entries
        for _ in 0..count {
            let entry = Entry::read_from(reader)?;
            node.add(entry);
        }

        Ok(node)
    }
}

/// Enum representing either an internal node or a leaf node
#[derive(Debug, Clone)]
pub enum Node<K: Key> {
    /// Internal node
    Internal(InternalNode<K>),
    /// Leaf node
    Leaf(LeafNode<K>),
}

impl<K: Key> Node<K> {
    /// Returns the type of this node
    pub fn node_type(&self) -> NodeType {
        match self {
            Node::Internal(_) => NodeType::Internal,
            Node::Leaf(_) => NodeType::Leaf,
        }
    }

    /// Returns true if this is a leaf node
    pub fn is_leaf(&self) -> bool {
        matches!(self, Node::Leaf(_))
    }

    /// Get a reference to the internal node if this is an internal node
    pub fn as_internal(&self) -> Option<&InternalNode<K>> {
        match self {
            Node::Internal(node) => Some(node),
            _ => None,
        }
    }

    /// Get a reference to the leaf node if this is a leaf node
    pub fn as_leaf(&self) -> Option<&LeafNode<K>> {
        match self {
            Node::Leaf(node) => Some(node),
            _ => None,
        }
    }

    /// Write the node to a writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        match self {
            Node::Internal(node) => node.write_to(writer),
            Node::Leaf(node) => node.write_to(writer),
        }
    }

    /// Read a node from a reader
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, Error> {
        // Read node type
        let node_type = NodeType::from_u8(reader.read_u8()?)?;

        // Read the rest of the node based on its type
        match node_type {
            NodeType::Internal => {
                let node = InternalNode::read_from(reader)?;
                Ok(Node::Internal(node))
            }
            NodeType::Leaf => {
                let node = LeafNode::read_from(reader)?;
                Ok(Node::Leaf(node))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;
    use std::io::Cursor;

    #[test]
    fn test_internal_node_serialization() {
        let mut node = InternalNode::<u32>::new();
        node.add(10, 100);
        node.add(20, 200);
        node.add(30, 300);

        let mut buffer = Vec::new();
        node.write_to(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let node_type = NodeType::from_u8(cursor.read_u8().unwrap()).unwrap();
        assert_eq!(node_type, NodeType::Internal);

        let deserialized = InternalNode::<u32>::read_from(&mut cursor).unwrap();

        assert_eq!(deserialized.keys.len(), 3);
        assert_eq!(deserialized.child_offsets.len(), 3);
        assert_eq!(deserialized.keys[0], 10);
        assert_eq!(deserialized.keys[1], 20);
        assert_eq!(deserialized.keys[2], 30);
        assert_eq!(deserialized.child_offsets[0], 100);
        assert_eq!(deserialized.child_offsets[1], 200);
        assert_eq!(deserialized.child_offsets[2], 300);
    }

    #[test]
    fn test_leaf_node_serialization() {
        let mut node = LeafNode::<u32>::new();
        node.add(Entry {
            key: 10,
            offset: 100,
        });
        node.add(Entry {
            key: 20,
            offset: 200,
        });
        node.add(Entry {
            key: 30,
            offset: 300,
        });
        node.next_leaf_offset = Some(400);

        let mut buffer = Vec::new();
        node.write_to(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let node_type = NodeType::from_u8(cursor.read_u8().unwrap()).unwrap();
        assert_eq!(node_type, NodeType::Leaf);

        let deserialized = LeafNode::<u32>::read_from(&mut cursor).unwrap();

        assert_eq!(deserialized.entries.len(), 3);
        assert_eq!(deserialized.next_leaf_offset, Some(400));
        assert_eq!(deserialized.entries[0].key, 10);
        assert_eq!(deserialized.entries[0].offset, 100);
        assert_eq!(deserialized.entries[1].key, 20);
        assert_eq!(deserialized.entries[1].offset, 200);
        assert_eq!(deserialized.entries[2].key, 30);
        assert_eq!(deserialized.entries[2].offset, 300);
    }

    #[test]
    fn test_node_enum_serialization() {
        // Create and serialize an internal node
        let mut internal = InternalNode::<u32>::new();
        internal.add(10, 100);
        internal.add(20, 200);

        let node = Node::Internal(internal);

        let mut buffer = Vec::new();
        node.write_to(&mut buffer).unwrap();

        // Read it back
        let mut cursor = Cursor::new(buffer);
        let deserialized = Node::<u32>::read_from(&mut cursor).unwrap();

        // Check type and cast
        assert_eq!(deserialized.node_type(), NodeType::Internal);
        let internal_node = deserialized.as_internal().unwrap();
        assert_eq!(internal_node.keys.len(), 2);
        assert_eq!(internal_node.keys[0], 10);

        // Create and serialize a leaf node
        let mut leaf = LeafNode::<u32>::new();
        leaf.add(Entry {
            key: 10,
            offset: 100,
        });

        let node = Node::Leaf(leaf);

        let mut buffer = Vec::new();
        node.write_to(&mut buffer).unwrap();

        // Read it back
        let mut cursor = Cursor::new(buffer);
        let deserialized = Node::<u32>::read_from(&mut cursor).unwrap();

        // Check type and cast
        assert_eq!(deserialized.node_type(), NodeType::Leaf);
        let leaf_node = deserialized.as_leaf().unwrap();
        assert_eq!(leaf_node.entries.len(), 1);
        assert_eq!(leaf_node.entries[0].key, 10);
    }

    #[test]
    fn test_internal_node_binary_search() {
        let mut node = InternalNode::<u32>::new();
        node.add(10, 100);
        node.add(20, 200);
        node.add(30, 300);

        assert_eq!(node.binary_search(&5), 0);
        assert_eq!(node.binary_search(&10), 0);
        assert_eq!(node.binary_search(&15), 1);
        assert_eq!(node.binary_search(&20), 1);
        assert_eq!(node.binary_search(&25), 2);
        assert_eq!(node.binary_search(&30), 2);
        assert_eq!(node.binary_search(&35), 2);
    }

    #[test]
    fn test_leaf_node_binary_search() {
        let mut node = LeafNode::<u32>::new();
        node.add(Entry {
            key: 10,
            offset: 100,
        });
        node.add(Entry {
            key: 20,
            offset: 200,
        });
        node.add(Entry {
            key: 30,
            offset: 300,
        });

        assert_eq!(node.binary_search(&5), Err(0));
        assert_eq!(node.binary_search(&10), Ok(0));
        assert_eq!(node.binary_search(&15), Err(1));
        assert_eq!(node.binary_search(&20), Ok(1));
        assert_eq!(node.binary_search(&25), Err(2));
        assert_eq!(node.binary_search(&30), Ok(2));
        assert_eq!(node.binary_search(&35), Err(3));
    }

    #[test]
    fn test_leaf_node_find_all() {
        let mut node = LeafNode::<u32>::new();
        node.add(Entry {
            key: 10,
            offset: 100,
        });
        node.add(Entry {
            key: 10,
            offset: 101,
        }); // Duplicate key
        node.add(Entry {
            key: 20,
            offset: 200,
        });
        node.add(Entry {
            key: 30,
            offset: 300,
        });
        node.add(Entry {
            key: 30,
            offset: 301,
        }); // Duplicate key
        node.add(Entry {
            key: 30,
            offset: 302,
        }); // Duplicate key

        let results = node.find_all(&10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], 100);
        assert_eq!(results[1], 101);

        let results = node.find_all(&20);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 200);

        let results = node.find_all(&30);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], 300);
        assert_eq!(results[1], 301);
        assert_eq!(results[2], 302);

        let results = node.find_all(&40);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_leaf_node_collect_in_range() {
        let mut node = LeafNode::<u32>::new();
        node.add(Entry {
            key: 10,
            offset: 100,
        });
        node.add(Entry {
            key: 10,
            offset: 101,
        }); // Duplicate key
        node.add(Entry {
            key: 20,
            offset: 200,
        });
        node.add(Entry {
            key: 30,
            offset: 300,
        });
        node.add(Entry {
            key: 30,
            offset: 301,
        }); // Duplicate key
        node.add(Entry {
            key: 30,
            offset: 302,
        }); // Duplicate key
        node.add(Entry {
            key: 40,
            offset: 400,
        });

        // Inclusive range [10, 30]
        let results = node.collect_in_range(&10, &30, true, true);
        assert_eq!(results.len(), 6);
        assert_eq!(results[0], 100);
        assert_eq!(results[1], 101);
        assert_eq!(results[2], 200);
        assert_eq!(results[3], 300);
        assert_eq!(results[4], 301);
        assert_eq!(results[5], 302);

        // Exclusive range (10, 30)
        let results = node.collect_in_range(&10, &30, false, false);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 200);

        // Half-inclusive range [10, 30)
        let results = node.collect_in_range(&10, &30, true, false);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], 100);
        assert_eq!(results[1], 101);
        assert_eq!(results[2], 200);

        // Half-inclusive range (10, 30]
        let results = node.collect_in_range(&10, &30, false, true);
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], 200);
        assert_eq!(results[1], 300);
        assert_eq!(results[2], 301);
        assert_eq!(results[3], 302);

        // Empty range
        let results = node.collect_in_range(&50, &60, true, true);
        assert_eq!(results.len(), 0);
    }
}
