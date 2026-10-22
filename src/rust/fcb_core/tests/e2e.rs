//! Whole-file round trips: a CityJSONSeq fixture is written with `FcbWriter`,
//! read back with `FcbReader`, and compared with the input as whole values,
//! not member by member, so a change anywhere in a feature fails the test.

use anyhow::Result;
use cjseq::{CityJSON, CityJSONFeature};
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    deserializer,
    header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, FcbReader, FcbWriter,
};
use pretty_assertions::assert_eq;
use std::{
    fs::File,
    io::{BufReader, Cursor},
    path::PathBuf,
};

/// A fixture, and what came back from writing it to FCB and reading it again.
struct RoundTrip {
    header: CityJSON,
    features: Vec<CityJSONFeature>,
    decoded_header: CityJSON,
    decoded_features: Vec<CityJSONFeature>,
}

fn roundtrip(fixture: &str) -> Result<RoundTrip> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(fixture);
    let seq = match read_cityjson_from_reader(BufReader::new(File::open(path)?), CJTypeKind::Seq)? {
        CJType::Seq(seq) => seq,
        _ => panic!("expected CityJSONSeq"),
    };

    // City Object attributes go in the attribute schema; a semantic surface's
    // extra members go in a schema of their own, or the writer has no column
    // to put them in.
    let mut attr_schema = AttributeSchema::new();
    let mut semantic_attr_schema = AttributeSchema::new();
    let templates = seq.cj.geometry_templates.iter().flat_map(|t| &t.templates);
    let objects = seq.features.iter().flat_map(|f| f.city_objects.values());
    for co in objects.clone() {
        if let Some(attributes) = &co.attributes {
            attr_schema.add_attributes(attributes);
        }
    }
    let geometries = objects.flat_map(|co| co.geometry.iter().flatten());
    for geometry in geometries.chain(templates) {
        for surface in geometry.surfaces().unwrap_or_default() {
            semantic_attr_schema.add_attributes(&surface.other);
        }
    }

    let mut buf = Vec::new();
    let mut fcb = FcbWriter::new(
        seq.cj.clone(),
        Some(HeaderWriterOptions {
            write_index: false,
            feature_count: seq.features.len() as u64,
            index_node_size: 16,
            attribute_indices: None,
            geographical_extent: None,
        }),
        Some(attr_schema),
        Some(semantic_attr_schema),
    )?;
    for feature in &seq.features {
        fcb.add_feature(feature)?;
    }
    fcb.write(&mut buf)?;

    let mut reader = FcbReader::open(Cursor::new(buf))?.select_all()?;
    let decoded_header = deserializer::to_cj_metadata(&reader.header())?;
    let mut decoded_features = Vec::new();
    while let Some(feature) = reader.next()? {
        decoded_features.push(feature.cur_cj_feature()?);
    }

    Ok(RoundTrip {
        header: seq.cj,
        features: seq.features,
        decoded_header,
        decoded_features,
    })
}

/// The input as FCB can store it. A `null` attribute is dropped: every
/// column type has a fixed width and the format has no null bit, so there is
/// nothing to write. Supporting it is a format change (a null bit or a Null
/// column type) and a C++ port.
fn without_null_attributes(mut features: Vec<CityJSONFeature>) -> Vec<CityJSONFeature> {
    for co in features
        .iter_mut()
        .flat_map(|f| f.city_objects.values_mut())
    {
        if let Some(attributes) = co.attributes.as_mut() {
            attributes.retain(|_, v| !v.is_null());
        }
    }
    features
}

/// 3DBAG buildings: attributes, three LoDs of Solids, semantic surfaces
/// with extra members, parents and children.
#[test]
fn a_3dbag_tile_roundtrips() -> Result<()> {
    let rt = roundtrip("small.city.jsonl")?;

    assert_eq!(rt.decoded_features, without_null_attributes(rt.features));

    // The header has a field per metadata member the schema names, and
    // nowhere for the ones it does not.
    let mut header = rt.header;
    if let Some(metadata) = header.metadata.as_mut() {
        metadata.other.clear();
    }
    assert_eq!(rt.decoded_header.metadata, header.metadata);
    assert_eq!(rt.decoded_header.version, header.version);
    assert_eq!(rt.decoded_header.transform, header.transform);
    Ok(())
}

/// Geometry templates in the header, with their appearance, and the
/// GeometryInstances that place them.
#[test]
fn geometry_templates_and_instances_roundtrip() -> Result<()> {
    let rt = roundtrip("geom_temp.city.jsonl")?;

    assert!(rt.header.geometry_templates.is_some());
    assert_eq!(
        rt.decoded_header.geometry_templates,
        rt.header.geometry_templates
    );
    assert_eq!(rt.decoded_header.appearance, rt.header.appearance);
    assert_eq!(rt.decoded_features, without_null_attributes(rt.features));
    Ok(())
}

/// The Noise Extension: the `extensions` member, `+`-prefixed City Object
/// and semantic surface types, and `+`-prefixed attributes.
#[test]
fn an_extension_roundtrips() -> Result<()> {
    let rt = roundtrip("noise_extension.city.jsonl")?;

    assert!(rt.header.extensions.is_some());
    assert_eq!(rt.decoded_header.extensions, rt.header.extensions);
    assert_eq!(rt.decoded_features, without_null_attributes(rt.features));
    Ok(())
}
