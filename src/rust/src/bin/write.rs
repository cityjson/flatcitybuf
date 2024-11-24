use flatcitybuf::{read_cityjson_from_bufreader, FcbWriter};
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter};
use std::path::PathBuf;

fn write_file() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir
        .join("tests")
        .join("data")
        .join("small.city.jsonl");
    let output_file = manifest_dir.join("temp").join("test_output.fgb");
    println!("input file: {}", input_file.display());
    println!("output file: {}", output_file.display());
    let input_file = File::open(input_file)?;
    let inputreader = BufReader::new(input_file);
    let cjj = read_cityjson_from_bufreader(inputreader)?;

    let output_file = File::create(output_file)?;
    let outputwriter = BufWriter::new(output_file);
    let fcb = FcbWriter::create(cjj)?;
    fcb.write(outputwriter)?;

    Ok(())
}

fn main() {
    write_file().unwrap();
}
