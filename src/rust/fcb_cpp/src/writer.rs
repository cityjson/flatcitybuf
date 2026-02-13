//! Writer bindings for C++

use cjseq::{CityJSON, CityJSONFeature};
use fcb_core::header_writer::HeaderWriterOptions;
use fcb_core::FcbWriter;
use std::fs::File;
use std::io::BufWriter;

/// Wrapper around FcbWriter for C++ interop
pub struct FcbFileWriter {
    cj_metadata: CityJSON,
    features: Vec<CityJSONFeature>,
}

/// Create a new FCB writer with CityJSON metadata
pub fn fcb_writer_new(metadata_json: &str) -> Result<Box<FcbFileWriter>, String> {
    let cj: CityJSON = serde_json::from_str(metadata_json)
        .map_err(|e| format!("Failed to parse CityJSON metadata: {}", e))?;

    Ok(Box::new(FcbFileWriter {
        cj_metadata: cj,
        features: Vec::new(),
    }))
}

/// Add a feature to the writer
pub fn fcb_writer_add_feature(
    writer: &mut FcbFileWriter,
    feature_json: &str,
) -> Result<(), String> {
    let feature: CityJSONFeature = serde_json::from_str(feature_json)
        .map_err(|e| format!("Failed to parse CityJSONFeature: {}", e))?;

    writer.features.push(feature);
    Ok(())
}

/// Write the FCB file to disk
pub fn fcb_writer_write(writer: Box<FcbFileWriter>, path: &str) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;
    let buf_writer = BufWriter::new(file);

    // Destructure Box to move fields instead of cloning
    let FcbFileWriter {
        cj_metadata,
        features,
    } = *writer;

    let header_options = HeaderWriterOptions {
        feature_count: features.len() as u64,
        ..HeaderWriterOptions::default()
    };

    let mut fcb_writer =
        FcbWriter::new(cj_metadata, Some(header_options), None, None)
            .map_err(|e| format!("Failed to create FCB writer: {}", e))?;

    for feature in &features {
        fcb_writer
            .add_feature(feature)
            .map_err(|e| format!("Failed to add feature: {}", e))?;
    }

    fcb_writer
        .write(buf_writer)
        .map_err(|e| format!("Failed to write FCB file: {}", e))?;

    Ok(())
}
