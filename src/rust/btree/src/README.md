# B-tree Implementation for FlatCityBuf

This module provides a B-tree based attribute indexing system for efficient querying of CityJSON attributes in the FlatCityBuf format.

## Overview

The B-tree implementation delivers efficient attribute-based queries with the following features:

- Fixed-block storage for optimization across different storage mediums (memory, file, HTTP)
- Configurable block size with LRU caching
- Support for different attribute types via the `KeyEncoder` trait
- Efficient range queries, exact matches, and complex query expressions
- HTTP support with range request optimization
- Query builder pattern for intuitive query construction

## Core Components

### Key Modules

- `entry.rs` - Defines the key-value entries stored in B-tree nodes
- `errors.rs` - Error types for the B-tree implementation
- `http.rs` - HTTP implementation for remote B-tree access
- `key.rs` - Encoders for different attribute types
- `node.rs` - B-tree node structure and operations
- `query.rs` - Query system for building and executing attribute queries
- `storage.rs` - Block storage abstractions (memory, file, HTTP)
- `stream.rs` - Streaming processors for B-tree data
- `tree.rs` - Core B-tree implementation

### Primary Types

- `BTree` - The core B-tree implementation
- `BTreeIndex` - Trait for B-tree index operations
- `QueryBuilder` - Builder pattern for constructing complex queries
- `BlockStorage` - Trait for block-based storage systems
- `KeyEncoder` - Trait for encoding different attribute types

## Usage Examples

### Basic B-tree Creation

```rust
use btree::{BTree, MemoryBlockStorage, IntegerKeyEncoder};

// Create memory-based block storage with 4KB blocks
let storage = MemoryBlockStorage::new(4096);

// Use integer key encoder for this example
let key_encoder = Box::new(IntegerKeyEncoder);

// Open a new B-tree with root at offset 0
let mut btree = BTree::open(storage, key_encoder, 0);

// Insert some key-value pairs
btree.insert(&42i32, 1001)?; // feature ID 1001
btree.insert(&75i32, 1002)?; // feature ID 1002
btree.insert(&13i32, 1003)?; // feature ID 1003

// Query for a specific key
let results = btree.search_exact(&42i32)?;
```

### Query Building

```rust
use btree::{QueryBuilder, conditions, LogicalOp};

// Create a query using the builder pattern
let query = QueryBuilder::new()
    // Find all buildings with height between 100 and 200 meters
    .attribute("height", conditions::between(100.0, 200.0), None)
    // AND with year built after 2000
    .attribute(
        "year_built", 
        conditions::gt(2000), 
        Some(LogicalOp::And)
    )
    // AND within a bounding box
    .spatial(10.0, 20.0, 30.0, 40.0, Some(LogicalOp::And))
    .build()
    .unwrap();
```

### HTTP Usage

```rust
use btree::{HttpBlockStorage, HttpBTreeReader, HttpConfig};
use http_range_client::AsyncBufferedHttpRangeClient;

// Create HTTP client
let client = AsyncBufferedHttpRangeClient::new("https://example.com/city.btree");

// Configure HTTP B-tree access
let config = HttpConfig {
    block_size: 4096,
    cache_size: 100,
    ..Default::default()
};

// Create HTTP block storage
let storage = HttpBlockStorage::new(client, config);

// Open a B-tree reader (read-only)
let reader = HttpBTreeReader::open(storage, Box::new(StringKeyEncoder { prefix_length: 8 }), 0);

// Perform search
let results = reader.search_range("A".."Z")?;
```

## Performance Characteristics

- Time complexity:
  - Search: O(log n)
  - Insert: O(log n)
  - Delete: O(log n)
  
- Space efficiency:
  - Fixed-size blocks minimize wasted space
  - Block caching reduces repeated downloads
  - Efficient key encoding reduces storage requirements

## HTTP Optimization

The B-tree structure is particularly well-suited for HTTP-based access patterns:

- Only necessary nodes are downloaded (no full index download)
- Fixed-size blocks work well with HTTP range requests
- Block caching reduces redundant network requests
- Progressive loading of search results
- Efficient queries over high-latency connections 