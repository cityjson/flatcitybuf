//! Implementation of a static B+tree data structure.
//!
//! This module provides a static B+tree implementation that uses an implicit
//! layout in memory. The tree structure is optimized for read operations and
//! supports configurable branching factors.

use crate::entry::Entry;
use crate::errors::{Error, Result, TreeError};
use crate::key::{KeyEncoder, KeyEncoderFactory, KeyType};
use crate::node::{max_node_size, Node, NodeType};
use crate::utils;

use core::error;
use std::cmp::Ordering;
use std::marker::PhantomData;

/// A static B+tree implementation with implicit layout.
///
/// This tree structure is optimized for read operations and maintains
/// a compact, cache-friendly layout in memory. The tree is static, meaning
/// it can only be built once and does not support modifications after
/// construction.
pub struct StaticBTree<K: 'static> {
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
pub struct StaticBTreeBuilder<K: 'static> {
    branching_factor: usize,
    entries: Vec<Entry>,
    key_encoder: Box<dyn KeyEncoder<K>>,
    _phantom: PhantomData<K>,
}

impl<K: 'static> StaticBTreeBuilder<K> {
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

        // Sort entries by key but preserve order of equal keys
        self.entries
            .sort_by(|a, b| self.key_encoder.compare(&a.key, &b.key));

        let size = self.entries.len();
        let height = utils::calculate_tree_height(size, self.branching_factor);

        // Allocate buffer for the entire tree
        let total_nodes = utils::calculate_total_nodes(size, self.branching_factor);
        let max_node_size = max_node_size(
            self.branching_factor,
            self.entries
                .first()
                .ok_or(Error::Tree(TreeError::EmptyTree))?
                .encoded_size(),
        ); //TODO: make sure this is correct. The last node might not be full.
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
        let max_node_size = max_node_size(
            self.branching_factor,
            self.entries
                .first()
                .ok_or(Error::Tree(TreeError::EmptyTree))?
                .encoded_size(),
        );

        // Calculate the node's offset in the buffer
        let node_offset = node_index * max_node_size;

        // Create node
        let node_type = if level == height - 1 {
            NodeType::Leaf
        } else {
            NodeType::Internal
        };

        let mut node = Node::new(node_type);

        if node_type == NodeType::Leaf {
            // For leaf nodes, add all entries
            for i in start_entry..=end_entry {
                node.add_entry(self.entries[i].clone(), self.branching_factor)?;
            }
        } else {
            // For internal nodes, we need to add separator keys and child pointers
            let children_per_node = self.branching_factor;
            let entries_per_child = entry_count.div_ceil(children_per_node);

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
                node.add_entry(
                    Entry::new(separator_entry.key.clone(), child_index as u64), //TODO: check if `child_index` is correct
                    self.branching_factor,
                )?;

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

        let encoded_node = node.encode(self.key_encoder.encoded_size())?;
        // Serialize the node into the buffer
        buffer[node_offset..node_offset + encoded_node.len()].copy_from_slice(&encoded_node);

        Ok(())
    }

    /// Find all values associated with a key in a leaf node
    fn find_all_values_in_node(
        node: &Node,
        key: &[u8],
        key_encoder: &dyn KeyEncoder<K>,
    ) -> Vec<u64> {
        let mut values = Vec::new();
        let i = node.find_lower_bound(key, |a, b| key_encoder.compare(a, b));

        // Check entries before the found index for duplicates
        let mut check_idx = i;
        while check_idx > 0 {
            check_idx -= 1;
            let entry = &node.entries[check_idx];
            if key_encoder.compare(&entry.key, key) != Ordering::Equal {
                break;
            }
            values.push(entry.value);
        }

        // Check the found index and entries after it
        check_idx = i;
        while check_idx < node.entries.len() {
            let entry = &node.entries[check_idx];
            if key_encoder.compare(&entry.key, key) != Ordering::Equal {
                break;
            }
            values.push(entry.value);
            check_idx += 1;
        }

        values
    }
}

impl<K: 'static> StaticBTree<K> {
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

    /// Find all values associated with a key
    pub fn find(&self, key: &[u8]) -> Result<Vec<u64>> {
        if self.is_empty() {
            return Ok(Vec::new());
        }

        let max_node_size = max_node_size(self.branching_factor, self.key_encoder.encoded_size());
        let mut node_index = 0;
        let mut values = Vec::new();

        // Traverse the tree from root to leaf
        for level in 0..self.height {
            let node_offset = node_index * max_node_size;
            let node = Node::decode(
                &self.data[node_offset..node_offset + max_node_size],
                self.key_encoder.encoded_size(),
            )?;

            if level == self.height - 1 {
                // At leaf level, collect all matching values
                values.extend(StaticBTreeBuilder::<K>::find_all_values_in_node(
                    &node,
                    key,
                    &*self.key_encoder,
                ));

                // Check adjacent nodes for duplicates at boundaries
                if !node.entries.is_empty()
                    && self.key_encoder.compare(&node.entries[0].key, key) == Ordering::Equal
                {
                    // Check previous node
                    if node_index > 0 {
                        let prev_offset = (node_index - 1) * max_node_size;
                        let prev_node = Node::decode(
                            &self.data[prev_offset..prev_offset + max_node_size],
                            self.key_encoder.encoded_size(),
                        )?;
                        values.extend(StaticBTreeBuilder::<K>::find_all_values_in_node(
                            &prev_node,
                            key,
                            &*self.key_encoder,
                        ));
                    }
                }

                if !node.entries.is_empty()
                    && self
                        .key_encoder
                        .compare(&node.entries[node.entries.len() - 1].key, key)
                        == Ordering::Equal
                {
                    // Check next node
                    let next_offset = (node_index + 1) * max_node_size;
                    if next_offset < self.data.len() {
                        let next_node = Node::decode(
                            &self.data[next_offset..next_offset + max_node_size],
                            self.key_encoder.encoded_size(),
                        )?;
                        if next_node.node_type == NodeType::Leaf {
                            values.extend(StaticBTreeBuilder::<K>::find_all_values_in_node(
                                &next_node,
                                key,
                                &*self.key_encoder,
                            ));
                        }
                    }
                }

                break;
            } else {
                // Internal node, find the next node to traverse
                let idx = node.find_lower_bound(key, |a, b| self.key_encoder.compare(a, b));
                if idx >= node.entries.len() {
                    if node.entries.is_empty() {
                        return Ok(values);
                    }
                    node_index = node.entries[node.entries.len() - 1].value as usize;
                } else {
                    node_index = node.entries[idx].value as usize;
                }
            }
        }

        Ok(values)
    }

    /// Find entries with keys in the range [start, end]
    pub fn range(&self, start: &[u8], end: &[u8]) -> Result<Vec<(Vec<u8>, u64)>> {
        let mut results = Vec::new();

        if self.is_empty() || self.key_encoder.compare(start, end) == Ordering::Greater {
            return Ok(results);
        }

        let max_node_size = max_node_size(self.branching_factor, self.key_encoder.encoded_size());
        let mut node_index = 0;

        // Traverse to the leaf node containing the start key
        for level in 0..self.height - 1 {
            let node_offset = node_index * max_node_size;
            let node = Node::decode(
                &self.data[node_offset..node_offset + max_node_size],
                self.key_encoder.encoded_size(),
            )?;

            let idx = node.find_lower_bound(start, |a, b| self.key_encoder.compare(a, b));
            if idx >= node.entries.len() {
                if node.entries.is_empty() {
                    return Ok(results);
                }
                node_index = node.entries[node.entries.len() - 1].value as usize;
            } else {
                node_index = node.entries[idx].value as usize;
            }
        }

        // Check if we need to look at the previous node for duplicates at the start
        if node_index > 0 {
            let prev_offset = (node_index - 1) * max_node_size;
            let prev_node = Node::decode(
                &self.data[prev_offset..prev_offset + max_node_size],
                self.key_encoder.encoded_size(),
            )?;

            if !prev_node.entries.is_empty() {
                let last_entry = &prev_node.entries[prev_node.entries.len() - 1];
                if self.key_encoder.compare(&last_entry.key, start) >= Ordering::Equal {
                    // Process entries from the previous node
                    for entry in &prev_node.entries {
                        let key = entry.key.clone();
                        if self.key_encoder.compare(&key, start) >= Ordering::Equal
                            && self.key_encoder.compare(&key, end) <= Ordering::Equal
                        {
                            results.push((key, entry.value));
                        }
                    }
                }
            }
        }

        // Scan leaf nodes
        loop {
            let node_offset = node_index * max_node_size;
            if node_offset >= self.data.len() {
                break;
            }

            let node = Node::decode(
                &self.data[node_offset..node_offset + max_node_size],
                self.key_encoder.encoded_size(),
            )?;

            if node.node_type != NodeType::Leaf {
                break;
            }

            let mut found_greater = false;
            for entry in &node.entries {
                let key = entry.key.clone();

                if self.key_encoder.compare(&key, end) > Ordering::Equal {
                    found_greater = true;
                    break;
                }

                if self.key_encoder.compare(&key, start) >= Ordering::Equal {
                    results.push((key, entry.value));
                }
            }

            if found_greater {
                break;
            }

            node_index += 1;
        }

        Ok(results)
    }

    /// Get the raw data buffer
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
