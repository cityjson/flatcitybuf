//! Round-trip tests for appearance (material/texture) mappings.
//!
//! These tests start from CityJSONSeq input, encode with the real writer
//! (`FcbWriter`), decode with the real reader (`FcbReader`), and compare the
//! decoded `material`/`texture` members of the geometry against the input.
//! They also dump the raw FlatBuffers mapping arrays (solids/shells/surfaces/
//! strings/vertices) that the encoder actually emitted, which is what makes the
//! point of the type-driven decoder visible: **several geometry types flatten
//! to byte-identical arrays**, and only the stored geometry type tells them
//! apart.
//!
//! The pairs that collide, each proved below by a `*_flatten_identically` test:
//!
//! | these two                          | emit identical arrays because       |
//! |------------------------------------|-------------------------------------|
//! | `Solid`, one-solid `MultiSolid`    | `solids == [n]` either way          |
//! | `MultiSurface`, `CompositeSurface` | same depth, no other difference     |
//! | `MultiSolid`, `CompositeSolid`     | same depth, no other difference     |
//!
//! Before the decoder took the geometry type as a parameter it guessed from
//! `solids.len() == 1` / `shells.len() == 1` / `strings.len() > 1`, and every
//! one of those collisions decoded at the wrong depth for one member of the
//! pair. That is finding #8.
//!
//! Note which types appear here and which do not: `geomprimitives.schema.json`
//! gives `MultiPoint` and `MultiLineString` no `material` and no `texture`
//! member and declares `additionalProperties: false`, so appearance on one of
//! them is not valid CityJSON. They are covered for boundaries and semantics
//! instead.

use anyhow::Result;
use cjseq::{
    CityJSONFeature, Geometry as CjGeometry, MaterialReference, TextureReference,
};
use fcb_core::{
    attribute::AttributeSchema, deserializer, header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, FcbReader, FcbWriter,
};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufReader, Cursor};

/// Raw arrays of one FlatBuffers `MaterialMapping`, as emitted by the encoder.
#[derive(Debug, Default, PartialEq)]
struct MaterialDump {
    theme: String,
    solids: Vec<u32>,
    shells: Vec<u32>,
    vertices: Vec<u32>,
    value: Option<u32>,
}

/// Raw arrays of one FlatBuffers `TextureMapping`, as emitted by the encoder.
#[derive(Debug, Default, PartialEq)]
struct TextureDump {
    theme: String,
    solids: Vec<u32>,
    shells: Vec<u32>,
    surfaces: Vec<u32>,
    strings: Vec<u32>,
    vertices: Vec<u32>,
}

fn material_of(g: &CjGeometry) -> Option<&HashMap<String, MaterialReference>> {
    g.common().and_then(|c| c.material.as_ref())
}

fn texture_of(g: &CjGeometry) -> Option<&HashMap<String, TextureReference>> {
    g.common().and_then(|c| c.texture.as_ref())
}

/// Encodes a single-geometry CityJSONSeq with the real writer, decodes it with
/// the real reader, and returns (input geometry, decoded geometry, raw
/// material mappings, raw texture mappings).
fn roundtrip_geometry(
    geometry: Value,
    vertex_count: usize,
) -> Result<(CjGeometry, CjGeometry, Vec<MaterialDump>, Vec<TextureDump>)> {
    let cj_line = json!({
        "type": "CityJSON",
        "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]},
        "CityObjects": {},
        "vertices": []
    });
    // Distinct dummy vertices so boundary indices survive re-indexing unchanged.
    let vertices: Vec<[i64; 3]> = (0..vertex_count).map(|i| [i as i64, i as i64, 0]).collect();
    let feature_line = json!({
        "type": "CityJSONFeature",
        "id": "feat-1",
        "CityObjects": {
            "co-1": {"type": "Building", "geometry": [geometry]}
        },
        "vertices": vertices
    });
    let input = format!("{cj_line}\n{feature_line}\n");

    let seq = match read_cityjson_from_reader(
        BufReader::new(Cursor::new(input.into_bytes())),
        CJTypeKind::Seq,
    )? {
        CJType::Seq(seq) => seq,
        _ => panic!("expected CityJSONSeq"),
    };
    let orig_geom = seq.features[0].city_objects["co-1"]
        .geometry
        .as_ref()
        .expect("input feature must have geometry")[0]
        .clone();

    // Encode with the real writer.
    let mut fcb_buf: Vec<u8> = Vec::new();
    {
        let mut fcb = FcbWriter::new(
            seq.cj.clone(),
            Some(HeaderWriterOptions {
                write_index: false,
                feature_count: seq.features.len() as u64,
                index_node_size: 16,
                attribute_indices: None,
                geographical_extent: None,
            }),
            Some(AttributeSchema::new()),
            None,
        )?;
        for feature in seq.features.iter() {
            fcb.add_feature(feature)?;
        }
        fcb.write(&mut fcb_buf)?;
    }

    // Decode with the real reader.
    let mut reader = FcbReader::open(Cursor::new(fcb_buf))?.select_all()?;
    let header = reader.header();
    let _ = deserializer::to_cj_metadata(&header)?;
    let mut decoded_feature: Option<CityJSONFeature> = None;
    let mut material_dumps = Vec::new();
    let mut texture_dumps = Vec::new();
    if let Ok(Some(feat_buf)) = reader.next() {
        // Dump the raw mapping arrays the encoder actually wrote.
        let raw_feature = feat_buf.cur_feature();
        let objects = raw_feature.objects().expect("feature has objects");
        for obj in objects.iter() {
            for geom in obj.geometry().into_iter().flatten() {
                for m in geom.material().into_iter().flatten() {
                    material_dumps.push(MaterialDump {
                        theme: m.theme().unwrap_or_default().to_string(),
                        solids: m.solids().map(|v| v.iter().collect()).unwrap_or_default(),
                        shells: m.shells().map(|v| v.iter().collect()).unwrap_or_default(),
                        vertices: m.vertices().map(|v| v.iter().collect()).unwrap_or_default(),
                        value: m.value(),
                    });
                }
                for t in geom.texture().into_iter().flatten() {
                    texture_dumps.push(TextureDump {
                        theme: t.theme().unwrap_or_default().to_string(),
                        solids: t.solids().map(|v| v.iter().collect()).unwrap_or_default(),
                        shells: t.shells().map(|v| v.iter().collect()).unwrap_or_default(),
                        surfaces: t.surfaces().map(|v| v.iter().collect()).unwrap_or_default(),
                        strings: t.strings().map(|v| v.iter().collect()).unwrap_or_default(),
                        vertices: t.vertices().map(|v| v.iter().collect()).unwrap_or_default(),
                    });
                }
            }
        }
        decoded_feature = Some(feat_buf.cur_cj_feature()?);
    }
    let decoded_feature = decoded_feature.expect("one feature must round-trip");
    let decoded_geom = decoded_feature.city_objects["co-1"]
        .geometry
        .as_ref()
        .expect("decoded feature must have geometry")[0]
        .clone();

    println!("encoder emitted material mappings: {material_dumps:?}");
    println!("encoder emitted texture mappings: {texture_dumps:?}");
    println!(
        "input    material: {:?}\ninput    texture: {:?}",
        material_of(&orig_geom),
        texture_of(&orig_geom)
    );
    println!(
        "decoded  material: {:?}\ndecoded  texture: {:?}",
        material_of(&decoded_geom),
        texture_of(&decoded_geom)
    );

    Ok((orig_geom, decoded_geom, material_dumps, texture_dumps))
}

/// Asserts that a geometry round-trips whole — type, lod, boundaries,
/// semantics, material and texture — by comparing the serialized CityJSON,
/// which is the only comparison that would catch a depth change.
fn assert_roundtrips(geometry: Value, vertex_count: usize) -> Result<(Vec<MaterialDump>, Vec<TextureDump>)>
{
    let (orig, decoded, materials, textures) = roundtrip_geometry(geometry.clone(), vertex_count)?;
    assert_eq!(
        serde_json::to_value(&decoded)?,
        serde_json::to_value(&orig)?,
        "the decoded geometry must be the input geometry"
    );
    // And the input must be exactly what was written, so a test whose expected
    // value was quietly normalized on the way in cannot pass vacuously.
    assert_eq!(
        serde_json::to_value(&orig)?,
        geometry,
        "the parsed input must be the JSON the test wrote"
    );
    Ok((materials, textures))
}

// ---------------------------------------------------------------------------
// the collisions: types whose flattened arrays are indistinguishable
// ---------------------------------------------------------------------------

/// A `Solid` with one shell and a `MultiSolid` with one solid of one shell emit
/// byte-identical material *and* texture arrays. Only the stored geometry type
/// separates them, which is exactly what finding #8 was: the decoder guessed
/// from `solids.len()` and sent one of the two to the wrong depth.
#[test]
fn solid_and_single_solid_multisolid_flatten_identically() -> Result<()> {
    let solid = json!({
        "type": "Solid",
        "lod": "1",
        "boundaries": [[[[0, 1, 2, 3]], [[4, 5, 6, 7]]]],
        "material": {"winter": {"values": [[0, 1]]}},
        "texture": {"winter": {"values": [[[[0, 8, 9, 10, 11]]], [[[1, 12, 13, 14, 15]]]]}}
    });
    let multi_solid = json!({
        "type": "MultiSolid",
        "lod": "1",
        "boundaries": [[[[[0, 1, 2, 3]], [[4, 5, 6, 7]]]]],
        "material": {"winter": {"values": [[[0, 1]]]}},
        "texture": {"winter": {"values": [[[[[0, 8, 9, 10, 11]]], [[[1, 12, 13, 14, 15]]]]]}}
    });

    let (solid_materials, solid_textures) = assert_roundtrips(solid, 8)?;
    let (multi_materials, multi_textures) = assert_roundtrips(multi_solid, 8)?;

    assert_eq!(
        solid_materials, multi_materials,
        "a Solid and a one-solid MultiSolid must emit identical material arrays"
    );
    assert_eq!(
        solid_textures, multi_textures,
        "a Solid and a one-solid MultiSolid must emit identical texture arrays"
    );
    Ok(())
}

/// `MultiSurface` and `CompositeSurface` differ only in their name; likewise
/// `MultiSolid` and `CompositeSolid`. Each pair must still round-trip to its
/// own type.
#[test]
fn same_depth_types_flatten_identically() -> Result<()> {
    let surface_body = json!({
        "lod": "1",
        "boundaries": [[[0, 1, 2]], [[3, 4, 5]]],
        "material": {"winter": {"values": [0, 1]}},
        "texture": {"winter": {"values": [[[0, 6, 7, 8]], [[1, 9, 10, 11]]]}}
    });
    let with_type = |t: &str, body: &Value| {
        let mut v = body.clone();
        v.as_object_mut()
            .expect("object")
            .insert("type".to_string(), json!(t));
        v
    };

    let (ms_m, ms_t) = assert_roundtrips(with_type("MultiSurface", &surface_body), 6)?;
    let (cs_m, cs_t) = assert_roundtrips(with_type("CompositeSurface", &surface_body), 6)?;
    assert_eq!(ms_m, cs_m);
    assert_eq!(ms_t, cs_t);

    let solid_body = json!({
        "lod": "1",
        "boundaries": [[[[[0, 1, 2]]]], [[[[3, 4, 5]]]]],
        "material": {"winter": {"values": [[[0]], [[1]]]}},
        "texture": {"winter": {"values": [[[[[0, 6, 7, 8]]]], [[[[1, 9, 10, 11]]]]]}}
    });
    let (msol_m, msol_t) = assert_roundtrips(with_type("MultiSolid", &solid_body), 6)?;
    let (csol_m, csol_t) = assert_roundtrips(with_type("CompositeSolid", &solid_body), 6)?;
    assert_eq!(msol_m, csol_m);
    assert_eq!(msol_t, csol_t);
    Ok(())
}

// ---------------------------------------------------------------------------
// one case per geometry type
// ---------------------------------------------------------------------------

/// `MultiPoint` can carry neither `material` nor `texture` — the schema names
/// neither and forbids additional properties — so this covers what it *can*
/// carry, and pins that the depth-1 boundaries survive.
#[test]
fn multipoint_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiPoint",
        "lod": "1",
        "boundaries": [0, 1, 2, 3],
        "semantics": {
            "surfaces": [{"type": "RoofSurface"}],
            "values": [0, null, 0, null]
        }
    });
    let (materials, textures) = assert_roundtrips(geometry, 4)?;
    assert!(materials.is_empty());
    assert!(textures.is_empty());
    Ok(())
}

/// Likewise `MultiLineString`. The old test here gave one a `texture`, which is
/// not valid CityJSON: the schema types `MultiLineString` with no `texture`
/// member at all. The depth-2 boundaries and the flat semantics are what it
/// actually has.
#[test]
fn multilinestring_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiLineString",
        "lod": "1",
        "boundaries": [[0, 1, 2]],
        "semantics": {
            "surfaces": [{"type": "RoofSurface"}],
            "values": [0]
        }
    });
    assert_roundtrips(geometry, 3)?;

    // More than one string, so the single-string case above is not the only
    // shape exercised.
    let geometry = json!({
        "type": "MultiLineString",
        "lod": "1",
        "boundaries": [[0, 1], [2, 3], [4, 5]],
        "semantics": {
            "surfaces": [{"type": "RoofSurface"}],
            "values": [0, null, 0]
        }
    });
    assert_roundtrips(geometry, 6)?;
    Ok(())
}

#[test]
fn multisurface_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiSurface",
        "lod": "1",
        "boundaries": [[[0, 1, 2]], [[3, 4, 5]], [[6, 7, 8]]],
        "material": {"winter": {"values": [0, null, 1]}},
        "texture": {
            "winter": {"values": [[[0, 9, 10, 11]], [[null]], [[1, 12, 13, 14]]]}
        }
    });
    let (materials, textures) = assert_roundtrips(geometry, 9)?;

    // What the encoder actually emits for this shape.
    assert_eq!(materials.len(), 1);
    assert_eq!(materials[0].solids, Vec::<u32>::new());
    assert_eq!(materials[0].shells, Vec::<u32>::new());
    assert_eq!(materials[0].vertices, vec![0, u32::MAX, 1]);

    assert_eq!(textures[0].solids, Vec::<u32>::new());
    assert_eq!(textures[0].shells, vec![3]);
    assert_eq!(textures[0].surfaces, vec![1, 1, 1]);
    Ok(())
}

/// A single-surface `MultiSurface`: the shape whose texture arrays used to be
/// confusable with a `MultiLineString`'s.
#[test]
fn multisurface_single_surface_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiSurface",
        "lod": "1",
        "boundaries": [[[0, 1, 2]]],
        "material": {"winter": {"values": [0]}},
        "texture": {"winter": {"values": [[[0, 3, 4, 5]]]}}
    });
    let (_, textures) = assert_roundtrips(geometry, 3)?;
    assert_eq!(textures[0].shells, vec![1]);
    assert_eq!(textures[0].surfaces, vec![1]);
    assert_eq!(textures[0].strings, vec![4]);
    Ok(())
}

#[test]
fn compositesurface_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "CompositeSurface",
        "lod": "1",
        "boundaries": [[[0, 1, 2]], [[3, 4, 5]]],
        "material": {"winter": {"values": [0, 1]}, "summer": {"value": 3}},
        "texture": {"winter": {"values": [[[0, 6, 7, 8]], [[1, 9, 10, 11]]]}}
    });
    let (materials, _) = assert_roundtrips(geometry, 6)?;
    // Two themes, one of them a whole-object `value`.
    assert_eq!(materials.len(), 2);
    assert!(materials.iter().any(|m| m.value == Some(3)));
    Ok(())
}

/// The commonest shape of all: a building with only an exterior shell. Its
/// material values used to come back one level too deep.
#[test]
fn solid_single_shell_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "Solid",
        "lod": "1",
        "boundaries": [[[[0, 1, 2, 3]], [[4, 5, 6, 7]]]],
        "material": {"winter": {"values": [[0, 1]]}},
        "texture": {"winter": {"values": [[[[0, 8, 9, 10, 11]], [[1, 12, 13, 14, 15]]]]}}
    });
    let (materials, textures) = assert_roundtrips(geometry, 8)?;

    assert_eq!(materials.len(), 1);
    assert_eq!(materials[0].solids, vec![1]);
    assert_eq!(materials[0].shells, vec![2]);
    assert_eq!(materials[0].vertices, vec![0, 1]);

    assert_eq!(textures[0].solids, vec![1]);
    assert_eq!(textures[0].shells, vec![2]);
    Ok(())
}

#[test]
fn solid_two_shells_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "Solid",
        "lod": "1",
        "boundaries": [
            [[[0, 1, 2, 3]], [[4, 5, 6, 7]]],
            [[[8, 9, 10, 11]]]
        ],
        "material": {"winter": {"values": [[0, 1], [2]]}},
        "texture": {
            "winter": {"values": [
                [[[0, 12, 13, 14, 15]], [[1, 16, 17, 18, 19]]],
                [[[2, 20, 21, 22, 23]]]
            ]}
        }
    });
    let (materials, textures) = assert_roundtrips(geometry, 12)?;
    assert_eq!(materials[0].solids, vec![2]);
    assert_eq!(materials[0].shells, vec![2, 1]);
    assert_eq!(textures[0].solids, vec![2]);
    assert_eq!(textures[0].shells, vec![2, 1]);
    Ok(())
}

#[test]
fn multisolid_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiSolid",
        "lod": "1",
        "boundaries": [
            [[[[0, 1, 2]]], [[[3, 4, 5]]]],
            [[[[6, 7, 8]]]]
        ],
        "material": {"winter": {"values": [[[0], [1]], [[2]]]}},
        "texture": {
            "winter": {"values": [
                [[[[0, 9, 10, 11]]], [[[1, 12, 13, 14]]]],
                [[[[2, 15, 16, 17]]]]
            ]}
        }
    });
    let (materials, textures) = assert_roundtrips(geometry, 9)?;
    assert_eq!(materials[0].solids, vec![2, 1]);
    assert_eq!(materials[0].shells, vec![1, 1, 1]);
    assert_eq!(materials[0].vertices, vec![0, 1, 2]);
    assert_eq!(textures[0].solids, vec![2, 1]);
    assert_eq!(textures[0].shells, vec![1, 1, 1]);
    Ok(())
}

#[test]
fn compositesolid_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "CompositeSolid",
        "lod": "2.2",
        "boundaries": [
            [[[[0, 1, 2]], [[3, 4, 5]]]],
            [[[[6, 7, 8]]]]
        ],
        "material": {"winter": {"values": [[[0, 1]], [[2]]]}},
        "texture": {
            "winter": {"values": [
                [[[[0, 9, 10, 11]], [[1, 12, 13, 14]]]],
                [[[[2, 15, 16, 17]]]]
            ]}
        },
        "semantics": {
            "surfaces": [{"type": "RoofSurface"}, {"type": "WallSurface"}],
            "values": [[[0, 1]], [[null]]]
        }
    });
    let (materials, textures) = assert_roundtrips(geometry, 9)?;
    assert_eq!(materials[0].solids, vec![1, 1]);
    assert_eq!(materials[0].shells, vec![2, 1]);
    assert_eq!(textures[0].solids, vec![1, 1]);
    assert_eq!(textures[0].shells, vec![2, 1]);
    Ok(())
}

// ---------------------------------------------------------------------------
// nullability
// ---------------------------------------------------------------------------

/// `material.values` is nullable at *every* level, not only at the leaf, and a
/// `None` must come back as `null` and never as `[]` — that is finding #7.
#[test]
fn null_material_shells_and_solids_roundtrip() -> Result<()> {
    // A whole null shell on a Solid.
    let geometry = json!({
        "type": "Solid",
        "lod": "1",
        "boundaries": [
            [[[0, 1, 2, 3]], [[4, 5, 6, 7]]],
            [[[8, 9, 10, 11]]]
        ],
        "material": {"winter": {"values": [[0, 1], null]}}
    });
    assert_roundtrips(geometry, 12)?;

    // A whole null solid on a CompositeSolid.
    let geometry = json!({
        "type": "CompositeSolid",
        "lod": "1",
        "boundaries": [
            [[[[0, 1, 2]]]],
            [[[[3, 4, 5]]]]
        ],
        "material": {"winter": {"values": [[[0]], null]}}
    });
    assert_roundtrips(geometry, 6)?;
    Ok(())
}

/// An explicit `"values": null` is not the same as an absent `values`: the
/// schema requires exactly one of `value`/`values` but separately permits
/// `null`, so re-emitting the first as the second produces a document a
/// validator rejects.
#[test]
fn an_explicitly_null_material_values_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiSurface",
        "lod": "1",
        "boundaries": [[[0, 1, 2]]],
        "material": {"winter": {"values": null}}
    });
    let (orig, decoded, _, _) = roundtrip_geometry(geometry, 3)?;
    assert_eq!(
        serde_json::to_value(material_of(&orig))?,
        json!({"winter": {"values": null}}),
        "the input itself must keep its explicit null"
    );
    assert_eq!(
        serde_json::to_value(material_of(&decoded))?,
        serde_json::to_value(material_of(&orig))?
    );
    Ok(())
}

/// A texture is nullable only at the leaf: an untextured ring is `[null]`, not
/// `null`. The `[null]` spelling must survive at every depth.
#[test]
fn an_untextured_ring_roundtrips_as_a_null_leaf() -> Result<()> {
    let geometry = json!({
        "type": "Solid",
        "lod": "1",
        "boundaries": [[[[0, 1, 2]], [[3, 4, 5]]]],
        "texture": {"winter": {"values": [[[[0, 6, 7, 8]], [[null]]]]}}
    });
    assert_roundtrips(geometry, 6)?;
    Ok(())
}
