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

2. **Performance Optimization for HTTP Fetching**
   - Improve fetching efficiency, reducing per-feature requests for batch retrieval.

3. **Performance Benchmarking for Large Datasets**
   - Evaluate large-scale data retrieval and identify performance bottlenecks.

4. **Support for Additional Encoding Formats**
   - Investigate formats like Parquet, Arrow, and Cap’n Proto for better efficiency.

5. **Multi-Language Implementations**
   - Expand implementation to other languages such as Python and C++.

6. **Web Viewer Integration**
   - Enhance the WebAssembly demo by integrating Three.js for visualization.

7. **Browser-Based Conversion to Other Formats**
   - Enable conversion from FlatCityBuf to OBJ, PLY, GLB, and GLTF.

8. **Improvement in Testing Strategy**
   - Introduce property-based testing, expand edge case coverage, and improve test automation.

9. **Documentation Enhancements**
   - Improve API documentation, add more usage examples, and create tutorials.

10. **CI/CD Pipeline Strengthening**
    - Automate dependency validation, integrate performance tests, and optimize deployment processes.

### Known Issues

1. **Testing Limitations**
   - Platform-dependent test execution issues.
   - Incomplete test coverage in certain modules.

2. **CI/CD Pipeline Gaps**
   - Manual dependency validation.
   - Unstable test performance in CI environments.

3. **Documentation Deficiencies**
   - Incomplete API documentation.
   - Lack of design pattern guidelines.

## Next Milestones

### Milestone 1
- Optimize Attribute Index for streaming.
- Improve batch retrieval for HTTP Fetching.

### Milestone 2
- Conduct performance benchmarking on large datasets.
- Introduce additional encoding formats (Parquet, Arrow, etc.).

### Milestone 3
- Expand implementations to Python and C++.
- Implement Three.js-based Web Viewer.

### Milestone 4
- Strengthen CI/CD workflows.
- Improve browser-based format conversions.

## Recent Updates
- Integrated spatial indexing and binary search tree.
- Added WebAssembly support for FlatCityBuf.
- Improved texture handling in CityJSON encoding.