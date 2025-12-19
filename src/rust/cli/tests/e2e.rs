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
        assert_eq!(result.features.len(), 3);
        assert!(result.metadata.transform.scale.len() >= 3);
    }

    #[test]
    fn test_merge_multiple_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file1 = copy_test_file(&temp_dir, "small.city.jsonl");
        let file2 = copy_test_file(&temp_dir, "noise_extension.city.jsonl");

        let result = merge_files(vec![file1, file2]).expect("Merge failed");

        // small has 3 features, noise_extension has 3 features = 6 total
        assert_eq!(result.features.len(), 6);
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
            for feature in merge_result.features.iter().take(100) {
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
            feature_count: merge_result.features.len() as u64,
            index_node_size: 16,
            attribute_indices: None,
            geographical_extent: None,
        };

        let mut fcb = FcbWriter::new(
            merge_result.metadata,
            Some(header_options),
            attr_schema,
            None, // semantic schema
        )
        .expect("Failed to create FCB writer");

        // Add features
        for feature in merge_result.features.iter() {
            fcb.add_feature(feature).expect("Failed to add feature");
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
        assert_eq!(merge_result.features.len(), 6); // 3 + 3 = 6

        // Create and write FCB
        let header_options = HeaderWriterOptions {
            write_index: true,
            feature_count: merge_result.features.len() as u64,
            index_node_size: 16,
            attribute_indices: None,
            geographical_extent: None,
        };

        let mut fcb = FcbWriter::new(
            merge_result.metadata,
            Some(header_options),
            None, // attr schema
            None, // semantic schema
        )
        .expect("Failed to create FCB writer");

        for feature in merge_result.features.iter() {
            fcb.add_feature(feature).expect("Failed to add feature");
        }

        let output_file = File::create(&output_path).expect("Failed to create output file");
        let writer = BufWriter::new(output_file);
        fcb.write(writer).expect("Failed to write FCB");

        // Verify output has 6 features
        let feature_count = get_fcb_feature_count(&output_path);
        assert_eq!(feature_count, 6);
    }
}
