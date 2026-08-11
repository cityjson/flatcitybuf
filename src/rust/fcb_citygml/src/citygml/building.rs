//! The building module: `bldg:Building`.
//!
//! The geometry and the attributes of the building itself are read here. Its
//! boundary surfaces and its nested parts and installations each arrive with
//! their own task, and each is additive: a property this reader does not
//! recognise is passed over silently rather than reported, because at this
//! stage nearly every property of a real building is one of those.

use super::attributes::read_common_attributes;
use crate::gml::{parse_geometry, GmlGeometry, XlinkRegistry};
use crate::model::{IntermediateGeometry, IntermediateObject};
use crate::xml::XmlNode;
use crate::{CityGmlError, ParseReport, Skipped};

/// Namespace URIs of the CityGML building module, 2.0 and 1.0.
const BUILDING_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/building/2.0",
    "http://www.opengis.net/citygml/building/1.0",
];

/// Local name of the one element this reader claims.
const BUILDING: &str = "Building";

/// A geometry property is `lod` + a digit + one of these, e.g. `lod2Solid`.
/// `lodXGeometry` is the property that may hold any geometry at all, so all
/// three are treated alike: whichever supported GML geometry the property
/// holds is the one taken. A `lod2Solid` holding a `MultiSurface` is not
/// valid CityGML, but it is unambiguous, and rejecting it would lose geometry
/// over a mislabelled property.
const GEOMETRY_SUFFIXES: [&str; 3] = ["Solid", "MultiSurface", "Geometry"];

/// The prefix every geometry property name starts with.
const LOD_PREFIX: &str = "lod";

/// The highest level of detail CityGML 2.0 defines.
const HIGHEST_LOD: u8 = 4;

/// Whether a node is a `bldg:Building`.
pub(crate) fn is_building(node: &XmlNode) -> bool {
    node.local == BUILDING && BUILDING_NS.contains(&node.ns.as_str())
}

/// Read a `bldg:Building` into the intermediate model.
///
/// `registry` indexes the polygons of the whole `cityObjectMember`, so a
/// solid whose faces are `xlink:href`s to polygons written elsewhere in the
/// building resolves. `member_index` names the object when it carries no
/// `gml:id`; the generated id is stable for a given document, which matters
/// because it ends up as a CityJSON object key.
///
/// # Errors
///
/// Propagates the geometry reader's errors: malformed geometry, and
/// references that name no polygon in the member.
pub(crate) fn read_building(
    node: &XmlNode,
    registry: &XlinkRegistry,
    member_index: usize,
    report: &mut ParseReport,
) -> Result<IntermediateObject, CityGmlError> {
    let id = node
        .gml_id()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("citygml-obj-{member_index}"));
    let mut object = IntermediateObject::new(id, cjseq::CityObjectType::Building);
    read_common_attributes(node, &mut object.attributes, report);
    object.geometries = read_lod_geometries(node, registry, report)?;
    Ok(object)
}

/// Read every geometry property of an object, in document order.
///
/// A property that holds nothing this converter can read is recorded and
/// passed over: the rest of the object is still worth having.
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
/// outside the building module's namespace.
fn lod_of(property: &XmlNode) -> Option<&str> {
    if !BUILDING_NS.contains(&property.ns.as_str()) {
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
    fn a_building_is_recognised_in_both_module_versions() {
        for ns in BUILDING_NS {
            assert!(is_building(&node(&format!(
                r#"<bldg:Building xmlns:bldg="{ns}"/>"#
            ))));
        }
        // The local name alone is not a building.
        assert!(!is_building(&node(r#"<Building/>"#)));
        assert!(!is_building(&node(
            r#"<b:Building xmlns:b="urn:example:other"/>"#
        )));
        assert!(!is_building(&bldg("BuildingPart")));
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
    }
}
