use anyhow::Result;
use cjseq::GeometryType as CjGeometryType;
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
                attribute_indices: None,
                geographical_extent: None,
            }),
            Some(attr_schema),
            None,
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
                                    println!("      deserialized: {des_sem:?}");
                                }

                                (Some(orig_sem), None) => {
                                    println!("    semantics: original Some, deserialized None");
                                    println!("      original: {orig_sem:?}");
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

#[test]
fn test_geometry_template_cycle() -> Result<()> {
    // 1. Setup paths for geom_temp.city.jsonl
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir
        .join("tests")
        .join("data")
        .join("geom_temp.city.jsonl"); // Use the correct file
    let temp_fcb = NamedTempFile::new()?;

    // 2. Read original CityJSONSeq
    let input_file = File::open(input_path)?;
    let input_reader = BufReader::new(input_file);
    // Assuming read_cityjson_from_reader handles the sequence format correctly
    let original_cj_seq = match read_cityjson_from_reader(input_reader, CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => panic!("Expected CityJSONSeq from geom_temp.city.jsonl"),
    };
    // Store original templates for later comparison
    let original_templates = original_cj_seq.cj.geometry_templates.clone();

    // 3. Write to FCB
    {
        let output_file = File::create(&temp_fcb)?;
        let output_writer = BufWriter::new(output_file);

        // Build attribute schema (important if instances have attributes)
        let mut attr_schema = AttributeSchema::new();
        for feature in original_cj_seq.features.iter() {
            for (_, co) in feature.city_objects.iter() {
                if let Some(attributes) = &co.attributes {
                    attr_schema.add_attributes(attributes);
                }
                // Also check attributes within semantic surfaces if applicable
                if let Some(geoms) = &co.geometry {
                    for geom in geoms {
                        if let Some(semantics) = &geom.semantics {
                            for surface in &semantics.surfaces {
                                // Assuming 'other' holds attributes, adjust if needed
                                if let Some(other) = &surface.other {
                                    attr_schema.add_attributes(other);
                                }
                            }
                        }
                    }
                }
            }
        }
        // Add attributes from header templates if they exist
        if let Some(gt) = &original_cj_seq.cj.geometry_templates {
            for template_geom in &gt.templates {
                if let Some(semantics) = &template_geom.semantics {
                    for surface in &semantics.surfaces {
                        if let Some(other) = &surface.other {
                            attr_schema.add_attributes(other);
                        }
                    }
                }
            }
        }

        let mut fcb = FcbWriter::new(
            original_cj_seq.cj.clone(), // Pass the CJ object with templates
            Some(HeaderWriterOptions {
                write_index: false, // Keep index off for simplicity unless needed
                feature_count: original_cj_seq.features.len() as u64,
                index_node_size: 16,
                attribute_indices: None,
                geographical_extent: None,
            }),
            Some(attr_schema),
            None,
        )?;
        for feature in original_cj_seq.features.iter() {
            fcb.add_feature(feature)?;
        }
        fcb.write(output_writer)?;
    }

    // 4. Read back from FCB
    let fcb_file = File::open(&temp_fcb)?;
    let fcb_reader = BufReader::new(fcb_file);
    let mut reader = FcbReader::open(fcb_reader)?.select_all()?;

    // 5. Deserialize Header & Features
    let header = reader.header();
    let deserialized_cj = deserializer::to_cj_metadata(&header)?; // This now decodes templates

    let mut deserialized_features = Vec::new();
    let feat_count = header.features_count();
    let mut feat_num = 0;
    while let Ok(Some(feat_buf)) = reader.next() {
        // Pass the schema derived from the header for attribute decoding
        let feature = feat_buf.cur_cj_feature()?; // Uses modified to_cj_feature
        deserialized_features.push(feature);
        feat_num += 1;
        if feat_num >= feat_count {
            break;
        }
    }

    // 6. Assertions
    // Assert Header Geometry Templates
    assert!(
        deserialized_cj.geometry_templates.is_some(),
        "Deserialized CityJSON should have geometry_templates"
    );
    assert!(
        original_templates.is_some(),
        "Original CityJSONSeq should have geometry_templates"
    );

    if let (Some(orig_gt), Some(des_gt)) = (original_templates, deserialized_cj.geometry_templates)
    {
        assert_eq!(
            orig_gt.templates.len(),
            des_gt.templates.len(),
            "Template count mismatch"
        );
        assert_eq!(
            orig_gt.vertices_templates.len(),
            des_gt.vertices_templates.len(),
            "Template vertex count mismatch"
        );
        // Deep comparison using PartialEq (ensure it's derived for GeometryTemplates and Geometry)
        assert_eq!(
            orig_gt, des_gt,
            "Deserialized GeometryTemplates differ from original"
        );
    }

    // Assert Features and Geometry Instances
    assert_eq!(
        original_cj_seq.features.len(),
        deserialized_features.len(),
        "Feature count mismatch"
    );
    for (orig_feat, des_feat) in original_cj_seq
        .features
        .iter()
        .zip(deserialized_features.iter())
    {
        assert_eq!(orig_feat.id, des_feat.id);
        assert_eq!(orig_feat.city_objects.len(), des_feat.city_objects.len());

        for (id, orig_co) in orig_feat.city_objects.iter() {
            let des_co = des_feat
                .city_objects
                .get(id)
                .unwrap_or_else(|| panic!("Deserialized CityObject missing for ID: {id}"));
            assert_eq!(orig_co.thetype, des_co.thetype);

            // Find original GeometryInstance (if any)
            let orig_instance_geom = orig_co.geometry.as_ref().and_then(|geoms| {
                geoms
                    .iter()
                    .find(|g| g.thetype == CjGeometryType::GeometryInstance)
            });

            if let Some(orig_instance) = orig_instance_geom {
                // Find the corresponding deserialized geometry instance
                let des_instance_geom = des_co
                    .geometry
                    .as_ref()
                    .and_then(|geoms| {
                        geoms.iter().find(|g| {
                            g.thetype == CjGeometryType::GeometryInstance
                                && g.template == orig_instance.template // Match by template index
                        })
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "Deserialized GeometryInstance missing or template mismatch for CO ID: {id}"
                        )
                    });

                assert_eq!(
                    des_instance_geom.thetype,
                    CjGeometryType::GeometryInstance,
                    "Type mismatch for instance in CO ID: {}",
                    id
                );
                assert_eq!(
                    des_instance_geom.template, orig_instance.template,
                    "Template index mismatch for instance in CO ID: {}",
                    id
                );
                assert_eq!(
                    des_instance_geom.boundaries, orig_instance.boundaries,
                    "Boundaries mismatch for instance in CO ID: {}",
                    id
                );
                // Compare transformation matrices (floating point comparison might need tolerance)
                assert_eq!(
                    des_instance_geom.transformation_matrix, orig_instance.transformation_matrix,
                    "Transformation matrix mismatch for instance in CO ID: {}",
                    id
                );
                println!("  GeometryInstance in CO ID: {id} matches");
            }
        }
    }

    Ok(())
}

#[test]
fn test_extension_serialization_cycle() -> Result<()> {
    // Setup paths
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let input_file = manifest_dir
        .join("tests")
        .join("data")
        .join("noise_extension.city.jsonl");

    let temp_fcb = NamedTempFile::new()?;

    // Read original CityJSONSeq with extensions
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
                attribute_indices: None,
                geographical_extent: None,
            }),
            Some(attr_schema),
            None,
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

    // Compare extensions
    if let (Some(orig_ext), Some(des_ext)) =
        (&original_cj_seq.cj.extensions, &deserialized_cj.extensions)
    {
        assert_eq!(orig_ext.len(), des_ext.len(), "Extension count mismatch");

        for (name, orig_ext_data) in orig_ext {
            let des_ext_data = des_ext
                .get(name)
                .unwrap_or_else(|| panic!("Extension {name} not found in deserialized data"));

            assert_eq!(
                orig_ext_data.url, des_ext_data.url,
                "URL mismatch for extension {}",
                name
            );
            assert_eq!(
                orig_ext_data.version, des_ext_data.version,
                "Version mismatch for extension {}",
                name
            );
        }
    } else if original_cj_seq.cj.extensions.is_some() {
        panic!("Extensions present in original but missing in deserialized");
    }

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

    // Test for extended city objects
    for (orig_feat, des_feat) in original_cj_seq
        .features
        .iter()
        .zip(deserialized_features.iter())
    {
        for (id, orig_co) in orig_feat.city_objects.iter() {
            if orig_co.thetype.starts_with("+") {
                let des_co = des_feat.city_objects.get(id).unwrap_or_else(|| {
                    panic!("Extended city object {id} not found in deserialized data")
                });

                println!(
                    "Found extended city object {} with type {}",
                    id, orig_co.thetype
                );
                assert_eq!(
                    orig_co.thetype, des_co.thetype,
                    "Extended city object type mismatch for {}",
                    id
                );

                // Check attributes particularly for extended objects
                if let (Some(orig_attrs), Some(des_attrs)) =
                    (&orig_co.attributes, &des_co.attributes)
                {
                    for (key, value) in orig_attrs.as_object().unwrap() {
                        if key.starts_with("+") {
                            println!("Found extended attribute: {key}");
                            let des_value = des_attrs.get(key);
                            assert!(
                                des_value.is_some(),
                                "Extended attribute {key} not found in deserialized data"
                            );
                            assert_eq!(
                                value,
                                des_value.unwrap(),
                                "Extended attribute value mismatch for {}",
                                key
                            );
                        }
                    }
                }
            }
        }
    }

    // Check for extended semantic surfaces
    for (orig_feat, des_feat) in original_cj_seq
        .features
        .iter()
        .zip(deserialized_features.iter())
    {
        for (id, orig_co) in orig_feat.city_objects.iter() {
            if let Some(orig_geoms) = &orig_co.geometry {
                for orig_geom in orig_geoms {
                    if let Some(orig_semantics) = &orig_geom.semantics {
                        for (i, orig_surface) in orig_semantics.surfaces.iter().enumerate() {
                            if orig_surface.thetype.starts_with("+") {
                                println!(
                                    "Found extended semantic surface: {}",
                                    orig_surface.thetype
                                );

                                // Find the corresponding surface in deserialized data
                                let des_co = des_feat.city_objects.get(id).unwrap();
                                let des_geom = des_co
                                    .geometry
                                    .as_ref()
                                    .and_then(|geoms| geoms.iter().find(|g| g.lod == orig_geom.lod))
                                    .unwrap_or_else(|| {
                                        panic!(
                                            "Geometry with LOD {:?} not found in deserialized data",
                                            orig_geom.lod
                                        )
                                    });

                                let des_semantics = des_geom
                                    .semantics
                                    .as_ref()
                                    .expect("Semantics not found in deserialized data");

                                // Try to find the matching surface
                                if i < des_semantics.surfaces.len() {
                                    let des_surface = &des_semantics.surfaces[i];
                                    assert_eq!(
                                        orig_surface.thetype, des_surface.thetype,
                                        "Extended semantic surface type mismatch"
                                    );
                                } else {
                                    panic!("Extended semantic surface index out of bounds");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
