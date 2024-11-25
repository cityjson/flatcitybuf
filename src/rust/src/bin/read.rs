use flatcitybuf::{size_prefixed_root_as_header, FcbReader, FcbWriter, VERSION};
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read};
use std::path::PathBuf;

fn read_file() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file_path = manifest_dir.join("temp").join("test_output.fgb");
    let input_file = File::open(input_file_path)?;
    let mut inputreader = BufReader::new(input_file);
    // let reader = FcbReader::open(inputreader)?;

    let mut magic_buf: [u8; 8] = [0; 8];
    inputreader.read_exact(&mut magic_buf)?;
    assert_eq!(magic_buf, [b'f', b'c', b'b', VERSION, b'f', b'c', b'b', 0]);

    let mut size_buf: [u8; 4] = [0; 4];
    inputreader.read_exact(&mut size_buf)?;
    let header_size = u32::from_le_bytes(size_buf) as usize;
    println!("header_size: {}", header_size);
    let mut header_buf = Vec::with_capacity(header_size + 4);
    header_buf.extend_from_slice(&size_buf);
    header_buf.resize(header_buf.capacity(), 0);
    inputreader.read_exact(&mut header_buf[4..])?;

    let header = size_prefixed_root_as_header(&header_buf).unwrap();
    let ref_system = header.reference_system();
    println!("ref_system: {:?}", ref_system);

    Ok(())
}

fn main() {
    read_file().unwrap();
}
