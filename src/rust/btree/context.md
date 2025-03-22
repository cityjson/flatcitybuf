# Persistent B-tree Indexing: Background and Design Policy

## Background and Motivation

Efficiently indexing and accessing large-scale record data on disk is a core requirement for many systems dealing with structured data. Traditional file systems operate on block-based I/O, typically using 4 KB blocks, which influences both how data is stored and how it should be accessed for performance. Additionally, modern operating systems use page caching and read-ahead mechanisms that further benefit from predictable, aligned access patterns.

In this research, the goal is to design a file format and indexing scheme that:

- Supports fast, scalable access to large datasets
- Can be efficiently queried via HTTP range requests (e.g., in browser-based applications)
- Maximizes I/O throughput by aligning with underlying system characteristics

To achieve this, we take inspiration from the storage and indexing strategies of relational databases (RDBMS), which have long optimized for similar requirements.

## Problems with Current Methods

Two implementations of binary search trees have been explored so far, each with significant limitations:

### In-Memory Binary Search Tree
- The entire index must be loaded into memory, which becomes inefficient when the index size grows large.
- This approach does not scale well for web-based applications or remote clients where loading the full index upfront is impractical.

### Streaming Binary Search Tree
- Entries are variable-length and accessed one-by-one.
- As a result, the method incurs significant disk I/O latency, especially in random access scenarios.
- Lack of fixed-size structure makes it less suitable for caching and prefetching mechanisms.
- Profiling shows excessive system calls and high I/O wait times.

These limitations motivated a shift toward a B-tree based structure, which provides a more cache-friendly, disk-efficient, and streaming-compatible indexing solution.

## File Format Overview

The FlatCityBuf file structure is as follows:

```
┌─────────────────────┐
│ Magic Bytes        │ (4 bytes)
├─────────────────────┤
│ Header Size        │ (4 bytes)
├─────────────────────┤
│ Header             │ (FlatBuffers encoded)
├─────────────────────┤
│ Packed R-tree Index │ (Spatial indexing)
├─────────────────────┤
│ B-tree Index       │ (Attribute indexing)
├─────────────────────┤
│ Features           │ (Actual city objects)
└─────────────────────┘
```

- **Magic bytes**: Used to identify the file type (fcb\0).
- **Header Size**: Length of the header in bytes (4-byte unsigned integer).
- **Header**: Contains metadata encoded using FlatBuffers.
- **Packed R-tree Index**: Spatial index structure for geographic queries.
- **B-tree Index**: Attribute index structure for efficient property-based queries.
- **Features**: The actual city objects encoded as FlatBuffers.

## Implementation Priorities

The implementation will focus on the following priorities in order:

1. **In-memory querying**: Fast, efficient query operations when indices are loaded in memory
2. **Disk-based querying with page caching**: Optimized block-aligned access with efficient page caching
3. **HTTP range request optimization**: (Future work) Extending the implementation for web-based access

This document primarily addresses the first two priorities, with HTTP optimizations to be considered in a later phase.

## Design and Implementation Policy

### 1. Fixed-Size, Page-Aligned B-tree Nodes

- Each node in the B-tree index is stored as a **4 KB block**.
- Nodes are **aligned** to 4 KB offsets within the file.
- I/O operations (both reads and writes) are always done in **multiples of 4 KB**, minimizing syscall overhead and making optimal use of OS page cache.

### 2. Use of Static B-tree Instead of Binary Search Tree

- Binary search trees can provide fast in-memory access but are suboptimal for disk-based or streaming access.
- A **static B-tree** (or B+ tree) allows for storing many entries in a single node, improving **spatial locality** and reducing the number of I/O operations required for traversal.
- This design is better suited for **streaming contexts**, such as reading over HTTP, where fetching larger contiguous regions is more efficient than issuing many small reads.

### 3. Range-Based Access Compatibility

- The file is designed with **HTTP range request compatibility** in mind.
- Since the B-tree index and record data are stored in fixed locations, clients can directly seek and retrieve only the necessary parts of the file.

### 4. Cache-Friendly Read Strategy

- Page-aligned reads allow the OS to efficiently cache index nodes in memory.
- When a node is accessed, its entire 4 KB block is loaded into the page cache, enabling fast in-memory traversal of node entries.

### 5. Streaming-Friendly Record Access

- The record section is optimized for sequential access and can be decoded progressively.
- Index entries in the B-tree map directly to offsets within the record section, enabling efficient, targeted data extraction.

## B-tree Node Structure

Each B-tree node is stored as a fixed-size 4 KB block with the following structure:

```
┌───────────────────────┐
│ Node Header          │
├───────────────────────┤
│ - Node Type          │ (1 byte: INTERNAL=0, LEAF=1)
│ - Entry Count        │ (2 bytes)
│ - Next Node Offset   │ (8 bytes, 0 if none)
│ - Reserved           │ (1 byte)
├───────────────────────┤
│ Entries              │
├───────────────────────┤
│ - Key                │ (fixed width per type)
│ - Value              │ (8-byte offset)
└───────────────────────┘
```

### Node Types

1. **Internal Nodes**
   - Contains keys and pointers to child nodes
   - Used for tree traversal
   - Stores (key, child_node_offset) pairs

2. **Leaf Nodes**
   - Contains keys and pointers to actual data records
   - Linked list structure (next_node_offset) for efficient range scans
   - Stores (key, data_record_offset) pairs

### Entry Storage

For optimal performance, entries within a node are stored in a fixed-width format:

- Keys are stored with fixed width based on their type:
  - Numeric types: native binary format (1-8 bytes)
  - String types: fixed-width prefix (16 bytes) with collision handling
  - Date/time: 8 bytes for timestamp + 4 bytes for nanoseconds
- Values are always 8-byte offsets (either to child nodes or data records)

## Key Type Handling

Different key types require specific encoding strategies to ensure both fixed-width storage and correct ordering:

### 1. Numeric Types

```rust
/// Integer key encoder
struct IntegerKeyEncoder;

impl KeyEncoder<i64> for IntegerKeyEncoder {
    fn encode(&self, value: &i64) -> Vec<u8> {
        let mut result = Vec::with_capacity(8);
        result.extend_from_slice(&value.to_le_bytes());
        result
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> std::cmp::Ordering {
        let a_val = i64::from_le_bytes([a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]]);
        let b_val = i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        a_val.cmp(&b_val)
    }

    fn encoded_size(&self) -> usize {
        8
    }
}

/// Float key encoder with NaN handling
struct FloatKeyEncoder;

impl KeyEncoder<f64> for FloatKeyEncoder {
    fn encode(&self, value: &f64) -> Vec<u8> {
        let ordered_float = ordered_float::OrderedFloat(*value);
        let bits = ordered_float.into_inner().to_bits();
        let mut result = Vec::with_capacity(8);
        result.extend_from_slice(&bits.to_le_bytes());
        result
    }

    // Compare implementation omitted for brevity
}
```

### 2. String Type

For string keys, a fixed-width prefix approach is used:

```rust
/// String key encoder with fixed prefix
struct StringKeyEncoder {
    prefix_length: usize,  // Typically 16 bytes
}

impl KeyEncoder<String> for StringKeyEncoder {
    fn encode(&self, value: &String) -> Vec<u8> {
        let mut result = vec![0u8; self.prefix_length];
        let bytes = value.as_bytes();
        let copy_len = std::cmp::min(bytes.len(), self.prefix_length);

        // Copy string prefix (with null padding if needed)
        result[..copy_len].copy_from_slice(&bytes[..copy_len]);
        result
    }

    // Additional methods for handling string collisions when prefixes match
    // would be implemented here
}
```

### 3. Date/Time Types

```rust
struct TimestampKeyEncoder;

impl KeyEncoder<chrono::DateTime<chrono::Utc>> for TimestampKeyEncoder {
    fn encode(&self, value: &chrono::DateTime<chrono::Utc>) -> Vec<u8> {
        let mut result = Vec::with_capacity(12);
        let timestamp = value.timestamp();
        let nanos = value.timestamp_subsec_nanos();

        result.extend_from_slice(&timestamp.to_le_bytes());
        result.extend_from_slice(&nanos.to_le_bytes());
        result
    }
}
```

## In-Memory and Disk-Based Query Implementation

### Core Interfaces

```rust
use thiserror::Error;
use std::fmt;

/// Error types for B-tree operations
#[derive(Error, Debug)]
pub enum BTreeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Block not found at offset {0}")]
    BlockNotFound(u64),

    #[error("Invalid tree structure: {0}")]
    InvalidStructure(String),

    #[error("Invalid node type: expected {expected}, got {actual}")]
    InvalidNodeType { expected: &'static str, actual: String },
}

/// Block storage interface
trait BlockStorage {
    fn read_block(&self, offset: u64) -> Result<Vec<u8>, BTreeError>;
    fn write_block(&self, offset: u64, data: &[u8]) -> Result<(), BTreeError>;
    fn allocate_block(&mut self) -> Result<u64, BTreeError>;
}

/// Key encoder for different data types
trait KeyEncoder<T> {
    fn encode(&self, value: &T) -> Vec<u8>;
    fn decode(&self, bytes: &[u8]) -> T;
    fn compare(&self, a: &[u8], b: &[u8]) -> std::cmp::Ordering;
    fn encoded_size(&self) -> usize;
}

/// B-tree node
struct Node {
    node_type: NodeType,
    entries: Vec<Entry>,
    next_node: Option<u64>,
}

/// B-tree index
struct BTree<K, V> {
    root: u64,
    key_encoder: Box<dyn KeyEncoder<K>>,
    storage: Box<dyn BlockStorage>,
    node_size: usize,
}
```

### Block Storage Implementations

#### 1. In-Memory Storage

```rust
use std::collections::HashMap;

/// Memory-based block storage for testing and small datasets
struct MemoryBlockStorage {
    blocks: HashMap<u64, Vec<u8>>,
    next_offset: u64,
}

impl BlockStorage for MemoryBlockStorage {
    fn read_block(&self, offset: u64) -> Result<Vec<u8>, BTreeError> {
        self.blocks.get(&offset)
            .cloned()
            .ok_or(BTreeError::BlockNotFound(offset))
    }

    fn write_block(&self, offset: u64, data: &[u8]) -> Result<(), BTreeError> {
        let mut data_copy = data.to_vec();
        // Ensure block is exactly 4KB
        data_copy.resize(4096, 0);
        self.blocks.insert(offset, data_copy);
        Ok(())
    }

    fn allocate_block(&mut self) -> Result<u64, BTreeError> {
        let offset = self.next_offset;
        self.next_offset += 4096; // Advance to next block
        Ok(offset)
    }
}
```

#### 2. File-Based Storage with Page Cache

```rust
use std::io::{Seek, SeekFrom, Read, Write};
use std::fs::File;
use lru::LruCache;

/// File-based block storage with LRU cache
struct CachedFileBlockStorage {
    file: File,
    cache: LruCache<u64, Vec<u8>>,
    cache_size: usize,
}

impl CachedFileBlockStorage {
    /// Create a new cached file storage
    fn new(file: File, cache_size: usize) -> Self {
        Self {
            file,
            cache: LruCache::new(cache_size),
            cache_size,
        }
    }
}

impl BlockStorage for CachedFileBlockStorage {
    fn read_block(&self, offset: u64) -> Result<Vec<u8>, BTreeError> {
        // Check cache first
        if let Some(data) = self.cache.get(&offset) {
            return Ok(data.clone());
        }

        // Cache miss - read from file
        let mut buffer = vec![0u8; 4096];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buffer)?;

        // Update cache
        self.cache.put(offset, buffer.clone());

        Ok(buffer)
    }

    fn write_block(&mut self, offset: u64, data: &[u8]) -> Result<(), BTreeError> {
        let mut data_copy = data.to_vec();
        // Ensure block is exactly 4KB
        data_copy.resize(4096, 0);

        // Write to file
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&data_copy)?;
        self.file.flush()?;

        // Update cache
        self.cache.put(offset, data_copy);

        Ok(())
    }

    fn allocate_block(&mut self) -> Result<u64, BTreeError> {
        // Get file length
        let offset = self.file.seek(SeekFrom::End(0))?;
        // Round up to next 4KB boundary if needed
        let aligned_offset = (offset + 4095) & !4095;
        if aligned_offset > offset {
            // Pad file to ensure alignment
            let padding = vec![0u8; (aligned_offset - offset) as usize];
            self.file.write_all(&padding)?;
        }
        Ok(aligned_offset)
    }
}
```

### Query Algorithms

#### 1. Exact Match Query

```rust
/// Helper function to deserialize a node from raw bytes
fn deserialize_node(data: &[u8]) -> Result<Node, BTreeError> {
    // Implementation details omitted
    // ...
    Err(BTreeError::Deserialization("Not implemented".to_string()))
}

/// Helper function to deserialize a value from raw bytes
fn deserialize_value<V>(data: &[u8]) -> Result<V, BTreeError> {
    // Implementation details omitted
    // ...
    Err(BTreeError::Deserialization("Not implemented".to_string()))
}

impl<K: PartialEq, V> BTree<K, V> {
    /// Search for a single key
    pub fn search(&self, key: &K) -> Result<Option<V>, BTreeError> {
        let encoded_key = self.key_encoder.encode(key);
        let mut current_node_offset = self.root;

        loop {
            // Read current node
            let node_data = self.storage.read_block(current_node_offset)?;
            let node = deserialize_node(&node_data)?;

            match node.node_type {
                NodeType::Internal => {
                    // Find child node to follow
                    match self.find_child_node(&node, &encoded_key)? {
                        Some(child_offset) => current_node_offset = child_offset,
                        None => return Ok(None), // Key not found
                    }
                },
                NodeType::Leaf => {
                    // Search for key in leaf node
                    return self.find_key_in_leaf(&node, &encoded_key);
                }
            }
        }
    }

    /// Find the appropriate child node in an internal node
    fn find_child_node(&self, node: &Node, key: &[u8]) -> Result<Option<u64>, BTreeError> {
        // Binary search to find the right child
        let mut low = 0;
        let mut high = node.entries.len();

        while low < high {
            let mid = low + (high - low) / 2;
            let entry = &node.entries[mid];

            match self.key_encoder.compare(&entry.key, key) {
                std::cmp::Ordering::Less => low = mid + 1,
                _ => high = mid,
            }
        }

        // If we're at the end, use the last entry's child
        if low == node.entries.len() {
            low = node.entries.len() - 1;
        }

        Ok(Some(node.entries[low].value))
    }

    /// Find a key in a leaf node
    fn find_key_in_leaf(&self, node: &Node, key: &[u8]) -> Result<Option<V>, BTreeError> {
        // Binary search for exact match
        match node.entries.binary_search_by(|entry| {
            self.key_encoder.compare(&entry.key, key)
        }) {
            Ok(idx) => {
                // Found exact match
                let value = deserialize_value::<V>(&node.entries[idx].value)?;
                Ok(Some(value))
            },
            Err(_) => {
                // No exact match found
                Ok(None)
            }
        }
    }
}
```

#### 2. Range Query

```rust
impl<K, V> BTree<K, V> {
    /// Range query to find all keys between start and end (inclusive)
    pub fn range_query(&self, start: &K, end: &K) -> Result<Vec<V>, BTreeError> {
        let encoded_start = self.key_encoder.encode(start);
        let encoded_end = self.key_encoder.encode(end);
        let mut results = Vec::new();

        // Find leaf containing start key
        let mut current_offset = self.find_leaf_containing(&encoded_start)?;

        loop {
            // Read current leaf node
            let node_data = self.storage.read_block(current_offset)?;
            let node = deserialize_node(&node_data)?;

            if node.node_type != NodeType::Leaf {
                return Err(BTreeError::InvalidNodeType {
                    expected: "Leaf",
                    actual: format!("{:?}", node.node_type)
                });
            }

            // Process entries in this leaf
            for entry in &node.entries {
                match self.key_encoder.compare(&entry.key, &encoded_end) {
                    // If entry key > end key, we're done
                    std::cmp::Ordering::Greater => return Ok(results),

                    // If entry key >= start key, include it in results
                    _ if self.key_encoder.compare(&entry.key, &encoded_start) != std::cmp::Ordering::Less => {
                        let value = deserialize_value::<V>(&entry.value)?;
                        results.push(value);
                    },

                    // Otherwise, skip this entry
                    _ => {}
                }
            }

            // Move to next leaf if available
            match node.next_node {
                Some(next_offset) => current_offset = next_offset,
                None => break, // No more leaves
            }
        }

        Ok(results)
    }

    /// Find the leaf node containing the given key
    fn find_leaf_containing(&self, key: &[u8]) -> Result<u64, BTreeError> {
        let mut current_offset = self.root;

        loop {
            let node_data = self.storage.read_block(current_offset)?;
            let node = deserialize_node(&node_data)?;

            match node.node_type {
                NodeType::Internal => {
                    current_offset = self.find_child_node(&node, key)?
                        .ok_or_else(|| BTreeError::InvalidStructure("Unable to find child node".to_string()))?;
                },
                NodeType::Leaf => {
                    return Ok(current_offset);
                }
            }
        }
    }
}
```

### Page Cache Optimization

The page caching strategy is crucial for performance when querying from disk. Key aspects include:

1. **Cache Size Tuning**
   - Cache size should be determined based on available memory and typical query patterns
   - For most workloads, caching 1,000-10,000 nodes (4-40 MB) provides a good balance

2. **Eviction Policy**
   - LRU (Least Recently Used) eviction works well for typical B-tree traversal patterns
   - Consider frequency-based policies for workloads with repeated access to specific nodes

3. **Prefetching Strategy**
   - When reading a leaf node for range queries, prefetch the next 1-2 leaf nodes
   - For internal nodes, consider prefetching child nodes that are likely to be accessed

```rust
/// Enhanced block storage with prefetching for range queries
impl CachedFileBlockStorage {
    /// Prefetch next leaf node(s) for range query
    fn prefetch_next_leaves(&mut self, node_offset: u64, count: usize) -> Result<(), BTreeError> {
        let mut current = node_offset;

        for _ in 0..count {
            // Read current node
            let node_data = self.read_block(current)?;
            let node = deserialize_node(&node_data)?;

            // If this is a leaf with a next pointer, prefetch it
            if let (NodeType::Leaf, Some(next_offset)) = (node.node_type, node.next_node) {
                // Only prefetch if not already in cache
                if !self.cache.contains(&next_offset) {
                    let mut buffer = vec![0u8; 4096];
                    self.file.seek(SeekFrom::Start(next_offset))?;
                    self.file.read_exact(&mut buffer)?;
                    self.cache.put(next_offset, buffer);
                }
                current = next_offset;
            } else {
                break;
            }
        }

        Ok(())
    }
}
```

## Integration with Packed R-tree

The B-tree index for attributes complements the packed R-tree index for spatial queries, enabling efficient filtering on both geographic and property-based criteria:

### Combined Query Strategy

For queries involving both spatial and attribute criteria, an optimal strategy is needed:

1. **Selectivity-Based Approach**
   - Determine which filter (spatial or attribute) is more selective
   - Apply the more selective filter first to minimize intermediate results
   - Apply the second filter to the reduced result set

2. **Query Planner Implementation**
   ```rust
   #[derive(Debug)]
   enum QueryPlan {
       SpatialFirst { bbox: BoundingBox, attr_filter: AttributeQuery },
       AttributeFirst { attr_query: AttributeQuery, spatial_filter: BoundingBox },
       SpatialOnly(BoundingBox),
       AttributeOnly(AttributeQuery),
       ScanAll,
   }

   fn plan_query(bbox: Option<BoundingBox>, attr_query: Option<AttributeQuery>) -> QueryPlan {
       match (bbox, attr_query) {
           (Some(bbox), Some(query)) => {
               // Estimate selectivity of each filter
               let spatial_selectivity = estimate_spatial_selectivity(&bbox);
               let attr_selectivity = estimate_attr_selectivity(&query);

               if spatial_selectivity < attr_selectivity {
                   QueryPlan::SpatialFirst {
                       bbox,
                       attr_filter: query,
                   }
               } else {
                   QueryPlan::AttributeFirst {
                       attr_query: query,
                       spatial_filter: bbox,
                   }
               }
           }
           (Some(bbox), None) => QueryPlan::SpatialOnly(bbox),
           (None, Some(query)) => QueryPlan::AttributeOnly(query),
           (None, None) => QueryPlan::ScanAll,
       }
   }
   ```

### Shared Resource Utilization

Both indices can benefit from shared strategies:

1. **Block Cache**
   - A single LRU cache can store both B-tree and R-tree nodes
   - Nodes are cached by file offset regardless of index type
   - Provides unified memory management across index types

2. **I/O Optimization**
   - Both indices use the same 4KB block alignment
   - Batched I/O operations can be used for both index types

## Performance Expectations

Based on the design, the following performance improvements are expected:

1. **I/O Reduction**
   - 5-10x fewer system calls compared to BST approach
   - Block-aligned access improves cache hit rates by 3-4x

2. **Memory Usage**
   - More efficient than loading the entire index
   - Cache hit rates of 80-95% expected for typical query patterns

3. **Practical Targets**
   - Local file system: 5-10x performance improvement over current BST
   - Memory usage: efficient with controlled growth based on cache size

## Future Work: HTTP Optimization

After completing and optimizing the in-memory and disk-based implementations, HTTP range request support will be added to enable efficient web-based access. This will include:

1. **Range Request Batching**
   - Grouping nearby blocks into single HTTP requests
   - Minimizing request overhead for tree traversal

2. **Progressive Loading**
   - Starting with minimal header information
   - Loading index nodes on demand
   - Batching feature retrieval

## Summary

This B-tree based attribute indexing approach addresses the performance issues identified in the current binary search tree implementation. By leveraging fixed-size, block-aligned nodes and cache-friendly access patterns, it significantly improves both in-memory and disk-based query performance for FlatCityBuf files. The initial implementation focuses on these core capabilities, with HTTP optimization to follow as future work.
