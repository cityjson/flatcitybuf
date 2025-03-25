//! Implementation of a static B+tree data structure.
//!
//! This module provides a static B+tree implementation that uses an implicit
//! layout in memory. The tree structure is optimized for read operations and
//! supports configurable branching factors.

use crate::entry::Entry;
use crate::errors::{Error, Result, TreeError};
use crate::key::{KeyEncoder, KeyEncoderFactory, KeyType};
use crate::node::{Node, NodeType};
use crate::utils;

use std::cmp::Ordering;
use std::marker::PhantomData;

/// A static B+tree implementation with implicit layout.
///
/// This tree structure is optimized for read operations and maintains
/// a compact, cache-friendly layout in memory. The tree is static, meaning
/// it can only be built once and does not support modifications after
/// construction.
pub struct StaticBTree<K> {
    /// The raw data buffer containing the serialized tree
    data: Vec<u8>,
    /// The height of the tree
    height: usize,
    /// The branching factor (number of entries per node)
    branching_factor: usize,
    /// Total number of elements in the tree
    size: usize,
    /// The encoder used for keys
    key_encoder: Box<dyn KeyEncoder<K>>,
    /// Phantom data for the key type
    _phantom: PhantomData<K>,
}

/// Builder for constructing a static B+tree
pub struct StaticBTreeBuilder<K> {
    branching_factor: usize,
    entries: Vec<Entry>,
    key_encoder: Box<dyn KeyEncoder<K>>,
    _phantom: PhantomData<K>,
}

impl<K> StaticBTreeBuilder<K> {
    /// Create a new builder with the specified branching factor and key type
    pub fn new(branching_factor: usize, key_type: KeyType) -> Self {
        if branching_factor < 4 {
            panic!("branching factor must be at least 4");
        }

        let key_encoder = KeyEncoderFactory::for_type::<K>(key_type);

        Self {
            branching_factor,
            entries: Vec::new(),
            key_encoder,
            _phantom: PhantomData,
        }
    }

    /// Add an entry to the tree
    pub fn add_entry(&mut self, key: &[u8], value: u64) -> &mut Self {
        self.entries.push(Entry::new(key.to_vec(), value));
        self
    }

    /// Build the static B+tree
    pub fn build(mut self) -> Result<StaticBTree<K>> {
        if self.entries.is_empty() {
            return Err(Error::Tree(TreeError::EmptyTree));
        }

        // Sort entries by key
        self.entries
            .sort_by(|a, b| self.key_encoder.compare(&a.key(), &b.key()));

        // Remove duplicates (keep only the last entry for each key)
        self.entries.dedup_by(|a, b| {
            if self.key_encoder.compare(&a.key(), &b.key()) == Ordering::Equal {
                // Keep b (the later entry) and drop a
                true
            } else {
                false
            }
        });

        let size = self.entries.len();
        let height = utils::calculate_tree_height(size, self.branching_factor);

        // Allocate buffer for the entire tree
        let total_nodes = utils::calculate_total_nodes(size, self.branching_factor);
        let max_node_size = Node::max_size(self.branching_factor);
        let buffer_size = total_nodes * max_node_size;
        let mut buffer = vec![0u8; buffer_size];

        // Build the tree structure
        self.build_tree_recursive(
            &mut buffer,
            0,        // node index
            0,        // start entry
            size - 1, // end entry
            height,
            0, // current level
        )?;

        Ok(StaticBTree {
            data: buffer,
            height,
            branching_factor: self.branching_factor,
            size,
            key_encoder: self.key_encoder,
            _phantom: PhantomData,
        })
    }

    /// Recursively build the tree structure
    fn build_tree_recursive(
        &self,
        buffer: &mut [u8],
        node_index: usize,
        start_entry: usize,
        end_entry: usize,
        height: usize,
        level: usize,
    ) -> Result<()> {
        let entry_count = end_entry - start_entry + 1;
        let max_node_size = Node::max_size(self.branching_factor);

        // Calculate the node's offset in the buffer
        let node_offset = node_index * max_node_size;

        // Create node
        let node_type = if level == height - 1 {
            NodeType::Leaf
        } else {
            NodeType::Internal
        };

        let mut node = Node::new(self.branching_factor, node_type);

        if node_type == NodeType::Leaf {
            // For leaf nodes, add all entries
            for i in start_entry..=end_entry {
                node.add_entry(&self.entries[i])?;
            }
        } else {
            // For internal nodes, we need to add separator keys and child pointers
            let children_per_node = self.branching_factor;
            let entries_per_child = (entry_count + children_per_node - 1) / children_per_node;

            let mut child_index = node_index * self.branching_factor + 1;

            for i in 0..children_per_node {
                let child_start = start_entry + i * entries_per_child;

                // If we've processed all entries, stop
                if child_start > end_entry {
                    break;
                }

                let child_end = std::cmp::min(child_start + entries_per_child - 1, end_entry);

                // Add the first key from this child's range as a separator
                let separator_entry = &self.entries[child_start];
                node.add_entry(&Entry::new(separator_entry.key(), child_index as u64))?;

                // Recursively build the child node
                self.build_tree_recursive(
                    buffer,
                    child_index,
                    child_start,
                    child_end,
                    height,
                    level + 1,
                )?;

                child_index += 1;
            }
        }

        // Serialize the node into the buffer
        node.encode(&mut buffer[node_offset..node_offset + max_node_size])?;

        Ok(())
    }
}

impl<K> StaticBTree<K> {
    /// Create a new static B+tree builder
    pub fn builder(branching_factor: usize, key_type: KeyType) -> StaticBTreeBuilder<K> {
        StaticBTreeBuilder::new(branching_factor, key_type)
    }

    /// Get the number of elements in the tree
    pub fn len(&self) -> usize {
        self.size
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Get the height of the tree
    pub fn height(&self) -> usize {
        self.height
    }

    /// Get the branching factor of the tree
    pub fn branching_factor(&self) -> usize {
        self.branching_factor
    }

    /// Find a value by its key
    pub fn find(&self, key: &[u8]) -> Result<Option<u64>> {
        if self.is_empty() {
            return Ok(None);
        }

        let max_node_size = Node::max_size(self.branching_factor);
        let mut node_index = 0;

        // Traverse the tree from root to leaf
        for level in 0..self.height {
            let node_offset = node_index * max_node_size;

            // Read the node
            let node = Node::decode(
                &self.data[node_offset..node_offset + max_node_size],
                self.branching_factor,
            )?;

            if level == self.height - 1 {
                // Leaf node, search for the exact key
                match node.find_entry(key, &*self.key_encoder) {
                    Some(entry) => return Ok(Some(entry.value())),
                    None => return Ok(None),
                }
            } else {
                // Internal node, find the next node to traverse
                let child_index = match node.find_lower_bound(key, &*self.key_encoder) {
                    Some(entry) => entry.value() as usize,
                    None => return Ok(None), // This should not happen in a well-formed tree
                };

                node_index = child_index;
            }
        }

        Ok(None)
    }

    /// Find entries with keys in the range [start, end]
    pub fn range(&self, start: &[u8], end: &[u8]) -> Result<Vec<(Vec<u8>, u64)>> {
        let mut results = Vec::new();

        if self.is_empty() || self.key_encoder.compare(start, end) == Ordering::Greater {
            return Ok(results);
        }

        // First, find the leaf node containing the start key
        let max_node_size = Node::max_size(self.branching_factor);
        let mut node_index = 0;

        // Traverse the tree to find the leaf node containing the start key
        for level in 0..self.height - 1 {
            let node_offset = node_index * max_node_size;

            // Read the node
            let node = Node::decode(
                &self.data[node_offset..node_offset + max_node_size],
                self.branching_factor,
            )?;

            // Find the next node to traverse
            let child_index = match node.find_lower_bound(start, &*self.key_encoder) {
                Some(entry) => entry.value() as usize,
                None => {
                    // If we can't find a lower bound, use the last entry
                    if node.len() > 0 {
                        node.get_entry(node.len() - 1).unwrap().value() as usize
                    } else {
                        return Ok(results); // Empty node, should not happen
                    }
                }
            };

            node_index = child_index;
        }

        // Scan leaf nodes until we find entries greater than the end key
        loop {
            let node_offset = node_index * max_node_size;

            // Check if we're still within the buffer bounds
            if node_offset >= self.data.len() {
                break;
            }

            // Read the leaf node
            let node = Node::decode(
                &self.data[node_offset..node_offset + max_node_size],
                self.branching_factor,
            )?;

            // Process entries in this leaf node
            for i in 0..node.len() {
                let entry = node.get_entry(i).unwrap();
                let key = entry.key();

                // If the key is greater than the end key, we're done
                if self.key_encoder.compare(&key, end) == Ordering::Greater {
                    return Ok(results);
                }

                // If the key is greater than or equal to the start key, add it to results
                if self.key_encoder.compare(&key, start) != Ordering::Less {
                    results.push((key, entry.value()));
                }
            }

            // Move to the next leaf node if it exists
            // In a well-formed tree, the next node is at index node_index + 1
            // but we need to check if it exists
            node_index += 1;

            // If we've reached a node index that would be out of bounds or not a leaf node,
            // we're done
            if node_index * max_node_size >= self.data.len() {
                break;
            }

            // Check if the next node is still a leaf node
            let next_node_offset = node_index * max_node_size;
            let next_node = Node::decode(
                &self.data[next_node_offset..next_node_offset + max_node_size],
                self.branching_factor,
            )?;

            if next_node.node_type() != NodeType::Leaf {
                break;
            }
        }

        Ok(results)
    }

    /// Get the raw data buffer
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
