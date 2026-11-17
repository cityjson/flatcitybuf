//! Merger module for combining multiple CityJSON/CityJSONSeq files
//!
//! This module handles merging multiple input files with transform alignment.
//! When files have different transforms (scale/translate), vertices from
//! subsequent files are converted to match the first file's transform.
//!
//! A CityJSONSeq (`.jsonl`) input is not held in memory: it is read again,
//! one line at a time, each time the merged features are iterated. A CityJSON
//! (`.json`) document has to be parsed whole to be sliced into features, so
//! its features are kept in memory.

use cjseq::{CityJSON, CityJSONFeature, Transform};
use std::borrow::Cow;
use std::path::PathBuf;

use crate::reader::{count_cityjsonseq_features, read_cityjsonseq, read_input_file, InputFormat};
use crate::CliError;

/// Several CityJSON/CityJSONSeq files merged onto the first one's transform.
///
/// The features are not collected: [`MergedInput::features`] reads them from
/// their files, in order, every time it is called.
pub struct MergedInput {
    /// Merged CityJSON metadata (from first file)
    pub metadata: CityJSON,
    sources: Vec<Source>,
    len: usize,
}

/// One input file.
enum Source {
    /// A CityJSONSeq file, read again on every pass, and its transform when it
    /// is not the reference transform.
    Sequence {
        path: PathBuf,
        transform: Option<Transform>,
    },
    /// A CityJSON document's features, already aligned to the reference
    /// transform.
    Document(Vec<CityJSONFeature>),
}

impl MergedInput {
    /// How many features the inputs hold.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The features of every input, in input order, with their vertices on
    /// the first input's transform. A CityJSONSeq input is parsed again, one
    /// line at a time, on each call.
    pub fn features(&self) -> impl Iterator<Item = FeatureResult<'_>> + '_ {
        self.sources
            .iter()
            .flat_map(|source| source.features(&self.metadata.transform))
    }
}

/// A feature of a [`MergedInput`]: borrowed when it is held in memory, owned
/// when it was read from its file for this pass.
pub type FeatureResult<'a> = Result<Cow<'a, CityJSONFeature>, CliError>;

impl Source {
    fn features<'a>(
        &'a self,
        reference: &'a Transform,
    ) -> Box<dyn Iterator<Item = FeatureResult<'a>> + 'a> {
        match self {
            Source::Document(features) => {
                Box::new(features.iter().map(|feature| Ok(Cow::Borrowed(feature))))
            }
            Source::Sequence { path, transform } => match read_cityjsonseq(path) {
                Err(error) => Box::new(std::iter::once(Err(error))),
                Ok((_, features)) => Box::new(features.map(move |feature| {
                    let feature = feature?;
                    Ok(Cow::Owned(match transform {
                        Some(source) => convert_feature_transform(feature, source, reference),
                        None => feature,
                    }))
                })),
            },
        }
    }
}

/// Open several CityJSON/CityJSONSeq files as one input.
///
/// The first file's transform becomes the reference. Features from subsequent
/// files have their vertices converted to use the reference transform.
///
/// Each CityJSONSeq file is read here only for its metadata line and its
/// feature count; its features are parsed when [`MergedInput::features`] is
/// iterated.
pub fn merge_files(paths: Vec<PathBuf>) -> Result<MergedInput, CliError> {
    if paths.is_empty() {
        return Err(CliError::NoInputFiles);
    }

    let mut metadata: Option<CityJSON> = None;
    let mut sources = Vec::with_capacity(paths.len());
    let mut len = 0;

    for path in paths {
        let (file_metadata, source) = match InputFormat::from_path(&path)? {
            InputFormat::CityJSONSeq => {
                let (file_metadata, _) = read_cityjsonseq(&path)?;
                len += count_cityjsonseq_features(&path)?;
                (
                    file_metadata,
                    Source::Sequence {
                        path,
                        transform: None,
                    },
                )
            }
            InputFormat::CityJSON => {
                let data = read_input_file(&path)?;
                len += data.features.len();
                (data.metadata, Source::Document(data.features))
            }
        };

        let source = match &metadata {
            // The first file's transform is the reference
            None => source,
            Some(reference) if transforms_equal(&file_metadata.transform, &reference.transform) => {
                source
            }
            // Different transform - need to convert vertices
            Some(reference) => match source {
                Source::Sequence { path, .. } => Source::Sequence {
                    path,
                    transform: Some(file_metadata.transform.clone()),
                },
                Source::Document(features) => Source::Document(
                    features
                        .into_iter()
                        .map(|feature| {
                            convert_feature_transform(
                                feature,
                                &file_metadata.transform,
                                &reference.transform,
                            )
                        })
                        .collect(),
                ),
            },
        };
        sources.push(source);
        metadata.get_or_insert(file_metadata);
    }

    Ok(MergedInput {
        metadata: metadata.ok_or(CliError::NoInputFiles)?,
        sources,
        len,
    })
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
        Transform { scale, translate }
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
        feature.vertices = vec![[1000, 2000, 3000]];

        let converted = convert_feature_transform(feature.clone(), &source, &target);
        assert_eq!(converted.vertices[0], [1000, 2000, 3000]);
    }

    #[test]
    fn test_convert_feature_transform_different() {
        // Source: vertex 1000 means 1.0 real coordinate
        let source = make_transform([0.001, 0.001, 0.001], [0.0, 0.0, 0.0]);
        // Target: 1.0 real coordinate should become 500
        let target = make_transform([0.002, 0.002, 0.002], [0.0, 0.0, 0.0]);

        let mut feature = CityJSONFeature::new();
        feature.vertices = vec![[1000, 2000, 3000]];

        let converted = convert_feature_transform(feature, &source, &target);
        // 1000 * 0.001 = 1.0 -> 1.0 / 0.002 = 500
        assert_eq!(converted.vertices[0], [500, 1000, 1500]);
    }

    #[test]
    fn test_convert_feature_transform_with_translate() {
        let source = make_transform([0.001, 0.001, 0.001], [100.0, 200.0, 0.0]);
        let target = make_transform([0.001, 0.001, 0.001], [0.0, 0.0, 0.0]);

        let mut feature = CityJSONFeature::new();
        feature.vertices = vec![[0, 0, 0]]; // Real: (100, 200, 0)

        let converted = convert_feature_transform(feature, &source, &target);
        // Real (100, 200, 0) with target translate (0, 0, 0) and scale 0.001
        // -> (100 / 0.001, 200 / 0.001, 0) = (100000, 200000, 0)
        assert_eq!(converted.vertices[0], [100000, 200000, 0]);
    }
}
