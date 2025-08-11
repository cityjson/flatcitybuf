use anyhow::Result;
use fcb_core::packed_rtree::Query;
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
    let attr_indices = vec![
        ("b3_h_dak_50p".to_string(), None),
        ("identificatie".to_string(), None),
    ];

    let mut memory_buffer = Cursor::new(Vec::new());
    let mut fcb = FcbWriter::new(
        original_cj_seq.cj.clone(),
        Some(HeaderWriterOptions {
            write_index: true,
            feature_count: original_cj_seq.features.len() as u64,
            index_node_size: 16,
            attribute_indices: Some(attr_indices),
            geographical_extent: None,
        }),
        Some(attr_schema),
        None,
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

    let mut fcb = FcbReader::open(&mut memory_buffer)?.select_query(
        Query::BBox(minx, miny, maxx, maxy),
        None,
        None,
    )?;

    assert_ne!(fcb.features_count(), None);
    let mut features = Vec::new();
    let mut bbox_cnt = 0;
    while let Some(feature) = fcb.next()? {
        bbox_cnt += 1;
        let cj_feat = feature.cur_cj_feature()?;
        features.push(cj_feat);
    }

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

#[test]
fn read_bbox_nonseekable() -> anyhow::Result<()> {
    use std::fs::File;
    use std::io::{BufReader, Cursor, Read};
    use std::path::PathBuf;

    // A small wrapper around a Cursor that only implements Read, not Seek.
    // This simulates a non‑seekable stream.
    struct NonSeekableCursor<T: AsRef<[u8]>> {
        inner: Cursor<T>,
    }

    impl<T: AsRef<[u8]>> Read for NonSeekableCursor<T> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.inner.read(buf)
        }
    }
    // note: we intentionally do not implement Seek on NonSeekableCursor

    // read the original CityJSON data from file
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir.join("tests/data/delft.city.jsonl");
    let file = File::open(input_file)?;
    let reader = BufReader::new(file);
    let original_cj_seq = match read_cityjson_from_reader(reader, CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => panic!("expected cityjsonseq"),
    };

    // collect attributes from city objects to build attribute schema
    let mut attr_schema = AttributeSchema::new();
    for feature in original_cj_seq.features.iter() {
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                attr_schema.add_attributes(attributes);
            }
        }
    }
    let attr_indices = vec![
        ("b3_h_dak_50p".to_string(), None),
        ("identificatie".to_string(), None),
    ];

    // write out the FCB data into a memory buffer
    let mut memory_buffer = Cursor::new(Vec::new());
    let mut fcb = FcbWriter::new(
        original_cj_seq.cj.clone(),
        Some(HeaderWriterOptions {
            write_index: true,
            feature_count: original_cj_seq.features.len() as u64,
            index_node_size: 16,
            attribute_indices: Some(attr_indices),
            geographical_extent: None,
        }),
        Some(attr_schema),
        None,
    )?;

    for feature in original_cj_seq.features.iter() {
        fcb.add_feature(feature)?;
    }
    fcb.write(&mut memory_buffer)?;

    // instead of seeking back to the start, simulate a non‑seekable stream by wrapping the buffer's data
    let data_vec = memory_buffer.into_inner();
    let nonseekable_reader = NonSeekableCursor {
        inner: Cursor::new(data_vec),
    };

    let minx = 84227.77;
    let miny = 445377.33;
    let maxx = 85323.23;
    let maxy = 446334.69;

    // open non‑seekable fcb reader and select features within the provided bbox.
    let mut fcb = FcbReader::open(nonseekable_reader)?
        .select_query_seq(Query::BBox(minx, miny, maxx, maxy))?;

    assert_ne!(fcb.features_count(), None);
    let mut bbox_cnt = 0;
    let mut features = Vec::new();
    while let Some(feature) = fcb.next()? {
        bbox_cnt += 1;
        let cj_feat = feature.cur_cj_feature()?;
        features.push(cj_feat);
    }

    assert!(bbox_cnt < fcb.header().features_count());

    // additional check: make sure that the features intersect the bbox
    let mut count_to_check = 0;
    for feature in features {
        let x_coords = feature.vertices.iter().map(|v| v[0]).collect::<Vec<_>>();
        let y_coords = feature.vertices.iter().map(|v| v[1]).collect::<Vec<_>>();
        // note: this retrieves all features that intersect the bbox boundary
        if x_coords.iter().any(|x| *x >= minx as i64)
            || y_coords.iter().any(|y| *y >= miny as i64)
            || x_coords.iter().any(|x| *x <= maxx as i64)
            || y_coords.iter().any(|y| *y <= maxy as i64)
        {
            count_to_check += 1;
        }
    }
    assert_eq!(count_to_check, bbox_cnt);

    Ok(())
}
