#![cfg_attr(docsrs, feature(doc_cfg))]
// The crate docs below link to `HttpFcbReader` and `http_reader`, which only
// exist with the (default) `http` feature. Documenting without it is a valid
// configuration, so silence the links there rather than dropping them.
#![cfg_attr(
    not(all(feature = "http", not(target_arch = "wasm32"))),
    allow(rustdoc::broken_intra_doc_links)
)]
//! **FlatCityBuf** (`.fcb`) — a cloud-optimized binary encoding of
//! [CityJSON]. It carries the standard's semantics in [FlatBuffers], laid out
//! so that a client can read only the bytes it actually needs.
//!
//! A file is five contiguous sections:
//!
//! ```text
//! | magic bytes | header | packed Hilbert | static B+tree | features |
//! | 8 bytes     |        | R-tree (opt.)  | indices (opt.)|          |
//! ```
//!
//! - the **header** is one FlatBuffers table: transform (scale/translate for
//!   the quantized integer vertices), CRS, geographical extent, appearance,
//!   geometry templates, and the attribute column schema;
//! - the **packed Hilbert R-tree** answers bbox and point queries without a
//!   scan. Features are stored in Hilbert order, so a hit list is a set of
//!   sorted, coalescible byte ranges;
//! - the **static B+tree** indices answer attribute queries (`==`, `!=`, `<`,
//!   `<=`, `>`, `>=`) over the columns chosen at write time;
//! - each **feature** is a size-prefixed `CityFeature` table — one per line of
//!   the source CityJSONSeq.
//!
//! Because the layout is seek-friendly, the same reader works over a local
//! file (`Read + Seek`), a non-seekable stream (`Read`), or a remote URL via
//! HTTP range requests ([`HttpFcbReader`], `http` feature, enabled by default).
//!
//! `fcb_core` is the reference implementation of the format and the only one
//! that *writes* it; the C++, Python and TypeScript readers in the same
//! repository are validated against its output.
//!
//! # Reading a file
//!
//! [`FcbReader::open`] parses and verifies the header; `select_*` then returns
//! a fallible streaming iterator over the features. Only one feature is held
//! in memory at a time.
//!
//! ```no_run
//! use fcb_core::{deserializer::to_cj_metadata, FcbReader};
//! use std::fs::File;
//! use std::io::BufReader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let file = BufReader::new(File::open("delft.fcb")?);
//! let mut features = FcbReader::open(file)?.select_all()?;
//!
//! // The CityJSON metadata object (version, transform, CRS, extent) is the
//! // header; it is the first line of the equivalent CityJSONSeq document.
//! let cj = to_cj_metadata(&features.header())?;
//! println!("CityJSON {}, {} features", cj.version, features.header().features_count());
//!
//! while let Some(feature) = features.next()? {
//!     let cj_feature = feature.cur_cj_feature()?;
//!     println!("{}: {} city object(s)", cj_feature.id, cj_feature.city_objects.len());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Spatial and attribute queries
//!
//! [`FcbReader::select_query`] uses the R-tree; [`FcbReader::select_attr_query`]
//! uses the B+tree indices. Both skip straight to the matching features.
//!
//! ```no_run
//! use fcb_core::{AttrQuery, FcbReader, KeyType, Operator, SpatialQuery};
//! use std::fs::File;
//! use std::io::BufReader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Everything inside a bounding box (min_x, min_y, max_x, max_y).
//! let file = BufReader::new(File::open("delft.fcb")?);
//! let bbox = SpatialQuery::BBox(84_000.0, 446_000.0, 85_000.0, 447_000.0);
//! let mut hits = FcbReader::open(file)?.select_query(bbox, None, None)?;
//! while let Some(feature) = hits.next()? {
//!     println!("{}", feature.cur_cj_feature()?.id);
//! }
//!
//! // Everything whose indexed `b3_h_dak_50p` attribute exceeds 2.0.
//! let file = BufReader::new(File::open("delft.fcb")?);
//! let query: AttrQuery = vec![(
//!     "b3_h_dak_50p".to_string(),
//!     Operator::Gt,
//!     KeyType::Float64(2.0.into()),
//! )];
//! let mut hits = FcbReader::open(file)?.select_attr_query(query)?;
//! while let Some(feature) = hits.next()? {
//!     println!("{}", feature.cur_cj_feature()?.id);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Writing a file
//!
//! [`FcbWriter`] takes the CityJSON metadata object plus a stream of
//! `CityJSONFeature`s and assembles header, indices and feature data on
//! [`FcbWriter::write`].
//!
//! ```no_run
//! use fcb_core::{
//!     attribute::{AttributeSchema, AttributeSchemaMethods},
//!     header_writer::HeaderWriterOptions,
//!     read_cityjson_from_reader, CJType, CJTypeKind, CityJSONSeq, FcbWriter,
//! };
//! use std::fs::File;
//! use std::io::{BufReader, BufWriter};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let input = BufReader::new(File::open("delft.city.jsonl")?);
//! let CJType::Seq(CityJSONSeq { cj, features }) =
//!     read_cityjson_from_reader(input, CJTypeKind::Seq)?
//! else {
//!     unreachable!("CJTypeKind::Seq always yields CJType::Seq")
//! };
//!
//! // Collect the attribute columns. Iterate the city objects in a
//! // deterministic order: `add_attributes` hands each new name the next free
//! // column index, so a `HashMap`'s random order would number the columns
//! // differently on every run.
//! let mut schema = AttributeSchema::new();
//! for feature in &features {
//!     let mut ids: Vec<&String> = feature.city_objects.keys().collect();
//!     ids.sort_unstable();
//!     for co in ids.into_iter().filter_map(|id| feature.city_objects.get(id)) {
//!         if let Some(attributes) = &co.attributes {
//!             schema.add_attributes(attributes);
//!         }
//!     }
//! }
//!
//! let options = HeaderWriterOptions {
//!     write_index: true,
//!     feature_count: features.len() as u64,
//!     index_node_size: 16,
//!     // Build a static B+tree over these columns. `None` = default
//!     // branching factor.
//!     attribute_indices: Some(vec![("b3_h_dak_50p".to_string(), None)]),
//!     geographical_extent: None,
//! };
//!
//! let mut fcb = FcbWriter::new(cj, Some(options), Some(schema), None)?;
//! for feature in &features {
//!     fcb.add_feature(feature)?;
//! }
//! fcb.write(BufWriter::new(File::create("delft.fcb")?))?;
//! # Ok(())
//! # }
//! ```
//!
//! # Reading over HTTP
//!
//! [`HttpFcbReader`] fetches the header, then the index, then only the byte
//! ranges holding the matching features — typically a handful of range
//! requests for a query against a multi-gigabyte file.
//!
//! ```no_run
//! # #[cfg(all(feature = "http", not(target_arch = "wasm32")))]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use fcb_core::{HttpFcbReader, SpatialQuery};
//!
//! let reader = HttpFcbReader::open("https://example.com/delft.fcb").await?;
//! let bbox = SpatialQuery::BBox(84_000.0, 446_000.0, 85_000.0, 447_000.0);
//! let mut features = reader.select_query(bbox).await?;
//!
//! while features.next().await?.is_some() {
//!     println!("{}", features.cur_cj_feature()?.id);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Feature flags
//!
//! | Flag | Default | Effect |
//! |---|---|---|
//! | `http` | **yes** | [`HttpFcbReader`] and the range-request query paths, via `reqwest` and `http-range-client`. Disable it (`default-features = false`) for a dependency-light, purely local reader. |
//!
//! [`http_reader`] is additionally gated on `not(target_arch = "wasm32")`
//! because it reaches for `reqwest`'s native client. The rest of the crate,
//! including the index search paths, still compiles for `wasm32`.
//!
//! # Attribution
//!
//! **Portions of this software are derived from FlatGeobuf**
//! - Source: <https://github.com/flatgeobuf/flatgeobuf>
//! - License: BSD 2-Clause License
//! - Copyright (c) 2018-2024, Björn Harrtell and contributors
//!
//! Specifically, the following components contain code derived from FlatGeobuf:
//! - Spatial indexing algorithms (packed R-tree implementation)
//! - HTTP range request handling (for Rust native part)
//! - Binary format design patterns
//!
//! We extend our gratitude to the FlatGeobuf team for their excellent work on efficient
//! geospatial binary formats, which provided the foundation for FlatCityBuf's spatial
//! indexing and serialization architecture.
//!
//! # License
//!
//! This project is licensed under the MIT License.
//! FlatGeobuf portions remain under their original BSD 2-Clause License.
//!
//! [CityJSON]: https://www.cityjson.org/
//! [FlatBuffers]: https://flatbuffers.dev/

mod cj_utils;
mod cjerror;
mod const_vars;
pub mod error;
pub mod fb;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
#[cfg_attr(docsrs, doc(cfg(feature = "http")))]
pub mod http_reader;
pub mod obj;

pub mod packed_rtree;
mod reader;
pub mod static_btree;
mod writer;

pub use cj_utils::*;
pub use const_vars::*;
pub use error::Error;
pub use fb::*;
pub use packed_rtree::{NodeItem, PackedRTree, Query as SpatialQuery, SearchResultItem};
pub use reader::*;
pub use static_btree::{
    Entry, FixedStringKey, Float, Key, KeyType, MemoryIndex, MemoryMultiIndex, MultiIndex,
    Operator, Query, QueryCondition, StreamIndex, StreamMultiIndex,
};
pub use writer::*;

#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
#[cfg_attr(docsrs, doc(cfg(feature = "http")))]
pub use http_reader::*;

/// Returns `true` if `bytes` starts with a FlatCityBuf magic-byte sequence
/// this build can read.
///
/// The 8-byte magic is `fcb` + a major version byte + `fcb` + a patch byte
/// (see [`MAGIC_BYTES`]). Only the two `fcb` triplets and the major version
/// are checked: byte 3 must be no greater than [`VERSION`], and byte 7 is
/// ignored.
///
/// # Panics
///
/// Panics if `bytes` is shorter than [`MAGIC_BYTES_SIZE`].
pub fn check_magic_bytes(bytes: &[u8]) -> bool {
    bytes[0..3] == MAGIC_BYTES[0..3] && bytes[4..7] == MAGIC_BYTES[4..7] && bytes[3] <= VERSION
}
