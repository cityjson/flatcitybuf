mod cityjson;
mod error;
mod feature_generated;
mod feature_writer;
mod file_writer;
mod header_generated;

pub use file_writer::*;

pub const VERSION: u8 = 1;
pub(crate) const MAGIC_BYTES: [u8; 8] = [b'F', b'C', b'B', VERSION, b'1', b'0', b'0', b'0'];
pub(crate) const HEADER_MAX_BUFFER_SIZE: usize = 1048576 * 10;

fn check_magic_byte(bytes: &[u8]) -> bool {
    bytes[0..3] == MAGIC_BYTES[0..3] && bytes[4..7] == MAGIC_BYTES[4..7] && bytes[3] <= VERSION
}
