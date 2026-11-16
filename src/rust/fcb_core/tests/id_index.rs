//! The index over City Object ids: written by default, queried as the
//! column `id`, and never stored as an attribute.

use anyhow::Result;
use cjseq::{CityJSON, CityJSONFeature};
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    FcbReader, FcbWriter, FixedStringKey, KeyType, Operator,
};
use std::io::Cursor;
use std::str::FromStr;

const METADATA: &str = r#"{"type":"CityJSON","version":"2.0","transform":{"scale":[1.0,1.0,1.0],"translate":[0.0,0.0,0.0]},"CityObjects":{},"vertices":[]}"#;

fn feature(id: &str, part_attributes: &str) -> CityJSONFeature {
    CityJSONFeature::from_str(&format!(
        r#"{{"type":"CityJSONFeature","id":"{id}","CityObjects":{{"{id}":{{"type":"Building","children":["{id}-0"],"geometry":[{{"type":"MultiSurface","lod":"0","boundaries":[[[0,1,2]]]}}]}},"{id}-0":{{"type":"BuildingPart","parents":["{id}"],"attributes":{part_attributes}}}}},"vertices":[[0,0,0],[1,0,0],[1,1,0]]}}"#
    ))
    .expect("the test feature is valid CityJSON")
}

fn write(features: &[CityJSONFeature], id_index: Option<u16>) -> Result<Vec<u8>> {
    let mut schema = AttributeSchema::new();
    for feature in features {
        let mut ids: Vec<&String> = feature.city_objects.keys().collect();
        ids.sort_unstable();
        for co in ids
            .into_iter()
            .filter_map(|id| feature.city_objects.get(id))
        {
            if let Some(attributes) = &co.attributes {
                schema.add_attributes(attributes);
            }
        }
    }
    let options = HeaderWriterOptions {
        feature_count: features.len() as u64,
        id_index,
        ..Default::default()
    };
    let mut writer = FcbWriter::new(
        CityJSON::from_str(METADATA)?,
        Some(options),
        Some(schema),
        None,
    )?;
    for feature in features {
        writer.add_feature(feature)?;
    }
    let mut out = Vec::new();
    writer.write(&mut out)?;
    Ok(out)
}

/// The features whose City Objects include one with this id, per the index.
fn select_by_id(fcb: &[u8], id: &str) -> Result<Vec<CityJSONFeature>> {
    let query = vec![(
        "id".to_string(),
        Operator::Eq,
        KeyType::StringKey50(FixedStringKey::from_str(id)),
    )];
    let mut iter = FcbReader::open(Cursor::new(fcb))?.select_attr_query(query)?;
    let mut features = Vec::new();
    while let Some(feature) = iter.next()? {
        features.push(feature.cur_cj_feature()?);
    }
    Ok(features)
}

#[test]
fn every_city_object_id_is_indexed_and_no_attribute_is_added() -> Result<()> {
    let features = [
        feature("a", r#"{"height":1.5}"#),
        feature("b", r#"{"height":2.5}"#),
        feature("c", r#"{"height":3.5}"#),
    ];
    let fcb = write(&features, HeaderWriterOptions::default().id_index)?;

    let header_columns = {
        let reader = FcbReader::open(Cursor::new(&fcb))?;
        let header = reader.header();
        let columns = header.columns().expect("the header has columns");
        columns
            .iter()
            .map(|c| (c.name().to_string(), c.primary_key()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        header_columns,
        [("height".to_string(), false), ("id".to_string(), true)]
    );

    // A child object's id finds the feature that holds it, and so does the
    // feature's own id.
    for id in ["b-0", "b"] {
        let hits = select_by_id(&fcb, id)?;
        assert_eq!(hits.len(), 1, "one feature holds `{id}`");
        assert_eq!(hits[0].id, "b");
        assert_eq!(hits[0], features[1], "the feature reads back unchanged");
    }
    assert!(select_by_id(&fcb, "missing")?.is_empty());
    Ok(())
}

#[test]
fn a_dataset_with_its_own_id_attribute_keeps_it_and_gets_no_id_index() -> Result<()> {
    let features = [feature("a", r#"{"id":"own-a"}"#)];
    let fcb = write(&features, HeaderWriterOptions::default().id_index)?;

    let reader = FcbReader::open(Cursor::new(&fcb))?;
    let header = reader.header();
    let columns = header.columns().expect("the header has columns");
    assert_eq!(columns.len(), 1);
    assert_eq!(columns.get(0).name(), "id");
    assert!(!columns.get(0).primary_key());
    assert!(header.attribute_index().is_none());
    Ok(())
}

#[test]
fn no_id_index_writes_neither_the_column_nor_the_index() -> Result<()> {
    let fcb = write(&[feature("a", r#"{"height":1.5}"#)], None)?;

    let reader = FcbReader::open(Cursor::new(&fcb))?;
    let header = reader.header();
    let columns = header.columns().expect("the header has columns");
    assert_eq!(columns.len(), 1);
    assert_eq!(columns.get(0).name(), "height");
    assert!(header.attribute_index().is_none());
    Ok(())
}
