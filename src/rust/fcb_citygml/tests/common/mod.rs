use fcb_citygml::{parse_citygml, ParseOptions};

/// Parse tests/fixtures/<name>.gml and compare, as serde_json::Value, the
/// metadata line + each feature line against tests/fixtures/<name>.expected.city.jsonl.
/// Whole-line equality; Value == ignores object key order but not array order.
pub fn assert_fixture(name: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let gml = std::fs::File::open(dir.join(format!("{name}.gml"))).unwrap();
    let (doc, _report) =
        parse_citygml(std::io::BufReader::new(gml), &ParseOptions::default()).unwrap();
    let expected_raw =
        std::fs::read_to_string(dir.join(format!("{name}.expected.city.jsonl"))).unwrap();
    let mut expected = expected_raw.lines().filter(|l| !l.trim().is_empty());

    let meta_actual: serde_json::Value = serde_json::to_value(&doc.metadata).unwrap();
    let meta_expected: serde_json::Value =
        serde_json::from_str(expected.next().expect("expected metadata line")).unwrap();
    pretty_assertions::assert_eq!(
        meta_expected,
        meta_actual,
        "metadata line differs for {}",
        name
    );

    for (i, feat) in doc.features.iter().enumerate() {
        let actual: serde_json::Value = serde_json::to_value(feat).unwrap();
        let exp_line = expected
            .next()
            .unwrap_or_else(|| panic!("missing expected feature line {i}"));
        let exp: serde_json::Value = serde_json::from_str(exp_line).unwrap();
        pretty_assertions::assert_eq!(exp, actual, "feature {} differs for {}", i, name);
    }
    assert!(expected.next().is_none(), "extra expected lines for {name}");
}
