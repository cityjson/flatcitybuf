use anyhow::Result;
use fcb_core::{AttrQuery, FcbReader, Float, KeyType, Operator};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

/// Read FCB file and count geometry types using attribute index with non-seekable reader (optimized MultiIndex).
fn read_fcb_with_attr_index_non_seekable(path: PathBuf) -> Result<()> {
    let input_file = File::open(path)?;
    let input_reader = BufReader::new(input_file);

    let query: AttrQuery = vec![
        (
            "b3_h_dak_50p".to_string(),
            Operator::Gt,
            KeyType::Float64(Float(2.0)),
        ),
        (
            "b3_h_dak_50p".to_string(),
            Operator::Lt,
            KeyType::Float64(Float(50.0)),
        ),
    ];

    // Use the non-seekable version with optimized MultiIndex
    let mut reader = FcbReader::open(input_reader)?.select_attr_query_seq(query)?;
    let header = reader.header();
    let feat_count = header.features_count();

    let mut feat_total = 0;
    while let Some(feat_buf) = reader.next()? {
        let feature = feat_buf.cur_cj_feature()?;
        feat_total += 1;
        if feat_total == 10 {
            break;
        }
        if feat_total == feat_count {
            break;
        }
    }
    println!("process finished");
    println!("feat_total: {feat_total}");

    Ok(())
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file_path = manifest_dir.join("benchmark_data/attribute/3dbag_partial.fcb");
    read_fcb_with_attr_index_non_seekable(input_file_path).unwrap();
}
