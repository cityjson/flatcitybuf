//! CityGML 2.0 input for `fcb ser`, end to end.
//!
//! These tests drive the same library path the `ser` subcommand does —
//! [`fcb_cli::reader::read_input_file`] and [`fcb_cli::merger::merge_files`] —
//! and then push the result through the `fcb_core` writer and reader, so a
//! `.gml` on the command line is proved to reach a `.fcb` and come back out
//! intact.
//!
//! The round-trip comparison is **modulo transform**. FCB stores quantised
//! vertices plus a transform, exactly as CityJSON does, but nothing promises
//! that the integers survive unchanged: the writer is free to renumber or
//! reorder the vertex list, and the header transform is its own. So both
//! sides are normalised before comparison — every boundary index is
//! dereferenced to a real-world coordinate through that side's own transform,
//! and the raw `vertices` array is dropped in favour of a sorted list of the
//! dequantised coordinates. Everything else (ids, types, attributes,
//! semantics, lod) is compared exactly.
//!
//! As it happens the current writer renumbers nothing: `fcb ser` on the
//! fixture followed by `fcb deser` reproduces `semantic_surfaces.expected.
//! city.jsonl` verbatim, transform and vertex integers included. The
//! normalisation is therefore slack the test does not currently need — which
//! is also why [`normalization_is_transform_independent`] exists, to prove the
//! normaliser is doing its stated job rather than silently comparing two
//! identical documents.

use fcb_cli::merger::merge_files;
use fcb_cli::reader::{read_input_file, InputFormat};
use fcb_core::{
    attribute::{AttributeSchema, AttributeSchemaMethods},
    header_writer::HeaderWriterOptions,
    FcbReader, FcbWriter,
};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The `fcb_citygml` fixture directory, resolved relative to this crate.
fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cli has a parent directory")
        .join("fcb_citygml/tests/fixtures")
}

/// A minimal single-building CityGML document, parameterised by id and by the
/// x offset of its one polygon.
fn minimal_citygml(gml_id: &str, x: f64) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:gml="http://www.opengis.net/gml">
  <core:cityObjectMember>
    <bldg:Building gml:id="{gml_id}">
      <bldg:lod0MultiSurface>
        <gml:MultiSurface srsName="EPSG:7415">
          <gml:surfaceMember>
            <gml:Polygon>
              <gml:exterior>
                <gml:LinearRing>
                  <gml:posList>{x} 2000 0 {x1} 2000 0 {x1} 2001 0 {x} 2001 0 {x} 2000 0</gml:posList>
                </gml:LinearRing>
              </gml:exterior>
            </gml:Polygon>
          </gml:surfaceMember>
        </gml:MultiSurface>
      </bldg:lod0MultiSurface>
    </bldg:Building>
  </core:cityObjectMember>
</core:CityModel>
"#,
        gml_id = gml_id,
        x = x,
        x1 = x + 1.0,
    )
}

/// A CityJSON `transform`, as the two sides of the comparison each carry one.
struct Transform {
    scale: [f64; 3],
    translate: [f64; 3],
}

impl Transform {
    fn from_json(value: &Value) -> Self {
        let axis = |key: &str| -> [f64; 3] {
            let arr = value[key].as_array().expect("transform component array");
            [
                arr[0].as_f64().expect("f64"),
                arr[1].as_f64().expect("f64"),
                arr[2].as_f64().expect("f64"),
            ]
        };
        Self {
            scale: axis("scale"),
            translate: axis("translate"),
        }
    }

    /// Dequantise one vertex to real-world coordinates, rendered to six
    /// decimals so that the comparison is exact at a 1e-6 tolerance without
    /// float equality.
    fn dequantize(&self, vertex: &[i64; 3]) -> Value {
        Value::Array(
            (0..3)
                .map(|i| {
                    let v = vertex[i] as f64 * self.scale[i] + self.translate[i];
                    Value::String(format!("{v:.6}"))
                })
                .collect(),
        )
    }
}

/// Read a feature's `vertices` array as integer triples.
fn vertices_of(feature: &Value) -> Vec<[i64; 3]> {
    feature["vertices"]
        .as_array()
        .expect("feature has vertices")
        .iter()
        .map(|v| {
            let a = v.as_array().expect("vertex is an array");
            [
                a[0].as_i64().expect("i64"),
                a[1].as_i64().expect("i64"),
                a[2].as_i64().expect("i64"),
            ]
        })
        .collect()
}

/// Replace every integer in a `boundaries` tree with the dequantised
/// coordinate it indexes.
fn deref_boundaries(node: &Value, vertices: &[[i64; 3]], transform: &Transform) -> Value {
    match node {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| deref_boundaries(item, vertices, transform))
                .collect(),
        ),
        Value::Number(n) => {
            let idx = n
                .as_u64()
                .expect("boundary index is a non-negative integer") as usize;
            transform.dequantize(&vertices[idx])
        }
        other => other.clone(),
    }
}

/// Normalise a `CityJSONFeature` for transform-independent comparison.
fn normalize(feature: &Value, transform: &Transform) -> Value {
    let vertices = vertices_of(feature);
    let mut out = feature.clone();

    if let Some(objects) = out.get_mut("CityObjects").and_then(|v| v.as_object_mut()) {
        for (_, co) in objects.iter_mut() {
            let Some(geometries) = co.get_mut("geometry").and_then(|g| g.as_array_mut()) else {
                continue;
            };
            for geometry in geometries.iter_mut() {
                if let Some(boundaries) = geometry.get_mut("boundaries") {
                    *boundaries = deref_boundaries(boundaries, &vertices, transform);
                }
            }
        }
    }

    // The raw vertex list is replaced by the sorted dequantised one: the
    // writer may renumber, but it may not lose or invent a coordinate.
    let mut dequantized: Vec<String> = vertices
        .iter()
        .map(|v| transform.dequantize(v).to_string())
        .collect();
    dequantized.sort();
    out["vertices"] = Value::Array(dequantized.into_iter().map(Value::String).collect());

    out
}

/// Write features to a `.fcb`, mirroring the writer setup in `serialize()`.
fn write_fcb(data: fcb_cli::reader::InputData, output: &Path) {
    let attr_schema = {
        let mut schema = AttributeSchema::new();
        for feature in data.features.iter() {
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
        (!schema.is_empty()).then_some(schema)
    };

    let semantic_attr_schema = {
        let mut schema = AttributeSchema::new();
        for feature in data.features.iter() {
            let mut ids: Vec<&String> = feature.city_objects.keys().collect();
            ids.sort_unstable();
            for co in ids
                .into_iter()
                .filter_map(|id| feature.city_objects.get(id))
            {
                let Some(geometry) = &co.geometry else {
                    continue;
                };
                for geom in geometry.iter() {
                    let Some(semantics) = geom.common().and_then(|c| c.semantics.as_ref()) else {
                        continue;
                    };
                    for sem_obj in semantics.surfaces.iter() {
                        if !sem_obj.other.is_empty() {
                            let other = Value::Object(sem_obj.other.clone().into_iter().collect());
                            schema.add_attributes(&other);
                        }
                    }
                }
            }
        }
        (!schema.is_empty()).then_some(schema)
    };

    let header_options = HeaderWriterOptions {
        write_index: true,
        feature_count: data.features.len() as u64,
        index_node_size: 16,
        attribute_indices: None,
        geographical_extent: None,
    };

    let mut fcb = FcbWriter::new(
        data.metadata,
        Some(header_options),
        attr_schema,
        semantic_attr_schema,
    )
    .expect("failed to create FCB writer");

    for feature in data.features.iter() {
        fcb.add_feature(feature).expect("failed to add feature");
    }

    let file = File::create(output).expect("failed to create output file");
    fcb.write(BufWriter::new(file))
        .expect("failed to write FCB");
}

/// Read a `.fcb` back as (metadata, features) JSON values.
fn read_fcb(path: &Path) -> (Value, Vec<Value>) {
    let file = File::open(path).expect("failed to open FCB file");
    let mut reader = FcbReader::open(BufReader::new(file))
        .expect("failed to read FCB header")
        .select_all_seq()
        .expect("failed to select all");

    let header = reader.header();
    let metadata =
        fcb_core::deserializer::to_cj_metadata(&header).expect("failed to decode header");
    let metadata = serde_json::to_value(&metadata).expect("metadata to JSON");

    let mut features = Vec::new();
    while let Some(buf) = reader.next().expect("failed to read feature") {
        let feature = buf.cur_cj_feature().expect("failed to decode feature");
        features.push(serde_json::to_value(&feature).expect("feature to JSON"));
    }

    (metadata, features)
}

#[test]
fn gml_extension_is_detected_as_citygml() {
    assert_eq!(
        InputFormat::from_path(Path::new("city.gml")).unwrap(),
        InputFormat::CityGML
    );
}

#[test]
fn xml_extension_is_detected_as_citygml() {
    assert_eq!(
        InputFormat::from_path(Path::new("city.xml")).unwrap(),
        InputFormat::CityGML
    );
}

/// The normaliser must equate two encodings of the same real-world geometry
/// under different transforms and vertex numbering, and must still separate
/// two encodings of different geometry.
#[test]
fn normalization_is_transform_independent() {
    let feature = |vertices: Value, boundaries: Value| -> Value {
        serde_json::json!({
            "type": "CityJSONFeature",
            "id": "b1",
            "CityObjects": {
                "b1": { "type": "Building", "geometry": [
                    { "type": "MultiSurface", "lod": "1", "boundaries": boundaries }
                ]}
            },
            "vertices": vertices,
        })
    };

    // (1000, 2000, 0) and (1000, 2001, 0), millimetre scale, no translation.
    let a = feature(
        serde_json::json!([[1_000_000, 2_000_000, 0], [1_000_000, 2_001_000, 0]]),
        serde_json::json!([[[0, 1]]]),
    );
    let ta = Transform {
        scale: [0.001, 0.001, 0.001],
        translate: [0.0, 0.0, 0.0],
    };

    // The same two points: centimetre scale, translated, and renumbered.
    let b = feature(
        serde_json::json!([[0, 100, 0], [0, 0, 0]]),
        serde_json::json!([[[1, 0]]]),
    );
    let tb = Transform {
        scale: [0.01, 0.01, 0.01],
        translate: [1000.0, 2000.0, 0.0],
    };

    assert_eq!(normalize(&a, &ta), normalize(&b, &tb));

    // A different coordinate must not normalise to the same thing.
    let c = feature(
        serde_json::json!([[0, 100, 0], [0, 0, 500]]),
        serde_json::json!([[[1, 0]]]),
    );
    assert_ne!(normalize(&a, &ta), normalize(&c, &tb));
}

#[test]
fn citygml_round_trips_through_fcb() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let gml_path = temp_dir.path().join("semantic_surfaces.gml");
    fs::copy(fixture_dir().join("semantic_surfaces.gml"), &gml_path)
        .expect("failed to copy fixture");

    let data = read_input_file(&gml_path).expect("failed to read CityGML");
    assert_eq!(data.features.len(), 1);

    let fcb_path = temp_dir.path().join("out.fcb");
    write_fcb(data, &fcb_path);

    let (actual_metadata, actual_features) = read_fcb(&fcb_path);
    assert_eq!(actual_features.len(), 1);
    let actual_transform = Transform::from_json(&actual_metadata["transform"]);

    let expected_text =
        fs::read_to_string(fixture_dir().join("semantic_surfaces.expected.city.jsonl"))
            .expect("failed to read expected JSONL");
    let mut lines = expected_text.lines();
    let expected_metadata: Value =
        serde_json::from_str(lines.next().expect("metadata line")).expect("metadata JSON");
    let expected_feature: Value =
        serde_json::from_str(lines.next().expect("feature line")).expect("feature JSON");
    let expected_transform = Transform::from_json(&expected_metadata["transform"]);

    assert_eq!(
        normalize(&actual_features[0], &actual_transform),
        normalize(&expected_feature, &expected_transform)
    );
}

#[test]
fn glob_of_two_citygml_files_merges() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    fs::write(
        temp_dir.path().join("a.gml"),
        minimal_citygml("bldg-a", 1000.0),
    )
    .expect("failed to write a.gml");
    fs::write(
        temp_dir.path().join("b.gml"),
        minimal_citygml("bldg-b", 2000.0),
    )
    .expect("failed to write b.gml");

    let pattern = temp_dir.path().join("*.gml");
    let mut paths: Vec<PathBuf> = glob::glob(pattern.to_str().expect("utf-8 path"))
        .expect("bad glob pattern")
        .filter_map(|e| e.ok())
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 2);

    let merged = merge_files(paths).expect("merge failed");
    assert_eq!(merged.features.len(), 2);

    let ids: Vec<String> = merged.features.iter().map(|f| f.id.clone()).collect();
    assert!(ids.contains(&"bldg-a".to_string()), "ids: {ids:?}");
    assert!(ids.contains(&"bldg-b".to_string()), "ids: {ids:?}");

    // And the merged result still writes a readable FCB.
    let fcb_path = temp_dir.path().join("merged.fcb");
    write_fcb(
        fcb_cli::reader::InputData {
            metadata: merged.metadata,
            features: merged.features,
        },
        &fcb_path,
    );
    let (_, features) = read_fcb(&fcb_path);
    assert_eq!(features.len(), 2);
}

#[test]
fn xml_extension_is_parsed_as_citygml() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let xml_path = temp_dir.path().join("city.xml");
    fs::write(&xml_path, minimal_citygml("bldg-x", 1000.0)).expect("failed to write city.xml");

    let data = read_input_file(&xml_path).expect("failed to read CityGML from .xml");
    assert_eq!(data.features.len(), 1);
    assert_eq!(data.features[0].id, "bldg-x");
}
