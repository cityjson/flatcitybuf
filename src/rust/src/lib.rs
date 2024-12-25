#![allow(clippy::manual_range_contains)]

mod cj_utils;
mod error;
mod fb_cj;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
mod feature_generated;
mod file_reader;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
mod header_generated;
mod writer;

pub use cj_utils::*;
pub use fb_cj::*;
pub use file_reader::*;
pub use header_generated::*;
pub use writer::*;

pub const VERSION: u8 = 1;
pub(crate) const MAGIC_BYTES: [u8; 8] = [b'f', b'c', b'b', VERSION, b'f', b'c', b'b', 0];
pub(crate) const HEADER_MAX_BUFFER_SIZE: usize = 1048576 * 10;

fn check_magic_bytes(bytes: &[u8]) -> bool {
    bytes[0..3] == MAGIC_BYTES[0..3] && bytes[4..7] == MAGIC_BYTES[4..7] && bytes[3] <= VERSION
}
