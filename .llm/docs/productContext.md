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
    - **Preserving CityJSON's semantic richness** while optimizing for **fast cloud retrieval** remains a challenge.

- **Goal of This Specification**:
  - Develop an **optimized CityJSON format** based on **FlatBuffers**, improving:
    - **Data retrieval speed** via **spatial indexing (Hilbert R-tree)**.
    - **Query performance** through **efficient attribute-based and spatial searches**.
    - **Cloud efficiency** with **HTTP Range Requests for partial fetching**.
  - Ensure **backward compatibility** with **CityJSON 2.0**.

---
