# Static B+Tree Query Implementation Plan

This document outlines the interfaces and module structure for query capabilities in the static-btree crate. This implementation has been completed.

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

The query module defines several core types:

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

/// Key types supported in typed queries
pub enum KeyType {
    Int32(i32),
    Int64(i64),
    UInt32(u32),
    UInt64(u64),
    Float32(ordered_float::OrderedFloat<f32>),
    Float64(ordered_float::OrderedFloat<f64>),
    Bool(bool),
    DateTime(chrono::DateTime<chrono::Utc>),
    StringKey20(FixedStringKey<20>),
    StringKey50(FixedStringKey<50>),
    StringKey100(FixedStringKey<100>),
}

/// A single query condition with heterogeneous key type support
pub struct TypedQueryCondition {
    pub field: String,       // Field name
    pub operator: Operator,  // Comparison operator
    pub key: KeyType,        // Key value with type information
}
```

## Implemented Query Modules

### query/memory.rs

The memory module provides in-memory index implementation:

```rust
/// In-memory index implementation
pub struct MemoryIndex<K: Key> {
    stree: Stree<K>,
    num_items: usize,
    branching_factor: u16,
    payload_size: usize,
}

impl<K: Key> MemoryIndex<K> {
    /// Build a memory index from a collection of entries
    pub fn build(entries: &[Entry<K>], branching_factor: u16) -> Result<Self>;

    /// Create a memory index from a buffer
    pub fn from_buf<R: Read + Seek>(
        buf: &mut R,
        num_items: usize,
        branching_factor: u16
    ) -> Result<Self>;

    /// Serialize the index to a writer
    pub fn serialize<W: Write>(&self, writer: &mut W) -> Result<usize>;

    /// Get the number of items in the index
    pub fn num_items(&self) -> usize;

    /// Get the branching factor of the index
    pub fn branching_factor(&self) -> u16;

    /// Get the payload size
    pub fn payload_size(&self) -> usize;

    /// Find exact matches for a key
    pub fn find_exact(&self, key: K) -> Result<Vec<u64>>;

    /// Find matches within a range
    pub fn find_range(
        &self,
        start: Option<K>,
        end: Option<K>
    ) -> Result<Vec<u64>>;
}

/// Container for multiple in-memory indices with heterogeneous key support
pub struct MemoryMultiIndex {
    indices: HashMap<String, Box<dyn TypedSearchIndex>>,
}

impl MemoryMultiIndex {
    /// Create a new empty multi-index
    pub fn new() -> Self;

    /// Add different index types
    pub fn add_i32_index(&mut self, field: String, index: MemoryIndex<i32>);
    pub fn add_i64_index(&mut self, field: String, index: MemoryIndex<i64>);
    pub fn add_u32_index(&mut self, field: String, index: MemoryIndex<u32>);
    pub fn add_u64_index(&mut self, field: String, index: MemoryIndex<u64>);
    pub fn add_f32_index(&mut self, field: String, index: MemoryIndex<OrderedFloat<f32>>);
    pub fn add_f64_index(&mut self, field: String, index: MemoryIndex<OrderedFloat<f64>>);
    pub fn add_bool_index(&mut self, field: String, index: MemoryIndex<bool>);
    pub fn add_datetime_index(&mut self, field: String, index: MemoryIndex<DateTime<Utc>>);
    pub fn add_string_index20(&mut self, field: String, index: MemoryIndex<FixedStringKey<20>>);
    pub fn add_string_index50(&mut self, field: String, index: MemoryIndex<FixedStringKey<50>>);
    pub fn add_string_index100(&mut self, field: String, index: MemoryIndex<FixedStringKey<100>>);

    /// Execute a query against the multi-index
    pub fn query(&self, conditions: &[TypedQueryCondition]) -> Result<Vec<u64>>;
}
```

### query/stream.rs

The stream module implements file-based index operations:

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

    /// Get the total serialized length of the index
    pub fn length(&self) -> usize;

    /// Find exact matches using a reader
    pub fn find_exact<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: K
    ) -> Result<Vec<u64>>;

    /// Find range matches using a reader
    pub fn find_range<R: Read + Seek>(
        &self,
        reader: &mut R,
        start: Option<K>,
        end: Option<K>
    ) -> Result<Vec<u64>>;
}

/// Container for multiple stream indices with heterogeneous key support
pub struct StreamMultiIndex {
    indices: HashMap<String, Box<dyn TypedStreamSearchIndex>>,
}

impl StreamMultiIndex {
    /// Create a new empty multi-index
    pub fn new() -> Self;

    /// Add different index types
    pub fn add_i32_index(&mut self, field: String, index: StreamIndex<i32>, index_len: usize);
    pub fn add_i64_index(&mut self, field: String, index: StreamIndex<i64>, index_len: usize);
    pub fn add_u32_index(&mut self, field: String, index: StreamIndex<u32>, index_len: usize);
    pub fn add_u64_index(&mut self, field: String, index: StreamIndex<u64>, index_len: usize);
    pub fn add_f32_index(&mut self, field: String, index: StreamIndex<OrderedFloat<f32>>, index_len: usize);
    pub fn add_f64_index(&mut self, field: String, index: StreamIndex<OrderedFloat<f64>>, index_len: usize);
    pub fn add_bool_index(&mut self, field: String, index: StreamIndex<bool>, index_len: usize);
    pub fn add_datetime_index(&mut self, field: String, index: StreamIndex<DateTime<Utc>>, index_len: usize);
    pub fn add_string_index20(&mut self, field: String, index: StreamIndex<FixedStringKey<20>>, index_len: usize);
    pub fn add_string_index50(&mut self, field: String, index: StreamIndex<FixedStringKey<50>>, index_len: usize);
    pub fn add_string_index100(&mut self, field: String, index: StreamIndex<FixedStringKey<100>>, index_len: usize);

    /// Execute a query using the provided reader
    pub fn query<R: Read + Seek>(
        &self,
        reader: &mut R,
        conditions: &[TypedQueryCondition]
    ) -> Result<Vec<u64>>;
}
```

### query/http.rs (with `http` feature)

The HTTP module implements remote index operations:

```rust
/// HTTP-based index for remote access
#[cfg(feature = "http")]
#[derive(Debug, Clone)]
pub struct HttpIndex<K: Key> {
    /// total number of items in the tree
    num_items: usize,
    /// branching factor of the B+tree
    branching_factor: u16,
    /// byte offset where the index begins
    index_offset: usize,
    /// size of the serialized index section
    attr_index_size: usize,
    /// size of the payload section (features data)
    payload_size: usize,
    /// threshold for combining HTTP requests to reduce roundtrips
    combine_request_threshold: usize,
    _marker: PhantomData<K>,
}

#[cfg(feature = "http")]
impl<K: Key> HttpIndex<K> {
    /// Create a new HTTP index descriptor with all necessary metadata
    pub fn new(
        num_items: usize,
        branching_factor: u16,
        index_offset: usize,
        attr_index_size: usize,
        payload_size: usize,
        combine_request_threshold: usize,
    ) -> Self;

    /// Find exact matches for a key via HTTP
    pub async fn find_exact<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        key: K,
    ) -> Result<Vec<u64>>;

    /// Find all items in [start..end] via HTTP. At least one bound is required.
    pub async fn find_range<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        start: Option<K>,
        end: Option<K>,
    ) -> Result<Vec<u64>>;
}

/// Trait for HTTP indices with heterogeneous key support
#[async_trait]
pub trait TypedHttpSearchIndex<T: AsyncHttpRangeClient + Send + Sync>:
    Send + Sync + std::fmt::Debug
{
    /// Execute a typed query condition over HTTP with a specific HTTP client
    async fn execute_query_condition(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        condition: &TypedQueryCondition,
    ) -> Result<Vec<u64>>;
}

/// Container for multiple HTTP indices keyed by field name
#[derive(Debug)]
pub struct HttpMultiIndex<T: AsyncHttpRangeClient + Send + Sync> {
    indices: HashMap<String, Box<dyn TypedHttpSearchIndex<T>>>,
}

#[cfg(feature = "http")]
impl<T: AsyncHttpRangeClient + Send + Sync> HttpMultiIndex<T> {
    /// Create a new empty HTTP multi-index
    pub fn new() -> Self;

    /// Add an index for any supported key type
    pub fn add_index<K: Key + 'static>(&mut self, field: String, index: HttpIndex<K>)
    where
        HttpIndex<K>: TypedHttpSearchIndex<T> + 'static;

    /// Execute a multi-condition query by AND-ing all conditions
    pub async fn query(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        conditions: &[TypedQueryCondition],
    ) -> Result<Vec<u64>>;
}
```

## Implementation Notes

1. **Implemented Features:**
   - The implementation supports all planned key types including integers, floats, booleans, DateTimes, and fixed-length strings.
   - Each index type (memory, stream, HTTP) offers equivalent functionality with appropriate interfaces.
   - The HTTP implementation is completely non-blocking and uses async patterns throughout.

2. **Key Improvements:**
   - The HTTP implementation supports all key types through a macro-based trait implementation system.
   - Operator functionality has been fully implemented with proper handling of equality, inequality, and range comparisons.
   - Result set handling performs proper intersection logic for combining multiple conditions.

3. **Type Safety:**
   - The implementation uses Rust's type system to ensure type safety at compile time.
   - Runtime type checking is used for the heterogeneous typed query conditions.

4. **Testing:**
   - Comprehensive test coverage for all index types and query conditions.
   - End-to-end tests verify that HTTP-based querying works correctly with real data.

## Integration Strategy

The query module provides a complete set of interfaces for querying indices of different types. Integration with fcb_core is the next step, which will involve:

1. Creating wrapper types that map to the fcb_core API
2. Implementing the necessary compatibility layer
3. Updating the build process to use static-btree for index construction
4. Adding performance benchmarks to compare with the existing implementation

## Implementation Status

All query module components are fully implemented and tested, including:

- Memory-based indices
- Stream-based indices
- HTTP-based indices (feature-gated)
- End-to-end tests for all index types

The next phase is integration with fcb_core as outlined in the implementation_integrate_w_flatcitybuf.md document.
