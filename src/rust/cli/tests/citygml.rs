//! CityGML in and out of `.fcb`, through the `fcb` binary.
//!
//! The oracle for the first leg is the CityGML crates' own CityJSON for the
//! same document (`fcb_cli::citygml::read`): `fcb ser` then `fcb deser`
//! must give the same features back, whole, so FlatCityBuf loses nothing the
//! crates read. The second leg writes the `.fcb` as CityGML and reads that
//! back through `fcb ser`, which must give the same features again, unless
//! the writer reports leaving something out.
//!
//! The committed fixtures are small real documents. `FCB_CITYGML_FIXTURES`
//! names a directory of further `.gml` files (the PLATEAU set the CityGML
//! crates test with) to run the same checks on.
#![cfg(feature = "citygml")]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/citygml")
}

/// Run `fcb` with `args`, failing the test with its stderr if it fails, and
/// return its stdout and stderr.
fn fcb(args: &[&Path]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_fcb"))
        .args(args)
        .output()
        .expect("run fcb");
    assert!(
        output.status.success(),
        "fcb {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
}

/// `feature` without an empty `geometry` array, which FlatCityBuf reads back
/// absent.
fn without_empty_geometry(mut feature: Value) -> Value {
    if let Some(objects) = feature["CityObjects"].as_object_mut() {
        for object in objects.values_mut() {
            let o = object.as_object_mut().unwrap();
            if o.get("geometry")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                o.remove("geometry");
            }
        }
    }
    feature
}

/// `feature` with each geometry's material and texture indices replaced by
/// the material, and the texture and UV coordinates, they index, and its
/// appearance pools dropped: a CityGML document has no pools, so reading one
/// back may number the same appearance differently.
fn dereferenced(mut feature: Value) -> Value {
    let appearance = feature
        .as_object_mut()
        .and_then(|f| f.remove("appearance"))
        .unwrap_or(Value::Null);
    let materials = appearance["materials"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let textures = appearance["textures"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let uvs = appearance["vertices-texture"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let at = |pool: &[Value], i: &Value| {
        i.as_u64()
            .and_then(|i| pool.get(i as usize).cloned())
            .unwrap_or(Value::Null)
    };
    if let Some(objects) = feature["CityObjects"].as_object_mut() {
        for object in objects.values_mut() {
            let o = object.as_object_mut().unwrap();
            for geometry in o
                .get_mut("geometry")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for theme in geometry
                    .get_mut("material")
                    .and_then(Value::as_object_mut)
                    .into_iter()
                    .flat_map(|m| m.values_mut())
                {
                    if let Some(v) = theme.get_mut("value") {
                        *v = at(&materials, v);
                    }
                    if let Some(v) = theme.get_mut("values") {
                        map_leaves(v, &mut |i| *i = at(&materials, i));
                    }
                }
                for theme in geometry
                    .get_mut("texture")
                    .and_then(Value::as_object_mut)
                    .into_iter()
                    .flat_map(|t| t.values_mut())
                {
                    if let Some(v) = theme.get_mut("values") {
                        map_rings(v, &mut |ring| {
                            for (k, i) in ring.iter_mut().enumerate() {
                                *i = if k == 0 {
                                    at(&textures, i)
                                } else {
                                    at(&uvs, i)
                                };
                            }
                        });
                    }
                }
            }
        }
    }
    feature
}

/// Apply `f` to every number or null in `v`'s nested arrays.
fn map_leaves(v: &mut Value, f: &mut impl FnMut(&mut Value)) {
    match v {
        Value::Array(items) => items.iter_mut().for_each(|i| map_leaves(i, f)),
        leaf => f(leaf),
    }
}

/// Apply `f` to every innermost array of `v`: a texture ring, its texture
/// index then its UV indices.
fn map_rings(v: &mut Value, f: &mut impl FnMut(&mut Vec<Value>)) {
    if let Value::Array(items) = v {
        if items.first().is_some_and(|i| !i.is_array()) {
            f(items);
        } else {
            items.iter_mut().for_each(|i| map_rings(i, f));
        }
    }
}

/// A CityJSONSeq file's header and its features by id.
fn read_seq(path: &Path) -> (Value, BTreeMap<String, Value>) {
    let mut lines = BufReader::new(File::open(path).unwrap()).lines();
    let header = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    let features = lines
        .map(|l| {
            let f: Value = serde_json::from_str(&l.unwrap()).unwrap();
            (
                f["id"].as_str().unwrap().to_string(),
                without_empty_geometry(f),
            )
        })
        .collect();
    (header, features)
}

/// The crates' own features for `gml`, by id.
fn oracle(gml: &Path) -> (cjseq::CityJSON, BTreeMap<String, Value>) {
    let input = fcb_cli::citygml::read(&[gml.to_path_buf()]).unwrap();
    let features = input
        .features
        .iter()
        .unwrap()
        .map(|f| {
            let f = serde_json::to_value(f.unwrap()).unwrap();
            (
                f["id"].as_str().unwrap().to_string(),
                without_empty_geometry(f),
            )
        })
        .collect();
    (input.header, features)
}

/// The first id whose feature differs between `expected` and `actual`.
fn first_difference(
    expected: &BTreeMap<String, Value>,
    actual: &BTreeMap<String, Value>,
) -> Option<String> {
    expected
        .keys()
        .chain(actual.keys())
        .find(|id| expected.get(*id) != actual.get(*id))
        .cloned()
}

fn assert_same_features(
    what: &str,
    expected: &BTreeMap<String, Value>,
    actual: &BTreeMap<String, Value>,
) {
    if let Some(id) = first_difference(expected, actual) {
        panic!(
            "{what}: feature {id} differs\nexpected {}\nactual   {}",
            expected.get(&id).map_or("none".into(), |v| v.to_string()),
            actual.get(&id).map_or("none".into(), |v| v.to_string()),
        );
    }
}

/// `gml` → `.fcb` → CityJSONSeq equals the crates' features; then `.fcb` →
/// CityGML `version` → `.fcb` → CityJSONSeq equals them again.
fn round_trip(gml: &Path, version: &str) {
    let dir = TempDir::new().unwrap();
    let fcb_path = dir.path().join("a.fcb");
    let seq = dir.path().join("a.city.jsonl");
    fcb(&[Path::new("ser"), gml, &fcb_path]);
    fcb(&[Path::new("deser"), &fcb_path, &seq]);

    let (header, expected) = oracle(gml);
    let (seq_header, actual) = read_seq(&seq);
    assert_eq!(
        seq_header["transform"],
        serde_json::to_value(&header.transform).unwrap(),
        "{}: transform",
        gml.display()
    );
    assert!(!expected.is_empty(), "{}: no features", gml.display());
    assert_same_features(&format!("{} → fcb", gml.display()), &expected, &actual);

    let gml_out = dir.path().join("b.gml");
    let fcb_again = dir.path().join("b.fcb");
    let seq_again = dir.path().join("b.city.jsonl");
    let written = fcb(&[
        Path::new("deser"),
        &fcb_path,
        &gml_out,
        Path::new("--citygml-version"),
        Path::new(version),
    ]);
    // What the CityGML writer reports leaving out cannot come back; the
    // first leg already holds FlatCityBuf to the whole document.
    if written.contains("left out:") {
        eprintln!(
            "{}: CityGML {version} writer left out:\n{written}",
            gml.display()
        );
        return;
    }
    fcb(&[Path::new("ser"), &gml_out, &fcb_again]);
    fcb(&[Path::new("deser"), &fcb_again, &seq_again]);
    let (_, again) = read_seq(&seq_again);
    let deref = |m: BTreeMap<String, Value>| -> BTreeMap<String, Value> {
        m.into_iter().map(|(id, f)| (id, dereferenced(f))).collect()
    };
    assert_same_features(
        &format!("{} → fcb → CityGML {version} → fcb", gml.display()),
        &deref(actual),
        &deref(again),
    );
}

#[test]
fn plateau_bridge_citygml2() {
    round_trip(&fixtures().join("53394610_brid_6697_op.gml"), "2.0");
}

#[test]
fn plateau_bridge_citygml3() {
    round_trip(&fixtures().join("53394610_brid_6697_op.v3.gml"), "3.0");
}

#[test]
fn implicit_geometries() {
    round_trip(&fixtures().join("railway_trees_fragment.gml"), "2.0");
}

/// Every `.gml` in `FCB_CITYGML_FIXTURES`, written back as the version it
/// was read as.
#[test]
fn fixtures_from_environment() {
    let Ok(dir) = std::env::var("FCB_CITYGML_FIXTURES") else {
        return;
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "gml"))
        .collect();
    paths.sort();
    for path in paths {
        let version = match citygml_core::sniff(&path) {
            Some(citygml_core::Version::V3_0) => "3.0",
            _ => "2.0",
        };
        eprintln!("{}", path.display());
        round_trip(&path, version);
    }
}
