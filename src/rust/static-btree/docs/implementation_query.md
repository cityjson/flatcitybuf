# Static B+Tree Query Implementation Plan

This document outlines the interfaces and module structure for implementing query capabilities in the static-btree crate.

## Module Structure

```
static-btree/
├── src/
│   ├── query/
│   │   ├── mod.rs         // Re-exports from submodules
│   │   ├── types.rs       // Query types and traits
│   │   ├── memory.rs      // In-memory index implementation
│   │   ├── stream.rs      // Stream-based index implementation
│   │   └── http.rs        // HTTP-based index implementation (conditional)
```

## Core Interfaces

### query/types.rs

```rust
/// Comparison operators for queries
pub enum Operator {
    Eq,    // Equal
    Ne,    // Not equal
    Gt,    // Greater than
    Lt,    // Less than
    Ge,    // Greater than or equal
    Le,    // Less than or equal
}

/// A single query condition
pub struct QueryCondition<K: Key> {
    pub field: String,      // Field name
    pub operator: Operator, // Comparison operator
    pub key: K,             // Key value
}

/// A complete query with multiple conditions
pub struct Query<K: Key> {
    pub conditions: Vec<QueryCondition<K>>,
}

/// Core trait for index searching capabilities
pub trait SearchIndex<K: Key> {
    /// Find exact matches for a key
    fn find_exact(&self, key: K) -> Result<Vec<u64>>;

    /// Find matches within a range (inclusive start, inclusive end)
    fn find_range(
        &self,
        start: Option<K>,
        end: Option<K>
    ) -> Result<Vec<u64>>;
}

/// Trait for multi-index query capabilities
pub trait MultiIndex<K: Key> {
    /// Execute a query and return matching offsets
    fn query(&self, query: &Query<K>) -> Result<Vec<u64>>;
}
```

### query/memory.rs

```rust
/// In-memory index implementation
pub struct MemoryIndex<K: Key> {
    stree: Stree<K>,
    num_items: usize,
    branching_factor: u16,
}

impl<K: Key> MemoryIndex<K> {
    /// Create a new memory index from an existing Stree
    pub fn new(stree: Stree<K>) -> Self;

    /// Build a memory index from a collection of entries
    pub fn build(entries: &[Entry<K>], branching_factor: u16) -> Result<Self>;
}

impl<K: Key> SearchIndex<K> for MemoryIndex<K> {
    // Implementation of SearchIndex trait
}

/// Container for multiple in-memory indices
pub struct MemoryMultiIndex<K: Key> {
    indices: HashMap<String, MemoryIndex<K>>,
}

impl<K: Key> MemoryMultiIndex<K> {
    /// Create a new empty multi-index
    pub fn new() -> Self;

    /// Add an index for a specific field
    pub fn add_index(&mut self, field: String, index: MemoryIndex<K>);
}

impl<K: Key> MultiIndex<K> for MemoryMultiIndex<K> {
    // Implementation of MultiIndex trait
}
```

### query/stream.rs

```rust
/// Stream-based index for file access
pub struct StreamIndex<K: Key> {
    num_items: usize,
    branching_factor: u16,
    index_offset: u64,
    payload_size: usize,
}

impl<K: Key> StreamIndex<K> {
    /// Create a new stream index with metadata
    pub fn new(
        num_items: usize,
        branching_factor: u16,
        index_offset: u64,
        payload_size: usize
    ) -> Self;
}

/// SearchIndex trait implementation requires a reader
impl<K: Key, R: Read + Seek> SearchIndex<K> for (StreamIndex<K>, &mut R) {
    // Implementation of SearchIndex trait using reader
}

/// Container for multiple stream indices
pub struct StreamMultiIndex<K: Key> {
    indices: HashMap<String, StreamIndex<K>>,
}

impl<K: Key> StreamMultiIndex<K> {
    /// Create a new empty multi-index
    pub fn new() -> Self;

    /// Add an index for a specific field
    pub fn add_index(&mut self, field: String, index: StreamIndex<K>);

    /// Execute a query using the provided reader
    pub fn query_with_reader<R: Read + Seek>(
        &self,
        reader: &mut R,
        query: &Query<K>
    ) -> Result<Vec<u64>>;
}
```

### query/http.rs (with `http` feature)

```rust
/// HTTP-based index for remote access
#[cfg(feature = "http")]
pub struct HttpIndex<K: Key> {
    num_items: usize,
    branching_factor: u16,
    index_offset: usize,
    attr_index_size: usize,
    payload_size: usize,
    combine_request_threshold: usize,
}

#[cfg(feature = "http")]
impl<K: Key> HttpIndex<K> {
    /// Create a new HTTP index with metadata
    pub fn new(
        num_items: usize,
        branching_factor: u16,
        index_offset: usize,
        attr_index_size: usize,
        payload_size: usize,
        combine_request_threshold: usize
    ) -> Self;

    /// Find exact matches using HTTP client
    pub async fn find_exact<C: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<C>,
        key: K
    ) -> Result<Vec<u64>>;

    /// Find range matches using HTTP client
    pub async fn find_range<C: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<C>,
        start: Option<K>,
        end: Option<K>
    ) -> Result<Vec<u64>>;
}

/// Container for multiple HTTP indices
#[cfg(feature = "http")]
pub struct HttpMultiIndex<K: Key> {
    indices: HashMap<String, HttpIndex<K>>,
}

#[cfg(feature = "http")]
impl<K: Key> HttpMultiIndex<K> {
    /// Create a new empty multi-index
    pub fn new() -> Self;

    /// Add an index for a specific field
    pub fn add_index(&mut self, field: String, index: HttpIndex<K>);

    /// Execute a query using the provided HTTP client
    pub async fn query_with_client<C: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<C>,
        query: &Query<K>
    ) -> Result<Vec<u64>>;
}
```

## Integration Notes

1. Each specialized index type (Memory, Stream, HTTP) is a thin wrapper around the existing Stree functionality.

2. The implementation should leverage the existing `find_exact` and `find_range` methods in Stree.

3. Each query condition uses the `K: Key` trait, allowing for type-safe queries.

4. Result sets from different conditions are combined using set intersections in the MultiIndex implementations.

## Implementation Strategy

1. Implement the core traits and types in `types.rs`
2. Implement the in-memory index in `memory.rs`
3. Implement the stream-based index in `stream.rs`
4. Implement the HTTP-based index in `http.rs` (conditionally)
5. Test each module separately and then together
