//! Reader module for CityJSON, CityJSONSeq and CityGML file reading
//!
//! This module provides utilities to read CityJSON (`.json`),
//! CityJSONTextSequence (`.jsonl`) and CityGML 2.0 (`.gml` / `.xml`) files and
//! convert them to a unified in-memory representation of CityJSON metadata and
//! features. CityGML is converted on the way in by [`fcb_citygml`], so the
//! rest of the pipeline never sees anything but CityJSON.

use cjseq::{CityJSON, CityJSONFeature};
use fcb_citygml::ParseOptions;
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
    /// CityGML 2.0 file (`.gml` or `.xml`)
    CityGML,
}

impl InputFormat {
    /// Detect the format from file extension
    pub fn from_path(path: &Path) -> Result<Self, CliError> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Ok(InputFormat::CityJSON),
            Some("jsonl") => Ok(InputFormat::CityJSONSeq),
            Some("gml") | Some("xml") => Ok(InputFormat::CityGML),
            _ => Err(CliError::UnsupportedFormat(
                path.display().to_string(),
                "expected .json, .jsonl, .gml or .xml extension".to_string(),
            )),
        }
    }
}

/// Result of reading an input file
pub struct InputData {
    /// CityJSON metadata (first line of CityJSONSeq)
    pub metadata: CityJSON,
    /// CityJSON features
    pub features: Vec<CityJSONFeature>,
}

/// Read a CityJSON, CityJSONSeq or CityGML file and return unified data
///
/// - `.json` files are parsed as CityJSON and converted to features
/// - `.jsonl` files are parsed as CityJSONTextSequence directly
/// - `.gml` / `.xml` files are parsed as CityGML 2.0 and converted
pub fn read_input_file(path: &Path) -> Result<InputData, CliError> {
    let format = InputFormat::from_path(path)?;

    match format {
        InputFormat::CityJSON => read_cityjson_file(path),
        InputFormat::CityJSONSeq => read_cityjsonseq_file(path),
        InputFormat::CityGML => read_citygml_file(path),
    }
}

/// Read a CityJSON file and convert to features
fn read_cityjson_file(path: &Path) -> Result<InputData, CliError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    // Parse as full CityJSON
    let mut cj: CityJSON = serde_json::from_reader(reader)?;
    cj.sort_cjfeatures(cjseq::SortingStrategy::Random);
    // Extract features using cjseq library pattern
    let mut features = Vec::new();
    let mut i = 0;
    while let Some(feature) = cj.get_cjfeature(i) {
        features.push(feature);
        i += 1;
    }

    // Get metadata (CityJSON without city_objects)
    let metadata = cj.get_metadata();

    Ok(InputData { metadata, features })
}

/// Read a CityJSONSeq file (first line is metadata, rest are features)
fn read_cityjsonseq_file(path: &Path) -> Result<InputData, CliError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    // First line is the CityJSON metadata
    let first_line = lines
        .next()
        .ok_or_else(|| CliError::EmptyFile(path.display().to_string()))??;
    let metadata: CityJSON = serde_json::from_str(&first_line)?;

    // Remaining lines are CityJSONFeatures
    let mut features = Vec::new();
    for line in lines {
        let line = line?;
        if !line.trim().is_empty() {
            let feature: CityJSONFeature = serde_json::from_str(&line)?;
            features.push(feature);
        }
    }

    Ok(InputData { metadata, features })
}

/// Read a CityGML 2.0 file and convert it to CityJSON metadata + features
///
/// Content that is valid CityGML but has no CityJSON representation is not an
/// error: the converter reports it, and it is logged here rather than dropped
/// silently.
fn read_citygml_file(path: &Path) -> Result<InputData, CliError> {
    let file = File::open(path)?;
    let (doc, report) = fcb_citygml::parse_citygml(BufReader::new(file), &ParseOptions::default())
        .map_err(|e| CliError::CityGml(path.display().to_string(), e.to_string()))?;

    for skipped in &report.skipped {
        tracing::warn!(
            file = %path.display(),
            element = %skipped.element,
            gml_id = ?skipped.gml_id,
            reason = %skipped.reason,
            "skipped CityGML element"
        );
    }
    for warning in &report.warnings {
        tracing::warn!(file = %path.display(), "{warning}");
    }
    if !report.skipped.is_empty() {
        eprintln!(
            "  ⚠ {}: skipped {} unsupported element(s)",
            path.display(),
            report.skipped.len()
        );
    }

    Ok(InputData {
        metadata: doc.metadata,
        features: doc.features,
    })
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
    fn test_detect_format_gml() {
        let path = PathBuf::from("city.gml");
        assert_eq!(InputFormat::from_path(&path).unwrap(), InputFormat::CityGML);
    }

    #[test]
    fn test_detect_format_xml() {
        let path = PathBuf::from("city.xml");
        assert_eq!(InputFormat::from_path(&path).unwrap(), InputFormat::CityGML);
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
