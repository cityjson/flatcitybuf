# Product Context

## Reason for the Project

Standardizing data formats for 3D city models is crucial for semantically storing real-world information as permanent records. CityJSON is a widely adopted OGC standard format for this purpose, and its text sequence variant, CityJSONSeq, has been developed to facilitate easier data utilization by software applications. However, the shift towards cloud-native environments and the increasing demand for handling massive datasets necessitate more efficient data processing methods both system-wide and on the web.

While optimized data formats such as PMTiles, FlatBuffers, Mapbox Vector Tiles, and Cloud Optimized GeoTIFF have been proposed for vector and raster data, options for 3D city models remain limited. This research aims to explore optimized data formats for CityJSON tailored for cloud-native processing and evaluate their performance and use cases.

## Problems to be Solved

### Lack of Efficient 3D City Model Data Formats

- Existing formats like CityJSON and CityJSONSeq are not optimized for large-scale cloud processing.
- Limited support for spatial indexing and efficient querying.
- Inefficiencies in downloading and processing large 3D city model datasets.

### Scalability Issues in Cloud-Native Environments

- High storage and processing costs for unoptimized 3D city models.
- Challenges in handling arbitrary extents of urban data dynamically.
- Lack of standardized methods for fetching, sorting, and filtering large-scale 3D datasets.

### Limited Adoption of Optimized Binary Formats

- Current 3D data formats do not leverage modern binary serialization techniques.
- Need for improved compression, indexing, and partial fetching for cloud and web applications.
- Performance limitations in current file formats for visualization and analysis.

### Research Gaps

- Lack of specialized approaches for cloud-native processing of 3D city models.
- Existing research has focused on text-based formats rather than optimized binary encoding.
- Limited studies evaluating real-world performance benefits of FlatBuffers in geospatial applications.
- Challenges in maintaining the semantic richness of CityJSON while optimizing for cloud-based retrieval.

## How It Should Work

### Implementation of FlatBuffers for CityJSON

- Integrate FlatBuffers as an optimized binary format for CityJSON.
- Support for spatial indexing to enhance data retrieval performance.
- Implement spatial sorting and partial fetching via HTTP Range requests.

### Optimization Methodology

1. **Comprehensive Review**: Evaluate existing optimized formats (e.g., PMTiles, Cloud Optimized GeoTIFF).
2. **Format Adaptation**: Modify CityJSON to incorporate efficient binary storage and indexing.
3. **Benchmarking**: Compare performance with traditional CityJSON and assess scalability in cloud environments.
4. **Implementation of Spatial Indexing**: Develop a Hilbert R-tree indexing approach to optimize query performance.
5. **Partial Data Retrieval Mechanism**: Enable downloading and processing of only relevant subsets of 3D city models.
6. **Web-Based Query Optimization**: Enhance interactive applications through HTTP Range requests and on-the-fly decoding.

### Cloud-Native Processing Enhancements

- Enable single-file containment of entire urban areas.
- Reduce cloud storage and computation costs through efficient serialization.
- Improve web-based access and real-time querying capabilities.
- Enable efficient attribute-based and spatial queries using hierarchical indexing.

## Progress Status

### Functional Components

1. **FlatCityBuf Read/Write Implementation**
   - Core writer modules for CityJSONSeq to FlatBuffers conversion.
   - Serialization and indexing of attributes.
   - FlatBuffers deserialization to CityJSONSeq.
   - HTTP-based reading of FCB files.
   - Unit tests covering file reading and HTTP-based retrieval.

2. **Spatial Indexing - Packed R-tree Implementation**
   - Implemented packed R-tree for spatial indexing of features.

3. **Binary Search Tree (BST) for Attribute Indexing**
   - Implemented ByteSerializable trait for efficient indexing.
   - Query execution and sorted index storage.

4. **WASM Build Support**
   - WebAssembly bindings for FlatCityBuf, enabling HTTP Range Request-based partial retrieval.

5. **WASM-Based JavaScript Demo**
   - JS-based demonstration for querying attributes and spatial search.

6. **Texture Encoding/Decoding**
   - Serialization and deserialization of Semantics, Material, and Texture within CityJSON structures.

### Remaining Work & Challenges

1. **Streaming Processing for Attribute Index**
   - Current approach loads all attributes at once; needs optimization for streaming access.
   - Implement progressive loading of attribute indices to reduce memory footprint.
   - Develop a buffering strategy that only keeps frequently accessed indices in memory.
   - Research efficient serialization formats for attribute indices that support partial loading.

2. **Performance Optimization for HTTP Fetching**
   - Improve fetching efficiency, reducing per-feature requests for batch retrieval.
   - Implement intelligent batching of nearby features based on spatial proximity.
   - Add client-side caching to avoid redundant requests for previously fetched data.

3. **Performance Benchmarking for Large Datasets**
   - Evaluate large-scale data retrieval and identify performance bottlenecks.
   - Develop standardized benchmark suite for comparing with other formats.
   - Test with datasets exceeding 10GB to validate scalability claims.
   - Measure memory usage patterns during complex spatial and attribute queries.
   - Profile CPU and I/O usage to identify optimization opportunities.

4. **Support for Additional Encoding Formats**
   - Investigate formats like Parquet, Arrow, and Cap'n Proto for better efficiency.
   - Benchmark alternative formats against FlatBuffers for specific use cases.
   - Develop adapters for seamless conversion between encoding formats.
   - Research compression techniques specific to 3D city model data.
   - Evaluate trade-offs between encoding complexity and query performance.

5. **Multi-Language Implementations**
   - Expand implementation to other languages such as Python and C++.
   - Ensure consistent API design across language implementations.
   - Develop language-specific optimizations while maintaining format compatibility.
   - Create comprehensive test suites for cross-language validation.
   - Publish language-specific packages to relevant package repositories.

6. **Web Viewer Integration**
   - Enhance the WebAssembly demo by integrating Three.js for visualization.
   - Implement level-of-detail rendering for efficient visualization of large models.
   - Add support for texture and material rendering in web environments.
   - Develop UI components for spatial and attribute filtering.
   - Optimize WebGL rendering for mobile devices.

7. **Browser-Based Conversion to Other Formats**
   - Enable conversion from FlatCityBuf to OBJ, PLY, GLB, and GLTF.
   - Implement streaming conversion to avoid memory limitations in browsers.
   - Add support for selective export of filtered subsets.
   - Develop preview capabilities before full conversion.
   - Ensure compatibility with common 3D modeling and GIS software.

8. **Improvement in Testing Strategy**
   - Introduce property-based testing, expand edge case coverage, and improve test automation.
   - Implement fuzzing tests to identify potential vulnerabilities or parsing issues.
   - Develop performance regression tests to prevent slowdowns in future versions.
   - Create integration tests with real-world GIS software.
   - Implement continuous benchmarking in CI pipeline.

9. **Documentation Enhancements**
   - Improve API documentation, add more usage examples, and create tutorials.
   - Develop interactive documentation with live code examples.
   - Create video tutorials for common workflows.
   - Document performance optimization strategies for different use cases.
   - Provide migration guides for users of other formats.

10. **CI/CD Pipeline Strengthening**
    - Automate dependency validation, integrate performance tests, and optimize deployment processes.
    - Implement cross-platform testing on Windows, macOS, and Linux.
    - Add security scanning for dependencies and generated code.
    - Automate release processes including changelog generation.
    - Implement canary releases for early testing of new features.

### Known Issues

1. **Testing Limitations**
   - Platform-dependent test execution issues, particularly on Windows systems.
   - Lack of automated performance regression tests.
   - Inconsistent test behavior with large datasets (>5GB).
   - Limited integration testing with third-party GIS software.

2. **CI/CD Pipeline Gaps**
   - Manual dependency validation process prone to human error.
   - Lack of automated benchmarking in the CI pipeline.
   - Incomplete cross-platform testing, especially for WebAssembly builds.
   - Missing security scanning for dependencies.

3. **Documentation Deficiencies**
   - Incomplete API documentation, particularly for advanced features.
   - Lack of design pattern guidelines for extending the library.
   - Missing examples for integration with popular GIS software.
   - Limited documentation for performance optimization techniques.

4. **Performance Bottlenecks**
   - Suboptimal memory usage during attribute indexing of large datasets.
   - Inefficient HTTP request patterns when querying dispersed features.

## Next Milestones

### Milestone 1: Core Optimization

- Optimize Attribute Index for streaming with progressive loading.
- Implement intelligent batching for HTTP Range Requests.
- Complete comprehensive benchmarking suite for large datasets.
- Address critical performance bottlenecks identified in profiling.
- Enhance documentation with performance optimization guidelines.

### Milestone 2: Format Extensions

- Evaluate and implement support for Arrow and Parquet encoding.
- Develop compression strategies for geometry and attribute data.
- Create adapters for seamless format conversion.
- Implement advanced spatial indexing techniques.
- Enhance CI/CD pipeline with automated performance testing.

### Milestone 3: Language Support

- Release Python implementation.
- Develop C++ implementation.
- Create JavaScript/TypeScript SDK for browser environments.
- Ensure cross-language test compatibility.
- Publish packages to language-specific repositories.

### Milestone 4: Visualization & Integration

- Implement Three.js-based Web Viewer with LOD support.
- Develop browser-based conversion tools for common 3D formats.
- Create plugins for QGIS, ArcGIS, and other GIS software.
- Implement texture and material rendering in web environments.
- Release comprehensive integration examples for third-party tools.

## Recent Updates

- Integrated spatial indexing and binary search tree.
- Added WebAssembly support for FlatCityBuf.
- Improved texture handling in CityJSON encoding.
- Completed initial benchmarking against CityJSON and CityJSONSeq.
- Created preliminary documentation for the file format specification.

## progress status

- [x] basic flatbuffers schema for cityjson - completed
- [x] spatial indexing implementation (hilbert r-tree) - completed
- [x] encoding of geoms with shared vertices - completed
- [x] encoding of materials - completed
- [x] encoding of textures - completed
- [x] encoding of appearance - completed
- [x] encoding of semantics - completed
- [x] encoding of attributes - completed
- [x] extension support - completed
- [ ] js/wasm query engine - in progress
- [ ] python wrapper - in progress
- [ ] web-based query optimizer - in progress
- [ ] partial geom retrieval - planned
- [ ] versioning support - planned

## what's next

- **query engine refinement:** optimize the query engine for more complex spatial and attribute queries
- **python wrapper:** complete the python interface for broader ecosystem integration
- **web-based query optimizer:** finish the visualization tool for query plan optimization
- **partial geometry retrieval:** implement efficient retrieval of partial geometries for large objects
- **extension documentation:** create more comprehensive documentation and examples for utilizing extensions
- **benchmarking extensions:** measure performance impact of different extension usage patterns

## known issues

- performance bottlenecks in attribute indexing with large datasets
- memory usage spikes during encoding of complex geometries
- limitations in the current implementation of lod switching
- extension attributes may impact query performance for complex filter operations
