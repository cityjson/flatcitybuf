//! CityGML in and out of `.fcb`, through the `fcb` binary.
//!
//! The oracle for the first leg is the CityGML crates' own CityJSON for the
//! same document (`fcb_cli::citygml::read`): `fcb ser` then `fcb deser`
//! must give the same features back, whole, so FlatCityBuf loses nothing the
//! crates read. The second leg writes the `.fcb` as CityGML and reads that
//! back through `fcb ser`, which must give the same features again, unless
//! the writer reports leaving something out. An ADE document is read from
//! where it lies, so the schema it names by a relative path is found.
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

/// `feature` with every number and boolean under an `attributes` member
/// written as its JSON text. The CityGML writer names an ADE's schema by its
/// published URL, which the reader never fetches, so an ADE attribute typed
/// from the schema next to the source document comes back as text.
fn untyped_attributes(mut feature: Value) -> Value {
    fn as_text(v: &mut Value) {
        match v {
            Value::Number(_) | Value::Bool(_) => *v = Value::String(v.to_string()),
            Value::Array(items) => items.iter_mut().for_each(as_text),
            Value::Object(members) => members.values_mut().for_each(as_text),
            _ => {}
        }
    }
    fn walk(v: &mut Value) {
        match v {
            Value::Array(items) => items.iter_mut().for_each(walk),
            Value::Object(members) => {
                for (k, m) in members.iter_mut() {
                    if k == "attributes" {
                        as_text(m);
                    } else {
                        walk(m);
                    }
                }
            }
            _ => {}
        }
    }
    walk(&mut feature);
    feature
}

/// A CityJSONSeq file's header and its features by id.
fn read_seq(path: &Path) -> (Value, BTreeMap<String, Value>) {
    let mut lines = BufReader::new(File::open(path).unwrap()).lines();
    let header = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    let features = lines
        .map(|l| {
            let f: Value = serde_json::from_str(&l.unwrap()).unwrap();
            (f["id"].as_str().unwrap().to_string(), f)
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
            (f["id"].as_str().unwrap().to_string(), f)
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
    // The transform cityparquet picks for the same document, unless the
    // document has no envelope to pick it from.
    let shared = citygml::header(gml).unwrap();
    if shared
        .metadata
        .as_ref()
        .is_some_and(|m| m.geographical_extent.is_some())
    {
        assert_eq!(
            serde_json::to_value(&header.transform).unwrap(),
            serde_json::to_value(&shared.transform).unwrap(),
            "{}: transform differs from citygml::header's",
            gml.display()
        );
    }
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
        m.into_iter()
            .map(|(id, f)| (id, untyped_attributes(f)))
            .collect()
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

/// A document with no `gml:boundedBy` in metres: the transform cannot
/// start from an envelope, and coordinates in the millions must still fit a
/// 32-bit vertex.
#[test]
fn no_envelope() {
    round_trip(&fixtures().join("melbourne_road_no_envelope.v3.gml"), "3.0");
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
        let version = match citygml::sniff(&path) {
            Some(citygml::Version::V3_0) => "3.0",
            _ => "2.0",
        };
        eprintln!("{}", path.display());
        round_trip(&path, version);
    }
}
