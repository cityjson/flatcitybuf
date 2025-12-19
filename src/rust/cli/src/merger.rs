//! Merger module for combining multiple CityJSON/CityJSONSeq files
//!
//! This module handles merging multiple input files with transform alignment.
//! When files have different transforms (scale/translate), vertices from
//! subsequent files are converted to match the first file's transform.

use cjseq::{CityJSON, CityJSONFeature, Transform};
use std::path::PathBuf;

use crate::reader::read_input_file;
use crate::CliError;

/// Result of merging multiple input files
pub struct MergeResult {
    /// Merged CityJSON metadata (from first file)
    pub metadata: CityJSON,
    /// All features from all files (with aligned transforms)
    pub features: Vec<CityJSONFeature>,
}

/// Merge multiple CityJSON/CityJSONSeq files into a single result
///
/// The first file's transform becomes the reference. Features from subsequent
/// files have their vertices converted to use the reference transform.
pub fn merge_files(paths: Vec<PathBuf>) -> Result<MergeResult, CliError> {
    if paths.is_empty() {
        return Err(CliError::NoInputFiles);
    }

    let mut paths_iter = paths.into_iter();

    // Read the first file - its transform becomes the reference
    let first_path = paths_iter.next().ok_or(CliError::NoInputFiles)?;
    let first_data = read_input_file(&first_path)?;
    let reference_transform = first_data.metadata.transform.clone();

    let mut result = MergeResult {
        metadata: first_data.metadata,
        features: first_data.features,
    };

    // Process remaining files
    for path in paths_iter {
        let data = read_input_file(&path)?;

        // Check if transforms are the same
        if transforms_equal(&data.metadata.transform, &reference_transform) {
            // Same transform - just append features
            result.features.extend(data.features);
        } else {
            // Different transform - need to convert vertices
            for feature in data.features {
                let converted = convert_feature_transform(
                    feature,
                    &data.metadata.transform,
                    &reference_transform,
                );
                result.features.push(converted);
            }
        }
    }

    Ok(result)
}

/// Check if two transforms are equal
fn transforms_equal(a: &Transform, b: &Transform) -> bool {
    a.scale == b.scale && a.translate == b.translate
}

/// Convert a feature's vertices from one transform to another
///
/// This converts:
/// 1. Integer vertices → real coordinates (using source transform)
/// 2. Real coordinates → integer vertices (using target transform)
fn convert_feature_transform(
    mut feature: CityJSONFeature,
    source: &Transform,
    target: &Transform,
) -> CityJSONFeature {
    for vertex in &mut feature.vertices {
        if vertex.len() >= 3 {
            // Convert to real coordinates using source transform
            let real_x = (vertex[0] as f64 * source.scale[0]) + source.translate[0];
            let real_y = (vertex[1] as f64 * source.scale[1]) + source.translate[1];
            let real_z = (vertex[2] as f64 * source.scale[2]) + source.translate[2];

            // Convert back to integers using target transform
            vertex[0] = ((real_x - target.translate[0]) / target.scale[0]).round() as i64;
            vertex[1] = ((real_y - target.translate[1]) / target.scale[1]).round() as i64;
            vertex[2] = ((real_z - target.translate[2]) / target.scale[2]).round() as i64;
        }
    }

    feature
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_transform(scale: [f64; 3], translate: [f64; 3]) -> Transform {
        Transform {
            scale: scale.to_vec(),
            translate: translate.to_vec(),
        }
    }

    #[test]
    fn test_transforms_equal() {
        let t1 = make_transform([0.001, 0.001, 0.001], [0.0, 0.0, 0.0]);
        let t2 = make_transform([0.001, 0.001, 0.001], [0.0, 0.0, 0.0]);
        let t3 = make_transform([0.001, 0.001, 0.001], [1.0, 0.0, 0.0]);

        assert!(transforms_equal(&t1, &t2));
        assert!(!transforms_equal(&t1, &t3));
    }

    #[test]
    fn test_convert_feature_transform_identity() {
        // Same transform should produce same vertices
        let source = make_transform([0.001, 0.001, 0.001], [0.0, 0.0, 0.0]);
        let target = source.clone();

        let mut feature = CityJSONFeature::new();
        feature.vertices = vec![vec![1000, 2000, 3000]];

        let converted = convert_feature_transform(feature.clone(), &source, &target);
        assert_eq!(converted.vertices[0], vec![1000, 2000, 3000]);
    }

    #[test]
    fn test_convert_feature_transform_different() {
        // Source: vertex 1000 means 1.0 real coordinate
        let source = make_transform([0.001, 0.001, 0.001], [0.0, 0.0, 0.0]);
        // Target: 1.0 real coordinate should become 500
        let target = make_transform([0.002, 0.002, 0.002], [0.0, 0.0, 0.0]);

        let mut feature = CityJSONFeature::new();
        feature.vertices = vec![vec![1000, 2000, 3000]];

        let converted = convert_feature_transform(feature, &source, &target);
        // 1000 * 0.001 = 1.0 -> 1.0 / 0.002 = 500
        assert_eq!(converted.vertices[0], vec![500, 1000, 1500]);
    }

    #[test]
    fn test_convert_feature_transform_with_translate() {
        let source = make_transform([0.001, 0.001, 0.001], [100.0, 200.0, 0.0]);
        let target = make_transform([0.001, 0.001, 0.001], [0.0, 0.0, 0.0]);

        let mut feature = CityJSONFeature::new();
        feature.vertices = vec![vec![0, 0, 0]]; // Real: (100, 200, 0)

        let converted = convert_feature_transform(feature, &source, &target);
        // Real (100, 200, 0) with target translate (0, 0, 0) and scale 0.001
        // -> (100 / 0.001, 200 / 0.001, 0) = (100000, 200000, 0)
        assert_eq!(converted.vertices[0], vec![100000, 200000, 0]);
    }
}
