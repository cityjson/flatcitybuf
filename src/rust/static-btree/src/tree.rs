use crate::entry::{Entry, Offset};
use crate::error::Error;
use crate::key::Key;
use crate::node::{Node, NodeType};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::mem;

/// Magic bytes to identify a StaticBTree file (SBTREE)
const MAGIC_BYTES: [u8; 6] = [b'S', b'B', b'T', b'R', b'E', b'E'];

/// Current version of the StaticBTree format
const FORMAT_VERSION: u16 = 1;

/// Static B+Tree implementation optimized for read-only access
#[derive(Debug)]
pub struct StaticBTree<K: Key> {
    /// Offset to the root node
    root_offset: u64,
    /// Height of the tree (1 = only root, 2 = root + leaves, etc.)
    height: u8,
    /// Node size in bytes (power of 2)
    node_size: u16,
    /// Total number of entries
    num_entries: u64,
    /// Phantom data for key type
    _phantom: PhantomData<K>,
}

impl<K: Key> StaticBTree<K> {
    /// Create a new StaticBTree
    pub fn new(root_offset: u64, height: u8, node_size: u16, num_entries: u64) -> Self {
        Self {
            root_offset,
            height,
            node_size,
            num_entries,
            _phantom: PhantomData,
        }
    }

    /// Get the root node offset
    pub fn root_offset(&self) -> u64 {
        self.root_offset
    }

    /// Get the height of the tree
    pub fn height(&self) -> u8 {
        self.height
    }

    /// Get the node size in bytes
    pub fn node_size(&self) -> u16 {
        self.node_size
    }

    /// Get the total number of entries
    pub fn num_entries(&self) -> u64 {
        self.num_entries
    }

    /// Serialize the tree header to a writer
    pub fn write_header<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        // Write magic bytes
        writer.write_all(&MAGIC_BYTES)?;

        // Write format version
        writer.write_u16::<LittleEndian>(FORMAT_VERSION)?;

        // Write tree metadata
        writer.write_u64::<LittleEndian>(self.root_offset)?;
        writer.write_u8(self.height)?;
        writer.write_u16::<LittleEndian>(self.node_size)?;
        writer.write_u64::<LittleEndian>(self.num_entries)?;

        Ok(())
    }

    /// Deserialize a tree header from a reader
    pub fn read_header<R: Read>(reader: &mut R) -> Result<Self, Error> {
        // Read and verify magic bytes
        let mut magic = [0u8; 6];
        reader.read_exact(&mut magic)?;

        if magic != MAGIC_BYTES {
            return Err(Error::InvalidFormat("Invalid magic bytes".to_string()));
        }

        // Read and verify format version
        let version = reader.read_u16::<LittleEndian>()?;
        if version != FORMAT_VERSION {
            return Err(Error::InvalidFormat(format!(
                "Unsupported format version: {}",
                version
            )));
        }

        // Read tree metadata
        let root_offset = reader.read_u64::<LittleEndian>()?;
        let height = reader.read_u8()?;
        let node_size = reader.read_u16::<LittleEndian>()?;
        let num_entries = reader.read_u64::<LittleEndian>()?;

        Ok(Self {
            root_offset,
            height,
            node_size,
            num_entries,
            _phantom: PhantomData,
        })
    }

    /// Find a key in the tree
    pub fn find<R: Read + Seek>(&self, key: &K, reader: &mut R) -> Result<Vec<Offset>, Error> {
        // Start at root node
        let mut node_offset = self.root_offset;

        // Traverse the tree
        for _ in 0..self.height - 1 {
            // Read the node
            reader.seek(SeekFrom::Start(node_offset))?;
            let node = Node::read_from(reader)?;

            if !node.is_leaf() {
                // Find child node to follow
                if let Some(internal) = node.as_internal() {
                    let idx = internal.binary_search(key);
                    node_offset = internal.child_offsets[idx];
                } else {
                    return Err(Error::InvalidNodeType);
                }
            } else {
                return Err(Error::InvalidNodeType);
            }
        }

        // Read the leaf node
        reader.seek(SeekFrom::Start(node_offset))?;
        let node = Node::read_from(reader)?;

        if node.is_leaf() {
            if let Some(leaf) = node.as_leaf() {
                // Find all entries with matching key
                Ok(leaf.find_all(key))
            } else {
                Err(Error::InvalidNodeType)
            }
        } else {
            Err(Error::InvalidNodeType)
        }
    }

    /// Find the leaf node containing a key
    fn find_leaf_containing<R: Read + Seek>(&self, key: &K, reader: &mut R) -> Result<u64, Error> {
        // Start at root node
        let mut node_offset = self.root_offset;

        // Traverse the tree
        for _ in 0..self.height - 1 {
            // Read the node
            reader.seek(SeekFrom::Start(node_offset))?;
            let node = Node::read_from(reader)?;

            if !node.is_leaf() {
                // Find child node to follow
                if let Some(internal) = node.as_internal() {
                    let idx = internal.binary_search(key);
                    node_offset = internal.child_offsets[idx];
                } else {
                    return Err(Error::InvalidNodeType);
                }
            } else {
                return Err(Error::InvalidNodeType);
            }
        }

        Ok(node_offset)
    }

    /// Execute a range query
    pub fn range<R: Read + Seek>(
        &self,
        start: &K,
        end: &K,
        include_start: bool,
        include_end: bool,
        reader: &mut R,
    ) -> Result<Vec<Offset>, Error> {
        // Find leaf containing start key
        let leaf_offset = self.find_leaf_containing(start, reader)?;
        let mut results = Vec::new();

        // Scan leaf nodes
        let mut current_offset = Some(leaf_offset);
        while let Some(offset) = current_offset {
            // Read the leaf node
            reader.seek(SeekFrom::Start(offset))?;
            let node = Node::read_from(reader)?;

            if node.is_leaf() {
                if let Some(leaf) = node.as_leaf() {
                    // Add entries in range
                    let mut entries = leaf.collect_in_range(start, end, include_start, include_end);
                    results.append(&mut entries);

                    // Stop if we've passed the end
                    if leaf.entries.last().map_or(false, |e| e.key > *end) {
                        break;
                    }

                    // Move to next leaf
                    current_offset = leaf.next_leaf_offset;
                } else {
                    return Err(Error::InvalidNodeType);
                }
            } else {
                return Err(Error::InvalidNodeType);
            }
        }

        Ok(results)
    }

    /// Serialize the entire tree to a writer
    pub fn serialize<W: Write + Seek>(&self, writer: &mut W) -> Result<(), Error> {
        // Write header
        self.write_header(writer)?;

        // The rest of the tree is already serialized in the reader
        // This method is mainly for completeness

        Ok(())
    }

    /// Deserialize a tree from a reader
    pub fn deserialize<R: Read + Seek>(reader: &mut R) -> Result<Self, Error> {
        // Read header
        Self::read_header(reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{InternalNode, LeafNode, Node};
    use std::io::Cursor;

    // Helper function to create a simple tree for testing
    fn create_test_tree() -> (StaticBTree<u32>, Vec<u8>) {
        // Create a leaf node
        let mut leaf = LeafNode::new();
        leaf.add(Entry {
            key: 10,
            offset: 100,
        });
        leaf.add(Entry {
            key: 20,
            offset: 200,
        });
        leaf.add(Entry {
            key: 30,
            offset: 300,
        });

        // Create an internal node pointing to the leaf
        let mut internal = InternalNode::new();
        internal.add(10, 100); // Offset where leaf node will be stored

        // Serialize the tree
        let mut buffer = Vec::new();

        // Write header (24 bytes)
        let tree = StaticBTree::new(50, 2, 4096, 3);
        tree.write_header(&mut buffer).unwrap();

        // Pad to root offset (50)
        while buffer.len() < 50 {
            buffer.push(0);
        }

        // Write internal node at offset 50
        let node = Node::Internal(internal);
        node.write_to(&mut buffer).unwrap();

        // Pad to leaf offset (100)
        while buffer.len() < 100 {
            buffer.push(0);
        }

        // Write leaf node at offset 100
        let node = Node::Leaf(leaf);
        node.write_to(&mut buffer).unwrap();

        (tree, buffer)
    }

    #[test]
    fn test_header_serialization() {
        let tree = StaticBTree::<u32>::new(100, 3, 4096, 1000);

        let mut buffer = Vec::new();
        tree.write_header(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let deserialized = StaticBTree::<u32>::read_header(&mut cursor).unwrap();

        assert_eq!(deserialized.root_offset(), 100);
        assert_eq!(deserialized.height(), 3);
        assert_eq!(deserialized.node_size(), 4096);
        assert_eq!(deserialized.num_entries(), 1000);
    }

    #[test]
    fn test_find() {
        let (tree, buffer) = create_test_tree();
        let mut cursor = Cursor::new(buffer);

        // Find existing keys
        let results = tree.find(&10, &mut cursor).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 100);

        let results = tree.find(&20, &mut cursor).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 200);

        let results = tree.find(&30, &mut cursor).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 300);

        // Find non-existing key
        let results = tree.find(&40, &mut cursor).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_range_query() {
        let (tree, buffer) = create_test_tree();
        let mut cursor = Cursor::new(buffer);

        // Inclusive range [10, 30]
        let results = tree.range(&10, &30, true, true, &mut cursor).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], 100);
        assert_eq!(results[1], 200);
        assert_eq!(results[2], 300);

        // Exclusive range (10, 30)
        let results = tree.range(&10, &30, false, false, &mut cursor).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 200);

        // Half-inclusive range [10, 30)
        let results = tree.range(&10, &30, true, false, &mut cursor).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], 100);
        assert_eq!(results[1], 200);

        // Half-inclusive range (10, 30]
        let results = tree.range(&10, &30, false, true, &mut cursor).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], 200);
        assert_eq!(results[1], 300);

        // Empty range
        let results = tree.range(&40, &50, true, true, &mut cursor).unwrap();
        assert_eq!(results.len(), 0);
    }
}
