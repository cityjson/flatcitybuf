//! End-to-end tests for the FCB CLI multi-file support
//!
//! These tests verify the merger and serialization functionality
//! by copying test files to temp directories and running full pipelines.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;
use tempfile::TempDir;

use fcb_cli::merger::merge_files;
use fcb_cli::reader::{read_input_file, InputFormat};
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    FcbReader, FcbWriter,
};
use std::io::BufWriter;

/// Get the path to the fcb_core test data directory
fn get_test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("fcb_core/tests/data")
}

/// Copy a test file to a temp directory
fn copy_test_file(temp_dir: &TempDir, filename: &str) -> PathBuf {
    let src = get_test_data_dir().join(filename);
    let dest = temp_dir.path().join(filename);
    fs::copy(&src, &dest).expect("Failed to copy test file");
    dest
}

/// Helper to read FCB file and get feature count
fn get_fcb_feature_count(path: &PathBuf) -> u64 {
    let file = File::open(path).expect("Failed to open FCB file");
    let reader = BufReader::new(file);
    let fcb_reader = FcbReader::open(reader)
        .expect("Failed to read FCB header")
        .select_all()
        .expect("Failed to select all");
    fcb_reader.header().features_count()
}

mod merger_tests {
    use super::*;

    #[test]
    fn test_merge_single_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = copy_test_file(&temp_dir, "small.city.jsonl");

        let result = merge_files(vec![file_path]).expect("Merge failed");

        // small.city.jsonl has 3 features
        assert_eq!(result.len(), 3);
        assert!(result.metadata.transform.scale.len() >= 3);
    }

    #[test]
    fn test_merge_multiple_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file1 = copy_test_file(&temp_dir, "small.city.jsonl");
        let file2 = copy_test_file(&temp_dir, "noise_extension.city.jsonl");

        let result = merge_files(vec![file1, file2]).expect("Merge failed");

        // small has 3 features, noise_extension has 3 features = 6 total
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_merge_empty_path_list() {
        let result = merge_files(vec![]);
        assert!(result.is_err());
    }
}

mod reader_tests {
    use super::*;

    #[test]
    fn test_read_cityjsonseq_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = copy_test_file(&temp_dir, "small.city.jsonl");

        let data = read_input_file(&file_path).expect("Failed to read file");

        assert_eq!(data.features.len(), 3);
        assert!(!data.metadata.transform.scale.is_empty());
    }

    #[test]
    fn test_format_detection() {
        let jsonl_path = PathBuf::from("test.city.jsonl");
        let json_path = PathBuf::from("test.city.json");

        assert_eq!(
            InputFormat::from_path(&jsonl_path).unwrap(),
            InputFormat::CityJSONSeq
        );
        assert_eq!(
            InputFormat::from_path(&json_path).unwrap(),
            InputFormat::CityJSON
        );
    }
}

mod serialization_tests {
    use super::*;

    #[test]
    fn test_full_serialization_pipeline() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Copy test files
        let file1 = copy_test_file(&temp_dir, "small.city.jsonl");
        let output_path = temp_dir.path().join("output.fcb");

        // Merge files
        let merge_result = merge_files(vec![file1]).expect("Merge failed");

        // Build schema
        let attr_schema = {
            let mut schema = AttributeSchema::new();
            for feature in merge_result.features().take(100) {
                let feature = feature.expect("Failed to read feature");
                for (_, co) in feature.city_objects.iter() {
                    if let Some(attributes) = &co.attributes {
                        schema.add_attributes(attributes);
                    }
                }
            }
            if schema.is_empty() {
                None
            } else {
                Some(schema)
            }
        };

        // Create FCB writer
        let header_options = HeaderWriterOptions {
            write_index: true,
            feature_count: merge_result.len() as u64,
            index_node_size: 16,
            attribute_indices: None,
            geographical_extent: None,
            id_index: None,
        };

        let mut fcb = FcbWriter::new(
            merge_result.metadata.clone(),
            Some(header_options),
            attr_schema,
            None, // semantic schema
        )
        .expect("Failed to create FCB writer");

        // Add features
        for feature in merge_result.features() {
            let feature = feature.expect("Failed to read feature");
            fcb.add_feature(&feature).expect("Failed to add feature");
        }

        // Write to file
        let output_file = File::create(&output_path).expect("Failed to create output file");
        let writer = BufWriter::new(output_file);
        fcb.write(writer).expect("Failed to write FCB");

        // Verify output
        let feature_count = get_fcb_feature_count(&output_path);
        assert_eq!(feature_count, 3);
    }

    #[test]
    fn test_multi_file_serialization() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Copy test files to subdirectories to test merging
        let subdir1 = temp_dir.path().join("dir1");
        let subdir2 = temp_dir.path().join("dir2");
        fs::create_dir(&subdir1).expect("Failed to create subdir1");
        fs::create_dir(&subdir2).expect("Failed to create subdir2");

        let src = get_test_data_dir().join("small.city.jsonl");
        let file1 = subdir1.join("small.city.jsonl");
        let file2 = subdir2.join("small.city.jsonl");
        fs::copy(&src, &file1).expect("Failed to copy to subdir1");
        fs::copy(&src, &file2).expect("Failed to copy to subdir2");

        let output_path = temp_dir.path().join("merged.fcb");

        // Merge both files
        let merge_result = merge_files(vec![file1, file2]).expect("Merge failed");

        // Verify we have double the features
        assert_eq!(merge_result.len(), 6); // 3 + 3 = 6

        // Create and write FCB
        let header_options = HeaderWriterOptions {
            write_index: true,
            feature_count: merge_result.len() as u64,
            index_node_size: 16,
            attribute_indices: None,
            geographical_extent: None,
            id_index: None,
        };

        let mut fcb = FcbWriter::new(
            merge_result.metadata.clone(),
            Some(header_options),
            None, // attr schema
            None, // semantic schema
        )
        .expect("Failed to create FCB writer");

        for feature in merge_result.features() {
            let feature = feature.expect("Failed to read feature");
            fcb.add_feature(&feature).expect("Failed to add feature");
        }

        let output_file = File::create(&output_path).expect("Failed to create output file");
        let writer = BufWriter::new(output_file);
        fcb.write(writer).expect("Failed to write FCB");

        // Verify output has 6 features
        let feature_count = get_fcb_feature_count(&output_path);
        assert_eq!(feature_count, 6);
    }
}

/// `fcb ser` reads a CityJSONSeq input once per pass instead of holding it in
/// memory. These tests hold it to what reading the input into memory gives.
mod streaming_tests {
    use super::*;
    use std::io::{BufRead, Write};
    use std::process::Command;

    /// What `fcb ser -A` writes for `features`, driven through the library
    /// from memory with the settings the CLI uses.
    fn write_from_memory(
        metadata: cjseq::CityJSON,
        features: &[cjseq::CityJSONFeature],
    ) -> Vec<u8> {
        let mut attr_schema = AttributeSchema::new();
        let mut semantic_attr_schema = AttributeSchema::new();
        for (n, feature) in features.iter().enumerate() {
            let mut ids: Vec<&String> = feature.city_objects.keys().collect();
            ids.sort_unstable();
            for co in ids.into_iter().map(|id| &feature.city_objects[id]) {
                if n < 1000 {
                    if let Some(attributes) = &co.attributes {
                        attr_schema.add_attributes(attributes);
                    }
                }
                for geom in co.geometry.iter().flatten() {
                    for surface in geom.surfaces().unwrap_or_default() {
                        semantic_attr_schema.add_attributes(&surface.other);
                    }
                }
            }
        }
        let non_empty = |schema: AttributeSchema| (!schema.is_empty()).then_some(schema);
        let header_options = HeaderWriterOptions {
            write_index: true,
            feature_count: features.len() as u64,
            index_node_size: 16,
            attribute_indices: Some(
                attr_schema
                    .keys()
                    .map(|name| (name.clone(), Some(256)))
                    .collect(),
            ),
            geographical_extent: None,
            id_index: (!attr_schema.contains_key("id")).then_some(256),
        };
        let mut fcb = FcbWriter::new(
            metadata,
            Some(header_options),
            non_empty(attr_schema),
            non_empty(semantic_attr_schema),
        )
        .expect("Failed to create FCB writer");
        for feature in features {
            fcb.add_feature(feature).expect("Failed to add feature");
        }
        let mut out = Vec::new();
        fcb.write(&mut out).expect("Failed to write FCB");
        out
    }

    fn fcb_ser(args: &[&std::path::Path]) {
        let output = Command::new(env!("CARGO_BIN_EXE_fcb"))
            .arg("ser")
            .arg("-A")
            .args(args)
            .output()
            .expect("run fcb");
        assert!(
            output.status.success(),
            "fcb ser failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn test_ser_streams_the_bytes_memory_writes() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let input = copy_test_file(&temp_dir, "delft.city.jsonl");
        let output = temp_dir.path().join("delft.fcb");

        fcb_ser(&[&input, &output]);

        let data = read_input_file(&input).expect("Failed to read file");
        let expected = write_from_memory(data.metadata, &data.features);
        assert!(
            fs::read(&output).unwrap() == expected,
            "fcb ser wrote different bytes from the in-memory writer"
        );
    }

    /// Two inputs on different transforms: the second one's vertices are
    /// moved onto the first one's as each feature is read back.
    #[test]
    fn test_ser_aligns_a_second_transform_while_streaming() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let first = copy_test_file(&temp_dir, "delft.city.jsonl");
        let shifted = temp_dir.path().join("shifted.city.jsonl");
        let output = temp_dir.path().join("merged.fcb");

        // The same features, on a transform translated by +1 m in x.
        let lines: Vec<String> = BufReader::new(File::open(&first).unwrap())
            .lines()
            .map(Result::unwrap)
            .collect();
        let mut header: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        let scale_x = header["transform"]["scale"][0].as_f64().unwrap();
        let translate_x = header["transform"]["translate"][0].as_f64().unwrap();
        header["transform"]["translate"][0] = serde_json::json!(translate_x + 1.0);
        let step = (1.0 / scale_x).round() as i64;
        let mut file = File::create(&shifted).unwrap();
        writeln!(file, "{header}").unwrap();
        for line in &lines[1..] {
            let mut feature: serde_json::Value = serde_json::from_str(line).unwrap();
            for v in feature["vertices"].as_array_mut().unwrap() {
                v[0] = serde_json::json!(v[0].as_i64().unwrap() - step);
            }
            writeln!(file, "{feature}").unwrap();
        }
        drop(file);

        fcb_ser(&[&first, &shifted, &output]);

        let data = read_input_file(&first).expect("Failed to read file");
        let features: Vec<_> = data
            .features
            .iter()
            .chain(data.features.iter())
            .cloned()
            .collect();
        let expected = write_from_memory(data.metadata, &features);
        assert!(
            fs::read(&output).unwrap() == expected,
            "fcb ser wrote different bytes from the in-memory writer"
        );
    }

    /// Blank lines are not features, in the count or in the read.
    #[test]
    fn test_merge_counts_what_it_reads() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let input = copy_test_file(&temp_dir, "small.city.jsonl");
        let blank = temp_dir.path().join("blank.city.jsonl");
        let text = fs::read_to_string(&input).unwrap();
        fs::write(&blank, text.replace('\n', "\n\n  \n")).unwrap();

        let merged = merge_files(vec![blank]).expect("Merge failed");
        assert_eq!(merged.len(), 3);
        // Every pass reads the whole input again.
        for _ in 0..2 {
            let ids: Vec<String> = merged
                .features()
                .map(|f| f.expect("Failed to read feature").id.clone())
                .collect();
            assert_eq!(ids.len(), 3);
        }
    }
}
