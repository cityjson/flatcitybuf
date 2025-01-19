#![allow(clippy::manual_range_contains)]

mod cj_utils;
mod const_vars;
mod error;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
mod feature_generated;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
mod header_generated;
mod http_reader;
mod packedrtree;
mod reader;
mod writer;

pub use cj_utils::*;
pub(crate) use const_vars::*;
pub use feature_generated::*;
pub use header_generated::*;
#[cfg(feature = "http")]
pub use http_reader::*;
pub use packedrtree::*;
pub use reader::*;
pub use writer::*;

fn check_magic_bytes(bytes: &[u8]) -> bool {
    bytes[0..3] == MAGIC_BYTES[0..3] && bytes[4..7] == MAGIC_BYTES[4..7] && bytes[3] <= VERSION
}
