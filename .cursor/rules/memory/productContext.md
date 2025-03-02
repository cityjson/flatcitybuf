
# **Cloud-Optimized CityJSON**

## **1. Introduction**
- **Motivation & Project Context**:
  - Standardizing **3D city model data formats** is crucial for long-term semantic storage of urban environments.
  - **CityJSON**, a widely adopted **OGC standard**, provides a structured JSON-based format for 3D city models.
  - **CityJSONSeq** improved streaming but lacks **cloud-native optimizations** for handling large-scale datasets.

- **Problem Statement**:
  - Existing 3D model formats like **CityJSON and CityJSONSeq** are **not optimized** for large-scale **cloud processing**.
  - **Scalability challenges** arise from high **storage costs, slow queries, and inefficient downloading** of large datasets.
  - **Limited support for binary serialization** and **spatial indexing** prevents efficient cloud-based data retrieval.
  - **Research Gaps**:
    - Few studies have evaluated **FlatBuffers in geospatial applications**.
    - Limited focus on **efficient cloud-native processing** of 3D city models.
    - **Preserving CityJSON’s semantic richness** while optimizing for **fast cloud retrieval** remains a challenge.

- **Goal of This Specification**:
  - Develop an **optimized CityJSON format** based on **FlatBuffers**, improving:
    - **Data retrieval speed** via **spatial indexing (Hilbert R-tree)**.
    - **Query performance** through **efficient attribute-based and spatial searches**.
    - **Cloud efficiency** with **HTTP Range Requests for partial fetching**.
  - Ensure **backward compatibility** with **CityJSON 2.0**.

---

## **2. Design Goals and Requirements**
- **Performance & Efficiency**:
  - Reduce **processing overhead** using **FlatBuffers' zero-copy access**.
  - **Optimize storage** via **binary encoding**, reducing file sizes.

- **Cloud & Web Compatibility**:
  - **Enable partial data retrieval** via **HTTP Range Requests**.
  - **Support spatial sorting and indexing** for scalable cloud processing.

- **Scalability & Integration**:
  - Ensure **interoperability** with **existing GIS tools** (QGIS, Cesium, Mapbox).
  - **Reduce cloud storage & computation costs**.

- **End-User Goals**:
  - **Faster downloads** of arbitrary **3D city model subsets**.
  - **Web applications** that **load city models instantly**.

---

## **3. Data Model and Encoding Structure**
### **3.1 Enhancements to CityJSON**
- **CityJSON 2.0**:
  - **JSON-based format** for 3D city models.
  - Uses **shared vertex lists** to improve storage efficiency.

- **CityJSONSeq (Streaming Format)**:
  - Breaks datasets into **individual objects** for **incremental processing**.
  - Still **text-based**, leading to **higher memory usage**.

### **3.2 FlatBuffers-Based Encoding**
- **Schema Definition**:
  - Stores **CityObjects as FlatBuffers tables**.
  - Implements **hierarchical storage** with **efficient geometry encoding**.

- **Memory Optimization**:
  - Uses **separate arrays for geometric primitives** (solids, shells, surfaces, rings).
  - **Avoids nested JSON objects**, leading to **faster parsing**.

### **3.3 File Structure**
| **Component** | **Description** |
|--------------|---------------|
| **Magic Bytes** | File identifier for format validation. |
| **Header** | Stores **metadata, CRS, transformations**. |
| **Index** | **Byte offsets** for fast random access. |
| **Features** | Encodes **CityJSON objects as FlatBuffers tables**. |

---

## **4. Data Organization and Storage Mechanism**
### **4.1 Spatial Indexing**
- Implements a **Hilbert R-tree** to:
  - **Speed up spatial queries**.
  - Enable **selective data retrieval**.

- **Optimized Query Performance**:
  - **Attribute-Based Indexing** (e.g., find buildings taller than 50m).
  - **Spatial Queries** (e.g., find objects within a bounding box).

### **4.2 HTTP Range Requests**
- Enables **partial fetching**:
  - Download **only required city features**, reducing data transfer.
  - Improves **cloud storage efficiency**.

---

## **5. Performance Optimizations**
### **5.1 Benchmarked Results**
| **Dataset** | **CityJSONSeq (Time)** | **FlatBuffers (Time)** | **Size Reduction** |
|------------|----------------------|----------------------|------------------|
| 3DBAG | 154ms | 69ms | 48% |
| NYC | 1.80s | 80ms | 71% |
| Zurich | 6.11s | 151ms | 60% |

- **Observations**:
  - **FlatBuffers-based CityJSON is 10-20× faster** in data retrieval.
  - **50-70% smaller file sizes** vs. JSON-based CityJSONSeq.

---

## **6. Streaming and Partial Fetching**
- **HTTP Range Requests**:
  - Supports **on-demand downloading** of CityJSON objects.
  - **Eliminates need to load entire datasets in memory**.

- **Comparison with CityJSONSeq**:
  - CityJSONSeq **supports streaming but is still text-based**.
  - **FlatBuffers further improves query speeds** and **reduces memory usage**.

---

## **7. Implementation Details**
### **7.1 FlatBuffers Schema**
```flatbuffers
table CityJSONFeature {
    id: string;
    type: string;
    geometry: Geometry;
    attributes: Attributes;
}
```

### **7.2 Rust-Based Implementation**
- Developed as a **Rust library** for:
  - **Encoding and decoding FlatBuffers-based CityJSON**.
  - **Integrating with GIS workflows**.
- **WebAssembly support** for in-browser processing.

---

## **8. Use Cases and Applications**
### **8.1 Urban Planning & Smart Cities**
- **Faster, interactive 3D city analysis** in smart city applications.
- **Real-time urban simulations**.

### **8.2 Cloud GIS Integration**
- **Optimized for cloud storage platforms** (AWS S3, Google Cloud).
- **Seamless web-based access**.

---

## **9. Comparison with Other Formats**
| **Format** | **Encoding Type** | **Spatial Indexing** | **Partial Fetching** | **Optimized for 3D Models** |
|-----------|-----------------|-----------------|------------------|-------------------|
| CityJSON | JSON | No | No | Yes |
| CityJSONSeq | JSON-Stream | No | Partial | Yes |
| **FlatCityBuf (This Work)** | **FlatBuffers** | **Yes (Hilbert R-tree)** | **Yes (HTTP Range)** | **Yes** |

---

## **10. Implementation Guide**
### **10.1 Conversion from CityJSON to FlatCityBuf**
```bash
./convert --input cityjson.json --output city.fbuf
```
### **10.2 Developer Best Practices**
- **Use HTTP Range Requests** to improve query speeds.
- **Precompute spatial indices** to optimize large datasets.

---

## **11. Future Work and Extensions**
- **Support for textures/materials** in FlatBuffers.
- **Adaptive tiling for large datasets**.
- **Cloud GIS standardization** for CityJSON.

---

## **12. Conclusion**
- **FlatBuffers-based CityJSON significantly improves query performance, storage efficiency, and cloud compatibility**.
- **Bridges the gap between CityJSONSeq and optimized binary formats**.
- **Enables scalable, real-time urban data processing**.

---

## **13. Success Metrics**
- **50-70% reduction in storage size**.
- **10-20× faster retrieval** vs. CityJSONSeq.
- **Adoption in GIS software & cloud platforms**.
