# FlatCityBuf Implementation Strategy

## Core Architecture

FlatCityBuf uses a multi-layered architecture for efficient spatial data indexing and querying:

1. **Index Layer**: Provides efficient key-value lookups with support for exact matches and range queries
2. **Query Layer**: Handles complex queries with multiple conditions across different indices
3. **Serialization Layer**: Enables persistent storage and streaming access to indices
4. **HTTP Layer**: Allows remote access to indices via HTTP range requests

```mermaid
graph TD
    subgraph "Application Layer"
        A[Client Application]
    end

    subgraph "Query Layer"
        B[Query]
        C[QueryCondition]
        D[MultiIndex]
        E[StreamableMultiIndex]
    end

    subgraph "Index Layer"
        F[BufferedIndex]
        G[IndexMeta]
        H[TypeErasedIndexMeta]
        I[SearchableIndex]
        J[TypedSearchableIndex]
    end

    subgraph "Serialization Layer"
        K[ByteSerializable]
        L[ByteSerializableType]
        M[IndexSerializable]
    end

    subgraph "HTTP Layer"
        N[AsyncHttpRangeClient]
        O[HttpSearchResultItem]
    end

    A -->|uses| B
    B -->|contains| C
    D -->|executes| B
    E -->|executes| B
    D -->|contains| I
    E -->|contains| H
    F -->|implements| I
    F -->|implements| J
    G -->|implements| J
    H -->|type-erased| G
    F -->|serializes via| M
    I -->|uses| K
    J -->|uses| K
    K -->|returns| L
    E -->|uses| N
    N -->|returns| O
```

## Binary Search Tree Streaming Process

The binary search tree (BST) in FlatCityBuf is designed for efficient streaming access, allowing queries to be executed without loading the entire index into memory. This section illustrates how the BST is structured and accessed during streaming queries.

### BST Structure and File Layout

```mermaid
graph TD
    subgraph "File Representation (Memory Buffer)"
        direction LR
        H["Type ID<br>(4 bytes)"] --- I["Entry Count<br>(8 bytes)"] --- J["Entry 1"] --- K["Entry 2"] --- L["Entry 3"] --- M["... Entry n"]

        subgraph "Entry Structure"
            direction LR
            N["Key Length<br>(8 bytes)"] --- O["Key Bytes<br>(variable e.g. 'delft' or <br>'rotterdam' for string keys)"] --- P["Offset Count<br>(8 bytes)"] --- Q["Offset Values<br>(8 bytes each)"]
        end

        J -.-> N
    end
```

The BST is serialized to a file in a format that preserves the sorted order of keys, allowing for efficient binary search directly on the serialized data. Each entry in the file contains a key and its associated offsets. The horizontal layout of the file representation reflects how the data is stored sequentially in memory or on disk as a continuous buffer.

### Binary Search on Serialized BST

```mermaid
graph TD
    subgraph "Binary Search Process"
        A["Start: left=0, right=entry_count-1"]
        B["Calculate mid = (left + right) / 2"]
        C["Seek to entry at position mid"]
        D["Read key at mid position"]
        E["Compare key with search key"]

        A --> B --> C --> D --> E

        E -->|"key < search_key"| F["left = mid + 1"]
        E -->|"key > search_key"| G["right = mid - 1"]
        E -->|"key = search_key"| H["Found match!"]

        F --> B
        G --> B
        H --> I["Read offsets"]
    end

    subgraph "File Navigation"
        J["File with serialized BST"]
        K["Cursor position"]

        J --> K

        L["Entry 1"]
        M["Entry 2 (mid)"]
        N["Entry 3"]
        O["Entry 4"]
        P["Entry 5"]

        J --> L --> M --> N --> O --> P

        K -.->|"1.Initial seek"| M
        K -.->|"2.Read key"| M
        K -.->|"3.Compare"| M
        K -.->|"4.Move to new mid"| O
    end
```

During a binary search:
1. The algorithm starts with the full range of entries
2. It calculates the middle position and seeks to that entry in the file
3. It reads the key at that position and compares it with the search key
4. Based on the comparison, it narrows the search range and repeats
5. When a match is found, it reads the associated offsets

### Range Query Process

```mermaid
graph TD
    subgraph "Range Query Process"
        A["Find lower bound"]
        B["Find upper bound"]
        C["Scan entries between bounds"]
        D["Collect all matching offsets"]

        A --> B --> C --> D
    end

    subgraph "File Layout with Range Query"
        E["Serialized BST"]

        F["Entry 1"]
        G["Entry 2 (lower bound)"]
        H["Entry 3"]
        I["Entry 4"]
        J["Entry 5 (upper bound)"]
        K["Entry 6"]

        E --> F --> G --> H --> I --> J --> K

        L["Cursor"]

        L -.->|"1.Binary search for lower bound"| G
        L -.->|"2.Sequential scan"| H
        L -.->|"3.Sequential scan"| I
        L -.->|"4.Stop at upper bound"| J
    end
```

For range queries:
1. Binary search is used to find the lower bound
2. Another binary search finds the upper bound
3. The algorithm then sequentially scans all entries between these bounds
4. All matching offsets are collected and returned

### StreamableMultiIndex Query Process

```mermaid
graph TD
    subgraph "StreamableMultiIndex"
        A["Query with multiple conditions"]

        B["Condition 1: field=height, op=Gt, value=20.0"]
        C["Condition 2: field=id, op=Eq, value='building1'"]

        A --> B
        A --> C

        D["Index 1: height"]
        E["Index 2: id"]

        B -->|"Execute on"| D
        C -->|"Execute on"| E

        F["Results from height index"]
        G["Results from id index"]

        D --> F
        E --> G

        H["Intersect results"]

        F --> H
        G --> H

        I["Final result set"]

        H --> I
    end
```

When executing a query with multiple conditions:
1. The StreamableMultiIndex saves the current cursor position
2. For each condition, it seeks to the appropriate index in the file
3. It executes the query on that index and collects the results
4. It intersects the results from all conditions to find records that match all criteria
5. Finally, it restores the original cursor position

### Memory Efficiency in Streaming Queries

```mermaid
graph TD
    subgraph "Memory Usage Comparison"
        A["BufferedIndex (In-Memory)"]
        B["StreamableMultiIndex (Streaming)"]

        A -->|"Memory Usage"| C["Entire index loaded in memory"]
        B -->|"Memory Usage"| D["Only metadata in memory"]

        C -->|"Scales with"| E["Size of index"]
        D -->|"Scales with"| F["Number of indices"]

        G["Large Dataset (1M entries)"]

        G -->|"With BufferedIndex"| H["High memory usage"]
        G -->|"With StreamableMultiIndex"| I["Low memory usage"]
    end

    subgraph "Metadata vs. Full Index"
        J["TypeErasedIndexMeta"]
        K["Full BufferedIndex"]

        J -->|"Contains"| L["entry_count: u64"]
        J -->|"Contains"| M["size: u64"]
        J -->|"Contains"| N["type_id: ByteSerializableType"]

        K -->|"Contains"| O["entries: Vec<KeyValue<T>>"]
        O -->|"Contains"| P["Many key-value pairs"]

        Q["Memory Footprint"]

        J -->|"Small (constant)"| Q
        K -->|"Large (proportional to data)"| Q
    end
```

The streaming approach offers significant memory efficiency:
1. BufferedIndex loads the entire index into memory, which can be problematic for large datasets
2. StreamableMultiIndex only keeps metadata in memory, using file I/O to access the actual data
3. This allows FlatCityBuf to handle datasets that would be too large to fit entirely in memory

## Type System

The type system in FlatCityBuf is built around the `ByteSerializable` trait, which provides methods for converting types to and from byte representations:

```rust
pub trait ByteSerializable: Send + Sync {
    fn to_bytes(&self) -> Vec<u8>;
    fn from_bytes(bytes: &[u8]) -> Self;
    fn value_type(&self) -> ByteSerializableType;
}
```

Key features:
- Implemented for common types (primitives, String, DateTime, etc.)
- Uses `OrderedFloat` for floating-point comparisons to handle NaN values
- Preserves type information in serialized format via `ByteSerializableType` enum
- Enables type-specific comparisons during binary search operations

The `ByteSerializableType` enum represents all supported types:

```rust
pub enum ByteSerializableType {
    I64, I32, I16, I8,
    U64, U32, U16, U8,
    F64, F32,
    Bool,
    String,
    NaiveDateTime, NaiveDate, DateTime,
}
```

Each type has a unique ID that is stored in the serialized index, allowing for correct type-specific comparisons when querying.

## Index Implementation

### BufferedIndex

The `BufferedIndex<T>` is an in-memory index implementation that stores key-value pairs where:
- Keys are of type `T` (which must be `Ord + ByteSerializable`)
- Values are vectors of offsets (`Vec<ValueOffset>`) pointing to the actual data

```rust
pub struct BufferedIndex<T: Ord + ByteSerializable + Send + Sync + 'static> {
    pub entries: Vec<KeyValue<T>>,
}
```

Key features:
- Maintains keys in sorted order for efficient binary search
- Supports exact match and range queries
- Fully type-aware with generic parameter `T`
- Implements both `SearchableIndex` and `TypedSearchableIndex` traits

### IndexMeta

The `IndexMeta<T>` structure provides metadata about an index and enables streaming access without loading the entire index into memory:

```rust
pub struct IndexMeta<T: Ord + ByteSerializable + Send + Sync + 'static> {
    /// Number of entries in the index.
    pub entry_count: u64,
    /// Total size of the index in bytes.
    pub size: u64,
    /// Phantom data to represent the type parameter.
    pub _phantom: std::marker::PhantomData<T>,
}
```

Key features:
- Stores only metadata, not the actual index data
- Provides methods for streaming queries directly from a file or HTTP source
- Uses binary search for efficient lookups
- Implements `TypedStreamableIndex<T>` trait for type-safe streaming access

### TypeErasedIndexMeta

The `TypeErasedIndexMeta` structure is a type-erased version of `IndexMeta<T>` that can work with any `ByteSerializable` type:

```rust
pub struct TypeErasedIndexMeta {
    /// Number of entries in the index.
    pub entry_count: u64,
    /// Total size of the index in bytes.
    pub size: u64,
    /// Type identifier for the index.
    pub type_id: ByteSerializableType,
}
```

Key features:
- Enables storing different index types in a single collection
- Performs type-specific comparisons based on the `type_id`
- Used by `StreamableMultiIndex` to handle multiple indices with different key types

## Query System

### Query Structure

Queries are represented by the `Query` struct which contains a list of conditions:

```rust
pub struct Query {
    pub conditions: Vec<QueryCondition>,
}

pub struct QueryCondition {
    /// The field identifier (e.g., "id", "name", etc.)
    pub field: String,
    /// The comparison operator.
    pub operator: Operator,
    /// The key value as a byte vector (obtained via ByteSerializable::to_bytes).
    pub key: Vec<u8>,
}
```

The system supports six comparison operators:
- `Eq`: Equal to
- `Ne`: Not equal to
- `Gt`: Greater than
- `Lt`: Less than
- `Ge`: Greater than or equal to
- `Le`: Less than or equal to

```mermaid
graph TD
    A[Query] -->|contains| B[QueryCondition]
    B -->|has| C[field: String]
    B -->|has| D[operator: Operator]
    B -->|has| E[key: Vec<u8>]
    D -->|can be| F[Eq]
    D -->|can be| G[Ne]
    D -->|can be| H[Gt]
    D -->|can be| I[Lt]
    D -->|can be| J[Ge]
    D -->|can be| K[Le]
```

### MultiIndex

The `MultiIndex` provides a way to query multiple indices simultaneously:

```rust
pub struct MultiIndex {
    /// A mapping from field names to their corresponding index.
    pub indices: HashMap<String, Box<dyn SearchableIndex>>,
}
```

Key features:
- Stores multiple indices by field name
- Executes queries across all relevant indices
- Intersects results to find records that match all conditions
- Uses trait objects (`Box<dyn SearchableIndex>`) for type erasure

### StreamableMultiIndex

The `StreamableMultiIndex` extends the concept of `MultiIndex` for streaming access:

```rust
pub struct StreamableMultiIndex {
    /// A mapping from field names to their corresponding index metadata.
    pub indices: HashMap<String, TypeErasedIndexMeta>,
    /// A mapping from field names to their offsets in the file.
    pub index_offsets: HashMap<String, u64>,
}
```

Key features:
- Stores index metadata and offsets instead of the actual indices
- Enables streaming queries without loading entire indices into memory
- Properly manages cursor positioning when querying multiple indices
- Supports the same query operators as `MultiIndex`

## Streaming Query Process

The streaming query process follows these steps:

```mermaid
sequenceDiagram
    participant Client
    participant StreamableMultiIndex
    participant TypeErasedIndexMeta
    participant FileReader

    Client->>StreamableMultiIndex: stream_query(query)
    StreamableMultiIndex->>FileReader: Save current position

    loop For each condition in query
        StreamableMultiIndex->>StreamableMultiIndex: Get index metadata and offset
        StreamableMultiIndex->>FileReader: Seek to index offset

        alt Operator is Eq
            StreamableMultiIndex->>TypeErasedIndexMeta: stream_query_exact(key)
            TypeErasedIndexMeta->>FileReader: Binary search for key
            FileReader-->>TypeErasedIndexMeta: Return matching offsets
        else Operator is range-based
            StreamableMultiIndex->>TypeErasedIndexMeta: stream_query_range(lower, upper)
            TypeErasedIndexMeta->>FileReader: Find bounds and scan range
            FileReader-->>TypeErasedIndexMeta: Return matching offsets
        end

        TypeErasedIndexMeta-->>StreamableMultiIndex: Return offsets
        StreamableMultiIndex->>StreamableMultiIndex: Add to candidate sets
    end

    StreamableMultiIndex->>StreamableMultiIndex: Intersect all candidate sets
    StreamableMultiIndex->>FileReader: Restore original position
    StreamableMultiIndex-->>Client: Return matching offsets
```

1. **Initialization**:
   - Save the current file position
   - Identify the relevant indices for the query conditions

2. **Query Execution**:
   - For each condition in the query:
     - Find the corresponding index metadata and offset
     - Seek to the correct offset in the file
     - Execute the appropriate query method (exact or range)
     - Collect the results into a candidate set

3. **Result Combination**:
   - Intersect all candidate sets to find records that match all conditions
   - Sort the results for consistent output

4. **Cursor Management**:
   - Restore the original file position after the query is complete

## HTTP Streaming Queries

The HTTP implementation extends the streaming concept to remote data sources:

```mermaid
sequenceDiagram
    participant Client
    participant StreamableMultiIndex
    participant TypeErasedIndexMeta
    participant HttpClient
    participant Server

    Client->>StreamableMultiIndex: http_stream_query(query)

    loop For each condition in query
        StreamableMultiIndex->>StreamableMultiIndex: Get index metadata and offset
        StreamableMultiIndex->>HttpClient: Request index range
        HttpClient->>Server: HTTP Range Request
        Server-->>HttpClient: Partial content response

        alt Operator is Eq
            StreamableMultiIndex->>TypeErasedIndexMeta: http_stream_query_exact(key)
            TypeErasedIndexMeta->>HttpClient: Binary search (multiple range requests)
            HttpClient->>Server: HTTP Range Requests
            Server-->>HttpClient: Partial content responses
            HttpClient-->>TypeErasedIndexMeta: Return matching offsets
        else Operator is range-based
            StreamableMultiIndex->>TypeErasedIndexMeta: http_stream_query_range(lower, upper)
            TypeErasedIndexMeta->>HttpClient: Find bounds and request ranges
            HttpClient->>Server: HTTP Range Requests
            Server-->>HttpClient: Partial content responses
            HttpClient-->>TypeErasedIndexMeta: Return matching offsets
        end

        TypeErasedIndexMeta-->>StreamableMultiIndex: Return offsets
        StreamableMultiIndex->>StreamableMultiIndex: Add to candidate sets
    end

    StreamableMultiIndex->>StreamableMultiIndex: Intersect all candidate sets
    StreamableMultiIndex-->>Client: Return matching HttpSearchResultItems
```

Key components:

1. **AsyncHttpRangeClient**:
   - Makes HTTP range requests to fetch specific byte ranges
   - Buffers data to minimize the number of requests
   - Handles network errors and retries

2. **HTTP Streaming Queries**:
   - Follow the same pattern as file-based streaming queries
   - Use range requests to fetch only the necessary parts of the index
   - Return `HttpSearchResultItem` objects with byte ranges for feature data

3. **Batching Strategy**:
   - Group nearby offsets to reduce the number of HTTP requests
   - Use a threshold parameter to control the maximum distance between offsets in a batch
   - Balance between minimizing requests and avoiding excessive data transfer

## Serialization Strategy

### Format

Each index is serialized with the following structure:

```
[Type Identifier (4 bytes)]
[Number of Entries (8 bytes)]
For each entry:
  [Key Length (8 bytes)]
  [Key Bytes (variable)]
  [Number of Offsets (8 bytes)]
  For each offset:
    [Offset Value (8 bytes)]
```

This format:
- Preserves type information for correct deserialization
- Maintains the sorted order of keys
- Allows efficient binary search directly on the serialized data
- Supports streaming access without loading the entire index

## Integration with CityJSON

FlatCityBuf is designed to optimize CityJSON for cloud-based applications:

1. **Binary Encoding**:
   - Reduces file size by 50-70% compared to JSON-based CityJSONSeq
   - Preserves all semantic information from the original CityJSON

2. **Spatial Indexing**:
   - Implements Hilbert R-tree for efficient spatial queries
   - Enables fast retrieval of city objects by location

3. **Attribute Indexing**:
   - Creates indices for commonly queried attributes
   - Supports complex queries combining spatial and attribute conditions

4. **Cloud Optimization**:
   - Enables partial data retrieval via HTTP range requests
   - Reduces bandwidth usage by downloading only needed data
   - Improves loading times for web applications

## Performance Considerations

1. **Memory Efficiency**:
   - Only metadata is loaded into memory, not the entire index
   - Streaming access minimizes memory usage for large datasets
   - Type-erased indices reduce memory overhead for multiple indices

2. **I/O Optimization**:
   - Binary search minimizes the number of reads
   - Cursor positioning is carefully managed to avoid unnecessary seeks
   - Batched HTTP requests reduce network overhead

3. **Type Safety**:
   - Type information is preserved in the serialized format
   - Type-specific comparisons ensure correct ordering
   - Generic implementations provide type safety at compile time

4. **Query Optimization**:
   - Conditions are processed in order, with no specific optimization yet
   - Future improvements could include reordering conditions based on selectivity
   - Caching frequently accessed index parts could improve performance

## Future Enhancements

1. **Query Optimization**:
   - Implement query planning to reorder conditions for optimal performance
   - Add statistics collection for better selectivity estimation

2. **Advanced HTTP Optimizations**:
   - Implement predictive prefetching for common query patterns
   - Add support for HTTP/2 multiplexing to reduce connection overhead

3. **Compression**:
   - Add optional compression for index and feature data
   - Support for compressed HTTP range requests

4. **Integration with Other Formats**:
   - Extend the approach to other geospatial formats
   - Add support for vector tiles and other web-friendly formats
