# HTTP Integration for B-tree Access

This document describes the HTTP integration for accessing B-tree indices over HTTP connections, which is crucial for web clients and distributed systems.

## Overview

The B-tree implementation provides HTTP-based access through three main components:

1. `HttpBlockStorage` - Implements the `BlockStorage` trait using HTTP range requests
2. `HttpBTreeReader` - A read-only B-tree reader optimized for HTTP access
3. `HttpBTreeBuilder` - Used to construct HTTP-based B-tree instances

The HTTP integration is designed to:
- Minimize data transfer by downloading only needed blocks
- Optimize performance through caching and prefetching
- Support progressive loading of search results
- Work efficiently with high-latency connections

## Architecture

```
┌────────────────┐      ┌─────────────────┐      ┌────────────────┐
│                │      │                 │      │                │
│  HTTP Client   │◄────►│ HttpBlockStorage│◄────►│  B-tree Logic  │
│                │      │                 │      │                │
└────────────────┘      └─────────────────┘      └────────────────┘
        │                        │                        │
        │                        │                        │
        ▼                        ▼                        ▼
┌────────────────┐      ┌─────────────────┐      ┌────────────────┐
│                │      │                 │      │                │
│  HTTP Cache    │      │   LRU Cache     │      │ Query Executor │
│                │      │                 │      │                │
└────────────────┘      └─────────────────┘      └────────────────┘
```


## Block Storage Implementation

The `HttpBlockStorage` implements the `BlockStorage` trait to provide HTTP-based access to B-tree blocks. Key features include:

- **LRU Cache**: Frequently accessed blocks are cached to minimize network requests
- **Concurrent Access**: Multiple requests can be processed simultaneously
- **Metrics Collection**: Tracks cache hits/misses and download statistics
- **Error Handling**: Proper error propagation and retry mechanisms

### Example Usage:

```rust
// Create a buffered HTTP client
let client = AsyncBufferedHttpRangeClient::new("https://example.com/city.btree");

// Configure HTTP storage
let config = HttpConfig {
    block_size: 4096,
    cache_size: 100,
    metrics_enabled: true,
    prefetch_enabled: true,
};

// Create HTTP block storage
let storage = HttpBlockStorage::new(client, config);

// Create B-tree reader
let btree = HttpBTreeReader::open(storage, Box::new(IntegerKeyEncoder), 0);
```

## Performance Optimizations

The HTTP integration includes several optimizations:

1. **Block Caching**
   - Uses an LRU cache to keep frequently accessed blocks in memory
   - Configurable cache size to balance memory usage vs. performance

2. **Prefetching**
   - Optional prefetching of adjacent blocks during range queries
   - Reduces latency for sequential access patterns

3. **Connection Reuse**
   - Leverages connection pooling from the underlying HTTP client
   - Reduces connection establishment overhead

4. **Minimal Data Transfer**
   - Only downloads needed blocks using HTTP range requests
   - B-tree structure minimizes the number of blocks needed for queries

## Metrics and Monitoring

The HTTP integration includes metrics collection for performance monitoring:

```rust
pub struct HttpMetrics {
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub bytes_downloaded: AtomicU64,
    pub requests_made: AtomicU64,
}
```

These metrics can be used to:
- Monitor cache efficiency
- Track network usage
- Identify performance bottlenecks
- Tune cache and prefetch settings

## Configuration Options

The `HttpConfig` struct provides various configuration options:

```rust
pub struct HttpConfig {
    /// Size of each block in bytes
    pub block_size: usize,

    /// Number of blocks to cache in memory
    pub cache_size: usize,

    /// Whether metrics collection is enabled
    pub metrics_enabled: bool,

    /// Whether prefetching is enabled
    pub prefetch_enabled: bool,
}
```


## WASM Integration

The HTTP B-tree implementation is designed to work well with WebAssembly:

- Small binary size due to minimal dependencies
- Efficient memory usage with fixed-size blocks
- Compatible with browser-based HTTP clients
- Works well with streaming data processing