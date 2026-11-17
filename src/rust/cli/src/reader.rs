//! Reader module for CityJSON and CityJSONSeq file reading
//!
//! This module provides utilities to read CityJSON (`.json`) and
//! CityJSONTextSequence (`.jsonl`) files and convert them to a unified
//! in-memory representation of CityJSON metadata and features.

use cjseq::{CityJSON, CityJSONFeature};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::CliError;

/// Detected input file format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    /// CityJSON file (`.json`)
    CityJSON,
    /// CityJSONTextSequence file (`.jsonl`)
    CityJSONSeq,
}

impl InputFormat {
    /// Detect the format from file extension
    pub fn from_path(path: &Path) -> Result<Self, CliError> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Ok(InputFormat::CityJSON),
            Some("jsonl") => Ok(InputFormat::CityJSONSeq),
            _ => Err(CliError::UnsupportedFormat(
                path.display().to_string(),
                "expected .json or .jsonl extension".to_string(),
            )),
        }
    }
}

/// Result of reading an input file into memory
pub struct InputData {
    /// CityJSON metadata (first line of CityJSONSeq)
    pub metadata: CityJSON,
    /// CityJSON features
    pub features: Vec<CityJSONFeature>,
}

/// Read a CityJSON or CityJSONSeq file into memory and return unified data
///
/// - `.json` files are parsed as CityJSON and converted to features
/// - `.jsonl` files are parsed as CityJSONTextSequence directly
pub fn read_input_file(path: &Path) -> Result<InputData, CliError> {
    let format = InputFormat::from_path(path)?;

    match format {
        InputFormat::CityJSON => read_cityjson_file(path),
        InputFormat::CityJSONSeq => read_cityjsonseq_file(path),
    }
}

/// Read a CityJSON file and convert to features
fn read_cityjson_file(path: &Path) -> Result<InputData, CliError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    // Parse as full CityJSON and slice it into a sequence: the header, then
    // one feature per top-level City Object, in document order.
    let cj: CityJSON = serde_json::from_reader(reader)?;
    let features = cj
        .features(cjseq::SortingStrategy::AsIs)
        .collect::<Result<Vec<_>, _>>()?;
    let metadata = cj.header()?;

    Ok(InputData { metadata, features })
}

/// Read a CityJSONSeq file (first line is metadata, rest are features)
fn read_cityjsonseq_file(path: &Path) -> Result<InputData, CliError> {
    let (metadata, features) = read_cityjsonseq(path)?;
    let features = features.collect::<Result<Vec<_>, _>>()?;
    Ok(InputData { metadata, features })
}

/// Open a CityJSONSeq file: its metadata line, parsed, and its features,
/// parsed one line at a time as the iterator is advanced. Blank lines are
/// skipped.
///
/// Nothing is held in memory but the line being parsed, so this is the way to
/// read a sequence too large to fit.
pub fn read_cityjsonseq(
    path: &Path,
) -> Result<
    (
        CityJSON,
        impl Iterator<Item = Result<CityJSONFeature, CliError>>,
    ),
    CliError,
> {
    let mut lines = open_cityjsonseq(path)?;
    let first_line = lines
        .next()
        .ok_or_else(|| CliError::EmptyFile(path.display().to_string()))??;
    let metadata: CityJSON = serde_json::from_str(&first_line)?;
    let features = lines.filter_map(|line| match line {
        Err(error) => Some(Err(error.into())),
        Ok(line) if line.trim().is_empty() => None,
        Ok(line) => Some(serde_json::from_str::<CityJSONFeature>(&line).map_err(Into::into)),
    });
    Ok((metadata, features))
}

/// How many features a CityJSONSeq file holds: its non-blank lines after the
/// metadata line, counted without parsing them.
pub fn count_cityjsonseq_features(path: &Path) -> Result<usize, CliError> {
    let mut lines = open_cityjsonseq(path)?;
    lines
        .next()
        .ok_or_else(|| CliError::EmptyFile(path.display().to_string()))??;
    lines.try_fold(0, |count, line| {
        Ok(count + usize::from(!line?.trim().is_empty()))
    })
}

fn open_cityjsonseq(path: &Path) -> Result<std::io::Lines<BufReader<File>>, CliError> {
    let file = File::open(path)?;
    Ok(BufReader::with_capacity(1 << 20, file).lines())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_format_jsonl() {
        let path = PathBuf::from("test.city.jsonl");
        assert_eq!(
            InputFormat::from_path(&path).unwrap(),
            InputFormat::CityJSONSeq
        );
    }

    #[test]
    fn test_detect_format_json() {
        let path = PathBuf::from("test.city.json");
        assert_eq!(
            InputFormat::from_path(&path).unwrap(),
            InputFormat::CityJSON
        );
    }

    #[test]
    fn test_detect_format_invalid() {
        let path = PathBuf::from("test.txt");
        assert!(InputFormat::from_path(&path).is_err());
    }

    #[test]
    fn test_read_cityjsonseq_file() {
        let test_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fcb_core/tests/data/small.city.jsonl");

        if test_file.exists() {
            let result = read_input_file(&test_file).unwrap();
            assert!(!result.features.is_empty());
        }
    }
}
