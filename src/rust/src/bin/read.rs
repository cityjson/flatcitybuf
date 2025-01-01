use flatcitybuf::fcb_deserializer::{to_cj_feature, to_cj_metadata};
use flatcitybuf::FcbReader;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

fn read_file() -> Result<(), Box<dyn Error>> {
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
    println!("cj: {:?}", cj);

    let mut features = Vec::new();
    while let Ok(Some(feat_buf)) = reader.next() {
        let feature = feat_buf.cur_feature();
        features.push(to_cj_feature(feature)?);
    }

    println!("features: {:?}", features);

    serde_json::to_writer(&mut outputwriter, &cj)?;
    for feature in features {
        if let Err(e) = serde_json::to_writer(&mut outputwriter, &feature) {
            println!("error: {}", e);
        }
    }

    Ok(())
}

fn main() {
    read_file().unwrap();
}
