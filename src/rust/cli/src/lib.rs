//! Support library for `fcb`, the FlatCityBuf command-line tool.
//!
//! The binary itself lives in `src/main.rs`; installing this crate gives you
//! `fcb`, which converts between CityJSON / CityJSONSeq and `.fcb`:
//!
//! ```text
//! fcb ser     city.jsonl city.fcb   # CityJSONSeq -> .fcb  (-A indexes every
//!                                   # attribute, -g writes the extent)
//! fcb deser   city.fcb   city.jsonl # .fcb -> CityJSONSeq
//! fcb inspect city.fcb              # terminal UI, or a static header report
//!                                   # when stdout is not a TTY (--static
//!                                   # forces it)
//! fcb cbor    city.json  city.cbor  # CityJSON -> CBOR (size comparison)
//! fcb bson    city.json  city.bson  # CityJSON -> BSON (size comparison)
//! ```
//!
//! Input and output are positional. `ser` takes any number of inputs before
//! the output, so `fcb ser a.jsonl b.jsonl merged.fcb` merges as it converts.
//!
//! The modules below are `pub` so the integration tests can drive them
//! directly; they are not a stable API. All the format work happens in
//! [`fcb_core`].
//!
//! `fcb ser` is what produces the oracle files the C++, Python and TypeScript
//! readers are validated against, so its output is load-bearing for the whole
//! repository.

pub mod inspect;
pub mod merger;
pub mod reader;

use fcb_core::error::Error;
use thiserror::Error;

/// CLI-specific error type
#[derive(Error, Debug)]
pub enum CliError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Glob pattern error: {0}")]
    GlobPattern(#[from] glob::PatternError),

    #[error("Glob error: {0}")]
    Glob(#[from] glob::GlobError),

    #[error("Unsupported file format for '{0}': {1}")]
    UnsupportedFormat(String, String),

    #[error("Empty file: {0}")]
    EmptyFile(String),

    #[error("No input files specified or matched")]
    NoInputFiles,

    #[error("FCB core error: {0}")]
    FcbCore(#[from] Error),
}
