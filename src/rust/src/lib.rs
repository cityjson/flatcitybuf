#![allow(clippy::manual_range_contains)]

mod cj_utils;
mod error;
mod fcb_serde;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
mod feature_generated;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
mod header_generated;
mod reader;
mod writer;

pub use cj_utils::*;
pub use fcb_serde::*;
pub use header_generated::*;
pub use reader::*;
pub use writer::*;

pub const VERSION: u8 = 1;
pub(crate) const MAGIC_BYTES: [u8; 8] = [b'f', b'c', b'b', VERSION, b'f', b'c', b'b', 0];
pub(crate) const HEADER_MAX_BUFFER_SIZE: usize = 1024 * 1024 * 512; // 512MB

fn check_magic_bytes(bytes: &[u8]) -> bool {
    bytes[0..3] == MAGIC_BYTES[0..3] && bytes[4..7] == MAGIC_BYTES[4..7] && bytes[3] <= VERSION
}
