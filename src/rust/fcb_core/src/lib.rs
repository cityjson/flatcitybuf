mod cj_utils;
mod cjerror;
mod const_vars;
pub mod error;
pub mod fb;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
mod http_reader;

pub mod packed_rtree;
mod reader;
pub mod static_btree;
mod writer;

pub use cj_utils::*;
pub use const_vars::*;
pub use error::*;
pub use fb::*;
pub use packed_rtree::Query as SpatialQuery;
pub use packed_rtree::*;
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
