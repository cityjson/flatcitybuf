//! # FlatCityBuf Core Library
//!
//! A high-performance Rust library for encoding and decoding CityJSON data to the FlatCityBuf (FCB) binary format.
//! FCB uses FlatBuffers for efficient serialization with support for spatial and attribute indexing.
//!
//! ## Attribution
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
//! ## License
//!
//! This project is licensed under the MIT License.
//! FlatGeobuf portions remain under their original BSD 2-Clause License.

mod cj_utils;
mod cjerror;
mod const_vars;
pub mod error;
pub mod fb;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub mod http_reader;

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
pub use http_reader::*;

pub fn check_magic_bytes(bytes: &[u8]) -> bool {
    bytes[0..3] == MAGIC_BYTES[0..3] && bytes[4..7] == MAGIC_BYTES[4..7] && bytes[3] <= VERSION
}
