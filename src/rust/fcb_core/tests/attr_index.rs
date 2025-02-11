use anyhow::Result;

use bst::{ByteSerializableValue, Operator};
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, FcbReader, FcbWriter,
};
use ordered_float::OrderedFloat;
use pretty_assertions::assert_eq;
use std::{
    fs::File,
    io::{BufReader, Cursor, Seek},
    path::PathBuf,
};

#[test]
fn test_attr_index() -> Result<()> {
    // Setup paths
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir
        .join("tests")
        .join("data")
        .join("small.city.jsonl");

    // Read original CityJSONSeq
    let input_file = File::open(input_file)?;
    let input_reader = BufReader::new(input_file);
    let original_cj_seq = match read_cityjson_from_reader(input_reader, CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => panic!("Expected CityJSONSeq"),
    };

    // Write to FCB

    let mut memory_buffer = Cursor::new(Vec::new());

    let mut attr_schema = AttributeSchema::new();
    for feature in original_cj_seq.features.iter() {
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                attr_schema.add_attributes(attributes);
            }
        }
    }
    let attr_indices = vec!["b3_h_dak_50p".to_string(), "identificatie".to_string()];
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

    let query: Vec<(String, Operator, ByteSerializableValue)> = vec![
        (
            "b3_h_dak_50p".to_string(),
            Operator::Gt,
            ByteSerializableValue::F64(OrderedFloat(2.0)),
        ),
        (
            "identificatie".to_string(),
            Operator::Eq,
            ByteSerializableValue::String("NL.IMBAG.Pand.0503100000012869".to_string()),
        ),
    ];
    memory_buffer.seek(std::io::SeekFrom::Start(0))?;

    let mut reader = FcbReader::open(memory_buffer)?.select_attr_query(query)?;

    let header = reader.header();
    let mut deserialized_features = Vec::new();
    let feat_count = header.features_count();
    let mut feat_num = 0;
    while let Ok(Some(feat_buf)) = reader.next() {
        let feature = feat_buf.cur_cj_feature()?;
        deserialized_features.push(feature);
        feat_num += 1;
        if feat_num >= feat_count {
            break;
        }
    }
    assert_eq!(deserialized_features.len(), 1);
    let feature = deserialized_features.first().unwrap();
    let mut contains_b3_h_dak_50p = false;
    let mut contains_identificatie = false;
    for co in feature.city_objects.values() {
        if co.attributes.is_some() {
            let attrs = co.attributes.as_ref().unwrap();
            if let Some(b3_h_dak_50p) = attrs.get("b3_h_dak_50p") {
                if b3_h_dak_50p.as_f64().unwrap() > 2.0 {
                    contains_b3_h_dak_50p = true;
                }
            }
            if let Some(identificatie) = attrs.get("identificatie") {
                if identificatie.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
                    contains_identificatie = true;
                }
            }
        }
    }
    assert!(contains_b3_h_dak_50p);
    assert!(contains_identificatie);

    Ok(())
}

#[test]
fn test_attr_index_seq() -> Result<()> {
    // Setup paths
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir
        .join("tests")
        .join("data")
        .join("small.city.jsonl");

    // Read original CityJSONSeq
    let input_file = File::open(input_file)?;
    let input_reader = BufReader::new(input_file);
    let original_cj_seq = match read_cityjson_from_reader(input_reader, CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => panic!("Expected CityJSONSeq"),
    };

    // Write to FCB

    let mut memory_buffer = Cursor::new(Vec::new());

    let mut attr_schema = AttributeSchema::new();
    for feature in original_cj_seq.features.iter() {
        for (_, co) in feature.city_objects.iter() {
            if let Some(attributes) = &co.attributes {
                attr_schema.add_attributes(attributes);
            }
        }
    }
    let attr_indices = vec!["b3_h_dak_50p".to_string(), "identificatie".to_string()];
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

    let query: Vec<(String, Operator, ByteSerializableValue)> = vec![
        (
            "b3_h_dak_50p".to_string(),
            Operator::Gt,
            ByteSerializableValue::F64(OrderedFloat(2.0)),
        ),
        (
            "identificatie".to_string(),
            Operator::Eq,
            ByteSerializableValue::String("NL.IMBAG.Pand.0503100000012869".to_string()),
        ),
    ];
    memory_buffer.seek(std::io::SeekFrom::Start(0))?;
    let mut reader = FcbReader::open(memory_buffer)?.select_attr_query_seq(query)?;

    let header = reader.header();
    let mut deserialized_features = Vec::new();
    let feat_count = header.features_count();
    let mut feat_num = 0;
    while let Ok(Some(feat_buf)) = reader.next() {
        let feature = feat_buf.cur_cj_feature()?;
        deserialized_features.push(feature);
        feat_num += 1;
        if feat_num >= feat_count {
            break;
        }
    }
    assert_eq!(deserialized_features.len(), 1);
    let feature = deserialized_features.first().unwrap();
    let mut contains_b3_h_dak_50p = false;
    let mut contains_identificatie = false;
    for co in feature.city_objects.values() {
        if co.attributes.is_some() {
            let attrs = co.attributes.as_ref().unwrap();
            if let Some(b3_h_dak_50p) = attrs.get("b3_h_dak_50p") {
                if b3_h_dak_50p.as_f64().unwrap() > 2.0 {
                    contains_b3_h_dak_50p = true;
                }
            }
            if let Some(identificatie) = attrs.get("identificatie") {
                if identificatie.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
                    contains_identificatie = true;
                }
            }
        }
    }
    assert!(contains_b3_h_dak_50p);
    assert!(contains_identificatie);

    Ok(())
}
