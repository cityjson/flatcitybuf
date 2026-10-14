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
//! [`construction`] reads the families that nest city objects — buildings,
//! bridges and tunnels — from a per-module descriptor, [`building`] being one
//! of those descriptors; [`simple`] reads the modules whose objects are
//! attributes, geometry and — for three of them — thematic surfaces; and
//! [`relief`] reads terrain, which is the one module whose geometry is not a
//! `lodX…` property.
//!
//! Namespaces are matched against both the CityGML 2.0 and the 1.0 URI of
//! each module: the two differ only in ways this converter does not read, and
//! files in the wild are still written against 1.0.

mod attributes;
mod building;
mod construction;
mod relief;
mod semantics;
mod simple;

use crate::gml::{flatten_implicit, parse_geometry, GmlGeometry, XlinkRegistry};
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

/// The one other thing a `lodX…` property may hold: a *placement* of a
/// prototype geometry rather than a geometry of its own. It reaches CityJSON
/// as an ordinary geometry, flattened; see [`crate::gml::flatten_implicit`].
const IMPLICIT_SUFFIX: &str = "ImplicitRepresentation";

/// The prefix every geometry property name starts with.
const LOD_PREFIX: &str = "lod";

/// Reason recorded for a `lodX…` property whose suffix names something
/// CityJSON has nowhere to put — `lod2TerrainIntersection`, `lod0FootPrint`,
/// `lod0RoofEdge`, `tran:lod0Network`.
///
/// These are geometry the source states and this converter does not write, so
/// they are reported rather than passed over in silence, one entry per
/// occurrence: each is a real drop, and collapsing them would hide how much
/// of a document went missing.
const NO_COUNTERPART: &str = "no CityJSON counterpart for this property";

/// The highest level of detail CityGML 2.0 defines.
const HIGHEST_LOD: u8 = 4;

/// Where a `cityObjectMember` sits in the document, and what to call an
/// object in it that carries no `gml:id`.
///
/// The prefix travels with the index because the index alone is unique only
/// within one document: two files converted into one dataset both start at
/// zero, and the second `citygml-obj-0` would overwrite the first. See
/// [`crate::ParseOptions::id_prefix`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct MemberId<'a> {
    pub prefix: &'a str,
    pub index: usize,
}

#[cfg(test)]
impl MemberId<'static> {
    /// The first member of a document, named the default way: what a reader's
    /// own tests pass when the generated id is not what they are testing.
    fn for_tests() -> Self {
        Self {
            prefix: crate::DEFAULT_ID_PREFIX,
            index: 0,
        }
    }
}

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
/// `member_id` is the position of this member among the document's members
/// and the prefix to go with it, and is used only to name an object whose
/// `gml:id` is missing.
///
/// Answers *every* top-level object the member yields, which is why it is a
/// vector and not one object. It is one object for all but one element:
/// a `dem:ReliefFeature` is a wrapper CityJSON has no type for, so each of the
/// terrain components inside it becomes a top-level object of its own and the
/// member yields as many objects as it holds readable components. It is empty
/// for a member this converter has no reader for, which has been recorded in
/// `report` — an unsupported city object is content that is valid CityGML, so
/// it is skipped rather than fatal.
///
/// # Errors
///
/// Propagates whatever the module reader raises: malformed geometry, and
/// `xlink:href`s that name nothing in the member.
pub(crate) fn read_member(
    member: &XmlNode,
    member_id: MemberId,
    report: &mut ParseReport,
) -> Result<Vec<IntermediateObject>, CityGmlError> {
    // A `cityObjectMember` holds exactly one city object. An empty one, or
    // one that only references an object elsewhere by `xlink:href`, holds
    // nothing this converter can read.
    let Some(object) = member.children.first() else {
        report.skipped.push(Skipped {
            element: member.local.clone(),
            gml_id: member.gml_id().map(str::to_owned),
            reason: format!("{UNSUPPORTED}: the member holds no city object"),
        });
        return Ok(Vec::new());
    };

    if let Some(spec) = construction::spec_of(object) {
        let registry = XlinkRegistry::collect(member);
        let object =
            construction::read_construction(object, spec, member, &registry, member_id, report)?;
        return Ok(vec![object]);
    }

    if let Some(kind) = simple::kind_of(object) {
        let registry = XlinkRegistry::collect(member);
        let object =
            simple::read_simple_object(object, kind, member, &registry, member_id, report)?;
        return Ok(vec![object]);
    }

    if relief::is_relief(object) {
        return Ok(relief::read_relief(object, member_id, report));
    }

    report.skipped.push(Skipped {
        element: object.local.clone(),
        gml_id: object.gml_id().map(str::to_owned),
        reason: format!("{UNSUPPORTED}: <{}> has no reader", object.local),
    });
    Ok(Vec::new())
}

/// The id of a top-level city object: its `gml:id`, or a stand-in built from
/// its member's position in the document and the caller's prefix.
///
/// The generated id is stable for a given document, which matters because it
/// ends up as a CityJSON object key.
fn member_object_id(node: &XmlNode, member_id: MemberId) -> String {
    node.gml_id().map(str::to_owned).unwrap_or_else(|| {
        let MemberId { prefix, index } = member_id;
        format!("{prefix}-{index}")
    })
}

/// Read every geometry property of an object, in document order.
///
/// The scan is by property *name* — `lod` + a digit + a geometry word — and
/// not by module, because every CityGML module names its geometry properties
/// that way and none of them means anything else by such a name. A reader for
/// a further module therefore inherits this without an argument or a table of
/// its own.
///
/// A `lodXImplicitRepresentation` is read here too, and becomes a geometry of
/// the object like any other: CityJSON has no implicit geometry, so the
/// prototype is flattened into the object's own coordinates at the LoD the
/// property names. `member` is the whole `cityObjectMember`, and is the scope
/// a prototype named by `xlink:href` is looked for in; see
/// [`flatten_implicit`].
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
    member: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Vec<IntermediateGeometry>, CityGmlError> {
    let mut geometries = Vec::new();
    for property in &node.children {
        if let Some(lod) = lod_of(property) {
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
        } else if let Some(lod) = implicit_lod_of(property) {
            // A placement this converter cannot flatten has already been
            // recorded, with the reason, by `flatten_implicit`.
            if let Some(geometry) = flatten_implicit(property, member, registry, report)? {
                geometries.push(IntermediateGeometry {
                    lod: lod.to_string(),
                    geometry,
                    surfaces: Vec::new(),
                });
            }
        } else if lod_property(property).is_some() {
            // A `lodX…` property of a kind CityJSON cannot hold. Only the
            // `lod`-prefixed ones are reported: an unmapped *simple* property
            // is an attribute this converter does not claim, which
            // [`attributes`] passes over by design, but a property named
            // after a level of detail is geometry, and geometry that goes
            // missing has to say so.
            report.skipped.push(Skipped {
                element: property.local.clone(),
                gml_id: property
                    .gml_id()
                    .or_else(|| node.gml_id())
                    .map(str::to_owned),
                reason: NO_COUNTERPART.to_string(),
            });
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
    let (lod, suffix) = lod_property(property)?;
    GEOMETRY_SUFFIXES.contains(&suffix).then_some(lod)
}

/// The level of detail a `lodXImplicitRepresentation` carries, e.g. `"2"` for
/// `lod2ImplicitRepresentation`.
///
/// The digit range is [`lod_of`]'s, rather than the 1 to 4 that most modules
/// declare: the generics module of CityGML 2.0 has a
/// `lod0ImplicitRepresentation` too, and the scan is by name.
fn implicit_lod_of(property: &XmlNode) -> Option<&str> {
    let (lod, suffix) = lod_property(property)?;
    (suffix == IMPLICIT_SUFFIX).then_some(lod)
}

/// A `lodX…` property name split into the LoD digit and what follows it, for
/// an element of a CityGML module namespace.
fn lod_property(property: &XmlNode) -> Option<(&str, &str)> {
    if !is_citygml_module(&property.ns) {
        return None;
    }
    let rest = property.local.strip_prefix(LOD_PREFIX)?;
    let digit = *rest.as_bytes().first()?;
    if !(b'0'..=b'0' + HIGHEST_LOD).contains(&digit) {
        return None;
    }
    // Splitting after an ASCII digit cannot land inside a character.
    Some(rest.split_at(1))
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

    /// A `lodX…` property CityJSON has nowhere to put is *reported*, once per
    /// occurrence, rather than passed over in silence.
    #[test]
    fn a_lod_property_without_a_counterpart_is_reported() {
        let object = node(
            r#"<bldg:Building xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                              xmlns:gml="http://www.opengis.net/gml" gml:id="b1">
                 <bldg:lod2TerrainIntersection><gml:MultiCurve/></bldg:lod2TerrainIntersection>
                 <bldg:lod2TerrainIntersection><gml:MultiCurve/></bldg:lod2TerrainIntersection>
                 <bldg:lod0FootPrint><gml:MultiSurface/></bldg:lod0FootPrint>
               </bldg:Building>"#,
        );
        let mut report = ParseReport::default();
        let geometries =
            read_lod_geometries(&object, &object, &XlinkRegistry::default(), &mut report).unwrap();
        assert!(geometries.is_empty());

        let skipped: Vec<(&str, Option<&str>, &str)> = report
            .skipped
            .iter()
            .map(|skip| {
                (
                    skip.element.as_str(),
                    skip.gml_id.as_deref(),
                    skip.reason.as_str(),
                )
            })
            .collect();
        assert_eq!(
            skipped,
            vec![
                ("lod2TerrainIntersection", Some("b1"), NO_COUNTERPART),
                ("lod2TerrainIntersection", Some("b1"), NO_COUNTERPART),
                ("lod0FootPrint", Some("b1"), NO_COUNTERPART),
            ]
        );
    }

    /// Every module spells these the same way, and none of them is a geometry
    /// CityJSON can hold.
    #[test]
    fn the_reported_lod_properties_are_the_ones_no_reader_claims() {
        for (ns, name) in [
            (
                "http://www.opengis.net/citygml/transportation/2.0",
                "lod0Network",
            ),
            (
                "http://www.opengis.net/citygml/building/2.0",
                "lod0RoofEdge",
            ),
            (
                "http://www.opengis.net/citygml/building/1.0",
                "lod3TerrainIntersection",
            ),
        ] {
            let object = node(&format!(r#"<m:Thing xmlns:m="{ns}"><m:{name}/></m:Thing>"#));
            let mut report = ParseReport::default();
            read_lod_geometries(&object, &object, &XlinkRegistry::default(), &mut report).unwrap();
            assert_eq!(report.skipped.len(), 1, "{name}");
            assert_eq!(report.skipped[0].element, name);
        }
    }

    /// The report is for geometry alone: an attribute, or an element outside
    /// a CityGML module, is not a dropped `lodX…` property and says nothing.
    #[test]
    fn an_unmapped_simple_property_is_still_passed_over_in_silence() {
        let object = node(
            r#"<bldg:Building xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                              xmlns:x="urn:example:ade">
                 <bldg:measuredHeight>10.0</bldg:measuredHeight>
                 <bldg:address/>
                 <x:lod2Something/>
               </bldg:Building>"#,
        );
        let mut report = ParseReport::default();
        read_lod_geometries(&object, &object, &XlinkRegistry::default(), &mut report).unwrap();
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }

    #[test]
    fn an_object_without_a_gml_id_is_named_after_its_member() {
        let member = |prefix| MemberId { prefix, index: 7 };
        assert_eq!(
            member_object_id(&bldg("Building"), member(crate::DEFAULT_ID_PREFIX)),
            "citygml-obj-7"
        );
        // A caller that names the source keeps two documents' generated ids
        // apart.
        assert_eq!(
            member_object_id(&bldg("Building"), member("tile-42")),
            "tile-42-7"
        );
        assert_eq!(
            member_object_id(
                &node(
                    r#"<bldg:Building xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                                      xmlns:gml="http://www.opengis.net/gml" gml:id="b1"/>"#
                ),
                member(crate::DEFAULT_ID_PREFIX)
            ),
            "b1"
        );
    }
}
