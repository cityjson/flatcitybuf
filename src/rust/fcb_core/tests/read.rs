use anyhow::Result;
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, FcbReader, FcbWriter,
};
use std::{
    fs::File,
    io::{BufReader, Cursor, Seek},
    path::PathBuf,
};

#[test]
fn read_bbox() -> Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir.join("tests/data/delft.city.jsonl");
    let input_file = File::open(input_file)?;
    let input_reader = BufReader::new(input_file);
    let original_cj_seq = match read_cityjson_from_reader(input_reader, CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => panic!("Expected CityJSONSeq"),
    };

    let mut attr_schema = AttributeSchema::new();
    for feature in original_cj_seq.features.iter() {
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                attr_schema.add_attributes(attributes);
            }
        }
    }
    let attr_indices = vec!["b3_h_dak_50p".to_string(), "identificatie".to_string()];

    let mut memory_buffer = Cursor::new(Vec::new());
    let mut fcb = FcbWriter::new(
        original_cj_seq.cj.clone(),
        Some(HeaderWriterOptions {
            write_index: true,
            feature_count: original_cj_seq.features.len() as u64,
            index_node_size: 16,
            attribute_indices: Some(attr_indices),
        }),
        Some(attr_schema),
    )?;

    for feature in original_cj_seq.features.iter() {
        fcb.add_feature(feature)?;
    }

    fcb.write(&mut memory_buffer)?;

    memory_buffer.seek(std::io::SeekFrom::Start(0))?;

    let minx = 84227.77;
    let miny = 445377.33;
    let maxx = 85323.23;
    let maxy = 446334.69;

    let mut fcb = FcbReader::open(&mut memory_buffer)?.select_bbox(minx, miny, maxx, maxy)?;

    assert_ne!(fcb.features_count(), None);
    let mut features = Vec::new();
    let mut bbox_cnt = 0;
    while let Some(feature) = fcb.next()? {
        bbox_cnt += 1;
        let cj_feat = feature.cur_cj_feature()?;
        features.push(cj_feat);
    }

    println!("bbox_cnt: {}", bbox_cnt);
    println!(
        "fcb.header().features_count(): {}",
        fcb.header().features_count()
    );

    assert!(bbox_cnt < fcb.header().features_count());

    let mut count_to_check = 0;
    for feature in features {
        let x_s = feature.vertices.iter().map(|v| v[0]).collect::<Vec<_>>();
        let y_s = feature.vertices.iter().map(|v| v[1]).collect::<Vec<_>>();

        // MEMO: it retrieves all features which has intersection with the bbox
        if x_s.iter().any(|x| *x >= minx as i64)
            || y_s.iter().any(|y| *y >= miny as i64)
            || x_s.iter().any(|x| *x <= maxx as i64)
            || y_s.iter().any(|y| *y <= maxy as i64)
        {
            count_to_check += 1;
        }
    }
    assert_eq!(count_to_check, bbox_cnt);

    Ok(())
}
