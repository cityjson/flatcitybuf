# Performance Characteristics of the B-tree Implementation

This document outlines the performance characteristics, optimization strategies, and benchmarking approach for the B-tree implementation in FlatCityBuf.

## Complexity Analysis

### Time Complexity

| Operation      | Average Case  | Worst Case | Notes                         |
|----------------|---------------|------------|-------------------------------|
| Search         | O(log n)      | O(log n)   | Balanced tree guarantees      |
| Insert         | O(log n)      | O(log n)   | Includes potential rebalancing|
| Delete         | O(log n)      | O(log n)   | Includes potential rebalancing|
| Range Query    | O(log n + k)  | O(log n + k)| Where k is the result size   |
| Bulk Load      | O(n)          | O(n)       | More efficient than n inserts |

### Space Complexity

| Component             | Space Usage                   | Notes                            |
|-----------------------|-------------------------------|----------------------------------|
| Internal Nodes        | O(n)                          | Fixed-size blocks                |
| Leaf Nodes            | O(n)                          | Fixed-size blocks                |
| LRU Cache             | O(c)                          | Where c is the cache size        |
| Key Storage           | Varies by key type            | Optimized for different types    |
| Overall               | O(n)                          | With minimal overhead            |

## Memory Efficiency

The B-tree implementation is optimized for memory efficiency:

1. **Fixed-size Blocks**
   - All nodes have the same fixed size (configurable)
   - Eliminates fragmentation and simplifies memory management
   - Efficient for disk I/O or HTTP range requests

2. **Optimized Key Storage**
   - Integer keys are stored directly
   - String keys use prefix encoding to reduce storage
   - Custom encoders for other data types (e.g., dates, floats)

3. **Lazy Loading**
   - Only loads needed blocks into memory
   - Uses LRU caching to manage memory usage
   - Progressive loading of search results

4. **Memory Usage Control**
   - Configurable cache size limits
   - No full index loading requirement
   - Automatic eviction of less-used blocks

## I/O Efficiency

The B-tree is designed to minimize I/O operations:

1. **Block-Based Design**
   - Aligns with typical disk block sizes (4KB default)
   - Minimizes the number of I/O operations required
   - Works well with file systems and HTTP range requests

2. **High Branching Factor**
   - Each node contains many keys (determined by block size)
   - Reduces the tree height
   - Fewer node accesses to reach leaves

3. **Sequential Access Patterns**
   - Range queries access adjacent leaf nodes
   - Can leverage prefetching and read-ahead mechanisms
   - Efficient for disk-based and HTTP-based storage

4. **Bulk Loading**
   - Bottom-up construction for optimal node packing
   - Minimizes the number of node modifications
   - Creates more balanced trees than incremental inserts

## Caching Strategy

The caching system is designed to maximize performance:

1. **LRU Block Cache**
   - Keeps frequently accessed blocks in memory
   - Automatically evicts least recently used blocks
   - Thread-safe implementation for concurrent access

2. **Cache Size Tuning**
   - Configurable based on available memory
   - Default settings optimized for typical use cases
   - Metrics to help with performance tuning

3. **Prefetching**
   - Optional prefetching for sequential access patterns
   - Can be enabled/disabled via configuration
   - Adaptive prefetching based on access patterns

4. **Cache Coherence**
   - Proper handling of modified blocks
   - Write-through policy for file-based storage
   - Thread-safe cache updates

## HTTP Optimization

The B-tree is specifically optimized for HTTP access:

1. **Range Request Efficiency**
   - Fixed-size blocks align perfectly with HTTP range requests
   - Minimizes the number of HTTP requests needed
   - Reduces bandwidth usage

2. **Progressive Loading**
   - Can start processing results before full download
   - Returns initial results quickly for large queries
   - Improves perceived performance

3. **Request Batching**
   - Combines adjacent block requests where possible
   - Reduces the HTTP request overhead
   - Better performance over high-latency connections

4. **Selective Download**
   - Only downloads needed blocks based on query
   - No need to download the entire index
   - Efficient for large datasets

## Benchmarking

The B-tree implementation has been benchmarked using the following approach:

### Benchmark Scenarios

1. **Local Performance**
   - In-memory operations (baseline)
   - File-based operations
   - Various dataset sizes (small to large)

2. **Network Performance**
   - Various latency conditions
   - Different bandwidth limitations
   - With and without prefetching

3. **Concurrent Access**
   - Multiple simultaneous queries
   - Mixed read/write workloads
   - Thread scaling efficiency

### Key Metrics

1. **Query Latency**
   - Average query time
   - 95th/99th percentile query times
   - Latency distribution

2. **Throughput**
   - Queries per second
   - Data throughput (MB/s)
   - Concurrent query scaling

3. **Resource Usage**
   - Memory consumption
   - I/O operations
   - CPU utilization

4. **Network Efficiency**
   - Number of HTTP requests
   - Bytes transferred
   - Cache hit ratio

## Baseline Comparison

Performance comparison with other indexing approaches:

| Metric               | B-tree    | Binary Search Tree | Linear Array   | Notes                      |
|----------------------|-----------|-------------------|----------------|----------------------------|
| Search (Exact)       | O(log n)  | O(log n)          | O(log n)       | BST can degrade to O(n)    |
| Search (Range)       | Fast      | Medium            | Slow           | B-tree excels at ranges    |
| Memory Usage         | Medium    | Low               | Low            | B-tree has node overhead   |
| Insert Performance   | Fast      | Medium            | Very slow      | Array requires resorting   |
| HTTP Efficiency      | Excellent | Poor              | Medium         | B-tree minimizes requests  |
| Build Time           | Fast      | Medium            | Fast           | Bulk loading is efficient  |
| Concurrent Access    | Good      | Poor              | Medium         | B-tree handles concurrency |

## Optimization Tips

1. **Block Size Selection**
   - Larger blocks increase branching factor but use more memory
   - Typical optimal values: 4KB - 16KB
   - For HTTP: align with typical HTTP chunk sizes

2. **Cache Size Tuning**
   - Start with cache size = 20% of dataset size
   - Increase for read-heavy workloads
   - Monitor cache hit ratio and adjust

3. **Key Encoder Selection**
   - Choose appropriate encoders for your data types
   - Use prefix encoding for strings (adjust prefix length)
   - Consider custom encoders for specialized types

4. **Bulk Loading**
   - Always use bulk loading for initial data
   - Significantly faster than individual inserts
   - Creates more balanced trees

5. **Prefetching Settings**
   - Enable for sequential access patterns
   - Disable for random access patterns
   - Adjust prefetch size based on latency 