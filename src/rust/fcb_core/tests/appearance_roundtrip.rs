//! Round-trip tests for appearance (material/texture) mappings.
//!
//! These tests start from CityJSONSeq input, encode with the real writer
//! (`FcbWriter`), decode with the real reader (`FcbReader`), and compare the
//! decoded `material`/`texture` members of the geometry against the input.
//! They also dump the raw FlatBuffers mapping arrays (solids/shells/surfaces/
//! strings/vertices) that the encoder actually emitted, so each decoder quirk
//! can be classified as reachable-through-our-own-writer or decode-only.
//!
//! Investigated quirks (see `fcb_core/src/reader/geom_decoder.rs`):
//! 1. materials: `solids == [1]` (Solid with exactly one shell) falls into the
//!    MultiSolid decode branch and comes back one level deeper. REACHABLE.
//! 2. textures: a MultiLineString with a single string fails the
//!    `strings.len() > 1` guard and decodes one level deeper. REACHABLE.
//! 3. textures: `shells.len() > 1` with no solids would drop the shell
//!    grouping, but the encoder never emits that shape (multiple shell entries
//!    always come with a solids entry). DECODE-ONLY.
//! 4. textures: the MultiLineString branch iterates `surfaces[0]` times
//!    instead of `strings.len()`, but the encoder guarantees
//!    `surfaces == [strings.len()]` for that shape. DECODE-ONLY.

use anyhow::Result;
use cjseq::{CityJSONFeature, Geometry as CjGeometry};
use fcb_core::{
    attribute::AttributeSchema, deserializer, header_writer::HeaderWriterOptions,
    read_cityjson_from_reader, CJType, CJTypeKind, FcbReader, FcbWriter,
};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
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
        orig_geom.material, orig_geom.texture
    );
    println!(
        "decoded  material: {:?}\ndecoded  texture: {:?}",
        decoded_geom.material, decoded_geom.texture
    );

    Ok((orig_geom, decoded_geom, material_dumps, texture_dumps))
}

/// Regression: material values on a Solid with exactly one shell. The
/// encoder emits `solids == [1]`, which used to fail the decoder's
/// `solids[0] > 1` guard and take the MultiSolid branch, returning the
/// values one level deeper than the input. The commonest shape there is --
/// a building with only an exterior shell.
#[test]
fn material_solid_single_shell_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "Solid",
        "lod": "1",
        // One shell with two surfaces.
        "boundaries": [[[[0, 1, 2, 3]], [[4, 5, 6, 7]]]],
        // Solid material values: one array per shell, one index per surface.
        "material": {"winter": {"values": [[0, 1]]}}
    });
    let (orig, decoded, materials, _) = roundtrip_geometry(geometry, 8)?;

    // What the encoder actually emits for this shape.
    assert_eq!(materials.len(), 1);
    assert_eq!(materials[0].solids, vec![1]);
    assert_eq!(materials[0].shells, vec![2]);
    assert_eq!(materials[0].vertices, vec![0, 1]);

    assert_eq!(orig.material, decoded.material);
    let decoded_values = serde_json::to_value(&decoded.material)?;
    assert_eq!(decoded_values, json!({"winter": {"values": [[0, 1]]}}));
    Ok(())
}

/// Control for quirk 1: a Solid with two shells round-trips its material
/// values unchanged (`solids == [2]` passes the `solids[0] > 1` guard).
#[test]
fn material_solid_two_shells_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "Solid",
        "lod": "1",
        "boundaries": [
            [[[0, 1, 2, 3]], [[4, 5, 6, 7]]],
            [[[8, 9, 10, 11]]]
        ],
        "material": {"winter": {"values": [[0, 1], [2]]}}
    });
    let (orig, decoded, materials, _) = roundtrip_geometry(geometry, 12)?;

    assert_eq!(materials[0].solids, vec![2]);
    assert_eq!(materials[0].shells, vec![2, 1]);
    assert_eq!(orig.material, decoded.material);
    Ok(())
}

/// Regression: texture values on a MultiLineString
/// with a single string. The encoder emits `surfaces == [1]`,
/// `strings == [n]`; `strings.len() == 1` fails the decoder's
/// `strings.len() > 1` guard, so decoding falls through to the MultiSurface
/// branch and returns the values one level deeper than the input.
#[test]
fn texture_multilinestring_single_string_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiLineString",
        "lod": "1",
        "boundaries": [[0, 1, 2]],
        // One value array per string: [texture index, uv indices...].
        "texture": {"winter": {"values": [[0, 10, 11, 12]]}}
    });
    let (orig, decoded, _, textures) = roundtrip_geometry(geometry, 3)?;

    // What the encoder actually emits for this shape.
    assert_eq!(textures.len(), 1);
    assert_eq!(textures[0].solids, Vec::<u32>::new());
    assert_eq!(textures[0].shells, Vec::<u32>::new());
    assert_eq!(textures[0].surfaces, vec![1]);
    assert_eq!(textures[0].strings, vec![4]);
    assert_eq!(textures[0].vertices, vec![0, 10, 11, 12]);

    assert_eq!(orig.texture, decoded.texture);
    let decoded_values = serde_json::to_value(&decoded.texture)?;
    assert_eq!(decoded_values, json!({"winter": {"values": [[0, 10, 11, 12]]}}));
    Ok(())
}

/// Control for quirk 2: the encoder output for a MultiSurface with one
/// surface differs from a single-string MultiLineString only by the extra
/// `shells == [1]` entry, so the decoder could distinguish the two shapes;
/// this one round-trips correctly through the shell branch.
#[test]
fn texture_multisurface_single_surface_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiSurface",
        "lod": "1",
        "boundaries": [[[0, 1, 2]]],
        "texture": {"winter": {"values": [[[0, 10, 11, 12]]]}}
    });
    let (orig, decoded, _, textures) = roundtrip_geometry(geometry, 3)?;

    assert_eq!(textures[0].shells, vec![1]);
    assert_eq!(textures[0].surfaces, vec![1]);
    assert_eq!(textures[0].strings, vec![4]);
    assert_eq!(orig.texture, decoded.texture);
    Ok(())
}

/// Quirk 3 (DECODE-ONLY): the decoder's shell branch is guarded on
/// `shells.len() == 1`, so `shells.len() > 1` with no solids would fall to
/// the surfaces branch and lose the shell grouping. However, the encoder
/// pushes one `shells` entry per depth-3 node in the values tree, and more
/// than one depth-3 node requires a depth-4 parent, which always pushes a
/// `solids` entry. This test shows the closest real inputs: a Solid with two
/// shells emits `solids == [2]` alongside `shells == [1, 1]` (decoded by the
/// solids branch), and a MultiSurface with two surfaces emits a single
/// `shells == [2]` entry. Both round-trip correctly; `shells.len() > 1`
/// without solids is unreachable from our writer.
#[test]
fn texture_shell_shapes_always_carry_solids_or_single_shell_entry() -> Result<()> {
    // Solid, two shells, one surface each.
    let geometry = json!({
        "type": "Solid",
        "lod": "1",
        "boundaries": [
            [[[0, 1, 2, 3]]],
            [[[4, 5, 6, 7]]]
        ],
        "texture": {"winter": {"values": [[[[0, 10, 11, 12, 13]]], [[[1, 14, 15, 16, 17]]]]}}
    });
    let (orig, decoded, _, textures) = roundtrip_geometry(geometry, 8)?;

    // Two shell entries are always accompanied by a solids entry.
    assert_eq!(textures[0].solids, vec![2]);
    assert_eq!(textures[0].shells, vec![1, 1]);
    assert_eq!(textures[0].surfaces, vec![1, 1]);
    assert_eq!(orig.texture, decoded.texture);

    // MultiSurface, two surfaces: a single shells entry with value 2.
    let geometry = json!({
        "type": "MultiSurface",
        "lod": "1",
        "boundaries": [[[0, 1, 2]], [[3, 4, 5]]],
        "texture": {"winter": {"values": [[[0, 10, 11, 12]], [[0, 13, 14, 15]]]}}
    });
    let (orig, decoded, _, textures) = roundtrip_geometry(geometry, 6)?;

    assert_eq!(textures[0].solids, Vec::<u32>::new());
    assert_eq!(textures[0].shells, vec![2]); // len 1, passes the == 1 guard
    assert_eq!(textures[0].surfaces, vec![1, 1]);
    assert_eq!(orig.texture, decoded.texture);
    Ok(())
}

/// Quirk 4 (DECODE-ONLY): the decoder's MultiLineString branch iterates
/// `surfaces[0]` times instead of `strings.len()`, which would drop surplus
/// strings if `surfaces[0] < strings.len()`. However, for the only shape
/// reaching that branch (no solids/shells, depth-2 values tree) the encoder
/// pushes exactly one `surfaces` entry whose value is the number of leaf
/// strings, so `surfaces == [strings.len()]` always holds and nothing is
/// dropped.
#[test]
fn texture_multilinestring_multiple_strings_roundtrips() -> Result<()> {
    let geometry = json!({
        "type": "MultiLineString",
        "lod": "1",
        "boundaries": [[0, 1], [2, 3], [4, 5]],
        "texture": {"winter": {"values": [[0, 10, 11], [0, 12, 13], [0, 14, 15]]}}
    });
    let (orig, decoded, _, textures) = roundtrip_geometry(geometry, 6)?;

    // Encoder invariant that keeps the decoder's surfaces[0] loop safe.
    assert_eq!(textures[0].surfaces, vec![3]);
    assert_eq!(textures[0].strings, vec![3, 3, 3]);
    assert_eq!(textures[0].surfaces[0] as usize, textures[0].strings.len());
    assert_eq!(orig.texture, decoded.texture);
    Ok(())
}
