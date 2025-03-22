mod cj_utils;
mod cjerror;
mod const_vars;
pub mod error;
pub mod fb;
#[allow(dead_code, unused_imports, clippy::all, warnings)]
#[cfg(feature = "http")]
#[cfg(feature = "wasm")]
mod http_reader;

mod reader;
mod writer;

pub use cj_utils::*;
pub use const_vars::*;
pub use error::*;
pub use fb::*;
pub use reader::*;
pub use writer::*;

#[cfg(feature = "http")]
#[cfg(feature = "wasm")]
pub use http_reader::*;

pub fn check_magic_bytes(bytes: &[u8]) -> bool {
    bytes[0..3] == MAGIC_BYTES[0..3] && bytes[4..7] == MAGIC_BYTES[4..7] && bytes[3] <= VERSION
}
