use fcb_citygml::{parse_citygml, CityGmlError, ParseOptions};
use std::io::BufReader;

#[test]
fn empty_city_model_parses_to_empty_document() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:gml="http://www.opengis.net/gml"/>"#;
    let (doc, report) =
        parse_citygml(BufReader::new(xml.as_bytes()), &ParseOptions::default()).unwrap();
    assert_eq!(doc.metadata.version, "2.0");
    assert!(doc.features.is_empty());
    assert!(report.skipped.is_empty());
}

#[test]
fn non_citymodel_root_is_unsupported_root() {
    let xml = r#"<foo xmlns="http://example.com"/>"#;
    let err = parse_citygml(BufReader::new(xml.as_bytes()), &ParseOptions::default()).unwrap_err();
    assert!(matches!(err, CityGmlError::UnsupportedRoot(_)));
}

#[test]
fn malformed_xml_is_xml_error_not_panic() {
    let xml = r#"<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"><unclosed"#;
    let err = parse_citygml(BufReader::new(xml.as_bytes()), &ParseOptions::default()).unwrap_err();
    assert!(matches!(err, CityGmlError::Xml { .. }));
}
