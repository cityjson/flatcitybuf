//! Attributes: the CityGML properties that become CityJSON `attributes`.
//!
//! Two unrelated things end up in the same JSON object. A handful of *thematic*
//! properties — `bldg:measuredHeight`, `veg:species` — are declared by the
//! CityGML schema and carry a type with them; the *generic* attributes of the
//! generics module carry their name and their type in the instance document
//! instead. CityJSON has neither distinction: `attributes` is a flat object of
//! JSON values, so both become entries in it and a name collision between the
//! two is possible. It is resolved the way a document-order reader has to
//! resolve it — the last value written wins — and recorded as a warning.
//!
//! The types matter more than they look. A `gml:CodeType` such as
//! `bldg:roofType` holds `"1030"`, a code from a code list, and turning that
//! into the number `1030` would lose a leading zero and invite arithmetic on
//! something that is not a quantity. A year is an integer, so it must not come
//! out as `1985.0`. Getting either wrong produces a document that still
//! validates, which is why each is pinned by a test.

use serde_json::{Map, Number, Value};

use crate::gml::is_gml;
use crate::xml::XmlNode;
use crate::{is_in, ParseReport};

/// Local name of the GML property naming a feature, and the CityJSON
/// attribute it becomes — the same word on both sides.
const NAME: &str = "name";

/// Namespace URIs of the CityGML generic-attributes module, 2.0 and 1.0.
const GENERICS_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/generics/2.0",
    "http://www.opengis.net/citygml/generics/1.0",
];

/// Local name of the child holding a generic attribute's value.
const VALUE: &str = "value";

/// The prefix shared by every CityGML module namespace URI.
const CITYGML_NS_PREFIX: &str = "http://www.opengis.net/citygml/";

/// The CityGML versions whose module namespaces are accepted.
const CITYGML_VERSIONS: [&str; 2] = ["2.0", "1.0"];

/// How a property's text becomes a JSON value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Kept verbatim: `gml:CodeType`, `xs:date`, `xs:anyURI`, plain strings.
    Text,
    /// A JSON integer.
    Integer,
    /// A JSON floating-point number.
    Double,
}

/// The thematic properties this converter carries over, and the JSON type
/// each becomes.
///
/// The list spans several modules on purpose — `species` and `trunkDiameter`
/// belong to vegetation, `roofType` to building — because the table is keyed
/// by local name and no module defines another property by any of these
/// names. A reader for a further module therefore inherits its attributes
/// without touching this file.
const THEMATIC: [(&str, Kind); 16] = [
    ("class", Kind::Text),
    ("function", Kind::Text),
    ("usage", Kind::Text),
    ("roofType", Kind::Text),
    ("species", Kind::Text),
    ("yearOfConstruction", Kind::Integer),
    ("yearOfDemolition", Kind::Integer),
    ("storeysAboveGround", Kind::Integer),
    ("storeysBelowGround", Kind::Integer),
    ("storeyHeightsAboveGround", Kind::Double),
    ("storeyHeightsBelowGround", Kind::Double),
    ("measuredHeight", Kind::Double),
    ("averageHeight", Kind::Double),
    ("height", Kind::Double),
    ("trunkDiameter", Kind::Double),
    ("crownDiameter", Kind::Double),
];

/// The generic-attribute elements, and the JSON type each declares.
///
/// A `measureAttribute` also carries a `uom` on its value. CityJSON has
/// nowhere to put a unit of measure — an attribute is one JSON value — so the
/// number is kept and the unit dropped, exactly as for `measuredHeight`.
const GENERIC: [(&str, Kind); 6] = [
    ("stringAttribute", Kind::Text),
    ("intAttribute", Kind::Integer),
    ("doubleAttribute", Kind::Double),
    ("dateAttribute", Kind::Text),
    ("uriAttribute", Kind::Text),
    ("measureAttribute", Kind::Double),
];

/// Read the attributes of one thematic object into `out`.
///
/// Only the object's *direct* children are read: an attribute written on a
/// nested object — a building part, a boundary surface — belongs to that
/// object, and the reader that builds it calls this in its turn.
///
/// A property this converter has no mapping for is passed over in silence
/// rather than reported. Nearly every property of a real city object is such
/// a property, so a note apiece would bury the warnings that matter; what is
/// recorded in `report` is only what was *recognised* and still could not be
/// carried over — a number that will not parse, or a name written twice.
pub(crate) fn read_common_attributes(
    node: &XmlNode,
    out: &mut Map<String, Value>,
    report: &mut ParseReport,
) {
    for child in &node.children {
        if is_gml(child, NAME) {
            if let Some(text) = simple_text(child) {
                insert(out, NAME, Value::String(text.to_string()), report);
            }
        } else if let Some(kind) = generic_attribute_kind(child) {
            read_generic_attribute(child, kind, out, report);
        } else if is_citygml_module(&child.ns) {
            read_thematic_attribute(child, out, report);
        }
    }
}

/// Read one schema-declared property, if it is one this converter maps.
fn read_thematic_attribute(node: &XmlNode, out: &mut Map<String, Value>, report: &mut ParseReport) {
    let Some(kind) = kind_of(&THEMATIC, &node.local) else {
        return;
    };
    // An empty property states nothing, and a property with element children
    // is a structured one this converter does not read; neither is an error.
    let Some(text) = simple_text(node) else {
        return;
    };
    if let Some(value) = parse_value(text, &node.local, kind, report) {
        insert(out, &node.local, value, report);
    }
}

/// The type a node declares as a generic attribute, if that is what it is.
///
/// The namespace alone does not make one: the generics module also declares
/// the properties of a `gen:GenericCityObject` — `gen:class`, `gen:function`
/// — and those are thematic properties like any other module's, read by name
/// from [`THEMATIC`]. Treating everything in the namespace as a generic
/// attribute would drop them in silence.
fn generic_attribute_kind(node: &XmlNode) -> Option<Kind> {
    GENERICS_NS
        .contains(&node.ns.as_str())
        .then(|| kind_of(&GENERIC, &node.local))
        .flatten()
}

/// Read one `gen:…Attribute`, whose name and type are in the document rather
/// than in the schema.
fn read_generic_attribute(
    node: &XmlNode,
    kind: Kind,
    out: &mut Map<String, Value>,
    report: &mut ParseReport,
) {
    let Some(name) = node.attr(NAME).filter(|name| !name.is_empty()) else {
        report.warnings.push(format!(
            "<{}> has no {NAME}; the generic attribute is skipped",
            node.local
        ));
        return;
    };
    let Some(value) = node
        .children
        .iter()
        .find(|child| is_in(child, &GENERICS_NS, VALUE))
    else {
        report.warnings.push(format!(
            "generic attribute {name:?} has no <{VALUE}> in the generics namespace; \
             the attribute is skipped"
        ));
        return;
    };
    // Unlike a thematic property, an empty value is not silently dropped: the
    // element exists to give this name a value, so "" is what it says.
    if let Some(value) = parse_value(&value.text, name, kind, report) {
        insert(out, name, value, report);
    }
}

/// Turn an attribute's text into the JSON value its type calls for.
///
/// Returns `None`, with a warning, for text the type cannot hold: the
/// alternative is a plausible-looking wrong number, and a converter that
/// invents data is worse than one that drops it and says so.
fn parse_value(text: &str, name: &str, kind: Kind, report: &mut ParseReport) -> Option<Value> {
    let (number, expected) = match kind {
        Kind::Text => return Some(Value::String(text.to_string())),
        Kind::Integer => (text.parse::<i64>().ok().map(Number::from), "an integer"),
        // `Number::from_f64` rejects NaN and the infinities, which parse
        // happily out of "NaN" and "inf" but are not JSON numbers.
        Kind::Double => (
            text.parse::<f64>().ok().and_then(Number::from_f64),
            "a finite number",
        ),
    };
    match number {
        Some(number) => Some(Value::Number(number)),
        None => {
            report.warnings.push(format!(
                "attribute {name:?}: {text:?} is not {expected}; the attribute is skipped"
            ));
            None
        }
    }
}

/// Write one attribute, noting a name that was already taken.
///
/// Last wins. CityGML allows several `bldg:function`s on one building and
/// nothing stops a generic attribute from being named after a schema-declared
/// one; CityJSON `attributes` is a JSON object, so only one value survives,
/// and the reader records which.
fn insert(out: &mut Map<String, Value>, name: &str, value: Value, report: &mut ParseReport) {
    if out.insert(name.to_string(), value).is_some() {
        report.warnings.push(format!(
            "attribute {name:?} is written more than once; the last value wins"
        ));
    }
}

/// The text of an element that holds text and nothing else.
///
/// `None` for an element with child elements — a structured property, not an
/// attribute — and for an empty one, which states no value at all.
fn simple_text(node: &XmlNode) -> Option<&str> {
    (node.children.is_empty() && !node.text.is_empty()).then_some(node.text.as_str())
}

/// The JSON type a table gives a local name, if it lists it.
fn kind_of(table: &[(&str, Kind)], local: &str) -> Option<Kind> {
    table
        .iter()
        .find(|(name, _)| *name == local)
        .map(|(_, kind)| *kind)
}

/// Whether a namespace URI is that of a CityGML module this converter reads.
///
/// Every module is accepted rather than only the calling reader's own, so that
/// a further thematic reader needs no argument here and no second table. The
/// looseness costs nothing in practice: the names in [`THEMATIC`] are unique
/// across the CityGML modules, so a property that matches one is that property
/// whichever module namespace it was written in.
fn is_citygml_module(ns: &str) -> bool {
    let Some(rest) = ns.strip_prefix(CITYGML_NS_PREFIX) else {
        return false;
    };
    match rest.split_once('/') {
        // A thematic module: `…/citygml/building/2.0`.
        Some((module, version)) => !module.is_empty() && CITYGML_VERSIONS.contains(&version),
        // The core module, which carries no name of its own: `…/citygml/2.0`.
        None => CITYGML_VERSIONS.contains(&rest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    /// The namespaces every test fixture below binds.
    const NS: &str = r#"xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
         xmlns:gen="http://www.opengis.net/citygml/generics/2.0"
         xmlns:other="urn:example:other"
         xmlns:gml="http://www.opengis.net/gml""#;

    /// Read `children` as the properties of a `bldg:Building`.
    fn read(children: &str) -> (Map<String, Value>, ParseReport) {
        let node = crate::xml::parse_str_for_tests(&format!(
            "<bldg:Building {NS}>{children}</bldg:Building>"
        ))
        .unwrap();
        let mut out = Map::new();
        let mut report = ParseReport::default();
        read_common_attributes(&node, &mut out, &mut report);
        (out, report)
    }

    #[test]
    fn gml_name_becomes_the_name_attribute() {
        let (out, report) = read("<gml:name>Town Hall</gml:name>");
        assert_eq!(out["name"], json!("Town Hall"));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// `gml:CodeType` and its kin are codes, not numbers: `"1030"` must not
    /// become `1030`, or a code list that starts with a zero loses it.
    #[test]
    fn code_type_attributes_stay_strings() {
        let (out, report) = read(
            r#"<bldg:class>1000</bldg:class>
               <bldg:function>1000</bldg:function>
               <bldg:usage>1050</bldg:usage>
               <bldg:roofType>1030</bldg:roofType>
               <bldg:species>0850</bldg:species>"#,
        );
        assert_eq!(out["class"], json!("1000"));
        assert_eq!(out["function"], json!("1000"));
        assert_eq!(out["usage"], json!("1050"));
        assert_eq!(out["roofType"], json!("1030"));
        assert_eq!(out["species"], json!("0850"));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// Years and storey counts are integers in CityGML, so they are integers
    /// in the JSON too — `1985`, never `1985.0`.
    #[test]
    fn years_and_storey_counts_are_integers() {
        let (out, report) = read(
            r#"<bldg:yearOfConstruction>1985</bldg:yearOfConstruction>
               <bldg:yearOfDemolition>2019</bldg:yearOfDemolition>
               <bldg:storeysAboveGround>4</bldg:storeysAboveGround>
               <bldg:storeysBelowGround>1</bldg:storeysBelowGround>"#,
        );
        for (name, value) in [
            ("yearOfConstruction", 1985),
            ("yearOfDemolition", 2019),
            ("storeysAboveGround", 4),
            ("storeysBelowGround", 1),
        ] {
            assert_eq!(out[name], json!(value), "{name}");
            assert!(
                out[name].is_i64(),
                "{name} is not an integer: {}",
                out[name]
            );
        }
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// Every length-like measure is a JSON number, and the `uom` attribute
    /// that qualifies it in CityGML has nowhere to go in CityJSON.
    #[test]
    fn measures_become_numbers_and_their_uom_is_dropped() {
        let (out, report) = read(
            r#"<bldg:measuredHeight uom="m">9.5</bldg:measuredHeight>
               <bldg:averageHeight uom="m">8.25</bldg:averageHeight>
               <bldg:height uom="m">12.5</bldg:height>
               <bldg:trunkDiameter uom="m">0.45</bldg:trunkDiameter>
               <bldg:crownDiameter uom="m">3.75</bldg:crownDiameter>
               <bldg:storeyHeightsAboveGround uom="m">3.5</bldg:storeyHeightsAboveGround>
               <bldg:storeyHeightsBelowGround uom="m">2.5</bldg:storeyHeightsBelowGround>"#,
        );
        for (name, value) in [
            ("measuredHeight", 9.5),
            ("averageHeight", 8.25),
            ("height", 12.5),
            ("trunkDiameter", 0.45),
            ("crownDiameter", 3.75),
            ("storeyHeightsAboveGround", 3.5),
            ("storeyHeightsBelowGround", 2.5),
        ] {
            assert_eq!(out[name], json!(value), "{name}");
            assert!(out[name].is_f64(), "{name} is not a float: {}", out[name]);
        }
        assert!(out.get("uom").is_none());
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn every_generic_attribute_kind_maps_to_its_json_type() {
        let (out, report) = read(
            r#"<gen:stringAttribute name="owner"><gen:value>Acme</gen:value></gen:stringAttribute>
               <gen:intAttribute name="floorCount"><gen:value>7</gen:value></gen:intAttribute>
               <gen:doubleAttribute name="floorArea"><gen:value>842.5</gen:value></gen:doubleAttribute>
               <gen:dateAttribute name="surveyDate"><gen:value>2019-03-04</gen:value></gen:dateAttribute>
               <gen:uriAttribute name="reference"><gen:value>https://example.org/b1</gen:value></gen:uriAttribute>
               <gen:measureAttribute name="volume"><gen:value uom="m3">2530.75</gen:value></gen:measureAttribute>"#,
        );
        assert_eq!(out["owner"], json!("Acme"));
        assert_eq!(out["floorCount"], json!(7));
        assert!(out["floorCount"].is_i64());
        assert_eq!(out["floorArea"], json!(842.5));
        assert!(out["floorArea"].is_f64());
        // A date is text: CityJSON has no date type, and the string keeps the
        // source's own representation rather than reformatting it.
        assert_eq!(out["surveyDate"], json!("2019-03-04"));
        assert_eq!(out["reference"], json!("https://example.org/b1"));
        // The measure's value is a number; its unit of measure is dropped.
        assert_eq!(out["volume"], json!(2530.75));
        assert!(out["volume"].is_f64());
        assert_eq!(out.len(), 6);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn an_unparseable_number_is_skipped_with_a_warning() {
        for property in [
            r#"<bldg:measuredHeight uom="m">tall</bldg:measuredHeight>"#,
            "<bldg:yearOfConstruction>nineteen</bldg:yearOfConstruction>",
            // A year is an integer, so a fractional one is not a year.
            "<bldg:yearOfConstruction>1985.5</bldg:yearOfConstruction>",
            // "NaN" and "inf" parse as f64 but are not JSON numbers.
            "<bldg:measuredHeight>NaN</bldg:measuredHeight>",
            "<bldg:measuredHeight>inf</bldg:measuredHeight>",
            r#"<gen:intAttribute name="floorCount"><gen:value>many</gen:value></gen:intAttribute>"#,
            r#"<gen:doubleAttribute name="floorArea"><gen:value/></gen:doubleAttribute>"#,
        ] {
            let (out, report) = read(property);
            assert!(out.is_empty(), "{property}: {out:?}");
            assert_eq!(report.warnings.len(), 1, "{property}");
        }
    }

    #[test]
    fn a_duplicate_name_keeps_the_last_value_and_warns() {
        // Both a repeated thematic property and a generic attribute that
        // collides with one already written.
        let (out, report) = read(
            r#"<bldg:function>1000</bldg:function>
               <bldg:function>2000</bldg:function>
               <bldg:measuredHeight>9.5</bldg:measuredHeight>
               <gen:doubleAttribute name="measuredHeight"><gen:value>12.0</gen:value></gen:doubleAttribute>"#,
        );
        assert_eq!(out["function"], json!("2000"));
        assert_eq!(out["measuredHeight"], json!(12.0));
        assert_eq!(out.len(), 2);
        assert_eq!(report.warnings.len(), 2, "{:?}", report.warnings);
    }

    #[test]
    fn a_generic_attribute_without_a_name_or_a_value_is_skipped_with_a_warning() {
        for property in [
            "<gen:stringAttribute><gen:value>Acme</gen:value></gen:stringAttribute>",
            r#"<gen:stringAttribute name=""><gen:value>Acme</gen:value></gen:stringAttribute>"#,
            r#"<gen:stringAttribute name="owner"/>"#,
            // The value must be in the generics namespace to be the value.
            r#"<gen:stringAttribute name="owner"><other:value>Acme</other:value></gen:stringAttribute>"#,
        ] {
            let (out, report) = read(property);
            assert!(out.is_empty(), "{property}: {out:?}");
            assert_eq!(report.warnings.len(), 1, "{property}");
        }
    }

    /// Files written against CityGML 1.0 are still in circulation, and the
    /// two versions spell these properties identically.
    #[test]
    fn the_citygml_1_0_namespaces_are_read_too() {
        let node = crate::xml::parse_str_for_tests(
            r#"<bldg:Building xmlns:bldg="http://www.opengis.net/citygml/building/1.0"
                              xmlns:gen="http://www.opengis.net/citygml/generics/1.0">
                 <bldg:measuredHeight>9.5</bldg:measuredHeight>
                 <gen:stringAttribute name="owner"><gen:value>Acme</gen:value></gen:stringAttribute>
               </bldg:Building>"#,
        )
        .unwrap();
        let mut out = Map::new();
        let mut report = ParseReport::default();
        read_common_attributes(&node, &mut out, &mut report);
        assert_eq!(out["measuredHeight"], json!(9.5));
        assert_eq!(out["owner"], json!("Acme"));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// A local name alone never identifies a property: an application schema
    /// is free to define its own `measuredHeight`, and it is not this one.
    #[test]
    fn properties_outside_a_citygml_module_namespace_are_ignored() {
        let (out, report) = read(
            r#"<other:measuredHeight>9.5</other:measuredHeight>
               <other:function>1000</other:function>
               <measuredHeight>9.5</measuredHeight>"#,
        );
        assert!(out.is_empty(), "{out:?}");
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// The generics module declares the properties of a
    /// `gen:GenericCityObject` as well as the generic attributes, and those
    /// properties are schema-declared like any other module's. Reading
    /// everything in the namespace as a generic attribute would drop them:
    /// they carry no `name`, so they would look like attributes that had lost
    /// theirs.
    #[test]
    fn the_generics_modules_own_properties_are_thematic() {
        let (out, report) = read(
            r#"<gen:class>1000</gen:class>
               <gen:function>1010</gen:function>
               <gen:stringAttribute name="owner"><gen:value>Acme</gen:value></gen:stringAttribute>"#,
        );
        assert_eq!(out["class"], json!("1000"));
        assert_eq!(out["function"], json!("1010"));
        assert_eq!(out["owner"], json!("Acme"));
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// Anything this converter has no mapping for is passed over in silence:
    /// at this stage most properties of a real building are such a property,
    /// and a warning apiece would bury the ones that matter.
    #[test]
    fn unmapped_and_structured_properties_are_ignored() {
        let (out, report) = read(
            r#"<bldg:lod0MultiSurface><gml:MultiSurface/></bldg:lod0MultiSurface>
               <bldg:address><bldg:Address/></bldg:address>
               <bldg:function/>
               <bldg:measuredHeight/>
               <gen:genericAttributeSet name="set"/>
               <gml:description>A description is not an attribute.</gml:description>"#,
        );
        assert!(out.is_empty(), "{out:?}");
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }
}
