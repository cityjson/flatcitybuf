# Static B-Tree Implementation - Task Handover Document

## Current Status

We've successfully implemented the core Static B+Tree functionality including:

- Complete core tree implementation with `find_exact` and `find_range` operations
- Payload handling for duplicate keys
- HTTP-based search operations with request batching
- Detailed query implementation design

The next phase involves implementing the query module to provide a higher-level interface for index operations and to prepare for integration with fcb_core.

## Query Module Implementation

The query module will provide a more abstract interface to the Static B+Tree, allowing it to be used as a drop-in replacement for the current BST implementation in fcb_core.

### Module Structure

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

### Core Interfaces

#### 1. Query Types (types.rs)

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
    fn find_range(&self, start: Option<K>, end: Option<K>) -> Result<Vec<u64>>;
}

/// Trait for multi-index query capabilities
pub trait MultiIndex<K: Key> {
    /// Execute a query and return matching offsets
    fn query(&self, query: &Query<K>) -> Result<Vec<u64>>;
}
```

#### 2. Memory-based Index (memory.rs)

The in-memory implementation will wrap the existing Stree functionality, providing:

```rust
/// In-memory index implementation
pub struct MemoryIndex<K: Key> {
    stree: Stree<K>,
    num_items: usize,
    branching_factor: u16,
}

/// Container for multiple in-memory indices
pub struct MemoryMultiIndex<K: Key> {
    indices: HashMap<String, MemoryIndex<K>>,
}
```

#### 3. Stream-based Index (stream.rs)

For file-based operations, we'll implement:

```rust
/// Stream-based index for file access
pub struct StreamIndex<K: Key> {
    num_items: usize,
    branching_factor: u16,
    index_offset: u64,
    payload_size: usize,
}

/// Container for multiple stream indices
pub struct StreamMultiIndex<K: Key> {
    indices: HashMap<String, StreamIndex<K>>,
}
```

#### 4. HTTP-based Index (http.rs)

For remote operations, with the `http` feature enabled:

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

/// Container for multiple HTTP indices
#[cfg(feature = "http")]
pub struct HttpMultiIndex<K: Key> {
    indices: HashMap<String, HttpIndex<K>>,
}
```

## Implementation Tasks

### 1. Base Module Setup

- Create the query module directory structure
- Implement the core types (Operator, QueryCondition, Query)
- Define the SearchIndex and MultiIndex traits

### 2. Memory Index Implementation

- Implement MemoryIndex as a wrapper around Stree
- Create MemoryMultiIndex for managing multiple indices
- Add unit tests for basic query operations

### 3. Stream Index Implementation

- Implement StreamIndex for file-based operations
- Create StreamMultiIndex with reader-based query methods
- Test with file I/O to verify correct behavior

### 4. HTTP Index Implementation

- Implement HttpIndex for HTTP-based operations
- Create HttpMultiIndex with async query methods
- Test with mock HTTP clients to ensure correct behavior

### 5. Integration Layer

- Create the compatibility layer as outlined in implementation_integrate_w_flatcitybuf.md
- Implement wrapper types that match the BST API
- Develop tests comparing the behavior with the existing BST implementation

## Implementation Guidelines

### 1. Type Safety

- Leverage Rust's type system, especially with the Key trait
- Ensure proper error handling without unwrap() calls
- Use Result types for all fallible operations

### 2. Optimization Focus

- Index operations should minimize allocations
- Search operations should read only the necessary nodes
- HTTP operations should batch requests when possible

### 3. Testing Strategy

- Write tests for each component before implementation
- Include both unit tests and integration tests
- Create comparative tests against the BST implementation

### 4. Documentation

- Document all public APIs with rustdoc
- Explain design decisions in comments
- Update the overview and progress documents as work advances

## Resources

- **implementation_query.md**: Detailed plan for the query implementation
- **implementation_integrate_w_flatcitybuf.md**: Strategy for integrating with fcb_core
- **overview.md**: Comprehensive overview of the static-btree crate
- **stree.rs**: Core tree implementation to leverage for index operations

## Coordination Points

Coordinate with the fcb_core team on:

1. The exact interface requirements for the compatibility layer
2. Performance expectations for the new implementation
3. Testing strategies to ensure correct behavior when integrated

Good luck with the implementation!
