//! End-to-end fixtures: a CityGML document in, the CityJSONSeq lines it must
//! produce out.
//!
//! Each expected file is hand-written from the fixture, never generated from
//! this crate's own output — a converter cannot be its own oracle. Comparison
//! is whole-line, as `serde_json::Value`, so a member this converter invents
//! or drops fails the test rather than going unnoticed.

mod common;

#[test]
fn lod1_building() {
    common::assert_fixture("lod1_building");
}

#[test]
fn attributes() {
    common::assert_fixture("attributes");
}

#[test]
fn semantic_surfaces() {
    common::assert_fixture("semantic_surfaces");
}
