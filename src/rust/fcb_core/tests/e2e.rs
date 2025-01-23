use anyhow::Result;
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    deserializer,
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, FcbReader, FcbWriter,
};
use pretty_assertions::assert_eq;
use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::PathBuf,
};
use tempfile::NamedTempFile;

#[test]
fn test_cityjson_serialization_cycle() -> Result<()> {
    // Setup paths
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir
        .join("tests")
        .join("data")
        .join("small.city.jsonl");

    let temp_fcb = NamedTempFile::new()?;

    // Read original CityJSONSeq
    let input_file = File::open(input_file)?;
    let input_reader = BufReader::new(input_file);
    let original_cj_seq = match read_cityjson_from_reader(input_reader, CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => panic!("Expected CityJSONSeq"),
    };

    // Write to FCB
    {
        let output_file = File::create(&temp_fcb)?;
        let output_writer = BufWriter::new(output_file);

        let mut attr_schema = AttributeSchema::new();
        for feature in original_cj_seq.features.iter() {
            for (_, co) in feature.city_objects.iter() {
                if let Some(attributes) = &co.attributes {
                    attr_schema.add_attributes(attributes);
                }
            }
        }
        let mut fcb = FcbWriter::new(
            original_cj_seq.cj.clone(),
            Some(HeaderWriterOptions {
                write_index: false,
                feature_count: original_cj_seq.features.len() as u64,
                index_node_size: 16,
            }),
            Some(attr_schema),
        )?;
        for feature in original_cj_seq.features.iter() {
            fcb.add_feature(feature)?;
        }
        fcb.write(output_writer)?;
    }

    // Read back from FCB
    let fcb_file = File::open(&temp_fcb)?;
    let fcb_reader = BufReader::new(fcb_file);
    let mut reader = FcbReader::open(fcb_reader)?.select_all()?;

    // Get header and convert to CityJSON
    let header = reader.header();
    let deserialized_cj = deserializer::to_cj_metadata(&header)?;
    // Read all features
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

    // Compare CityJSON metadata
    assert_eq!(original_cj_seq.cj.version, deserialized_cj.version);
    assert_eq!(original_cj_seq.cj.thetype, deserialized_cj.thetype);

    if let (Some(orig_meta), Some(des_meta)) =
        (&original_cj_seq.cj.metadata, &deserialized_cj.metadata)
    {
        assert_eq!(orig_meta, des_meta)
    }

    // Compare features
    assert_eq!(original_cj_seq.features.len(), deserialized_features.len());
    for (orig_feat, des_feat) in original_cj_seq
        .features
        .iter()
        .zip(deserialized_features.iter())
    {
        // assert_eq!(orig_feat, des_feat);
        assert_eq!(orig_feat.thetype, des_feat.thetype);
        assert_eq!(orig_feat.id, des_feat.id);
        assert_eq!(orig_feat.city_objects.len(), des_feat.city_objects.len());
        assert_eq!(orig_feat.vertices.len(), des_feat.vertices.len());
        // Compare vertices
        for (orig_vert, des_vert) in orig_feat.vertices.iter().zip(des_feat.vertices.iter()) {
            assert_eq!(orig_vert, des_vert);
        }

        // Compare city objects
        assert_eq!(orig_feat.city_objects.len(), des_feat.city_objects.len());
        for (id, orig_co) in orig_feat.city_objects.iter() {
            // ===============remove these lines later=================
            println!(
                "is CityObject same? {:?}",
                orig_co == des_feat.city_objects.get(id).unwrap()
            );

            println!(
                "is attribute same======? {:?}",
                orig_co.attributes == des_feat.city_objects.get(id).unwrap().attributes
            );
            if orig_co.attributes != des_feat.city_objects.get(id).unwrap().attributes {
                println!("  attributes======:");

                let orig_attrs = orig_co.attributes.as_ref().unwrap();
                let des_attrs = des_feat
                    .city_objects
                    .get(id)
                    .unwrap()
                    .attributes
                    .as_ref()
                    .unwrap();
                if orig_attrs.is_object() && des_attrs.is_object() {
                    for (key, value) in orig_attrs.as_object().unwrap() {
                        let des_value = des_attrs.get(key);
                        if des_value.is_none() {
                            println!("  key not found: {:?}", key);
                        } else if value != des_value.unwrap() {
                            println!("  key: {:?}", key);
                            println!("    original: {:?}", value);
                            println!("    deserialized: {:?}", des_value.unwrap());
                        }
                    }
                }
            }
            // ===============remove these lines later=================
            // FIXME: Later, just compare CityObject using "=="

            let des_co = des_feat.city_objects.get(id).unwrap();

            // Compare type
            if orig_co.thetype != des_co.thetype {
                println!("  type: '{}' != '{}'", orig_co.thetype, des_co.thetype);
            }

            // Compare children
            if orig_co.children != des_co.children {
                println!("  children:");
                println!("    original: {:?}", orig_co.children);
                println!("    deserialized: {:?}", des_co.children);
            }

            // Compare parents
            if orig_co.parents != des_co.parents {
                println!("  parents:");
                println!("    original: {:?}", orig_co.parents);
                println!("    deserialized: {:?}", des_co.parents);
            }

            // Compare geographical extent
            if orig_co.geographical_extent != des_co.geographical_extent {
                println!("  geographical_extent:");
                println!("    original: {:?}", orig_co.geographical_extent);
                println!("    deserialized: {:?}", des_co.geographical_extent);
            }

            // Compare attributes
            // TODO: implement attributes
            // if orig_co.attributes != des_co.attributes {
            //     println!("  attributes:");
            //     println!("    original: {:?}", orig_co.attributes);
            //     println!("    deserialized: {:?}", des_co.attributes);
            // }

            // Compare geometries
            if let (Some(orig_geoms), Some(des_geoms)) = (&orig_co.geometry, &des_co.geometry) {
                if orig_geoms.len() != des_geoms.len() {
                    println!(
                        "  geometry count mismatch: {} != {}",
                        orig_geoms.len(),
                        des_geoms.len()
                    );
                } else {
                    // Compare geometries by matching LOD values
                    for (i, orig_geom) in orig_geoms.iter().enumerate() {
                        let des_geom = des_geoms
                            .iter()
                            .find(|g| g.lod == orig_geom.lod)
                            .unwrap_or_else(|| {
                                panic!(
                                    "No matching geometry with LOD {:?} found in deserialized data",
                                    orig_geom.lod
                                )
                            });

                        if orig_geom != des_geom {
                            println!("  geometry[{}] with LOD {:?} differs:", i, orig_geom.lod);
                            if orig_geom.boundaries != des_geom.boundaries {
                                println!("    boundaries differ:");
                                println!("      original: {:?}", orig_geom.boundaries);
                                println!("      deserialized: {:?}", des_geom.boundaries);
                            }

                            // Compare semantics
                            match (&orig_geom.semantics, &des_geom.semantics) {
                                (Some(orig_sem), Some(des_sem)) => {
                                    if orig_sem.surfaces != des_sem.surfaces {
                                        println!("    semantic surfaces differ:");
                                        println!("      original: {:?}", orig_sem.surfaces);
                                        println!("      deserialized: {:?}", des_sem.surfaces);
                                    }
                                    if orig_sem.values != des_sem.values {
                                        println!("    semantic values differ:");
                                        println!("      original: {:?}", orig_sem.values);
                                        println!("      deserialized: {:?}", des_sem.values);
                                    }
                                }
                                (None, Some(des_sem)) => {
                                    println!("    semantics: original None, deserialized Some");
                                    println!("      deserialized: {:?}", des_sem);
                                }
                                (Some(orig_sem), None) => {
                                    println!("    semantics: original Some, deserialized None");
                                    println!("      original: {:?}", orig_sem);
                                }
                                (None, None) => {}
                            }
                        }
                    }
                }
            } else if orig_co.geometry.is_some() != des_co.geometry.is_some() {
                println!("  geometry presence mismatch:");
                println!("    original: {:?}", orig_co.geometry.is_some());
                println!("    deserialized: {:?}", des_co.geometry.is_some());
            }
        }
    }

    Ok(())
}
