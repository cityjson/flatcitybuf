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

## User Experience Goals

### End-Users
- Ability to download arbitrary extensions of 3D city models efficiently.
- Faster and more responsive applications using 3D city data.
- Seamless integration with smart city platforms and urban planning tools.
- Enhanced support for selective retrieval and filtering of model attributes.

### Developers
- Simplified cloud architecture for handling large 3D datasets.
- Efficient single-file storage for improved data management.
- Accelerated processing capabilities for software applications utilizing 3D city models.
- Ability to integrate FlatBuffers-based CityJSON with existing GIS workflows and visualization platforms.

### Research and Industry Adoption
- Adoption of optimized CityJSON format in cloud GIS and urban modeling.
- Facilitation of scalable, real-time city data visualization.
- Enhanced data accessibility and interoperability across applications.
- Contribution to the development of cloud-native geospatial standards.

## Success Metrics

- Reduction in storage size and computational cost of CityJSON datasets.
- Faster retrieval and visualization of 3D city models in web applications.
- Improved efficiency of spatial queries through optimized indexing.
- Increased adoption of the proposed format in urban planning and GIS software.
- Demonstrable improvements in query performance and data access speed.
- Adoption of the methodology by organizations managing large-scale 3D city models.
