use anyhow::Result;
use fcb_core::deserializer::to_cj_metadata;
use fcb_core::FcbReader;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;

fn read_file() -> Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file_path = manifest_dir.join("temp").join("test_output.fcb");
    let input_file = File::open(input_file_path)?;
    let inputreader = BufReader::new(input_file);

    let output_file = manifest_dir
        .join("temp")
        .join("test_output_header.city.jsonl");
    let output_file = File::create(output_file)?;
    let mut outputwriter = BufWriter::new(output_file);

    let mut reader = FcbReader::open(inputreader)?.select_all()?;
    let header = reader.header();
    let cj = to_cj_metadata(&header)?;
    let mut features = Vec::new();
    let feat_count = header.features_count();
    let mut feat_num = 0;
    while let Some(feat_buf) = reader.next()? {
        let feature = feat_buf.cur_cj_feature()?;
        features.push(feature);
        feat_num += 1;
        if feat_num >= feat_count {
            break;
        }
    }

    outputwriter.write_all(format!("{}\n", serde_json::to_string(&cj).unwrap()).as_bytes())?;

    for feature in &features {
        outputwriter
            .write_all(format!("{}\n", serde_json::to_string(feature).unwrap()).as_bytes())?;
    }

    Ok(())
}

fn main() {
    read_file().unwrap();
}
