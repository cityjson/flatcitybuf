use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, CityJSONSeq, FcbWriter,
};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

fn write_file() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir
        .join("tests")
        .join("data")
        .join("small.city.jsonl");
    let output_file = manifest_dir.join("temp").join("test_output.fcb");
    let input_file = File::open(input_file)?;
    let inputreader = BufReader::new(input_file);
    let cj_seq = read_cityjson_from_reader(inputreader, CJTypeKind::Seq)?;
    if let CJType::Seq(cj_seq) = cj_seq {
        let CityJSONSeq { cj, features } = cj_seq;

        let output_file = File::create(output_file)?;
        let outputwriter = BufWriter::new(output_file);

        let attr_indices = vec![
            ("b3_h_dak_50p".to_string(), None),
            ("identificatie".to_string(), None),
        ];
        let header_options = Some(HeaderWriterOptions {
            write_index: false,
            feature_count: features.len() as u64,
            index_node_size: 16,
            attribute_indices: Some(attr_indices),
            geographical_extent: None,
        });
        let mut attr_schema = AttributeSchema::new();
        for feature in features.iter() {
            for (_, co) in feature.city_objects.iter() {
                if let Some(attributes) = &co.attributes {
                    attr_schema.add_attributes(attributes);
                }
            }
        }
        let mut fcb = FcbWriter::new(cj, header_options, Some(attr_schema), None)?;
        for feature in features.iter() {
            fcb.add_feature(feature)?;
        }
        fcb.write(outputwriter)?;
    }

    Ok(())
}

fn main() {
    write_file().unwrap();
}
