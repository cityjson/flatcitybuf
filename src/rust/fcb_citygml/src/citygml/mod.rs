//! The thematic module readers: one CityGML city object in, one
//! [`IntermediateObject`] out.
//!
//! Every CityGML module — building, vegetation, transportation — has its own
//! namespace and its own element names, but they share a shape: a feature
//! with attributes, geometry properties named after the level of detail they
//! carry, and nested objects. This module owns the dispatch from a
//! `cityObjectMember` to the reader that knows the module in question;
//! [`building`] is the first of those readers.
//!
//! Namespaces are matched against both the CityGML 2.0 and the 1.0 URI of
//! each module: the two differ only in ways this converter does not read, and
//! files in the wild are still written against 1.0.

mod attributes;
pub(crate) mod building;

use crate::gml::XlinkRegistry;
use crate::model::IntermediateObject;
use crate::xml::XmlNode;
use crate::{CityGmlError, ParseReport, Skipped};

/// Reason recorded for a city object no module reader recognises.
const UNSUPPORTED: &str = "unsupported CityObject";

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

    report.skipped.push(Skipped {
        element: object.local.clone(),
        gml_id: object.gml_id().map(str::to_owned),
        reason: format!("{UNSUPPORTED}: <{}> has no reader", object.local),
    });
    Ok(None)
}
