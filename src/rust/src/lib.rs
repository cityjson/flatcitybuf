#![allow(clippy::manual_range_contains)]

mod cj_utils;
mod error;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
mod feature_generated;
mod feature_writer;
mod file_writer;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
mod header_generated;

pub use cj_utils::*;
pub use cjseq::*;
pub use file_writer::*;

// pub use feature_generated::*;
// pub use header_generated::*;

pub const VERSION: u8 = 1;
pub(crate) const MAGIC_BYTES: [u8; 8] = [b'F', b'C', b'B', VERSION, b'1', b'0', b'0', b'0'];
pub(crate) const HEADER_MAX_BUFFER_SIZE: usize = 1048576 * 10;

fn check_magic_byte(bytes: &[u8]) -> bool {
    bytes[0..3] == MAGIC_BYTES[0..3] && bytes[4..7] == MAGIC_BYTES[4..7] && bytes[3] <= VERSION
}
