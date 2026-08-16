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
/// error: the converter reports it, and what it reports is written to stderr
/// rather than dropped silently.
///
/// The file's stem becomes the prefix of any object id the converter has to
/// invent, so that two files merged into one dataset cannot both name an
/// object `citygml-obj-0`.
fn read_citygml_file(path: &Path) -> Result<InputData, CliError> {
    let file = File::open(path)?;
    let opts = ParseOptions {
        id_prefix: path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned),
        ..ParseOptions::default()
    };
    let (doc, report) = fcb_citygml::parse_citygml(BufReader::new(file), &opts)
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
    // stderr as well as `tracing`, and not instead of it: nothing in this CLI
    // installs a subscriber, so every `tracing` call above is a no-op today
    // and a diagnostic that only went there would reach nobody.
    let _ = write_diagnostics(path, &report, &mut std::io::stderr());

    Ok(InputData {
        metadata: doc.metadata,
        features: doc.features,
    })
}

/// Write what the converter reported about one file: every warning in full,
/// and the skipped elements as a count.
///
/// The warnings are the ones a user can act on — an unrecognised `srsName`,
/// an appearance that paints nothing — and there are few of them. Skips are
/// counted instead: a real city file drops the same unsupported property
/// hundreds of times over, and `tracing` has each in full for anyone who
/// wants them.
fn write_diagnostics(
    path: &Path,
    report: &fcb_citygml::ParseReport,
    out: &mut impl std::io::Write,
) -> std::io::Result<()> {
    for warning in &report.warnings {
        writeln!(out, "  ⚠ {}: {warning}", path.display())?;
    }
    if !report.skipped.is_empty() {
        writeln!(
            out,
            "  ⚠ {}: skipped {} unsupported element(s)",
            path.display(),
            report.skipped.len()
        )?;
    }
    Ok(())
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

    /// A CityGML document with no CRS and an unreadable member: the warning
    /// and the skip both reach the user, on stderr, without a `tracing`
    /// subscriber existing anywhere in this CLI.
    #[test]
    fn diagnostics_are_written_out_in_full() {
        let report = fcb_citygml::ParseReport {
            skipped: vec![fcb_citygml::Skipped {
                element: "lod2TerrainIntersection".to_string(),
                gml_id: Some("b1".to_string()),
                reason: "no CityJSON counterpart for this property".to_string(),
            }],
            warnings: vec!["no srsName found; referenceSystem omitted".to_string()],
        };
        let mut out = Vec::new();
        write_diagnostics(Path::new("city.gml"), &report, &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            concat!(
                "  ⚠ city.gml: no srsName found; referenceSystem omitted\n",
                "  ⚠ city.gml: skipped 1 unsupported element(s)\n",
            )
        );
    }

    #[test]
    fn a_clean_document_says_nothing() {
        let mut out = Vec::new();
        write_diagnostics(
            Path::new("city.gml"),
            &fcb_citygml::ParseReport::default(),
            &mut out,
        )
        .unwrap();
        assert!(out.is_empty());
    }

    /// An object with no `gml:id` is named after the file it came from, so
    /// that merging two such files does not have them both claim
    /// `citygml-obj-0`.
    #[test]
    fn generated_object_ids_carry_the_file_stem() {
        let gml = r#"<?xml version="1.0" encoding="UTF-8"?>
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0">
  <core:cityObjectMember><bldg:Building/></core:cityObjectMember>
</core:CityModel>"#;
        let dir = tempfile::tempdir().unwrap();
        let ids = |name: &str| -> Vec<String> {
            let path = dir.path().join(name);
            std::fs::write(&path, gml).unwrap();
            let data = read_input_file(&path).unwrap();
            data.features
                .iter()
                .flat_map(|feature| feature.city_objects.keys().cloned())
                .collect()
        };
        assert_eq!(ids("tile-a.gml"), vec!["tile-a-0".to_string()]);
        assert_eq!(ids("tile-b.gml"), vec!["tile-b-0".to_string()]);
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
