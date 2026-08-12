//! The thematic module readers: one CityGML city object in, one
//! [`IntermediateObject`] out.
//!
//! Every CityGML module — building, vegetation, transportation — has its own
//! namespace and its own element names, but they share a shape: a feature
//! with attributes, geometry properties named after the level of detail they
//! carry, and nested objects. This module owns the dispatch from a
//! `cityObjectMember` to the reader that knows the module in question, and
//! the parts of that shape none of them define differently: the scan for
//! `lodX…` geometry properties lives here rather than in any one reader.
//!
//! [`building`] reads the building family, which is the only one with nested
//! city objects; [`simple`] reads the modules whose objects are attributes,
//! geometry and — for three of them — thematic surfaces.
//!
//! Namespaces are matched against both the CityGML 2.0 and the 1.0 URI of
//! each module: the two differ only in ways this converter does not read, and
//! files in the wild are still written against 1.0.

mod attributes;
pub(crate) mod building;
mod semantics;
mod simple;

use crate::gml::{parse_geometry, GmlGeometry, XlinkRegistry};
use crate::model::{IntermediateGeometry, IntermediateObject};
use crate::xml::XmlNode;
use crate::{CityGmlError, ParseReport, Skipped};

/// Reason recorded for a city object no module reader recognises.
const UNSUPPORTED: &str = "unsupported CityObject";

/// The prefix shared by every CityGML module namespace URI.
const CITYGML_NS_PREFIX: &str = "http://www.opengis.net/citygml/";

/// The CityGML versions whose module namespaces are accepted.
const CITYGML_VERSIONS: [&str; 2] = ["2.0", "1.0"];

/// A geometry property is `lod` + a digit + one of these, e.g. `lod2Solid`.
/// `lodXGeometry` is the property that may hold any geometry at all, so they
/// are treated alike: whichever supported GML geometry the property holds is
/// the one taken. A `lod2Solid` holding a `MultiSurface` is not valid
/// CityGML, but it is unambiguous, and rejecting it would lose geometry over a
/// mislabelled property.
const GEOMETRY_SUFFIXES: [&str; 4] = ["Solid", "MultiSolid", "MultiSurface", "Geometry"];

/// The prefix every geometry property name starts with.
const LOD_PREFIX: &str = "lod";

/// The highest level of detail CityGML 2.0 defines.
const HIGHEST_LOD: u8 = 4;

/// Read one `cityObjectMember` into the intermediate model.
///
/// `member` is the *property* element — the `core:cityObjectMember` — not the
/// city object inside it, because the xlink registry has to index the whole
/// property subtree: the standard CityGML pattern writes a boundary surface's
/// polygons under the object and points at them from its solid, and both
/// sides of that reference must be in scope. Collecting the registry here,
/// rather than in the caller, keeps that invariant with the code that depends
/// on it.
///
/// `member_index` is the position of this member among the document's
/// members, and is used only to name an object whose `gml:id` is missing.
///
/// Returns `Ok(None)` for a member this converter has no reader for, having
/// recorded it in `report` — an unsupported city object is content that is
/// valid CityGML, so it is skipped rather than fatal.
///
/// # Errors
///
/// Propagates whatever the module reader raises: malformed geometry, and
/// `xlink:href`s that name nothing in the member.
pub(crate) fn read_member(
    member: &XmlNode,
    member_index: usize,
    report: &mut ParseReport,
) -> Result<Option<IntermediateObject>, CityGmlError> {
    // A `cityObjectMember` holds exactly one city object. An empty one, or
    // one that only references an object elsewhere by `xlink:href`, holds
    // nothing this converter can read.
    let Some(object) = member.children.first() else {
        report.skipped.push(Skipped {
            element: member.local.clone(),
            gml_id: member.gml_id().map(str::to_owned),
            reason: format!("{UNSUPPORTED}: the member holds no city object"),
        });
        return Ok(None);
    };

    if building::is_building(object) {
        let registry = XlinkRegistry::collect(member);
        return building::read_building(object, &registry, member_index, report).map(Some);
    }

    if let Some(kind) = simple::kind_of(object) {
        let registry = XlinkRegistry::collect(member);
        return simple::read_simple_object(object, kind, &registry, member_index, report).map(Some);
    }

    report.skipped.push(Skipped {
        element: object.local.clone(),
        gml_id: object.gml_id().map(str::to_owned),
        reason: format!("{UNSUPPORTED}: <{}> has no reader", object.local),
    });
    Ok(None)
}

/// The id of a top-level city object: its `gml:id`, or a stand-in built from
/// its member's position in the document.
///
/// The generated id is stable for a given document, which matters because it
/// ends up as a CityJSON object key.
fn member_object_id(node: &XmlNode, member_index: usize) -> String {
    node.gml_id()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("citygml-obj-{member_index}"))
}

/// Read every geometry property of an object, in document order.
///
/// The scan is by property *name* — `lod` + a digit + a geometry word — and
/// not by module, because every CityGML module names its geometry properties
/// that way and none of them means anything else by such a name. A reader for
/// a further module therefore inherits this without an argument or a table of
/// its own.
///
/// A property that holds nothing this converter can read is recorded and
/// passed over: the rest of the object is still worth having.
///
/// # Errors
///
/// Propagates the geometry reader's errors: malformed geometry, and
/// `xlink:href`s that name nothing in the member.
fn read_lod_geometries(
    node: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Vec<IntermediateGeometry>, CityGmlError> {
    let mut geometries = Vec::new();
    for property in &node.children {
        let Some(lod) = lod_of(property) else {
            continue;
        };
        match read_geometry_property(property, registry, report)? {
            Some(geometry) => geometries.push(IntermediateGeometry {
                lod: lod.to_string(),
                geometry,
                surfaces: Vec::new(),
            }),
            None => report.skipped.push(Skipped {
                element: property.local.clone(),
                gml_id: property.gml_id().map(str::to_owned),
                reason: format!("no supported GML geometry in <{}>", property.local),
            }),
        }
    }
    Ok(geometries)
}

/// The geometry inside one `lodX…` property, if it holds one this converter
/// can read.
fn read_geometry_property(
    property: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Option<GmlGeometry>, CityGmlError> {
    for child in &property.children {
        if let Some(geometry) = parse_geometry(child, registry, report)? {
            return Ok(Some(geometry));
        }
    }
    Ok(None)
}

/// The level of detail a geometry property name carries, e.g. `"2"` for
/// `lod2Solid`.
///
/// Returns `None` for any other element, including one with a matching name
/// outside a CityGML module namespace.
fn lod_of(property: &XmlNode) -> Option<&str> {
    if !is_citygml_module(&property.ns) {
        return None;
    }
    let rest = property.local.strip_prefix(LOD_PREFIX)?;
    let digit = *rest.as_bytes().first()?;
    if !(b'0'..=b'0' + HIGHEST_LOD).contains(&digit) {
        return None;
    }
    // Splitting after an ASCII digit cannot land inside a character.
    let (lod, suffix) = rest.split_at(1);
    GEOMETRY_SUFFIXES.contains(&suffix).then_some(lod)
}

/// Whether a namespace URI is that of a CityGML module this converter reads.
///
/// Both callers — the attribute reader and [`lod_of`] — accept every module
/// rather than only the calling reader's own, so that a further thematic
/// reader needs no argument here and no second table. The looseness costs
/// nothing in practice: no two modules give the same name to properties of
/// different kinds, so a property that matches one of this converter's tables
/// is that property whichever module namespace it was written in.
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

    fn node(xml: &str) -> XmlNode {
        crate::xml::parse_str_for_tests(xml).unwrap()
    }

    /// An element of the building module with the given local name.
    fn bldg(local: &str) -> XmlNode {
        node(&format!(
            r#"<bldg:{local} xmlns:bldg="http://www.opengis.net/citygml/building/2.0"/>"#
        ))
    }

    #[test]
    fn geometry_property_names_yield_their_lod() {
        for suffix in GEOMETRY_SUFFIXES {
            for digit in 0..=HIGHEST_LOD {
                let name = format!("{LOD_PREFIX}{digit}{suffix}");
                assert_eq!(lod_of(&bldg(&name)).unwrap(), digit.to_string(), "{name}");
            }
        }
    }

    /// The scan is by name and not by module: a `veg:lod1Geometry` is as much
    /// a geometry property as a `bldg:lod2Solid`.
    #[test]
    fn every_citygml_module_names_its_geometry_the_same_way() {
        for ns in [
            "http://www.opengis.net/citygml/vegetation/2.0",
            "http://www.opengis.net/citygml/transportation/1.0",
            "http://www.opengis.net/citygml/waterbody/2.0",
            "http://www.opengis.net/citygml/2.0",
        ] {
            let property = node(&format!(r#"<m:lod1MultiSurface xmlns:m="{ns}"/>"#));
            assert_eq!(lod_of(&property), Some("1"), "{ns}");
        }
    }

    #[test]
    fn other_property_names_are_not_geometry_properties() {
        for name in [
            "lod5Solid",      // CityGML 2.0 stops at LoD 4
            "lodXSolid",      // not a digit
            "lod2Sphere",     // not a geometry this converter reads
            "lod2",           // no suffix
            "lod",            // no digit either
            "measuredHeight", // an attribute
            "lod2SolidExtra", // a longer name that merely starts right
            "consistsOfBuildingPart",
        ] {
            assert!(lod_of(&bldg(name)).is_none(), "{name}");
        }
        // The right name in the wrong namespace is not a geometry property.
        assert!(lod_of(&node(r#"<x:lod2Solid xmlns:x="urn:example:other"/>"#)).is_none());
        assert!(lod_of(&node(
            r#"<x:lod2Solid xmlns:x="http://www.opengis.net/citygml/building/3.0"/>"#
        ))
        .is_none());
    }

    #[test]
    fn an_object_without_a_gml_id_is_named_after_its_member() {
        assert_eq!(member_object_id(&bldg("Building"), 7), "citygml-obj-7");
        assert_eq!(
            member_object_id(
                &node(
                    r#"<bldg:Building xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                                      xmlns:gml="http://www.opengis.net/gml" gml:id="b1"/>"#
                ),
                7
            ),
            "b1"
        );
    }
}
