# FlatCityBuf API (fcb_api)

A cloud-optimized API for serving 3D city models from FlatCityBuf files, designed as a modern alternative to the traditional 3DBAG API (Flask + PostgreSQL).

---

## Overview

The FlatCityBuf API provides an OGC API-compatible interface for querying and retrieving 3D city models stored in the FlatCityBuf format. Built with Rust and Axum, it leverages HTTP range requests and efficient spatial/attribute indexing for fast, scalable access to large city datasets.

### Key Features

- 🚀 **High Performance**: Zero-copy data access with FlatBuffers
- 🌐 **Cloud-Native**: Optimized for HTTP range requests and serverless deployment
- 📍 **Spatial Indexing**: Packed R-tree for efficient bounding box queries
- 🔍 **Attribute Filtering**: Static B+Tree indices for fast attribute-based searches
- 🗺️ **CRS Support**: Automatic coordinate transformation between CRS systems
- 📦 **Multiple Output Formats**: CityJSONFeature, CityJSON, CityJSONSeq, OBJ
- 🔗 **OGC API Compatible**: Standards-compliant endpoints and responses

---

## Architecture

### Technology Stack

- **Backend Framework**: Axum (Rust)
- **Storage**: FlatCityBuf files (cloud storage compatible)
- **Data Access**: HTTP Range Requests
- **Deployment**: Docker + Cloud Run (Google Cloud)

### Data Flow

```
Client Request
    ↓
API Server (Axum)
    ↓
HTTP Range Request → FlatCityBuf File (Cloud Storage)
    ↓
Spatial/Attribute Index Lookup
    ↓
Feature Retrieval (zero-copy)
    ↓
Response (CityJSONFeature/CityJSON/OBJ)
```

---

## API Endpoints

### Core Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Landing page with API information |
| `/conformance` | GET | Conformance declaration |
| `/collections` | GET | List available collections |
| `/collections/{collection_id}` | GET | Collection metadata |
| `/collections/{collection_id}/items` | GET | Query and retrieve features |
| `/collections/{collection_id}/items/{item_id}` | GET | Retrieve a specific feature by ID |

**Note**: Currently, only the `pand` collection is supported.

---

## Query Capabilities

### 🗺️ Spatial Queries (Bounding Box)

Query features within a geographic bounding box using the `bbox` parameter.

**Supported CRS:**

- **EPSG:7415** (default, storage CRS): RD New + NAP height
- **EPSG:4326**: WGS 84 (lat/lon)
- **EPSG:28992**: Amersfoort / RD New (projected)

**Examples:**

```bash
# Default CRS (EPSG:7415)
GET /collections/pand/items?bbox=84000,445000,85000,446000

# WGS 84 (EPSG:4326) - lon/lat order
GET /collections/pand/items?bbox=4.367394,51.995031,4.398037,52.023820&bbox-crs=EPSG:4326

# RD New (EPSG:28992)
GET /collections/pand/items?bbox=84000,445000,85000,446000&bbox-crs=EPSG:28992
```

**Implementation Details:**

- Uses packed R-tree spatial index for efficient lookup
- Automatic CRS transformation when `bbox-crs` is specified
- Coordinates are transformed to storage CRS (EPSG:7415) for querying
- Results are returned in storage CRS

### 🔍 Attribute Filtering

Filter features based on attribute values using CQL-like syntax.

**Supported Operators:**

- `=` (Equal)
- `!=`, `<>` (Not Equal)
- `>` (Greater Than)
- `<` (Less Than)
- `>=` (Greater Than or Equal)
- `<=` (Less Than or Equal)
- `BETWEEN ... AND ...` (Range)
- `AND` (Logical conjunction)

**Supported Data Types:**

- String (up to 50 characters, mainly for identifiers)
- Integer (Int8, Int16, Int32)
- Unsigned Integer (UInt8, UInt16, UInt32)
- Float (Float32)
- Boolean (typically not used)

**Examples:**

```bash
# Simple equality
GET /collections/pand/items?filter=identificatie=NL.IMBAG.Pand.0153100000209948

# Comparison
GET /collections/pand/items?filter=b3_h_dak_50p>30

# Range query
GET /collections/pand/items?filter=oorspronkelijkbouwjaar BETWEEN 1900 AND 1950

# Combined filters
GET /collections/pand/items?filter=b3_bouwlagen>2 AND status='Pand in gebruik'
```

**Performance Notes:**

- ID lookups are highly optimized via direct attribute index access
- Bounding box queries perform well with R-tree spatial index
- General attribute filters may be slower due to multiple HTTP range requests
- Acceptable for typical use cases but not optimal for high-throughput complex filtering

### 📄 Pagination

Control result set size and offset for large queries.

**Parameters:**

- `limit`: Maximum number of features to return (default: 100, configurable max)
- `offset`: Number of features to skip

**Example:**

```bash
GET /collections/pand/items?bbox=84000,445000,85000,446000&limit=50&offset=100
```

**Response Metadata:**

```json
{
  "numberMatched": 9271,
  "numberReturned": 50,
  ...
}
```

### 🆔 Feature ID Lookup

Retrieve a specific feature by its identifier.

**Example:**

```bash
GET /collections/pand/items/NL.IMBAG.Pand.0153100000209948
```

---

## Output Formats

### CityJSONFeature (Default)

OGC API-compatible response format with individual CityJSON features.

**Response Structure:**

```json
{
  "type": "FeatureCollection",
  "features": [
    {
      "feature": { /* CityJSONFeature object */ },
      "id": "NL.IMBAG.Pand.1926100000492192",
      "links": [
        {
          "href": "https://api.example.com/collections/pand/items/NL.IMBAG.Pand.1926100000492192",
          "rel": "self",
          "type": "application/city+json",
          "title": "this document"
        },
        {
          "href": "https://api.example.com/collections/pand",
          "rel": "collection",
          "type": "application/json",
          "title": "Collection"
        }
      ]
    }
  ],
  "links": [
    {
      "href": "https://api.example.com/collections/pand/items?limit=10&offset=0",
      "rel": "self",
      "type": "application/json",
      "title": "this document"
    },
    {
      "href": "https://api.example.com/collections/pand/items?limit=10&offset=0",
      "rel": "first",
      "type": "application/json",
      "title": "First page"
    },
    {
      "href": "https://api.example.com/collections/pand/items?limit=10&offset=10",
      "rel": "next",
      "type": "application/json",
      "title": "Next page"
    },
    {
      "href": "https://api.example.com/collections/pand/items?limit=10&offset=9270",
      "rel": "last",
      "type": "application/json",
      "title": "Last page"
    }
  ],
  "timeStamp": "2025-10-13T13:14:04.409093129+00:00",
  "numberMatched": 9271,
  "numberReturned": 10
}
```

**Link Headers:**
The API also includes RFC 8288 compliant `Link` headers for pagination:

```
Link: <https://api.example.com/collections/pand/items?limit=10&offset=0>; rel="self",
      <https://api.example.com/collections/pand/items?limit=10&offset=10>; rel="next",
      <https://api.example.com/collections/pand/items?limit=10&offset=9270>; rel="last"
```

### Alternative Formats

Use the `format` query parameter to request different output formats:

```bash
# CityJSON file
GET /collections/pand/items?format=cityjson

# CityJSONSeq (newline-delimited)
GET /collections/pand/items?format=cjseq

# Wavefront OBJ (3D mesh)
GET /collections/pand/items?format=obj
```

---

## Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `FCB_URL` | URL or file path to FlatCityBuf file | `https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb` | Yes |
| `BASE_URL` | API base URL for generating links | `https://api.3dbag.nl` | Yes |
| `MAX_RETURN_FEATURES` | Maximum features per request | `100` | No |
| `HOST` | Server bind address | `127.0.0.1` (binary) / `0.0.0.0` (Docker) | No |
| `PORT` | Server port | `8080` | No |

---

## Deployment

### Current Production Setup

- **Platform**: Google Cloud Run (serverless)
- **Data Source**: CityJSONSeq from gilfoyle server (September 22nd snapshot)
- **Storage**: Google Cloud Storage
  - URL: `https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb`
  - File Size: ~70GB
  - Includes spatial and attribute indices
- **Architecture**: Stateless, auto-scaling containers

### Building and Running

#### Option 1: Docker (Recommended)

```bash
# Build from repository root
docker build -f src/rust/Dockerfile -t fcb_api:latest .

# Run with environment variables
docker run -p 8080:8080 \
  -e FCB_URL="https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb" \
  -e BASE_URL="https://api.3dbag.nl" \
  -e MAX_RETURN_FEATURES="100" \
  fcb_api:latest
```

#### Option 2: Cargo Binary

```bash
# Build from src/rust directory
cd src/rust
cargo build --release -p fcb_api

# Run with environment variables
FCB_URL="https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb" \
BASE_URL="https://api.3dbag.nl" \
MAX_RETURN_FEATURES="100" \
./target/release/fcb_api
```

### Updating Data

When the source 3DBAG data is updated, regenerate the FlatCityBuf file:

```bash
# Install FlatCityBuf CLI (if not already installed)
cargo install fcb

# Convert CityJSONSeq to FlatCityBuf with full indexing
# -A option indexes all attributes
fcb ser \
  -i /path/to/3dbag.city.jsonl \
  -o 3dbag_all_index.fcb \
  -A \
  --attr-branching-factor 256

# Verify the generated file
fcb info -i 3dbag_all_index.fcb

# Upload to cloud storage
gsutil cp 3dbag_all_index.fcb gs://your-bucket/
# or rsync to your server
```

---

## Feature Comparison: Current API vs FlatCityBuf API

| Feature | Current API (Flask) | FlatCityBuf API | Status |
|---------|-------------------|-----------------|--------|
| **Endpoints** | | | |
| Landing page | ✅ | ✅ | Compatible |
| Conformance | ✅ | ✅ | Compatible |
| Collections list | ✅ | ✅ | Compatible |
| Collection metadata | ✅ | ✅ | Compatible |
| Feature items | ✅ | ✅ | Compatible |
| Feature by ID | ✅ | ✅ | Compatible |
| **Query Types** | | | |
| Bounding box | ✅ | ✅ | Compatible |
| Attribute filters | ❌ | ✅ | **Enhanced** |
| Pagination | ✅ | ✅ | Compatible |
| **CRS Support** | | | |
| EPSG:7415 | ✅ | ✅ | Compatible |
| EPSG:4326 (WGS 84) | [TBD] | ✅ | **Enhanced** |
| EPSG:28992 (RD New) | [TBD] | ✅ | **Enhanced** |
| **Output Formats** | | | |
| CityJSONFeature | ✅ | ✅ | Compatible |
| CityJSON | ❌ | ✅ | **Enhanced** |
| CityJSONSeq | ❌ | ✅ | **Enhanced** |
| OBJ | ❌ | ✅ | **Enhanced** |
| **Response Features** | | | |
| Link headers (RFC 8288) | ❌ | ✅ | **Enhanced** |
| Per-feature links | ❌ | ✅ | **Enhanced** |

---

## Known Limitations

### Attribute Filter Performance ⚠️

While attribute filtering is supported, performance varies by query type:

- ✅ **Bounding box queries**: Excellent performance with R-tree spatial index
- ✅ **ID lookups**: Excellent performance with direct attribute index access
- ⚠️ **General attribute filters**: Moderate performance due to data format design
  - B+Tree index lookup is efficient
  - Feature retrieval requires multiple HTTP range requests
  - Not optimized for complex multi-attribute filtering

**Impact**: Response times for attribute-filtered queries may be higher than bbox/ID queries, but acceptable for typical use cases.

---

## Production Deployment Considerations

### Critical Decisions

#### 1. Storage Location

- **Option A: 3DBAG Server (gilfoyle)**
  - ✅ Control, existing infrastructure, free
  - ❌ Limited bandwidth, needs HTTP range request support

- **Option B: Current GCS Bucket**
  - ✅ Already working, minimal migration
  - ❌ Personal account dependency, costs

- **Option C: CDN with Origin Storage**
  - ✅ Global distribution, caching, low latency
  - ❌ Additional setup, costs

#### 2. Deployment Method

- **Option A: Docker on 3DBAG Server**
  - ✅ Control, existing infrastructure, free
  - ❌ Requires Docker setup

- **Option B: Binary on 3DBAG Server**
  - ✅ Simpler, no container overhead
  - ❌ Manual binary deployment

- **Option C: Cloud Run (Current)**
  - ✅ Auto-scaling, pay-per-use, zero maintenance
  - ❌ Cold start latency, vendor lock-in, costs

#### 3. Maintenance Ownership

- **Option A: 3DBAG Team**
  - Requires: Training, documentation, handoff period

- **Option B: FlatCityBuf Developer**
  - Requires: Long-term commitment, access credentials

- **Option C: Shared Responsibility**
  - Requires: Clear SLA, communication protocol

### Migration Milestones

- [ ] Decision to proceed with migration
- [ ] Deploy beta API for testing
- [ ] Beta testing by 3DBAG team
- [ ] Replace production API
- [ ] Deprecate old API

---

## Example Queries

### Live API Examples

**Current deployment**: `https://flatcitybuf-api-264879243442.europe-west4.run.app`

```bash
# Bounding box query (default CRS: EPSG:7415)
GET https://flatcitybuf-api-264879243442.europe-west4.run.app/collections/pand/items?bbox=84000,445000,85000,446000&limit=10

# Bounding box with WGS 84 coordinates
GET https://flatcitybuf-api-264879243442.europe-west4.run.app/collections/pand/items?bbox=4.367394,51.995031,4.398037,52.023820&bbox-crs=EPSG:4326

# Attribute filter: building height
GET https://flatcitybuf-api-264879243442.europe-west4.run.app/collections/pand/items?filter=b3_h_dak_50p>30&limit=10

# Specific feature by ID
GET https://flatcitybuf-api-264879243442.europe-west4.run.app/collections/pand/items/NL.IMBAG.Pand.0153100000209948

# Combined spatial and attribute query
GET https://flatcitybuf-api-264879243442.europe-west4.run.app/collections/pand/items?bbox=84000,445000,85000,446000&filter=b3_bouwlagen>2

# Request CityJSON format
GET https://flatcitybuf-api-264879243442.europe-west4.run.app/collections/pand/items?bbox=84000,445000,85000,446000&format=cityjson
```

---

## References

- [FlatCityBuf GitHub Repository](https://github.com/cityjson/flatcitybuf)
- [FlatCityBuf Master Thesis](https://resolver.tudelft.nl/uuid:6727c979-5e46-4fe0-9349-a7803e825d02)
- [OGC API - Features Specification](https://ogcapi.ogc.org/features/)
- [CityJSON Specification](https://www.cityjson.org/)

---

## License

See the [LICENSE](../../../LICENSE) file in the repository root.

---

## Contributing

See [CONTRIBUTING.md](../../../CONTRIBUTING.md) for development guidelines and contribution instructions.
