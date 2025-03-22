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
    fn encode(&self, value: &i64) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(8);
        result.extend_from_slice(&value.to_le_bytes());
        Ok(result)
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
    fn encode(&self, value: &f64) -> Result<Vec<u8>> {
        let bits = if value.is_nan() {
            // Handle NaN: Use a specific bit pattern
            u64::MAX
        } else {
            value.to_bits()
        };

        let mut result = Vec::with_capacity(8);
        result.extend_from_slice(&bits.to_le_bytes());
        Ok(result)
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
    fn encode(&self, value: &String) -> Result<Vec<u8>> {
        let mut result = vec![0u8; self.prefix_length];
        let bytes = value.as_bytes();
        let copy_len = std::cmp::min(bytes.len(), self.prefix_length);

        // Copy string prefix (with null padding if needed)
        result[..copy_len].copy_from_slice(&bytes[..copy_len]);
        Ok(result)
    }

    // Additional methods for handling string collisions when prefixes match
    // would be implemented here
}
```

### 3. Date/Time Types

```rust
struct TimestampKeyEncoder;

impl KeyEncoder<chrono::DateTime<chrono::Utc>> for TimestampKeyEncoder {
    fn encode(&self, value: &chrono::DateTime<chrono::Utc>) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(12);
        let timestamp = value.timestamp();
        let nanos = value.timestamp_subsec_nanos();

        result.extend_from_slice(&timestamp.to_le_bytes());
        result.extend_from_slice(&nanos.to_le_bytes());
        Ok(result)
    }
}
```

## Core Components of the Implementation

### 1. Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BTreeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Key error: {0}")]
    Key(#[from] KeyError),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Block not found at offset {0}")]
    BlockNotFound(u64),

    #[error("Invalid tree structure: {0}")]
    InvalidStructure(String),

    #[error("Invalid node type: expected {expected}, got {actual}")]
    InvalidNodeType { expected: &'static str, actual: String },

    #[error("Alignment error: offset {0} is not aligned to block size")]
    AlignmentError(u64),
}
```

### 2. Storage Interfaces

The implementation provides two primary storage backends:

#### In-Memory Storage

```rust
/// Memory-based block storage for testing and small datasets
pub struct MemoryBlockStorage {
    blocks: HashMap<u64, Vec<u8>>,
    next_offset: u64,
    block_size: usize,
}
```

#### File-Based Storage with LRU Cache

```rust
/// File-based block storage with LRU cache
pub struct CachedFileBlockStorage {
    file: RefCell<File>,
    cache: RefCell<LruCache<u64, Vec<u8>>>,
    block_size: usize,
}
```

Both implementations share a common interface:

```rust
pub trait BlockStorage {
    fn read_block(&self, offset: u64) -> Result<Vec<u8>>;
    fn write_block(&mut self, offset: u64, data: &[u8]) -> Result<()>;
    fn allocate_block(&mut self) -> Result<u64>;
    fn block_size(&self) -> usize;
    fn flush(&mut self) -> Result<()>;
}
```

### 3. B-tree Implementation

The core B-tree structure encapsulates the tree's behavior:

```rust
pub struct BTree<K, S> {
    root_offset: u64,
    storage: S,
    key_encoder: Box<dyn KeyEncoder<K>>,
    _phantom: PhantomData<K>,
}
```

With methods for:
- Opening existing B-trees
- Building B-trees from sorted entries
- Searching for exact matches
- Performing range queries
- Traversing the tree structure

### 4. B-tree Builder

For efficient bulk loading:

```rust
struct BTreeBuilder<K, S: BlockStorage> {
    storage: S,
    key_encoder: Box<dyn KeyEncoder<K>>,
    leaf_nodes: Vec<u64>,
    current_leaf: Node,
    key_size: usize,
    current_level: Vec<u64>,
    node_size: usize,
}
```

This builder constructs a B-tree from the bottom up, creating leaf nodes first, then internal nodes, resulting in a balanced and compact tree.

### 5. Query System

A comprehensive query system has been implemented to support complex queries across both B-tree (attribute) and R-tree (spatial) indices:

#### Query Conditions

```rust
pub enum Condition<T> {
    Exact(T),                               // Exact match
    Compare(ComparisonOp, T),               // Comparison (>, <, >=, <=)
    Range(T, T),                            // Range query
    In(Vec<T>),                             // Set membership
    Prefix(String),                         // String prefix match
    Predicate(Box<dyn Fn(&T) -> bool>),     // Custom predicate
}
```

#### Query Execution

The `QueryExecutor` aggregates multiple indices and optimizes query execution:

```rust
pub struct QueryExecutor<'a> {
    btree_indices: std::collections::HashMap<String, &'a dyn BTreeIndex>,
    rtree_index: Option<&'a dyn RTreeIndex>,
}
```

#### Query Planning

The system includes a query planner that determines the most efficient execution strategy:

```rust
enum QueryPlan {
    SpatialFirst { /* ... */ },
    AttributeFirst { /* ... */ },
    SpatialOnly(/* ... */),
    AttributeOnly(/* ... */),
    ScanAll,
    Logical(/* ... */),
}
```

#### Query Builder

A fluent API for building complex queries:

```rust
let query = QueryBuilder::new()
    .attribute("name", conditions::eq("Tower".to_string()), None)
    .attribute("height", conditions::between(100.0, 200.0), Some(LogicalOp::And))
    .spatial(10.0, 20.0, 30.0, 40.0, Some(LogicalOp::And))
    .build()
    .unwrap();
```

## Integration with Packed R-tree

The B-tree index for attributes complements the packed R-tree index for spatial queries, enabling efficient filtering on both geographic and property-based criteria:

### Combined Query Strategy

For queries involving both spatial and attribute criteria, an optimal strategy is needed:

1. **Selectivity-Based Approach**
   - Determine which filter (spatial or attribute) is more selective
   - Apply the more selective filter first to minimize intermediate results
   - Apply the second filter to the reduced result set

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

This B-tree based attribute indexing approach addresses the performance issues identified in the current binary search tree implementation. By leveraging fixed-size, block-aligned nodes and cache-friendly access patterns, it significantly improves both in-memory and disk-based query performance for FlatCityBuf files. The static nature of the B-tree structure makes it particularly suitable for read-only access patterns common in geographic data analysis. The initial implementation focuses on these core capabilities, with HTTP optimization to follow as future work.
