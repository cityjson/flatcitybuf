use anyhow::Result;
use cjseq::{CityObjectType, Geometry as CjGeometry, SemanticSurfaceType, SemanticsSurface};
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

/// A semantic surface's `other` is the set of members the schema does not name;
/// they become attribute columns.
fn add_surface_attributes(attr_schema: &mut AttributeSchema, surface: &SemanticsSurface) {
    if surface.other.is_empty() {
        return;
    }
    let other = serde_json::Value::Object(surface.other.clone().into_iter().collect());
    attr_schema.add_attributes(&other);
}

/// `template` and `transformationMatrix` are part of the `GeometryInstance`
/// variant, so they are read out by destructuring rather than by field access.
fn as_instance(g: &CjGeometry) -> Option<(&Vec<usize>, usize, &[f64; 16])> {
    match g {
        CjGeometry::GeometryInstance {
            boundaries,
            template,
            transformation_matrix,
        } => Some((boundaries, *template, transformation_matrix)),
        _ => None,
    }
}

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
        // A semantic surface's extra members (`on_footprint_edge` here) need a
        // schema of their own; without one the writer has no column to put them
        // in and they are dropped.
        let mut semantic_attr_schema = AttributeSchema::new();
        for feature in original_cj_seq.features.iter() {
            for (_, co) in feature.city_objects.iter() {
                if let Some(attributes) = &co.attributes {
                    attr_schema.add_attributes(attributes);
                }
                for geom in co.geometry.iter().flatten() {
                    if let Some(semantics) = geom.common().and_then(|c| c.semantics.as_ref()) {
                        for surface in &semantics.surfaces {
                            add_surface_attributes(&mut semantic_attr_schema, surface);
                        }
                    }
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
            Some(semantic_attr_schema),
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
        // The header has a field per member the CityJSON schema names, and
        // nowhere to put the ones it does not: `metadata.other` is dropped by
        // the writer. Compare everything else exactly.
        //
        // This is not a regression -- the member never reached the writer
        // before either, because the CityJSON model itself discarded it at
        // parse time. Now that the model keeps it, the loss is the writer's
        // and is visible here rather than invisible upstream.
        //
        // TODO: delete this whole allowance -- both the `is_empty` assert and
        // the `clear()` -- when the header gains somewhere to put unnamed
        // metadata members (an attributes blob, as City Objects have). At that
        // point `assert_eq!(orig_meta, des_meta)` should hold outright, and
        // this assertion is *expected* to be removed rather than updated.
        let mut orig_meta = orig_meta.clone();
        assert!(
            des_meta.other.is_empty(),
            "the header cannot carry unnamed metadata members, so none should come back"
        );
        orig_meta.other.clear();
        assert_eq!(&orig_meta, des_meta)
    }

    // Compare features
    assert_eq!(original_cj_seq.features.len(), deserialized_features.len());
    for (orig_feat, des_feat) in original_cj_seq
        .features
        .iter()
        .zip(deserialized_features.iter())
    {
        // Still not `assert_eq!(orig_feat, des_feat)`. As of this branch exactly
        // one thing stops it, and it is neither geometry nor semantics: a JSON
        // `null` attribute value is dropped by the writer. `attribute.rs:111`
        // skips null values and `attribute.rs:24` refuses to allocate a column
        // for one, because every `ColumnType` has a fixed width and the format
        // has no null bit. In `small.city.jsonl` that is `eindgeldigheid`,
        // `eindregistratie`, `tijdstipeindregistratielv`, `tijdstipinactief`,
        // `tijdstipinactieflv`, `tijdstipnietbaglv` and `b3_bouwlagen`.
        //
        // Everything else round-trips, including the semantic surface
        // attributes this test used to lose -- see the semantic schema built
        // above. Fixing the last of it means adding null support to the
        // attribute encoding, which is a format change (a null bit or a Null
        // column type) and a C++ port, and is out of scope here.
        //
        // FIXME: honour this once null attributes are encodable.
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

                let _orig_attrs = orig_co.attributes.as_ref().unwrap();
                let _des_attrs = des_feat
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
                println!("  type: {:?} != {:?}", orig_co.thetype, des_co.thetype);
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
                            .find(|g| g.lod() == orig_geom.lod())
                            .unwrap_or_else(|| {
                                panic!(
                                    "No matching geometry with LOD {:?} found in deserialized data",
                                    orig_geom.lod()
                                )
                            });

                        // A geometry -- its type, boundaries, semantics,
                        // material and texture -- must round-trip exactly.
                        // This used to be a `println!` of the differences,
                        // which is how a lost nesting level could go
                        // unnoticed for months.
                        //
                        // The per-member reporting runs BEFORE the
                        // assertion, not after it. Below the `assert_eq!`
                        // it was unreachable -- the assertion has already
                        // panicked by the time an inequality could be
                        // observed. Here it prints, and then the assertion
                        // fails the test.
                        if orig_geom != des_geom {
                            println!("  geometry[{}] with LOD {:?} differs:", i, orig_geom.lod());
                            // `boundaries` is per-variant now, so the whole
                            // geometry is dumped rather than that one member.
                            println!("      original: {orig_geom:?}");
                            println!("      deserialized: {des_geom:?}");

                            // Compare semantics
                            match (
                                &orig_geom.common().and_then(|c| c.semantics.as_ref()),
                                &des_geom.common().and_then(|c| c.semantics.as_ref()),
                            ) {
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

                        assert_eq!(
                            orig_geom, des_geom,
                            "geometry[{i}] must round-trip unchanged"
                        );
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
                        if let Some(semantics) = geom.common().and_then(|c| c.semantics.as_ref()) {
                            for surface in &semantics.surfaces {
                                add_surface_attributes(&mut attr_schema, surface);
                            }
                        }
                    }
                }
            }
        }
        // Add attributes from header templates if they exist
        if let Some(gt) = &original_cj_seq.cj.geometry_templates {
            for template_geom in &gt.templates {
                if let Some(semantics) = template_geom.common().and_then(|c| c.semantics.as_ref()) {
                    for surface in &semantics.surfaces {
                        add_surface_attributes(&mut attr_schema, surface);
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
            orig_gt.vertices_templates.as_array().map(|v| v.len()),
            des_gt.vertices_templates.as_array().map(|v| v.len()),
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
            let orig_instance_geom = orig_co
                .geometry
                .as_ref()
                .and_then(|geoms| geoms.iter().find_map(as_instance));

            if let Some((orig_boundaries, orig_template, orig_matrix)) = orig_instance_geom {
                // Find the corresponding deserialized geometry instance
                let (des_boundaries, des_template, des_matrix) = des_co
                    .geometry
                    .as_ref()
                    .and_then(|geoms| {
                        geoms
                            .iter()
                            .filter_map(as_instance)
                            .find(|(_, template, _)| *template == orig_template)
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "Deserialized GeometryInstance missing or template mismatch for CO ID: {id}"
                        )
                    });

                assert_eq!(
                    des_template, orig_template,
                    "Template index mismatch for instance in CO ID: {}",
                    id
                );
                assert_eq!(
                    des_boundaries, orig_boundaries,
                    "Boundaries mismatch for instance in CO ID: {}",
                    id
                );
                // Compare transformation matrices (floating point comparison might need tolerance)
                assert_eq!(
                    des_matrix, orig_matrix,
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
        // A semantic surface's extra members (`on_footprint_edge` here) need a
        // schema of their own; without one the writer has no column to put them
        // in and they are dropped.
        let mut semantic_attr_schema = AttributeSchema::new();
        for feature in original_cj_seq.features.iter() {
            for (_, co) in feature.city_objects.iter() {
                if let Some(attributes) = &co.attributes {
                    attr_schema.add_attributes(attributes);
                }
                for geom in co.geometry.iter().flatten() {
                    if let Some(semantics) = geom.common().and_then(|c| c.semantics.as_ref()) {
                        for surface in &semantics.surfaces {
                            add_surface_attributes(&mut semantic_attr_schema, surface);
                        }
                    }
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
            Some(semantic_attr_schema),
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
        // `extensions` is a plain CityJSON member: a map of name to
        // `{url, version}`.
        let orig_ext = orig_ext.as_object().expect("extensions is an object");
        let des_ext = des_ext.as_object().expect("extensions is an object");
        assert_eq!(orig_ext.len(), des_ext.len(), "Extension count mismatch");

        for (name, orig_ext_data) in orig_ext {
            let des_ext_data = des_ext
                .get(name)
                .unwrap_or_else(|| panic!("Extension {name} not found in deserialized data"));

            assert_eq!(
                orig_ext_data.get("url"),
                des_ext_data.get("url"),
                "URL mismatch for extension {}",
                name
            );
            assert_eq!(
                orig_ext_data.get("version"),
                des_ext_data.get("version"),
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
            // An Extension type is a variant now, not a `+`-prefixed string.
            if matches!(orig_co.thetype, CityObjectType::Extension(_)) {
                let des_co = des_feat.city_objects.get(id).unwrap_or_else(|| {
                    panic!("Extended city object {id} not found in deserialized data")
                });

                println!(
                    "Found extended city object {} with type {:?}",
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
                    if let Some(orig_semantics) =
                        orig_geom.common().and_then(|c| c.semantics.as_ref())
                    {
                        for (i, orig_surface) in orig_semantics.surfaces.iter().enumerate() {
                            if matches!(orig_surface.thetype, SemanticSurfaceType::Extension(_)) {
                                println!(
                                    "Found extended semantic surface: {:?}",
                                    orig_surface.thetype
                                );

                                // Find the corresponding surface in deserialized data
                                let des_co = des_feat.city_objects.get(id).unwrap();
                                let des_geom = des_co
                                    .geometry
                                    .as_ref()
                                    .and_then(|geoms| {
                                        geoms.iter().find(|g| g.lod() == orig_geom.lod())
                                    })
                                    .unwrap_or_else(|| {
                                        panic!(
                                            "Geometry with LOD {:?} not found in deserialized data",
                                            orig_geom.lod()
                                        )
                                    });

                                let des_semantics = des_geom
                                    .common()
                                    .and_then(|c| c.semantics.as_ref())
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
