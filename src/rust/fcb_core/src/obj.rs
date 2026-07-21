//! A minimal CityJSON-to-OBJ writer.
//!
//! Ported from `cjseq2` 0.1.1's `conv::obj`, which the typed rewrite dropped.
//! The behaviour is unchanged, including its crude notion of a face: every ring
//! becomes an `f` line, interior rings included, and nothing is triangulated.
//!
//! The one thing that did change is how the rings are found. The old version
//! recursed through an untyped `Boundaries` tree until it hit a leaf; here each
//! geometry's ring iterator comes from its variant, so the depth is never
//! guessed at.

use cjseq::{CityJSON, Geometry, Ring};
use std::io::{Result as IoResult, Write};

/// Converts a CityJSON object to OBJ format and returns it as a string.
pub fn to_obj_string(city_json: &CityJSON) -> String {
    let mut output = Vec::new();
    // Writing into a `Vec<u8>` cannot fail, and the bytes are the ASCII we
    // just wrote, so neither unwrap can fire.
    to_obj(city_json, &mut output).expect("writing to a Vec cannot fail");
    String::from_utf8(output).expect("OBJ output is ASCII")
}

/// Writes a CityJSON object as OBJ to `writer`.
pub fn to_obj<W: Write>(city_json: &CityJSON, writer: &mut W) -> IoResult<()> {
    writeln!(writer, "# Converted from CityJSON to OBJ")?;
    writeln!(writer, "# by CJSeq converter")?;
    writeln!(writer)?;

    // Vertices are stored as integers plus a transform; OBJ wants the real
    // coordinates.
    let scale = &city_json.transform.scale;
    let translate = &city_json.transform.translate;

    for vertex in &city_json.vertices {
        let x = (vertex[0] as f64 * scale[0]) + translate[0];
        let y = (vertex[1] as f64 * scale[1]) + translate[1];
        let z = (vertex[2] as f64 * scale[2]) + translate[2];
        writeln!(writer, "v {x} {y} {z}")?;
    }

    writeln!(writer)?;

    for city_object in city_json.city_objects.values() {
        if let Some(geometries) = &city_object.geometry {
            for geometry in find_highest_lod_geometry(geometries) {
                for ring in rings(geometry) {
                    write_obj_face(ring, writer)?;
                }
            }
        }
    }

    Ok(())
}

/// Every ring of a geometry, in document order. The nesting depth comes from
/// the variant, so no runtime inspection of the boundaries is needed.
fn rings(geometry: &Geometry) -> Box<dyn Iterator<Item = &Ring> + '_> {
    match geometry {
        Geometry::MultiPoint { boundaries, .. } | Geometry::GeometryInstance { boundaries, .. } => {
            Box::new(std::iter::once(boundaries))
        }
        Geometry::MultiLineString { boundaries, .. } => Box::new(boundaries.iter()),
        Geometry::MultiSurface { boundaries, .. }
        | Geometry::CompositeSurface { boundaries, .. } => Box::new(boundaries.iter().flatten()),
        Geometry::Solid { boundaries, .. } => Box::new(boundaries.iter().flatten().flatten()),
        Geometry::MultiSolid { boundaries, .. } | Geometry::CompositeSolid { boundaries, .. } => {
            Box::new(boundaries.iter().flatten().flatten().flatten())
        }
    }
}

/// The geometries with the highest numeric lod, or all of them when no
/// geometry has an lod that parses as a number.
fn find_highest_lod_geometry(geometries: &[Geometry]) -> Vec<&Geometry> {
    let lod_of = |g: &Geometry| g.lod().and_then(|l| l.parse::<f64>().ok());

    let Some(max_lod) = geometries
        .iter()
        .filter_map(lod_of)
        .fold(None, |max: Option<f64>, lod| {
            Some(max.map_or(lod, |m| m.max(lod)))
        })
    else {
        return geometries.iter().collect();
    };

    geometries
        .iter()
        .filter(|g| lod_of(g).is_some_and(|lod| (lod - max_lod).abs() < f64::EPSILON))
        .collect()
}

/// Writes one ring as an OBJ face. OBJ indices are 1-based, CityJSON's 0-based.
fn write_obj_face<W: Write>(indices: &[usize], writer: &mut W) -> IoResult<()> {
    if indices.is_empty() {
        return Ok(());
    }

    write!(writer, "f")?;
    for idx in indices {
        write!(writer, " {}", idx + 1)?;
    }
    writeln!(writer)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn city_json(geometry: serde_json::Value) -> CityJSON {
        let doc = json!({
            "type": "CityJSON",
            "version": "2.0",
            "transform": {"scale": [0.001, 0.001, 0.001], "translate": [1.0, 2.0, 3.0]},
            "CityObjects": {"co-1": {"type": "Building", "geometry": [geometry]}},
            "vertices": [[0, 0, 0], [1000, 0, 0], [1000, 1000, 0]]
        });
        serde_json::from_value(doc).expect("test document must parse")
    }

    #[test]
    fn vertices_are_written_in_real_world_coordinates() {
        let obj = to_obj_string(&city_json(
            json!({"type": "MultiSurface", "lod": "2", "boundaries": [[[0, 1, 2]]]}),
        ));
        assert!(obj.contains("v 1 2 3"), "{obj}");
        assert!(obj.contains("v 2 2 3"), "{obj}");
        assert!(obj.contains("v 2 3 3"), "{obj}");
    }

    /// Each geometry type must reach its own rings; the depth comes from the
    /// variant, which is the whole point of the rewrite.
    #[test]
    fn every_geometry_type_yields_its_rings_as_faces() {
        for (geometry, expected) in [
            (
                json!({"type": "MultiLineString", "lod": "1", "boundaries": [[0, 1, 2]]}),
                "f 1 2 3",
            ),
            (
                json!({"type": "MultiSurface", "lod": "1", "boundaries": [[[0, 1, 2]]]}),
                "f 1 2 3",
            ),
            (
                json!({"type": "Solid", "lod": "1", "boundaries": [[[[0, 1, 2]]]]}),
                "f 1 2 3",
            ),
            (
                json!({"type": "CompositeSolid", "lod": "1", "boundaries": [[[[[0, 1, 2]]]]]}),
                "f 1 2 3",
            ),
        ] {
            let obj = to_obj_string(&city_json(geometry));
            assert!(obj.contains(expected), "{obj}");
        }
    }

    #[test]
    fn only_the_highest_lod_is_written() {
        let doc = json!({
            "type": "CityJSON",
            "version": "2.0",
            "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]},
            "CityObjects": {"co-1": {"type": "Building", "geometry": [
                {"type": "MultiSurface", "lod": "1", "boundaries": [[[0, 1, 2]]]},
                {"type": "MultiSurface", "lod": "2", "boundaries": [[[2, 1, 0]]]}
            ]}},
            "vertices": [[0, 0, 0], [1, 0, 0], [1, 1, 0]]
        });
        let cj: CityJSON = serde_json::from_value(doc).expect("document must parse");
        let obj = to_obj_string(&cj);
        assert!(obj.contains("f 3 2 1"), "{obj}");
        assert!(!obj.contains("f 1 2 3"), "{obj}");
    }
}
