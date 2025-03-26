// Static B+tree node implementation
//
// This module provides the node structure for static B+tree indexes. Unlike traditional
// B-tree nodes, these nodes do not store explicit pointers to child nodes, but rely on
// an implicit layout where child relationships are determined by array indices.

use crate::entry::Entry;
use crate::errors::{NodeError, Result};

/// Type of node in the static B+tree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    /// Internal node contains keys that guide the search
    Internal = 0,

    /// Leaf node contains keys and values that represent actual data
    Leaf = 1,
}

impl NodeType {
    /// Convert from u8 to NodeType
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not 0 or 1
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(NodeType::Internal),
            1 => Ok(NodeType::Leaf),
            _ => Err(NodeError::InvalidType {
                expected: "0 or 1".to_string(),
                actual: value.to_string(),
            }
            .into()),
        }
    }

    /// Convert NodeType to u8
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }
}

/// Node in a static B+tree
///
/// Unlike traditional B-tree nodes, static B+tree nodes do not contain pointers to child nodes.
/// Instead, child relationships are determined by the node's position in the tree array.
///
/// # Node Structure
///
/// The on-disk/serialized structure of a node is:
/// - 1 byte: node type (0 = internal, 1 = leaf)
/// - 2 bytes: entry count (little-endian u16)
/// - 1 byte: reserved for future use
/// - Entries: array of entry records, each containing:
///   - N bytes: key (fixed size defined by key_size)
///   - 8 bytes: value (little-endian u64)
#[derive(Debug, Clone)]
pub struct Node {
    /// Type of node (internal or leaf)
    pub node_type: NodeType,

    /// Entries in this node, sorted by key
    pub entries: Vec<Entry>,
}

impl Node {
    /// Create a new empty node of the given type
    ///
    /// # Parameters
    ///
    /// * `node_type` - The type of node to create (Internal or Leaf)
    pub fn new(node_type: NodeType) -> Self {
        Self {
            node_type,
            entries: Vec::new(),
        }
    }

    /// Create a new empty internal node
    pub fn new_internal() -> Self {
        Self::new(NodeType::Internal)
    }

    /// Create a new empty leaf node
    pub fn new_leaf() -> Self {
        Self::new(NodeType::Leaf)
    }

    /// Check if this node is a leaf node
    pub fn is_leaf(&self) -> bool {
        self.node_type == NodeType::Leaf
    }

    /// Add an entry to this node
    ///
    /// # Parameters
    ///
    /// * `entry` - The entry to add
    /// * `max_entries` - The maximum number of entries this node can hold
    ///
    /// # Errors
    ///
    /// Returns an error if adding this entry would exceed the node's capacity
    pub fn add_entry(&mut self, entry: Entry, max_entries: usize) -> Result<()> {
        if self.entries.len() >= max_entries {
            return Err(NodeError::Overflow.into());
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Find an entry by key using binary search
    ///
    /// # Parameters
    ///
    /// * `key` - The key to search for
    /// * `compare` - A function that compares two keys and returns ordering
    ///
    /// # Returns
    ///
    /// The index of the entry if found, or None if not found
    pub fn find_entry(
        &self,
        key: &[u8],
        compare: impl Fn(&[u8], &[u8]) -> std::cmp::Ordering,
    ) -> Option<usize> {
        self.entries
            .binary_search_by(|entry| compare(&entry.key, key))
            .ok()
    }

    /// Find the index of the first entry with a key greater than or equal to the given key
    ///
    /// This is used during tree traversal to find the correct child node to follow.
    ///
    /// # Parameters
    ///
    /// * `key` - The key to search for
    /// * `compare` - A function that compares two keys and returns ordering
    ///
    /// # Returns
    ///
    /// The index of the first entry with key >= search key, or entries.len() if no such entry
    pub fn find_lower_bound(
        &self,
        key: &[u8],
        compare: impl Fn(&[u8], &[u8]) -> std::cmp::Ordering,
    ) -> usize {
        match self
            .entries
            .binary_search_by(|entry| compare(&entry.key, key))
        {
            Ok(idx) => idx,
            Err(idx) => idx,
        }
    }

    /// Encode this node to bytes for storage
    ///
    /// # Parameters
    ///
    /// * `key_size` - The fixed size of keys in bytes
    ///
    /// # Returns
    ///
    /// A byte vector containing the encoded node
    pub fn encode(&self, key_size: usize) -> Result<Vec<u8>> {
        // Calculate the size of the encoded node
        let header_size = 4; // node_type(1) + entry_count(2) + reserved(1)
        let entries_size = self.entries.len() * (key_size + 8);
        let total_size = header_size + entries_size;

        let mut result = Vec::with_capacity(total_size);

        // Write header
        result.push(self.node_type.to_u8());
        result.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        result.push(0); // Reserved byte

        // Write entries
        for entry in &self.entries {
            // Ensure the key is exactly key_size
            if entry.key.len() != key_size {
                return Err(NodeError::Serialization(format!(
                    "Key size mismatch: expected {}, got {}",
                    key_size,
                    entry.key.len()
                ))
                .into());
            }
            result.extend_from_slice(&entry.key);
            result.extend_from_slice(&entry.value.to_le_bytes());
        }

        Ok(result)
    }

    /// Decode a node from bytes
    ///
    /// # Parameters
    ///
    /// * `bytes` - The byte slice containing the encoded node
    /// * `key_size` - The fixed size of keys in bytes
    ///
    /// # Returns
    ///
    /// The decoded node
    ///
    /// # Errors
    ///
    /// Returns an error if the byte slice is too small or contains invalid data
    pub fn decode(bytes: &[u8], key_size: usize) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(NodeError::Deserialization(
                "Insufficient bytes for node header".to_string(),
            )
            .into());
        }

        // Read header
        let node_type = NodeType::from_u8(bytes[0])?;
        let entry_count = u16::from_le_bytes([bytes[1], bytes[2]]) as usize;
        // bytes[3] is reserved

        // Read entries
        let entry_size = key_size + 8;
        let mut entries = Vec::with_capacity(entry_count);

        for i in 0..entry_count {
            let offset = 4 + i * entry_size;
            if offset + entry_size > bytes.len() {
                return Err(NodeError::Deserialization(
                    "Insufficient bytes for entries".to_string(),
                )
                .into());
            }

            let entry = Entry::decode(&bytes[offset..offset + entry_size], key_size)?;
            entries.push(entry);
        }

        Ok(Self { node_type, entries })
    }
}

/// Calculates the child node index for a given node in the static B+tree
///
/// In a static B+tree with an implicit layout, the child nodes of node k with branching
/// factor B are numbered k * (B + 1) + i + 1 for i ∈ [0, B].
///
/// # Parameters
///
/// * `node_index` - The index of the parent node
/// * `branch` - The branch number (0 to branching_factor)
/// * `branching_factor` - The branching factor of the tree
///
/// # Returns
///
/// The index of the child node
pub fn child_index(node_index: usize, branch: usize, branching_factor: usize) -> usize {
    node_index * (branching_factor + 1) + branch + 1
}

/// Calculates the parent node index for a given node in the static B+tree
///
/// This is the inverse operation of child_index.
///
/// # Parameters
///
/// * `node_index` - The index of the child node
/// * `branching_factor` - The branching factor of the tree
///
/// # Returns
///
/// A tuple containing the parent index and the branch number, or None if this is the root
pub fn parent_index(node_index: usize, branching_factor: usize) -> Option<(usize, usize)> {
    if node_index == 0 {
        // Root node has no parent
        None
    } else {
        // Calculate parent index and branch
        let branch = (node_index - 1) % (branching_factor + 1);
        let parent = (node_index - 1 - branch) / (branching_factor + 1);
        Some((parent, branch))
    }
}

/// Calculates the maximum number of entries that can fit in a node
///
/// # Parameters
///
/// * `branching_factor` - The branching factor of the tree
///
/// # Returns
///
/// The maximum number of entries per node (equal to branching_factor)
pub fn max_entries_per_node(branching_factor: usize) -> usize {
    branching_factor
}

pub fn max_node_size(branching_factor: usize, entry_size: usize) -> usize {
    let header_size = 4; // node_type(1) + entry_count(2) + reserved(1)
    let max_entries = max_entries_per_node(branching_factor);
    header_size + max_entries * entry_size
}
