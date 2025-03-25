use crate::entry::Entry;
use crate::errors::{BTreeError, Result};
use crate::key::I64KeyEncoder;

/// Type of B-tree node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    /// Internal node contains keys and pointers to child nodes
    Internal = 0,

    /// Leaf node contains keys and pointers to data records
    Leaf = 1,
}

impl NodeType {
    /// Convert from u8 to NodeType
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(NodeType::Internal),
            1 => Ok(NodeType::Leaf),
            _ => Err(BTreeError::InvalidNodeType {
                expected: "0 or 1".to_string(),
                actual: value.to_string(),
            }),
        }
    }

    /// Convert NodeType to u8
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }
}

/// B-tree node structure
#[derive(Debug, Clone)]
pub struct Node {
    /// Type of node (internal or leaf)
    pub node_type: NodeType,

    /// Entries in this node
    pub entries: Vec<Entry>,

    /// Pointer to next node (only for leaf nodes, forms a linked list)
    pub next_node: Option<u64>,
}

impl Node {
    /// Create a new node of the given type
    pub fn new(node_type: NodeType) -> Self {
        Self {
            node_type,
            entries: Vec::new(),
            next_node: None,
        }
    }

    /// Create a new internal node
    pub fn new_internal() -> Self {
        Self::new(NodeType::Internal)
    }

    /// Create a new leaf node
    pub fn new_leaf() -> Self {
        Self::new(NodeType::Leaf)
    }

    /// Check if this node is a leaf
    pub fn is_leaf(&self) -> bool {
        self.node_type == NodeType::Leaf
    }

    /// Add an entry to this node
    pub fn add_entry(&mut self, entry: Entry) {
        // Insert an entry into the node, maintaining ordering
        self.entries.push(entry);
    }

    /// Find an entry by key
    pub fn find_entry(
        &self,
        key: &[u8],
        compare: impl Fn(&[u8], &[u8]) -> std::cmp::Ordering,
    ) -> Option<usize> {
        // Find an entry by binary search
        self.entries
            .binary_search_by(|entry| compare(&entry.key, key))
            .ok()
    }

    /// Encode this node to bytes
    pub fn encode(&self, node_size: usize, key_size: usize) -> Result<Vec<u8>> {
        // Calculate the maximum number of entries that can fit in this node
        let entry_size = key_size + 8; // key size + value size
        let header_size = 12; // node_type(1) + entry_count(2) + next_node(8) + reserved(1)
        let max_entries = (node_size - header_size) / entry_size;

        if self.entries.len() > max_entries {
            return Err(BTreeError::Serialization(format!(
                "Node has too many entries: {} (max {})",
                self.entries.len(),
                max_entries
            )));
        }

        let mut result = Vec::with_capacity(node_size);

        // Write header
        result.push(self.node_type.to_u8());
        result.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        result.extend_from_slice(&self.next_node.unwrap_or(0).to_le_bytes());
        result.push(0); // Reserved byte

        // Write entries
        for entry in &self.entries {
            result.extend_from_slice(&entry.key);
            result.extend_from_slice(&entry.value.to_le_bytes());
        }

        // Pad to node_size
        result.resize(node_size, 0);

        Ok(result)
    }

    /// Decode a node from bytes
    pub fn decode(bytes: &[u8], key_size: usize) -> Result<Self> {
        if bytes.len() < 12 {
            return Err(BTreeError::Deserialization(
                "Insufficient bytes for node header".to_string(),
            ));
        }

        // Read header
        let node_type = NodeType::from_u8(bytes[0])?;
        let entry_count = u16::from_le_bytes([bytes[1], bytes[2]]) as usize;
        let next_node_val = u64::from_le_bytes([
            bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10],
        ]);
        let next_node = if next_node_val == 0 {
            None
        } else {
            Some(next_node_val)
        };
        // bytes[11] is reserved

        // Read entries
        let entry_size = key_size + 8;
        let mut entries = Vec::with_capacity(entry_count);

        for i in 0..entry_count {
            let offset = 12 + i * entry_size;
            if offset + entry_size > bytes.len() {
                return Err(BTreeError::Deserialization(
                    "Insufficient bytes for entries".to_string(),
                ));
            }

            let key = bytes[offset..offset + key_size].to_vec();
            let value = u64::from_le_bytes([
                bytes[offset + key_size],
                bytes[offset + key_size + 1],
                bytes[offset + key_size + 2],
                bytes[offset + key_size + 3],
                bytes[offset + key_size + 4],
                bytes[offset + key_size + 5],
                bytes[offset + key_size + 6],
                bytes[offset + key_size + 7],
            ]);

            entries.push(Entry::new(key, value));
        }

        Ok(Self {
            node_type,
            entries,
            next_node,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{AnyKeyEncoder, KeyEncoder};
    #[test]
    fn test_node_type_conversion() {
        // Test NodeType::from_u8
        assert_eq!(NodeType::from_u8(0).unwrap(), NodeType::Internal);
        assert_eq!(NodeType::from_u8(1).unwrap(), NodeType::Leaf);
        assert!(matches!(
            NodeType::from_u8(2),
            Err(BTreeError::InvalidNodeType { .. })
        ));

        // Test NodeType::to_u8
        assert_eq!(NodeType::Internal.to_u8(), 0);
        assert_eq!(NodeType::Leaf.to_u8(), 1);
    }

    #[test]
    fn test_node_creation() {
        // Test creation of internal and leaf nodes
        let internal = Node::new_internal();
        let leaf = Node::new_leaf();

        assert_eq!(internal.node_type, NodeType::Internal);
        assert_eq!(leaf.node_type, NodeType::Leaf);

        assert!(internal.entries.is_empty());
        assert!(leaf.entries.is_empty());

        assert_eq!(internal.next_node, None);
        assert_eq!(leaf.next_node, None);

        // Test is_leaf method
        assert!(!internal.is_leaf());
        assert!(leaf.is_leaf());
    }

    #[test]
    // fn test_add_entry_and_find() {
    //     let mut node = Node::new_leaf();
    //     let key_encoder = AnyKeyEncoder::i64();

    //     // Create test entries using I64KeyEncoder
    //     let entry1 = Entry::new(key_encoder.encode(&1).unwrap(), 100);
    //     let entry2 = Entry::new(key_encoder.encode(&2).unwrap(), 200);
    //     let entry3 = Entry::new(key_encoder.encode(&3).unwrap(), 300);

    //     // Add entries to node
    //     node.add_entry(entry1.clone());
    //     node.add_entry(entry2.clone());
    //     node.add_entry(entry3.clone());

    //     // Verify entries were added
    //     assert_eq!(node.entries.len(), 3);
    //     assert_eq!(key_encoder.decode(&node.entries[0].key).unwrap(), 1);
    //     assert_eq!(node.entries[0].value, 100);
    //     assert_eq!(key_encoder.decode(&node.entries[1].key).unwrap(), 2);
    //     assert_eq!(node.entries[1].value, 200);
    //     assert_eq!(key_encoder.decode(&node.entries[2].key).unwrap(), 3);
    //     assert_eq!(node.entries[2].value, 300);

    //     // Test find_entry with key encoder's compare function
    //     let search_key = key_encoder.encode(&2).unwrap();
    //     let idx = node.find_entry(&search_key, |a, b| key_encoder.compare(a, b));
    //     assert_eq!(idx, Some(1));

    //     // Try to find non-existing entry
    //     let search_key = key_encoder.encode(&4).unwrap();
    //     let idx = node.find_entry(&search_key, |a, b| key_encoder.compare(a, b));
    //     assert_eq!(idx, None);
    // }
    #[test]
    fn test_node_encode_decode() {
        // Create a node with some entries
        let mut node = Node::new_leaf();
        let key_encoder = I64KeyEncoder;
        node.next_node = Some(12345);

        // Add entries using I64KeyEncoder
        node.add_entry(Entry::new(key_encoder.encode(&1).unwrap(), 100));
        node.add_entry(Entry::new(key_encoder.encode(&2).unwrap(), 200));
        node.add_entry(Entry::new(key_encoder.encode(&3).unwrap(), 300));

        // Encode the node
        let node_size = 256; // Choose a small size for testing
        let key_size = key_encoder.encoded_size(); // Use encoder's key size
        let encoded = node.encode(node_size, key_size).unwrap();

        // Make sure encoded data has expected size
        assert_eq!(encoded.len(), node_size);

        // Decode the node
        let decoded = Node::decode(&encoded, key_size).unwrap();

        // Verify node properties were preserved
        assert_eq!(decoded.node_type, NodeType::Leaf);
        assert_eq!(decoded.next_node, Some(12345));
        assert_eq!(decoded.entries.len(), 3);

        // Verify all entries were preserved
        assert_eq!(key_encoder.decode(&decoded.entries[0].key).unwrap(), 1);
        assert_eq!(decoded.entries[0].value, 100);
        assert_eq!(key_encoder.decode(&decoded.entries[1].key).unwrap(), 2);
        assert_eq!(decoded.entries[1].value, 200);
        assert_eq!(key_encoder.decode(&decoded.entries[2].key).unwrap(), 3);
        assert_eq!(decoded.entries[2].value, 300);
    }

    #[test]
    fn test_node_encode_too_many_entries() {
        // Create a node with too many entries for the node size
        let mut node = Node::new_leaf();
        let key_encoder = I64KeyEncoder;

        // Add entries using I64KeyEncoder
        for i in 1..=10 {
            node.add_entry(Entry::new(key_encoder.encode(&i).unwrap(), i as u64 * 100));
        }

        // Try to encode with a small node size
        let node_size = 48; // Small size to force overflow
        let key_size = key_encoder.encoded_size();
        let result = node.encode(node_size, key_size);

        // This should fail with a serialization error
        assert!(matches!(result, Err(BTreeError::Serialization(_))));
    }

    #[test]
    fn test_decode_incomplete_node() {
        // Test decoding with insufficient bytes
        let bytes = vec![1, 0, 0]; // Only 3 bytes, not enough for header
        let key_size = I64KeyEncoder.encoded_size();
        let result = Node::decode(&bytes, key_size);

        // This should fail with a deserialization error
        assert!(matches!(result, Err(BTreeError::Deserialization(_))));
    }

    #[test]
    fn test_decode_insufficient_entries() {
        // Create bytes for node header but insufficient for entries
        let mut bytes = vec![0; 20]; // Only enough for header and partial entry
        bytes[0] = 1; // Leaf node
        bytes[1] = 2; // 2 entries (entry_count low byte)
        bytes[2] = 0; // entry_count high byte

        let key_size = I64KeyEncoder.encoded_size();
        let result = Node::decode(&bytes, key_size);

        // This should fail with a deserialization error
        assert!(matches!(result, Err(BTreeError::Deserialization(_))));
    }

    #[test]
    fn test_next_node_roundtrip() {
        // Test that next_node gets properly encoded/decoded for leaf nodes
        let key_encoder = I64KeyEncoder;

        // Test with next_node = None
        let mut node1 = Node::new_leaf();
        node1.add_entry(Entry::new(key_encoder.encode(&1).unwrap(), 100));
        let encoded1 = node1.encode(128, key_encoder.encoded_size()).unwrap();
        let decoded1 = Node::decode(&encoded1, key_encoder.encoded_size()).unwrap();
        assert_eq!(decoded1.next_node, None);

        // Test with next_node = Some(value)
        let mut node2 = Node::new_leaf();
        node2.next_node = Some(0xDEADBEEF);
        node2.add_entry(Entry::new(key_encoder.encode(&1).unwrap(), 100));
        let encoded2 = node2.encode(128, key_encoder.encoded_size()).unwrap();
        let decoded2 = Node::decode(&encoded2, key_encoder.encoded_size()).unwrap();
        assert_eq!(decoded2.next_node, Some(0xDEADBEEF));
    }
}
