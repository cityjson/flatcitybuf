use std::path::Path;

use fcb_citygml::{parse_citygml, ParseOptions, ParseReport};

/// Parse tests/fixtures/<name>.gml and compare, as serde_json::Value, the
/// metadata line + each feature line against tests/fixtures/<name>.expected.city.jsonl.
/// Whole-line equality; Value == ignores object key order but not array order.
///
/// The [`ParseReport`] is checked too, against
/// tests/fixtures/<name>.expected.report.txt — see [`assert_report`]. What a
/// conversion *drops* is as much of its output as what it writes, and a
/// fixture that started reporting a skip nobody asked for would otherwise
/// pass.
pub fn assert_fixture(name: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let gml = std::fs::File::open(dir.join(format!("{name}.gml"))).unwrap();
    let (doc, report) =
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

    assert_report(&report, &dir, name);
}

/// Hold a fixture's diagnostics against `<name>.expected.report.txt`.
///
/// One line per entry, in the order the converter reports them:
/// `skip <element>|<reason>` for a skipped element and `warn <text>` for a
/// warning. Blank lines and `#` comments are ignored, so the file can say why
/// each entry is there.
///
/// The file is optional, and its absence is an expectation of its own: a
/// fixture without one must report *nothing at all*. Most of them do report
/// nothing, and writing an empty file each time would say less than the rule
/// does.
fn assert_report(report: &ParseReport, dir: &Path, name: &str) {
    let actual: Vec<String> = report
        .skipped
        .iter()
        .map(|skipped| format!("skip {}|{}", skipped.element, skipped.reason))
        .chain(
            report
                .warnings
                .iter()
                .map(|warning| format!("warn {warning}")),
        )
        .collect();

    let path = dir.join(format!("{name}.expected.report.txt"));
    let expected: Vec<String> = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect();

    pretty_assertions::assert_eq!(
        expected,
        actual,
        "the parse report differs for {} (expected file: {})",
        name,
        path.display()
    );
}
