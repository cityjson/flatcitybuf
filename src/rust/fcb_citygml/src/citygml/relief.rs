//! The relief module: terrain as a triangulated surface.
//!
//! CityGML models terrain in two layers. A `dem:ReliefFeature` is the terrain
//! of an area *at one level of detail*, and it holds one or more
//! `dem:reliefComponent`s — a TIN, a grid, a set of break lines, a set of mass
//! points — that make it up. CityJSON has no counterpart to that wrapper: its
//! terrain types are the components themselves, and `TINRelief` is the only
//! one this converter can write.
//!
//! So a `dem:ReliefFeature` is not an object here. It is skipped with a note
//! and each `dem:TINRelief` under it becomes a top-level object of its own,
//! which is why reading one member may yield several objects. A bare
//! `dem:TINRelief` member — legal CityGML, and what a document round-tripped
//! through a CityJSON tool comes back as — is read the same way, minus the
//! unwrapping.
//!
//! The geometry is not a `lodX…` property: a TIN states its level of detail in
//! a `dem:lod` element and its surface in a `dem:tin`, so neither the LoD scan
//! nor the geometry scan of [`super`] applies, and both are read here.

use super::attributes::read_common_attributes;
use super::member_object_id;
use crate::gml::{parse_triangles, GmlGeometry};
use crate::model::{IntermediateGeometry, IntermediateObject};
use crate::xml::XmlNode;
use crate::{is_in, ParseReport, Skipped};

/// Namespace URIs of the CityGML relief module, 2.0 and 1.0. The prefix is
/// `dem:` although the module is called relief, which is the schema's own
/// inconsistency and not this reader's.
const RELIEF_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/relief/2.0",
    "http://www.opengis.net/citygml/relief/1.0",
];

/// The elements this reader claims: the one that becomes an object, and the
/// wrapper that does not.
const TIN_RELIEF: &str = "TINRelief";
const RELIEF_FEATURE: &str = "ReliefFeature";

/// Local names of the properties read here: the wrapper's components, a TIN's
/// surface, and the level of detail either of them states.
const RELIEF_COMPONENT: &str = "reliefComponent";
const TIN: &str = "tin";
const LOD: &str = "lod";

/// The level of detail a `dem:TINRelief` that states none is read at.
///
/// `dem:lod` is mandatory in the schema, so this is a repair rather than a
/// default: a TIN without one is still terrain, and LoD 1 is the level at
/// which terrain is most often given.
const DEFAULT_LOD: &str = "1";

/// The word a generated id is built from, for a component of a relief feature
/// that carries no `gml:id`: `relief-1-tin-2`.
const TIN_WORD: &str = "tin";

/// Whether a node is an element of the relief module this reader reads.
pub(crate) fn is_relief(node: &XmlNode) -> bool {
    is_in(node, &RELIEF_NS, TIN_RELIEF) || is_in(node, &RELIEF_NS, RELIEF_FEATURE)
}

/// Read one relief member into the intermediate model.
///
/// Answers *every* object the member yields, which is one for a bare
/// `dem:TINRelief` and one per readable `dem:reliefComponent` for a
/// `dem:ReliefFeature` — none, when the feature holds only components this
/// converter cannot write.
///
/// No xlink registry is taken. A TIN's triangles are patches written inline;
/// unlike a `gml:surfaceMember` there is no form of `gml:Triangle` that names
/// a polygon written elsewhere, so there is nothing here for a registry to
/// resolve.
pub(crate) fn read_relief(
    node: &XmlNode,
    member_index: usize,
    report: &mut ParseReport,
) -> Vec<IntermediateObject> {
    if is_in(node, &RELIEF_NS, TIN_RELIEF) {
        return vec![read_tin_relief(
            node,
            member_object_id(node, member_index),
            report,
        )];
    }

    // A `dem:ReliefFeature`. Its own attributes are lost with it: they
    // describe the terrain of an area as a whole, and there is no object left
    // to carry them once the components have become objects of their own.
    report.skipped.push(Skipped {
        element: node.local.clone(),
        gml_id: node.gml_id().map(str::to_owned),
        reason: format!(
            "<{RELIEF_FEATURE}> has no CityJSON counterpart; its <{RELIEF_COMPONENT}>s become \
             city objects of their own and the wrapper's own properties are dropped"
        ),
    });

    let parent_id = member_object_id(node, member_index);
    let mut objects = Vec::new();
    let mut components = 0usize;
    for property in &node.children {
        if !is_in(property, &RELIEF_NS, RELIEF_COMPONENT) {
            continue;
        }
        let mut read_any = false;
        for component in &property.children {
            if !is_in(component, &RELIEF_NS, TIN_RELIEF) {
                continue;
            }
            components += 1;
            // The counter runs over every TIN of the feature, named or not, so
            // that an id which is present does not shift the ones generated
            // after it.
            let id = component
                .gml_id()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{parent_id}-{TIN_WORD}-{components}"));
            objects.push(read_tin_relief(component, id, report));
            read_any = true;
        }
        if !read_any {
            // A raster relief, a set of mass points, or a reference to a
            // component written elsewhere: content this converter loses.
            report.skipped.push(Skipped {
                element: property.local.clone(),
                gml_id: property.gml_id().map(str::to_owned),
                reason: format!(
                    "<{RELIEF_COMPONENT}> holds no <{TIN_RELIEF}> this reader can read"
                ),
            });
        }
    }
    objects
}

/// Read one `dem:TINRelief`: its attributes, and the triangulation under its
/// `dem:tin`.
///
/// The triangles become a CityJSON `CompositeSurface` rather than a
/// `MultiSurface`, because that is what a triangulation is: the patches of a
/// `gml:TriangulatedSurface` meet along shared edges and describe one surface,
/// not a collection of unrelated ones.
fn read_tin_relief(node: &XmlNode, id: String, report: &mut ParseReport) -> IntermediateObject {
    let mut object = IntermediateObject::new(id, cjseq::CityObjectType::TINRelief);
    read_common_attributes(node, &mut object.attributes, report);
    let lod = lod_of(node, report);

    for property in &node.children {
        if !is_in(property, &RELIEF_NS, TIN) {
            continue;
        }
        let triangles: Vec<_> = property
            .children
            .iter()
            .flat_map(|child| parse_triangles(child, report))
            .collect();
        if triangles.is_empty() {
            report.skipped.push(Skipped {
                element: property.local.clone(),
                gml_id: property.gml_id().map(str::to_owned),
                reason: format!("<{TIN}> holds no GML triangulation this reader can read"),
            });
            continue;
        }
        object.geometries.push(IntermediateGeometry {
            lod: lod.clone(),
            geometry: GmlGeometry::CompositeSurface(triangles),
            surfaces: Vec::new(),
        });
    }
    object
}

/// The level of detail a `dem:lod` element states.
///
/// CityJSON's `lod` is a string — it allows `"2.1"` — but a relief's is one of
/// the levels of detail CityGML defines, so text that is not one of those is a
/// document error rather than a finer level of detail. Taking it verbatim
/// would write a `lod` no CityJSON reader could match against another object's.
fn lod_of(node: &XmlNode, report: &mut ParseReport) -> String {
    let Some(lod) = node
        .children
        .iter()
        .find(|child| is_in(child, &RELIEF_NS, LOD))
    else {
        return DEFAULT_LOD.to_string();
    };
    if lod
        .text
        .parse::<u8>()
        .is_ok_and(|level| level <= super::HIGHEST_LOD)
    {
        return lod.text.clone();
    }
    report.warnings.push(format!(
        "<{LOD}> {:?} is not a level of detail CityGML defines; LoD {DEFAULT_LOD} is assumed",
        lod.text
    ));
    DEFAULT_LOD.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(xml: &str) -> XmlNode {
        crate::xml::parse_str_for_tests(xml).unwrap()
    }

    /// The namespaces every fixture below binds.
    const NS: &str = r#"xmlns:dem="http://www.opengis.net/citygml/relief/2.0"
         xmlns:gml="http://www.opengis.net/gml"
         xmlns:xlink="http://www.w3.org/1999/xlink""#;

    /// Read one relief member, as the member scan would.
    fn read(xml: &str) -> (Vec<IntermediateObject>, ParseReport) {
        let node = node(xml);
        assert!(is_relief(&node), "<{}> is not a relief element", node.local);
        let mut report = ParseReport::default();
        let objects = read_relief(&node, 0, &mut report);
        (objects, report)
    }

    /// A `gml:Triangle` over `pos_list`.
    fn triangle(pos_list: &str) -> String {
        format!(
            "<gml:Triangle><gml:exterior><gml:LinearRing>\
             <gml:posList>{pos_list}</gml:posList>\
             </gml:LinearRing></gml:exterior></gml:Triangle>"
        )
    }

    /// A `dem:tin` holding a `gml:TriangulatedSurface` over `patches`.
    fn tin(patches: &str) -> String {
        format!(
            "<dem:tin><gml:TriangulatedSurface><gml:trianglePatches>{patches}\
             </gml:trianglePatches></gml:TriangulatedSurface></dem:tin>"
        )
    }

    /// The two triangles of the fixtures below, sharing an edge.
    fn two_triangles() -> String {
        format!(
            "{}{}",
            triangle("0 0 0 2 0 0 2 1 1 0 0 0"),
            triangle("0 0 0 2 1 1 0 1 2 0 0 0"),
        )
    }

    /// The polygons of an object's one geometry, and its LoD.
    fn geometry(object: &IntermediateObject) -> (&str, usize) {
        let geometry = &object.geometries[0];
        assert!(
            matches!(geometry.geometry, GmlGeometry::CompositeSurface(_)),
            "a TIN is a CompositeSurface, not {:?}",
            geometry.geometry
        );
        (geometry.lod.as_str(), geometry.geometry.polygons().len())
    }

    /// A bare `dem:TINRelief` member is an object in its own right: no
    /// wrapper, nothing skipped.
    #[test]
    fn a_bare_tin_relief_member_becomes_one_object() {
        let (objects, report) = read(&format!(
            "<dem:TINRelief {NS} gml:id=\"tin-1\">\
               <gml:name>Terrain patch</gml:name>\
               <dem:lod>2</dem:lod>{}\
             </dem:TINRelief>",
            tin(&two_triangles())
        ));

        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].id, "tin-1");
        assert_eq!(objects[0].co_type, cjseq::CityObjectType::TINRelief);
        assert_eq!(
            objects[0].attributes["name"],
            serde_json::json!("Terrain patch")
        );
        assert_eq!(geometry(&objects[0]), ("2", 2));
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A `dem:ReliefFeature` is not an object: it is skipped with a note, and
    /// each of its TIN components becomes an object of its own.
    #[test]
    fn a_relief_feature_yields_one_object_per_tin_component() {
        let (objects, report) = read(&format!(
            "<dem:ReliefFeature {NS} gml:id=\"relief-1\">\
               <gml:name>Terrain</gml:name>\
               <dem:lod>1</dem:lod>\
               <dem:reliefComponent><dem:TINRelief gml:id=\"tin-1\">\
                 <dem:lod>1</dem:lod>{tin}\
               </dem:TINRelief></dem:reliefComponent>\
               <dem:reliefComponent><dem:TINRelief>\
                 <dem:lod>2</dem:lod>{tin}\
               </dem:TINRelief></dem:reliefComponent>\
             </dem:ReliefFeature>",
            tin = tin(&two_triangles())
        ));

        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].id, "tin-1");
        assert_eq!(geometry(&objects[0]), ("1", 2));
        // A component with no gml:id is named after the feature that held it
        // and its place among that feature's components.
        assert_eq!(objects[1].id, "relief-1-tin-2");
        assert_eq!(geometry(&objects[1]), ("2", 2));
        // The wrapper is the one thing that was lost, and it is reported.
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].element, RELIEF_FEATURE);
        assert_eq!(report.skipped[0].gml_id.as_deref(), Some("relief-1"));
        // The wrapper's own attributes go with it.
        assert!(objects
            .iter()
            .all(|object| object.attributes.get("name").is_none()));
    }

    /// A component this converter cannot write — a raster relief, or a
    /// reference to a component elsewhere — is reported rather than dropped
    /// in silence.
    #[test]
    fn a_component_that_is_not_a_tin_is_reported() {
        let (objects, report) = read(&format!(
            r##"<dem:ReliefFeature {NS} gml:id="relief-1">
                  <dem:reliefComponent><dem:RasterRelief gml:id="raster-1"/></dem:reliefComponent>
                  <dem:reliefComponent xlink:href="#tin-9"/>
                </dem:ReliefFeature>"##
        ));

        assert!(objects.is_empty());
        // The wrapper, and one entry per component that yielded nothing.
        assert_eq!(report.skipped.len(), 3, "{report:?}");
        assert_eq!(report.skipped[1].element, RELIEF_COMPONENT);
        assert_eq!(report.skipped[2].element, RELIEF_COMPONENT);
    }

    /// A `gml:Tin` is a `gml:TriangulatedSurface` with the data it was
    /// computed from attached, and its patches are read the same way.
    #[test]
    fn a_gml_tin_is_read_like_a_triangulated_surface() {
        let (objects, report) = read(&format!(
            "<dem:TINRelief {NS} gml:id=\"tin-1\">\
               <dem:lod>1</dem:lod>\
               <dem:tin><gml:Tin><gml:trianglePatches>{}</gml:trianglePatches>\
                 <gml:stopLines/></gml:Tin></dem:tin>\
             </dem:TINRelief>",
            two_triangles()
        ));

        assert_eq!(geometry(&objects[0]), ("1", 2));
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A patch that is not a triangle once its ring has been repaired is not
    /// a triangle: it is skipped, and the triangles around it are kept.
    #[test]
    fn a_patch_that_is_not_a_triangle_is_skipped() {
        let (objects, report) = read(&format!(
            "<dem:TINRelief {NS} gml:id=\"tin-1\">{}</dem:TINRelief>",
            tin(&format!(
                "{}{}{}",
                triangle("0 0 0 2 0 0 2 1 1 0 0 0"),
                // Four distinct points: a quadrangle written as a Triangle.
                triangle("0 0 0 2 0 0 2 1 0 0 1 0 0 0 0"),
                // And one that collapses to a line.
                triangle("0 0 0 2 0 0 0 0 0"),
            ))
        ));

        assert_eq!(geometry(&objects[0]), ("1", 1));
        assert_eq!(report.skipped.len(), 2, "{report:?}");
        assert!(report
            .skipped
            .iter()
            .all(|skipped| skipped.element == "Triangle"));
    }

    /// A TIN with no readable triangulation keeps no geometry, and says so.
    #[test]
    fn a_tin_without_a_triangulation_is_reported() {
        let (objects, report) = read(&format!(
            "<dem:TINRelief {NS} gml:id=\"tin-1\">\
               <dem:tin><gml:MultiSurface/></dem:tin>\
             </dem:TINRelief>"
        ));

        assert!(objects[0].geometries.is_empty());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].element, TIN);
    }

    /// `dem:lod` is the only thing that states a TIN's level of detail, and a
    /// TIN without a readable one is read at LoD 1 rather than lost.
    #[test]
    fn a_missing_or_malformed_lod_falls_back_to_one() {
        // CityGML 2.0 stops at LoD 4, so "9" is as unreadable as "high".
        for lod in [
            "",
            "<dem:lod/>",
            "<dem:lod>high</dem:lod>",
            "<dem:lod>9</dem:lod>",
        ] {
            let (objects, report) = read(&format!(
                "<dem:TINRelief {NS} gml:id=\"tin-1\">{lod}{}</dem:TINRelief>",
                tin(&two_triangles())
            ));
            assert_eq!(geometry(&objects[0]), ("1", 2), "{lod:?}");
            // Only a *stated* level of detail that cannot be read is a
            // warning; an absent one is the schema's business, not a loss.
            assert_eq!(report.warnings.is_empty(), lod.is_empty(), "{lod:?}");
        }
    }

    /// The local name alone is not a relief element: an application schema is
    /// free to define a `TINRelief` of its own, and it is not the CityGML one.
    #[test]
    fn the_local_name_alone_is_not_a_relief_element() {
        assert!(!is_relief(&node(r#"<TINRelief/>"#)));
        assert!(!is_relief(&node(
            r#"<x:TINRelief xmlns:x="urn:example:other"/>"#
        )));
        assert!(!is_relief(&node(&format!("<dem:RasterRelief {NS}/>"))));
        // Both module versions are read.
        for ns in RELIEF_NS {
            assert!(is_relief(&node(&format!(
                r#"<dem:TINRelief xmlns:dem="{ns}"/>"#
            ))));
        }
    }
}
