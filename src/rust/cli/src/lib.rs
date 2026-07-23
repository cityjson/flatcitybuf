//! FCB CLI Library
//!
//! This library exposes the merger and reader modules for integration testing.
//! The main CLI binary is in main.rs.

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

    #[error("inspect requires an interactive terminal; use `fcb info` for static output")]
    NotATerminal,
}
