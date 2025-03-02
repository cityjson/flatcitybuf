# flatcitybuf specification

## overview of the file format

flatcitybuf is a cloud-optimized binary format for storing and retrieving 3d city models based on the cityjson standard. it combines the semantic richness of cityjson with the performance benefits of flatbuffers binary serialization and spatial indexing techniques.

the format is designed to address several limitations of existing cityjson formats:

- **performance**: traditional json-based formats require full parsing before data access, while flatcitybuf enables zero-copy access to specific city objects.
- **cloud optimization**: supports http range requests for partial data retrieval, reducing bandwidth usage and improving load times.
- **spatial indexing**: implements a packed r-tree for efficient spatial queries.
- **attribute indexing**: uses binary search trees for fast attribute-based filtering.
- **size efficiency**: binary encoding reduces file sizes by 50-70% compared to text-based formats.

the format maintains backward compatibility with cityjson 2.0 while significantly improving query performance (10-20× faster) and storage efficiency.

## flatbuffers schema explanation

flatcitybuf uses two primary schema files to define its structure:

### header.fbs

the `header.fbs` schema defines the metadata and indexing structures of a flatcitybuf file:

```flatbuffers
table Header {
  transform: Transform;                     // Transformation vectors
  appearance: Appearance;                   // Appearance object for materials and textures
  columns: [Column];                        // Attribute columns schema
  features_count: ulong;                    // Number of features in the dataset
  index_node_size: ushort = 16;             // Index node size for R-tree
  attribute_index: [AttributeIndex];        // Attribute indexing information
  geographical_extent: GeographicalExtent;  // Bounds
  reference_system: ReferenceSystem;        // Spatial Reference System
  // ... additional metadata fields ...
  version: string (required);               // CityJSON version
}
```

key components include:
- **transform**: stores scale and translation vectors for vertex coordinates
- **appearance**: contains materials and textures information
- **columns**: schema for attribute data
- **attribute_index**: indexing for fast attribute queries
- **geographical_extent**: bounding box of the dataset
- **reference_system**: coordinate reference system information

### feature.fbs

the `feature.fbs` schema defines the structure of city objects and their geometries:

```flatbuffers
table CityFeature {
  id: string (key, required);
  objects: [CityObject];
  vertices: [Vertex];
  appearance: Appearance;
}

table CityObject {
  type: CityObjectType;
  id: string (key, required);
  geographical_extent: GeographicalExtent;
  geometry: [Geometry];
  attributes: [ubyte];
  columns: [Column];
  children: [string];
  children_roles: [string];
  parents: [string];
}
```

key components include:
- **cityfeature**: the root object containing city objects and shared vertices
- **cityobject**: individual 3d features with type, geometry, and attributes
- **geometry**: complex structure for 3d geometries with boundaries and semantics
- **semanticobject**: semantic classification of geometry parts

### design rationale

the schema design follows several key principles:

1. **flatbuffers efficiency**: uses flatbuffers' zero-copy access for fast data retrieval
2. **hierarchical structure**: maintains cityjson's hierarchical object model
3. **shared vertices**: uses indexed vertices to reduce redundancy
4. **semantic preservation**: maintains rich semantic information from cityjson
5. **extensibility**: allows for future extensions while maintaining backward compatibility

## file storage overview

a flatcitybuf file consists of the following sections:

```
┌─────────────────┐
│ magic bytes     │ 4 bytes identifier "fcb\0"
├─────────────────┤
│ header size     │ 4 bytes uint32 (size of header in bytes)
├─────────────────┤
│ header          │ flatbuffer-encoded header
├─────────────────┤
│ r-tree index    │ spatial index for city objects
├─────────────────┤
│ attribute index │ binary search trees for attribute queries
├─────────────────┤
│ features        │ flatbuffer-encoded city features
└─────────────────┘
```

1. **magic bytes**: a 4-byte identifier ("fcb\0") to identify the file format
2. **header size**: a 4-byte unsigned integer indicating the size of the header section
3. **header**: contains metadata, coordinate transformations, and schema information
4. **r-tree index**: packed r-tree structure for spatial indexing
5. **attribute index**: binary search trees for attribute-based queries
6. **features**: the actual city objects encoded as flatbuffers

each section is aligned to facilitate efficient http range requests, allowing clients to fetch only the parts they need.

## rtree indexing

flatcitybuf implements a packed r-tree for spatial indexing, based on the hilbert r-tree algorithm:

### encoding structure

the r-tree is stored as a flat array of node items:

```
┌─────────────────┐
│ nodes           │ array of node entries
└─────────────────┘
```

each node entry contains:
- **min_x, min_y**: minimum coordinates of 2d bounding box
- **max_x, max_y**: maximum coordinates of 2d bounding box
- **offset**: byte offset to the feature in the features section

note that the packed r-tree implementation is 2d only, using x and y coordinates. the z dimension is not included in the spatial indexing, though it remains part of the feature data.

the node size (number of entries per node) is stored in the header as `index_node_size`, and the total number of nodes is calculated based on the number of features and the node size.

### feature size determination

the size of each feature is not stored explicitly in the r-tree. instead, it is determined implicitly:

1. for non-leaf nodes: the size is not needed as they only point to other nodes
2. for leaf nodes: the size of a feature is determined by the difference between its offset and the offset of the next feature
3. for the last feature: the size extends to the end of the file

this approach saves space in the index structure while still allowing efficient access to features.

### hilbert ordering

features are ordered using a hilbert space-filling curve to improve spatial locality:

1. compute the hilbert value for each feature's centroid (using only x,y coordinates)
2. sort features by their hilbert values
3. build the r-tree bottom-up from the sorted features

this approach ensures that spatially close objects are also close in the file, improving cache efficiency and range request performance.

### query algorithm

to query the r-tree:
1. start at the root node
2. for each entry in the node, check if the query intersects the 2d bounding box
3. if it's a leaf node, return the feature offsets
4. if it's an internal node, recursively query the child nodes

for 3d filtering, additional z-coordinate filtering must be performed after retrieving the features that match the 2d query.

## attribute indexing

flatcitybuf implements a sorted array-based index for efficient attribute queries:

### encoding structure

for each indexed attribute, a sorted index is stored:

```
┌─────────────────┐
│ entry count     │ 8 bytes uint64 (number of key-value entries)
├─────────────────┤
│ entries         │ array of key-value entries
└─────────────────┘
```

each key-value entry contains:
- **key length**: 8 bytes uint64 (length of serialized key)
- **key**: the attribute value serialized as bytes (variable size based on type)
- **offsets count**: 8 bytes uint64 (number of feature offsets)
- **offsets**: array of 8-byte uint64 values pointing to features

the index is sorted by key value to enable efficient binary search operations.

### serialization by type

different attribute types are serialized differently using the `ByteSerializable` trait:
- **integers**: stored in little-endian binary format (i8, i16, i32, i64, u8, u16, u32, u64)
- **floating point**: wrapped in `OrderedFloat` for proper ordering and stored in little-endian format (f32, f64)
- **string**: utf-8 bytes without length prefix (length is stored separately)
- **boolean**: single byte (1 for true, 0 for false)
- **datetime**: 12 bytes (8 bytes for seconds since epoch + 4 bytes for nanoseconds)
- **date**: 12 bytes (4 bytes for year, 4 bytes for month, 4 bytes for day)

### query algorithm

the index supports several query operations:

1. **exact match**: binary search to find the exact key
   ```rust
   binary_search_by_key(&key, |kv| &kv.key)
   ```

2. **range queries**: find all keys in a given range
   ```rust
   // Find starting index using binary search
   // Iterate through entries until upper bound is reached
   ```

3. **comparison operators**:
   - **equals (=)**: exact match using binary search
   - **not equals (!=)**: all offsets minus those matching the key
   - **greater than (>)**: range query from key (exclusive) to end
   - **greater than or equal (>=)**: range query from key (inclusive) to end
   - **less than (<)**: range query from start to key (exclusive)
   - **less than or equal (<=)**: range query from start to key (inclusive)

4. **compound queries**:
   - multiple conditions are combined using set intersection
   - each condition produces a set of matching offsets
   - the final result is the intersection of all sets

### multi-index system

flatcitybuf uses a `MultiIndex` that maps field names to their corresponding indices:

```rust
pub struct MultiIndex {
    pub indices: HashMap<String, Box<dyn AnyIndex>>,
}
```

this allows queries to reference fields by name and supports heterogeneous index types for different attribute types.

### http optimization

when used with http range requests, the attribute index enables efficient filtering:
1. query the attribute index to find matching feature offsets
2. batch nearby offsets to minimize http requests
3. fetch only the features that match the query criteria

## boundaries, semantics, and appearances encoding

### boundaries encoding

geometry boundaries in flatcitybuf use a hierarchical indexing approach:

```
┌─────────────┬─────────────┬─────────────┬─────────────┐
│ solids      │ shells      │ surfaces    │ strings     │
└─────────────┴─────────────┴─────────────┴─────────────┘
      │              │             │             │
      │              │             │             └─ indices into boundaries array
      │              │             └─ indices into strings array
      │              └─ indices into surfaces array
      └─ indices into shells array

boundaries: [v1, v2, v3, v4, v5, v6, ...] // vertex indices
```

this structure allows for efficient representation of complex 3d geometries:
- **multipoint**: only boundaries array is used
- **multilinestring**: strings and boundaries arrays are used
- **multisurface/compositesurface**: surfaces, strings, and boundaries arrays are used
- **solid**: shells, surfaces, strings, and boundaries arrays are used
- **multisolid/compositesolid**: all arrays are used

### semantics encoding

semantic information is stored similarly to boundaries:

```
semantics: [s1, s2, s3, ...] // indices into semantics_objects array
semantics_objects: [SemanticObject1, SemanticObject2, ...]
```

each semantic object contains:
- **type**: the semantic surface type (e.g., wall, roof)
- **attributes**: additional semantic attributes
- **parent/children**: hierarchical relationships

### appearances encoding

appearances (materials and textures) are encoded using:

```
material: [MaterialMapping1, MaterialMapping2, ...]
texture: [TextureMapping1, TextureMapping2, ...]
```

each mapping contains:
- **theme**: the theme name
- **indices**: hierarchical indices matching the boundaries structure
- **values**: material or texture indices

this approach allows for efficient mapping of materials and textures to specific geometry parts.

## attributes encoding

attributes in flatcitybuf are encoded as binary data with a schema defined in the header:

### column schema

each attribute has a column definition:
```flatbuffers
table Column {
  index: ushort;                // Column index
  name: string (required);      // Column name
  type: ColumnType;             // Column type
  // ... additional metadata ...
}
```

### binary encoding

attributes are stored as a binary blob with values encoded according to their type:
- **numeric types**: native binary representation
- **string**: length-prefixed utf-8 string
- **boolean**: single byte (0 or 1)
- **json**: length-prefixed json string
- **binary**: length-prefixed binary data

### attribute access

to access an attribute:
1. find the column definition in the header
2. locate the attribute data in the feature's attributes array
3. deserialize according to the column type

## http range requests mechanism

flatcitybuf is designed for efficient access over http using range requests, allowing clients to fetch only the parts of the file they need:

### range request workflow

1. **header retrieval**:
   - client first fetches the magic bytes (4 bytes)
   - then fetches the header size (4 bytes)
   - finally fetches the header (variable size)
   - the client also prefetches a small portion of the r-tree index to optimize subsequent requests

2. **spatial query**:
   - client traverses the r-tree index using range requests
   - for leaf nodes, the client determines feature locations from their offsets
   - feature sizes are determined implicitly by the difference between consecutive offsets
   - for the last feature, the range extends to the end of the file

3. **attribute query**:
   - client traverses the attribute index using range requests
   - retrieves feature offsets for matching attribute values
   - batches feature requests to minimize http overhead

4. **feature retrieval**:
   - client fetches features using their byte ranges
   - features are decoded using flatbuffers zero-copy access
   - geometry and attributes are processed on demand

### optimization techniques

flatcitybuf implements several optimizations for http access:

1. **request batching**:
   - nearby features are grouped into batches to reduce the number of http requests
   - a configurable threshold determines when to combine requests vs. making separate requests
   - this balances between minimizing requests and avoiding excessive data transfer

2. **buffered client**:
   - uses a buffered http client that caches previously fetched data
   - avoids redundant requests for overlapping ranges
   - implements speculative prefetching for anticipated data

3. **minimal header size**:
   - the header is kept small to minimize initial loading time
   - only essential metadata is included in the header

4. **progressive loading**:
   - features are loaded on demand as they're needed
   - supports streaming iteration through features
   - allows applications to start processing data before the entire file is downloaded

## file dependencies graph

the dependencies between components in flatcitybuf are:

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ header.fbs  │────>│ feature.fbs │<────│ client app  │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       v                   v                   v
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ encoder     │────>│ fcb file    │<────│ decoder     │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       v                   v                   v
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ cityjson    │     │ http server │────>│ web viewer  │
└─────────────┘     └─────────────┘     └─────────────┘
```

key dependencies:
- **schema files**: define the structure of the format
- **encoder/decoder**: convert between cityjson and flatcitybuf
- **http server**: serves flatcitybuf files with range request support
- **client applications**: web viewers, gis software, analysis tools

the modular design allows for implementation in multiple languages (rust, javascript, python) while maintaining format compatibility.