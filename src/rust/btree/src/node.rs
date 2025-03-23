use crate::entry::Entry;
use crate::errors::{BTreeError, Result};

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
