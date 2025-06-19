# FlatCityBuf Core Library

A high-performance Rust library for encoding and decoding CityJSON data to the FlatCityBuf (FCB) binary format. FCB uses FlatBuffers for efficient serialization with support for spatial and attribute indexing.

## Features

- **Binary Format**: Efficient storage using FlatBuffers
- **Spatial Indexing**: Fast spatial queries with R-tree indexing
- **Attribute Indexing**: Query features by attribute values
- **HTTP Support**: Stream features over HTTP with range requests
- **Memory Efficient**: Streaming readers for large datasets
- **CityJSON Compatibility**: Full support for CityJSON 2.0 specification

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
fcb_core = "0.1.0"

# For HTTP support
fcb_core = { version = "0.1.0", features = ["http"] }
```

## Quick Start

### Writing FCB Files

```rust
use fcb_core::{
    FcbWriter, HeaderWriterOptions, AttributeSchema, AttributeSchemaMethods,
    read_cityjson_from_reader, CJTypeKind
};
use std::fs::File;
use std::io::{BufReader, BufWriter};

// read cityjson data
let input_file = File::open("input.city.jsonl")?;
let input_reader = BufReader::new(input_file);
let cj_seq = read_cityjson_from_reader(input_reader, CJTypeKind::Seq)?;

if let CJType::Seq(cj_seq) = cj_seq {
    // build attribute schema
    let mut attr_schema = AttributeSchema::new();
    for feature in cj_seq.features.iter() {
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                attr_schema.add_attributes(attributes);
            }
        }
    }

    // configure writer options
    let attr_indices = vec![
        ("building_type".to_string(), None),
        ("height".to_string(), None),
    ];

    let header_options = HeaderWriterOptions {
        write_index: true,
        feature_count: cj_seq.features.len() as u64,
        index_node_size: 16,
        attribute_indices: Some(attr_indices),
        geographical_extent: None,
    };

    // create writer and add features
    let output_file = File::create("output.fcb")?;
    let output_writer = BufWriter::new(output_file);

    let mut fcb = FcbWriter::new(
        cj_seq.cj,
        Some(header_options),
        Some(attr_schema),
        None
    )?;

    for feature in cj_seq.features.iter() {
        fcb.add_feature(feature)?;
    }

    fcb.write(output_writer)?;
}
```

you can also use the `fcb_cli` to serialize CityJSON to FCB. Check the [CLI README](../cli/README.md) for more details.

### Reading FCB Files

#### Read All Features

```rust
use fcb_core::{FcbReader, deserializer::to_cj_metadata};
use std::fs::File;
use std::io::BufReader;

let input_file = File::open("input.fcb")?;
let input_reader = BufReader::new(input_file);

let mut reader = FcbReader::open(input_reader)?.select_all()?;
let header = reader.header();
let cj_metadata = to_cj_metadata(&header)?;

println!("features: {}", header.features_count());

while let Some(feature_buf) = reader.next()? {
    let cj_feature = feature_buf.cur_cj_feature()?;
    // process feature
    println!("feature id: {}", cj_feature.id);
}
```

#### Spatial Queries (Bounding Box)

```rust
use fcb_core::{FcbReader, packed_rtree::Query};

let mut reader = FcbReader::open(input_reader)?
    .select_query(Query::BBox(minx, miny, maxx, maxy))?;

while let Some(feature_buf) = reader.next()? {
    let cj_feature = feature_buf.cur_cj_feature()?;
    // process spatially filtered feature
}
```

#### Attribute Queries

```rust
use fcb_core::{FcbReader, KeyType, Operator, FixedStringKey, Float};

let query = vec![
    (
        "height".to_string(),
        Operator::Gt,
        KeyType::Float64(Float(10.0)),
    ),
    (
        "building_type".to_string(),
        Operator::Eq,
        KeyType::StringKey50(FixedStringKey::from_str("residential")),
    ),
];

let mut reader = FcbReader::open(input_reader)?.select_attr_query(query)?;

while let Some(feature_buf) = reader.next()? {
    let cj_feature = feature_buf.cur_cj_feature()?;
    // process filtered features
}
```

### HTTP Streaming

```rust
use fcb_core::HttpFcbReader;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http_reader = HttpFcbReader::open("https://example.com/data.fcb").await?;

    // spatial query over http
    let mut iter = http_reader
        .select_query(Query::BBox(minx, miny, maxx, maxy))
        .await?;

    while let Some(feature) = iter.next().await? {
        let cj_feature = feature.cj_feature()?;
        // process feature streamed over http
    }

    Ok(())
}
```

## Attribution

Portions of this software are derived from [FlatGeobuf](https://github.com/flatgeobuf/flatgeobuf) (BSD 2-Clause License).
See [ATTRIBUTION.md](ATTRIBUTION.md) for detailed attribution information.

## API Reference

### Core Types

#### `FcbWriter<'a>`

Main writer for serializing CityJSON to FCB format.

**Methods:**

- `new(cj, header_options, attr_schema, semantic_attr_schema) -> Result<Self>`
- `add_feature(&mut self, feature) -> Result<()>`
- `write(self, output) -> Result<()>`

#### `FcbReader<R>`

Reader for deserializing FCB files.

**Methods:**

- `open(reader) -> Result<Self>`
- `select_all(self) -> Result<FeatureIter<R, Seekable>>`
- `select_query(self, query) -> Result<FeatureIter<R, Seekable>>`
- `select_attr_query(self, query) -> Result<FeatureIter<R, Seekable>>`
- `select_all_seq(self) -> Result<FeatureIter<R, NotSeekable>>`
- `select_query_seq(self, query) -> Result<FeatureIter<R, NotSeekable>>`
- `select_attr_query_seq(self, query) -> Result<FeatureIter<R, NotSeekable>>`

#### `HttpFcbReader<T>`

HTTP-based streaming reader.

**Methods:**

- `open(url) -> Result<Self>`
- `select_all(self) -> Result<AsyncFeatureIter<T>>`
- `select_query(self, query) -> Result<AsyncFeatureIter<T>>`
- `select_attr_query(self, query) -> Result<AsyncFeatureIter<T>>`

### Configuration

#### `HeaderWriterOptions`

Configuration for FCB header writing.

```rust
pub struct HeaderWriterOptions {
    pub write_index: bool,
    pub feature_count: u64,
    pub index_node_size: u16,
    pub attribute_indices: Option<Vec<(String, Option<u16>)>>,
    pub geographical_extent: Option<[f64; 6]>,
}
```

#### `AttributeSchema`

Schema for managing attribute types and indexing.

**Methods:**

- `new() -> Self`
- `add_attributes(&mut self, attributes)`
- `get(&self, name) -> Option<(u16, ColumnType)>`

### Query Types

#### Spatial Queries

```rust
use fcb_core::packed_rtree::Query;

// bounding box query
let bbox_query = Query::BBox(minx, miny, maxx, maxy);
```

#### Attribute Queries

```rust
use fcb_core::{KeyType, Operator, FixedStringKey, Float};

type AttrQuery = Vec<(String, Operator, KeyType)>;

// numeric comparison
let height_query = (
    "height".to_string(),
    Operator::Gt,
    KeyType::Float64(Float(10.0))
);

// string exact match
let type_query = (
    "building_type".to_string(),
    Operator::Eq,
    KeyType::StringKey50(FixedStringKey::from_str("residential"))
);

// datetime comparison
let date_query = (
    "registration_date".to_string(),
    Operator::Gt,
    KeyType::DateTime(chrono::DateTime::from_str("2020-01-01T00:00:00Z")?)
);
```

### Supported Operators

- `Operator::Eq` - equals
- `Operator::Gt` - greater than
- `Operator::Lt` - less than
- `Operator::Gte` - greater than or equal
- `Operator::Lte` - less than or equal

### Supported Key Types

- `KeyType::Float64(Float)` - 64-bit floating point
- `KeyType::StringKey50(FixedStringKey)` - fixed-length strings up to 50 chars
- `KeyType::DateTime(chrono::DateTime<Utc>)` - datetime values
- `KeyType::UByte(u8)` - unsigned 8-bit integer

## Error Handling

The library uses `thiserror` for structured error handling:

```rust
use fcb_core::error::{Error, Result};

match fcb_reader.select_all() {
    Ok(reader) => {
        // process features
    }
    Err(Error::NoIndex) => {
        println!("file has no spatial index");
    }
    Err(Error::AttributeIndexNotFound) => {
        println!("no attribute index found");
    }
    Err(e) => {
        println!("error: {}", e);
    }
}
```

## Features

Enable optional features in `Cargo.toml`:

```toml
[dependencies]
fcb_core = { version = "0.1.0", features = ["http"] }
```

- `http` - enables HTTP streaming capabilities

## Examples

See the `tests/` directory for comprehensive examples:

- `tests/attr_index.rs` - attribute indexing and querying
- `tests/http.rs` - HTTP streaming examples
- `tests/e2e.rs` - end-to-end serialization/deserialization
- `tests/read.rs` - various reading patterns

## Related

- [FlatCityBuf CLI](../cli/) - command-line tools
- [FlatCityBuf WASM](../wasm/) - web assembly bindings
- [CityJSON Specification](https://cityjson.org/)
- [FlatBuffers](https://flatbuffers.dev/)

## License

MIT License - see LICENSE file for details.
