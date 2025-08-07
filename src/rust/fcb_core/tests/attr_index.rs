use anyhow::Result;
use cjseq::CityJSONFeature;
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, FcbReader, FcbWriter, Operator,
};
use std::{
    fs::File,
    io::{BufReader, Cursor, Seek, SeekFrom},
    path::PathBuf,
};

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use fcb_core::{FixedStringKey, Float, KeyType};
    use pretty_assertions::assert_eq;

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
        let attr_indices = vec![
            ("b3_h_dak_50p".to_string(), None),
            ("identificatie".to_string(), None),
        ];
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

        let query: Vec<(String, Operator, KeyType)> = vec![
            (
                "b3_h_dak_50p".to_string(),
                Operator::Gt,
                KeyType::Float64(Float(2.0)),
            ),
            (
                "identificatie".to_string(),
                Operator::Eq,
                KeyType::StringKey50(FixedStringKey::from_str("NL.IMBAG.Pand.0503100000012869")),
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
        let attr_indices = vec![
            ("b3_h_dak_50p".to_string(), None),
            ("identificatie".to_string(), None),
        ];
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

        let query: Vec<(String, Operator, KeyType)> = vec![
            (
                "b3_h_dak_50p".to_string(),
                Operator::Gt,
                KeyType::Float64(Float(2.0)),
            ),
            (
                "identificatie".to_string(),
                Operator::Eq,
                KeyType::StringKey50(FixedStringKey::from_str("NL.IMBAG.Pand.0503100000012869")),
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

    #[test]
    fn test_attr_index_multiple_queries() -> Result<()> {
        // --- Prepare FCB data ---
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let input_file = manifest_dir
            .join("tests")
            .join("data")
            .join("small.city.jsonl");

        let input_file = File::open(input_file)?;
        let input_reader = BufReader::new(input_file);
        let original_cj_seq = match read_cityjson_from_reader(input_reader, CJTypeKind::Seq)? {
            CJType::Seq(seq) => seq,
            _ => panic!("Expected CityJSONSeq"),
        };

        // Build attribute schema from features.
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
            ("tijdstipregistratie".to_string(), None),
        ];
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
        let mut memory_buffer = Cursor::new(Vec::new());
        fcb.write(&mut memory_buffer)?;
        // Clone the underlying byte vector to re-create a fresh Cursor for every test case.
        let fcb_data = memory_buffer.get_ref().clone();

        // --- Helper: Run a query test ---
        fn run_query_test(
            data: &[u8],
            query: &Vec<(String, Operator, KeyType)>,
        ) -> Result<Vec<CityJSONFeature>> {
            // Create a new Cursor from the data.
            let mut cursor = Cursor::new(data.to_vec());
            cursor.seek(SeekFrom::Start(0))?;
            let mut reader = FcbReader::open(cursor)?.select_attr_query(query.clone())?;
            let feat_count = reader.header().features_count();
            let mut features = Vec::new();
            let mut feat_num = 0;
            while let Ok(Some(feat_buf)) = reader.next() {
                let feature = feat_buf.cur_cj_feature()?;
                features.push(feature);
                feat_num += 1;
                if feat_num >= feat_count {
                    break;
                }
            }
            Ok(features)
        }

        // --- Define Test Cases ---
        #[derive(Debug)]
        struct QueryTestCase {
            test_name: &'static str,
            query: Vec<(String, Operator, KeyType)>,
            expected_count: usize,
            /// A validator function that returns true if the feature satisfies expected conditions.
            validator: fn(&CityJSONFeature) -> bool,
        }

        let test_cases = vec![
            // Test case: Expect one matching feature with b3_h_dak_50p > 2.0 and matching identificatie.
            QueryTestCase {
                test_name: "test_attr_index_multiple_queries: b3_h_dak_50p > 2.0 and identificatie == NL.IMBAG.Pand.0503100000012869",
                query: vec![
                    (
                        "b3_h_dak_50p".to_string(),
                        Operator::Gt,
                        KeyType::Float64(Float(2.0)),
                    ),
                    (
                        "identificatie".to_string(),
                        Operator::Eq,
                        KeyType::StringKey50(FixedStringKey::from_str(
                            "NL.IMBAG.Pand.0503100000012869",
                        )),
                    ),
                ],
                expected_count: 1,
                validator: |feature: &CityJSONFeature| {
                    let mut valid_b3 = false;
                    let mut valid_ident = false;
                    for co in feature.city_objects.values() {
                        if let Some(attrs) = &co.attributes {
                            if let Some(val) = attrs.get("b3_h_dak_50p") {
                                if val.as_f64().unwrap() > 2.0 {
                                    valid_b3 = true;
                                }
                            }
                            if let Some(ident) = attrs.get("identificatie") {
                                if ident.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
                                    valid_ident = true;
                                }
                            }
                        }
                    }
                    valid_b3 && valid_ident
                },
            },
            // Test case: Expect zero features where tijdstipregistratie is before 2008-01-01.
            QueryTestCase {
                test_name: "test_attr_index_multiple_queries: tijdstipregistratie < 2008-01-01",
                query: vec![(
                    "tijdstipregistratie".to_string(),
                    Operator::Lt,
                    KeyType::DateTime(chrono::DateTime::<chrono::Utc>::from_str(
                        "2008-01-01T00:00:00Z",
                    )?),
                )],
                expected_count: 0,
                validator: |feature: &CityJSONFeature| {
                    let mut valid_tijdstip = true;
                    let query_tijdstip = chrono::NaiveDate::from_ymd(2008, 1, 1).and_hms(0, 0, 0);
                    for co in feature.city_objects.values() {
                        if let Some(attrs) = &co.attributes {
                            if let Some(val) = attrs.get("tijdstipregistratie") {
                                let val_tijdstip = chrono::NaiveDateTime::parse_from_str(
                                    val.as_str().unwrap(),
                                    "%Y-%m-%dT%H:%M:%S",
                                )
                                .unwrap();
                                if val_tijdstip < query_tijdstip {
                                    valid_tijdstip = false;
                                }
                            }
                        }
                    }
                    valid_tijdstip
                },
            },
            // // Test case: Expect zero features where tijdstipregistratie is after 2008-01-01.
            QueryTestCase {
                test_name: "test_attr_index_multiple_queries: tijdstipregistratie > 2008-01-01",
                query: vec![(
                    "tijdstipregistratie".to_string(),
                    Operator::Gt,
                    KeyType::DateTime(chrono::DateTime::<chrono::Utc>::from_utc(
                        chrono::NaiveDate::from_ymd(2008, 1, 1).and_hms(0, 0, 0),
                        chrono::Utc,
                    )),
                )],
                expected_count: 3,
                validator: |feature: &CityJSONFeature| {
                    let mut valid_tijdstip = false;
                    let query_tijdstip = chrono::NaiveDate::from_ymd(2008, 1, 1).and_hms(0, 0, 0);
                    for co in feature.city_objects.values() {
                        if let Some(attrs) = &co.attributes {
                            if let Some(val) = attrs.get("tijdstipregistratie") {
                                let val_tijdstip =
                                    chrono::DateTime::parse_from_rfc3339(val.as_str().unwrap())
                                        .map_err(|e| eprintln!("Failed to parse datetime: {e}"))
                                        .map(|dt| dt.naive_utc())
                                        .unwrap_or_else(|_| {
                                            chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap()
                                        });
                                if val_tijdstip > query_tijdstip {
                                    valid_tijdstip = true;
                                }
                            }
                        }
                    }
                    valid_tijdstip
                },
            },
        ];

        // --- Run Test Cases ---
        for test_case in test_cases.into_iter() {
            let features = run_query_test(&fcb_data, &test_case.query)?;
            println!("running test: {}", test_case.test_name);
            assert_eq!(
                features.len(),
                test_case.expected_count,
                "Unexpected feature count for query: {:?}",
                test_case.query
            );
            for feature in features {
                assert!(
                    (test_case.validator)(&feature),
                    "Validator failed for feature: {feature:?}"
                );
            }
        }
        Ok(())
    }
}
