//! The building module: `bldg:Building`, and the objects nested in it.
//!
//! The geometry, the attributes and the boundary surfaces of a building are
//! read here, and so are its `bldg:BuildingPart`s and its
//! `bldg:BuildingInstallation`s — by the same reader, because CityGML
//! describes all three the same way. A part may hold parts of its own, so the
//! reading is recursive; the whole tree becomes one CityJSON feature, which is
//! the converter's half of the arrangement.
//!
//! Each addition is additive: a property this reader does not recognise is
//! passed over silently rather than reported, because at this stage nearly
//! every property of a real building is one of those. A property that *is*
//! recognised and still yields nothing — a `boundedBy` or a
//! `consistsOfBuildingPart` that only references an object elsewhere — is
//! reported, because that is content this converter lost.

use super::attributes::read_common_attributes;
use super::semantics::{read_semantic_surfaces, SurfaceProperty, SurfaceSpec};
use super::{member_object_id, read_lod_geometries};
use crate::gml::XlinkRegistry;
use crate::model::IntermediateObject;
use crate::xml::XmlNode;
use crate::{is_in, CityGmlError, ParseReport, Skipped};

/// Namespace URIs of the CityGML building module, 2.0 and 1.0.
const BUILDING_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/building/2.0",
    "http://www.opengis.net/citygml/building/1.0",
];

/// Local name of the one element this reader claims.
const BUILDING: &str = "Building";

/// Local name of the property holding a thematic boundary surface, and of the
/// property holding one of that surface's openings.
const BOUNDED_BY: &str = "boundedBy";
const OPENING: &str = "opening";

/// The properties holding a nested city object, and the element each of them
/// holds.
const CONSISTS_OF_BUILDING_PART: &str = "consistsOfBuildingPart";
const BUILDING_PART: &str = "BuildingPart";
const OUTER_BUILDING_INSTALLATION: &str = "outerBuildingInstallation";
const BUILDING_INSTALLATION: &str = "BuildingInstallation";

/// One kind of nested object a building-family object may hold.
struct ChildKind {
    /// The property that carries the object.
    property: &'static str,
    /// The element that property holds.
    element: &'static str,
    /// The CityJSON type the object becomes.
    co_type: cjseq::CityObjectType,
    /// The word a generated id is built from: `b1-part-2`, `b1-inst-1`.
    id_word: &'static str,
}

/// The nested objects this reader knows, in no particular order: a document
/// may write them in any, and it is the document's order that is kept.
///
/// `bldg:interiorBuildingInstallation` is deliberately absent. It is a
/// property of a `bldg:Room`, and rooms — with their interior boundary
/// surfaces — arrive with the reader that reads them.
static CHILD_KINDS: [ChildKind; 2] = [
    ChildKind {
        property: CONSISTS_OF_BUILDING_PART,
        element: BUILDING_PART,
        co_type: cjseq::CityObjectType::BuildingPart,
        id_word: "part",
    },
    ChildKind {
        property: OUTER_BUILDING_INSTALLATION,
        element: BUILDING_INSTALLATION,
        co_type: cjseq::CityObjectType::BuildingInstallation,
        id_word: "inst",
    },
];

/// The thematic boundary surfaces of the building module, each of which is a
/// CityJSON semantic surface type spelled the same way.
///
/// The interior surfaces — `InteriorWallSurface`, `CeilingSurface`,
/// `FloorSurface` — are deliberately absent: they are boundaries of a
/// `bldg:Room`, not of a building, and they arrive with the reader that reads
/// rooms.
const BOUNDARY_SURFACES: [&str; 6] = [
    "RoofSurface",
    "WallSurface",
    "GroundSurface",
    "ClosureSurface",
    "OuterCeilingSurface",
    "OuterFloorSurface",
];

/// The openings a boundary surface may hold. Each becomes a semantic surface
/// of its own, pointing at the surface it opens.
const OPENINGS: [&str; 2] = ["Window", "Door"];

/// Where the building module writes its thematic surfaces: under
/// `bldg:boundedBy`, with the openings of each under `bldg:opening`. It is
/// the only module with openings.
static BUILDING_SURFACES: SurfaceSpec = SurfaceSpec {
    namespaces: &BUILDING_NS,
    properties: &[SurfaceProperty {
        property: BOUNDED_BY,
        elements: &BOUNDARY_SURFACES,
    }],
    openings: &[SurfaceProperty {
        property: OPENING,
        elements: &OPENINGS,
    }],
    container: BOUNDED_BY,
};

/// Whether a node is a `bldg:Building`.
pub(crate) fn is_building(node: &XmlNode) -> bool {
    node.local == BUILDING && BUILDING_NS.contains(&node.ns.as_str())
}

/// Whether a node is one of the named elements of the building module.
///
/// The local name alone is not enough: an application schema may define a
/// `WallSurface` of its own, and it is not the CityGML one.
fn is_building_element(node: &XmlNode, locals: &[&str]) -> bool {
    BUILDING_NS.contains(&node.ns.as_str()) && locals.contains(&node.local.as_str())
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
    let id = member_object_id(node, member_index);
    read_building_object(node, id, cjseq::CityObjectType::Building, registry, report)
}

/// Read one object of the building family — a `Building`, a `BuildingPart`, a
/// `BuildingInstallation` — into the intermediate model.
///
/// The three are read alike because CityGML describes them alike: each is an
/// `_AbstractBuilding` or a feature shaped like one, with its own attributes,
/// its own `lodX…` geometry properties, its own boundary surfaces and — for
/// the first two — nested objects of its own. A `BuildingPart` may therefore
/// hold parts and installations, which is what makes this recursive.
///
/// `id` is settled by the caller, because what names an object without a
/// `gml:id` differs: a top-level building is named after its member's
/// position, a nested object after its parent and its place among that
/// parent's children.
///
/// The order of the three reading steps is not free. `read_lod_geometries`
/// must run before `read_semantic_surfaces`, because the second pass
/// deduplicates its diagnostics against the entries the first one recorded;
/// reversed, one lost polygon would be reported twice.
///
/// # Errors
///
/// Propagates the geometry reader's errors, for this object and every object
/// nested in it.
fn read_building_object(
    node: &XmlNode,
    id: String,
    co_type: cjseq::CityObjectType,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<IntermediateObject, CityGmlError> {
    let mut object = IntermediateObject::new(id, co_type);
    read_common_attributes(node, &mut object.attributes, report);
    object.geometries = read_lod_geometries(node, registry, report)?;
    read_semantic_surfaces(
        node,
        &BUILDING_SURFACES,
        registry,
        &mut object.geometries,
        report,
    )?;
    object.children = read_children(node, &object.id, registry, report)?;
    Ok(object)
}

/// Read the objects nested in one building-family object, in document order.
///
/// The order is the document's rather than one kind after the other: it is
/// what the CityJSON `children` array is written in, and the source's own
/// order is the only one that means anything.
///
/// A child with no `gml:id` is named `{parent}-{kind}-{n}`, where `n` counts
/// the children of that kind under that parent from **one**, whether or not
/// they carried an id themselves. Counting the named ones too keeps the
/// generated ids stable against a document that gives one sibling an id and
/// not another, and — because the parent's own id is in the name — a
/// generated id nests: a part of `b1-part-1` is `b1-part-1-part-1`.
///
/// # Errors
///
/// Propagates the nested objects' errors, as [`read_building_object`] does.
fn read_children(
    node: &XmlNode,
    parent_id: &str,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Vec<IntermediateObject>, CityGmlError> {
    let mut children = Vec::new();
    let mut counts = [0usize; CHILD_KINDS.len()];
    for property in &node.children {
        let Some((index, kind)) = CHILD_KINDS
            .iter()
            .enumerate()
            .find(|(_, kind)| is_in(property, &BUILDING_NS, kind.property))
        else {
            continue;
        };
        let mut read_any = false;
        for child in &property.children {
            if !is_building_element(child, &[kind.element]) {
                continue;
            }
            counts[index] += 1;
            let id = child
                .gml_id()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{parent_id}-{}-{}", kind.id_word, counts[index]));
            children.push(read_building_object(
                child,
                id,
                kind.co_type.clone(),
                registry,
                report,
            )?);
            read_any = true;
        }
        if !read_any {
            // As with `boundedBy`: a property this reader took nothing from is
            // content that was lost — most often a reference to an object
            // written elsewhere, which this converter does not follow.
            report.skipped.push(Skipped {
                element: property.local.clone(),
                gml_id: property.gml_id().map(str::to_owned),
                reason: format!(
                    "<{}> holds no <{}> this reader can read",
                    property.local, kind.element
                ),
            });
        }
    }
    Ok(children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::citygml::semantics::POLYGON;
    use crate::model::IntermediateGeometry;

    fn node(xml: &str) -> XmlNode {
        crate::xml::parse_str_for_tests(xml).unwrap()
    }

    /// An element of the building module with the given local name.
    fn bldg(local: &str) -> XmlNode {
        node(&format!(
            r#"<bldg:{local} xmlns:bldg="http://www.opengis.net/citygml/building/2.0"/>"#
        ))
    }

    /// A `bldg:Building` holding `properties`, with every namespace these
    /// tests use bound on it.
    fn building(properties: &str) -> String {
        format!(
            r#"<bldg:Building xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                              xmlns:gen="http://www.opengis.net/citygml/generics/2.0"
                              xmlns:gml="http://www.opengis.net/gml"
                              xmlns:xlink="http://www.w3.org/1999/xlink"
                              gml:id="b1">{properties}</bldg:Building>"#
        )
    }

    /// Read a whole building, with the xlink registry the member scan would
    /// have collected for it — which covers the building's own subtree, so a
    /// reference either way round resolves.
    fn read(xml: &str) -> (IntermediateObject, ParseReport) {
        let building = node(xml);
        let registry = XlinkRegistry::collect(&building);
        let mut report = ParseReport::default();
        let object = read_building(&building, &registry, 0, &mut report)
            .unwrap_or_else(|err| panic!("read failed: {err}"));
        (object, report)
    }

    /// A unit square at height `z`, as a `gml:Polygon` carrying `gml:id`.
    fn polygon(gml_id: &str, z: f64) -> String {
        format!(
            r#"<gml:Polygon gml:id="{gml_id}"><gml:exterior><gml:LinearRing>
                 <gml:posList>0 0 {z} 1 0 {z} 1 1 {z} 0 0 {z}</gml:posList>
               </gml:LinearRing></gml:exterior></gml:Polygon>"#
        )
    }

    /// A polygon whose ring collapses to fewer than three distinct points, so
    /// that it carries no area and the geometry readers drop it.
    fn degenerate(gml_id: &str) -> String {
        format!(
            r#"<gml:Polygon gml:id="{gml_id}"><gml:exterior><gml:LinearRing>
                 <gml:posList>0 0 0 1 0 0 0 0 0</gml:posList>
               </gml:LinearRing></gml:exterior></gml:Polygon>"#
        )
    }

    /// A `gml:Solid` whose one shell holds `members`.
    fn solid(members: &str) -> String {
        format!(
            "<gml:Solid><gml:exterior><gml:CompositeSurface>{members}\
             </gml:CompositeSurface></gml:exterior></gml:Solid>"
        )
    }

    /// A `gml:MultiSurface` around `members`.
    fn multi_surface(members: &str) -> String {
        format!("<gml:MultiSurface>{members}</gml:MultiSurface>")
    }

    /// A `gml:surfaceMember` holding an `xlink:href` to `gml_id`.
    fn member_ref(gml_id: &str) -> String {
        format!(r##"<gml:surfaceMember xlink:href="#{gml_id}"/>"##)
    }

    /// A `gml:surfaceMember` holding a polygon inline.
    fn member(polygon: &str) -> String {
        format!("<gml:surfaceMember>{polygon}</gml:surfaceMember>")
    }

    /// A `bldg:boundedBy` holding one thematic surface of `stype`, whose
    /// `lod{lod}MultiSurface` holds `members`.
    fn bounded_by(stype: &str, lod: u8, members: &str) -> String {
        format!(
            "<bldg:boundedBy><bldg:{stype}><bldg:lod{lod}MultiSurface>{}\
             </bldg:lod{lod}MultiSurface></bldg:{stype}></bldg:boundedBy>",
            multi_surface(members)
        )
    }

    /// The semantic surface each polygon of a geometry points at, in
    /// document order.
    fn sem_indices(geometry: &IntermediateGeometry) -> Vec<Option<usize>> {
        geometry
            .geometry
            .polygons()
            .iter()
            .map(|polygon| polygon.sem_idx)
            .collect()
    }

    /// The types of a geometry's semantic surfaces, in index order.
    fn stypes(geometry: &IntermediateGeometry) -> Vec<&str> {
        geometry
            .surfaces
            .iter()
            .map(|surface| surface.stype.as_str())
            .collect()
    }

    /// The standard CityGML pattern: the polygons are written under the
    /// boundary surfaces and the solid points at each of them.
    #[test]
    fn a_solid_of_xlinks_inherits_the_semantics_of_the_polygons_it_names() {
        let (object, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>{}{}",
            solid(&format!(
                "{}{}{}",
                member_ref("r1"),
                member_ref("w1"),
                member_ref("w2")
            )),
            bounded_by("RoofSurface", 2, &member(&polygon("r1", 3.0))),
            bounded_by(
                "WallSurface",
                2,
                &format!(
                    "{}{}",
                    member(&polygon("w1", 1.0)),
                    member(&polygon("w2", 2.0))
                )
            ),
        )));

        let geometry = &object.geometries[0];
        assert_eq!(geometry.lod, "2");
        assert_eq!(stypes(geometry), vec!["RoofSurface", "WallSurface"]);
        // The solid's own order, not the order the surfaces were written in.
        assert_eq!(sem_indices(geometry), vec![Some(0), Some(1), Some(1)]);
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// The same file written the other way round: the polygons are inline in
    /// the solid and the boundary surface points at them. The join is by
    /// `gml:id` either way.
    #[test]
    fn a_boundary_surface_may_point_at_the_solids_own_polygons() {
        let (object, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>{}{}",
            solid(&format!(
                "{}{}",
                member(&polygon("g1", 0.0)),
                member(&polygon("r1", 3.0))
            )),
            bounded_by("GroundSurface", 2, &member_ref("g1")),
            bounded_by("RoofSurface", 2, &member_ref("r1")),
        )));

        let geometry = &object.geometries[0];
        assert_eq!(stypes(geometry), vec!["GroundSurface", "RoofSurface"]);
        assert_eq!(sem_indices(geometry), vec![Some(0), Some(1)]);
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A polygon no boundary surface claimed keeps no semantics rather than
    /// borrowing its neighbour's.
    #[test]
    fn an_unclaimed_polygon_keeps_no_semantics() {
        let (object, _) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>{}",
            solid(&format!(
                "{}{}",
                member_ref("w1"),
                // Inline, with an id no boundary surface mentions.
                member(&polygon("loose", 9.0))
            )),
            bounded_by("WallSurface", 2, &member(&polygon("w1", 1.0))),
        )));

        assert_eq!(sem_indices(&object.geometries[0]), vec![Some(0), None]);
    }

    /// An opening is a semantic surface of its own, linked to the surface it
    /// opens from both ends.
    #[test]
    fn an_opening_becomes_its_own_surface_linked_to_the_wall() {
        let wall = format!(
            "<bldg:boundedBy><bldg:WallSurface>\
               <bldg:lod3MultiSurface>{}</bldg:lod3MultiSurface>\
               <bldg:opening><bldg:Window>\
                 <bldg:lod3MultiSurface>{}</bldg:lod3MultiSurface>\
               </bldg:Window></bldg:opening>\
               <bldg:opening><bldg:Door>\
                 <bldg:lod3MultiSurface>{}</bldg:lod3MultiSurface>\
               </bldg:Door></bldg:opening>\
             </bldg:WallSurface></bldg:boundedBy>",
            multi_surface(&member(&polygon("w1", 1.0))),
            multi_surface(&member(&polygon("win1", 2.0))),
            multi_surface(&member(&polygon("door1", 3.0))),
        );
        let (object, report) = read(&building(&format!(
            "<bldg:lod3MultiSurface>{}</bldg:lod3MultiSurface>{wall}",
            multi_surface(&format!(
                "{}{}{}",
                member_ref("w1"),
                member_ref("win1"),
                member_ref("door1")
            )),
        )));

        let geometry = &object.geometries[0];
        assert_eq!(stypes(geometry), vec!["WallSurface", "Window", "Door"]);
        assert_eq!(
            sem_indices(geometry),
            vec![Some(0), Some(1), Some(2)],
            "{report:?}"
        );
        assert_eq!(geometry.surfaces[0].parent, None);
        assert_eq!(geometry.surfaces[0].children, vec![1, 2]);
        assert_eq!(geometry.surfaces[1].parent, Some(0));
        assert!(geometry.surfaces[1].children.is_empty());
        assert_eq!(geometry.surfaces[2].parent, Some(0));
    }

    /// One `WallSurface` with geometry at two levels of detail is one entry
    /// in each geometry's list, not two in either.
    #[test]
    fn a_surface_written_at_two_levels_of_detail_joins_both_geometries() {
        let wall = format!(
            "<bldg:boundedBy><bldg:WallSurface>\
               <bldg:lod2MultiSurface>{}</bldg:lod2MultiSurface>\
               <bldg:lod3MultiSurface>{}</bldg:lod3MultiSurface>\
             </bldg:WallSurface></bldg:boundedBy>",
            multi_surface(&member(&polygon("w2", 1.0))),
            multi_surface(&member(&polygon("w3", 2.0))),
        );
        let (object, _) = read(&building(&format!(
            "<bldg:lod2MultiSurface>{}</bldg:lod2MultiSurface>\
             <bldg:lod3MultiSurface>{}</bldg:lod3MultiSurface>{wall}",
            multi_surface(&member_ref("w2")),
            multi_surface(&member_ref("w3")),
        )));

        assert_eq!(object.geometries.len(), 2);
        for geometry in &object.geometries {
            assert_eq!(stypes(geometry), vec!["WallSurface"]);
            assert_eq!(sem_indices(geometry), vec![Some(0)]);
        }
    }

    /// Semantics with no geometry at their level of detail cannot be written
    /// in CityJSON at all, so they are dropped — and said to be.
    #[test]
    fn boundary_surfaces_at_a_lod_with_no_geometry_are_reported() {
        let (object, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>{}",
            solid(&member(&polygon("f1", 0.0))),
            bounded_by("WallSurface", 3, &member(&polygon("w3", 1.0))),
        )));

        assert!(object.geometries[0].surfaces.is_empty());
        assert_eq!(sem_indices(&object.geometries[0]), vec![None]);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].element, BOUNDED_BY);
        assert!(report.skipped[0].reason.contains("LoD 3"), "{report:?}");
    }

    /// A boundary-surface polygon with no `gml:id` can never be recognised in
    /// the object's geometry, so what is lost is reported rather than dropped
    /// in silence.
    #[test]
    fn a_boundary_polygon_without_a_gml_id_is_reported() {
        let (object, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>\
             <bldg:boundedBy><bldg:WallSurface><bldg:lod2MultiSurface>{}\
             </bldg:lod2MultiSurface></bldg:WallSurface></bldg:boundedBy>",
            solid(&member(&polygon("w1", 1.0))),
            multi_surface(
                r#"<gml:surfaceMember><gml:Polygon><gml:exterior><gml:LinearRing>
                     <gml:posList>0 0 1 1 0 1 1 1 1 0 0 1</gml:posList>
                   </gml:LinearRing></gml:exterior></gml:Polygon></gml:surfaceMember>"#
            ),
        )));

        // The surface still exists — it is only its polygon that is lost.
        assert_eq!(stypes(&object.geometries[0]), vec!["WallSurface"]);
        assert_eq!(sem_indices(&object.geometries[0]), vec![None]);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].element, POLYGON);
        assert!(report.skipped[0].reason.contains("gml:id"), "{report:?}");
    }

    /// The report entries that name a given `gml:id`.
    fn skipped_for<'a>(report: &'a ParseReport, gml_id: &str) -> Vec<&'a Skipped> {
        report
            .skipped
            .iter()
            .filter(|skipped| skipped.gml_id.as_deref() == Some(gml_id))
            .collect()
    }

    /// A polygon written under a boundary surface and named by the solid is
    /// parsed twice — once by each pass — but it is one polygon, and a report
    /// that named it twice would count one loss as two.
    #[test]
    fn a_polygon_dropped_by_both_passes_is_reported_once() {
        let (_, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>{}",
            solid(&format!(
                "{}{}",
                member(&polygon("f1", 0.0)),
                member_ref("flat")
            )),
            bounded_by("WallSurface", 2, &member(&degenerate("flat"))),
        )));

        let entries = skipped_for(&report, "flat");
        assert_eq!(entries.len(), 1, "{report:?}");
        assert_eq!(entries[0].element, POLYGON);
        assert_eq!(entries[0].reason, "degenerate ring");
    }

    /// And a polygon only the boundary pass ever sees is still reported: the
    /// deduplication must not cost a diagnostic nothing else would raise.
    #[test]
    fn a_polygon_only_the_boundary_pass_drops_is_still_reported() {
        let (_, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>{}",
            solid(&member(&polygon("f1", 0.0))),
            bounded_by(
                "WallSurface",
                2,
                &format!(
                    "{}{}",
                    member(&polygon("w1", 1.0)),
                    member(&degenerate("flat"))
                )
            ),
        )));

        let entries = skipped_for(&report, "flat");
        assert_eq!(entries.len(), 1, "{report:?}");
        assert_eq!(entries[0].element, POLYGON);
        assert_eq!(entries[0].reason, "degenerate ring");
    }

    /// Two *different* polygons dropped for the same reason are two losses,
    /// and stay two entries: only an identified element can be shown to be
    /// the same one twice.
    #[test]
    fn two_different_polygons_dropped_alike_stay_two_entries() {
        let (_, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>{}{}",
            solid(&member(&polygon("f1", 0.0))),
            bounded_by("WallSurface", 2, &member(&degenerate("flat-a"))),
            bounded_by("RoofSurface", 2, &member(&degenerate("flat-b"))),
        )));

        assert_eq!(skipped_for(&report, "flat-a").len(), 1, "{report:?}");
        assert_eq!(skipped_for(&report, "flat-b").len(), 1, "{report:?}");
    }

    /// The attributes of a boundary surface are the surface's own, and the
    /// building's are the building's.
    #[test]
    fn a_boundary_surface_keeps_its_own_attributes() {
        let (object, _) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>\
             <bldg:boundedBy><bldg:RoofSurface>\
               <gml:name>North roof</gml:name>\
               <gen:doubleAttribute name=\"slope\"><gen:value>38.7</gen:value></gen:doubleAttribute>\
               <bldg:lod2MultiSurface>{}</bldg:lod2MultiSurface>\
             </bldg:RoofSurface></bldg:boundedBy>",
            solid(&member_ref("r1")),
            multi_surface(&member(&polygon("r1", 3.0))),
        )));

        let attributes = &object.geometries[0].surfaces[0].attributes;
        assert_eq!(attributes["name"], serde_json::json!("North roof"));
        assert_eq!(attributes["slope"], serde_json::json!(38.7));
        assert!(
            object.attributes.is_empty(),
            "the surface's attributes are not the building's: {:?}",
            object.attributes
        );
    }

    /// A `boundedBy` this reader can take nothing from — a reference to a
    /// surface shared with another feature, or a surface type it does not
    /// know — is content that was lost.
    #[test]
    fn a_bounded_by_holding_no_known_surface_is_reported() {
        let (object, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>\
             <bldg:boundedBy xlink:href=\"#shared-wall\"/>",
            solid(&member(&polygon("f1", 0.0))),
        )));

        assert!(object.geometries[0].surfaces.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].element, BOUNDED_BY);
    }

    /// `gml:boundedBy` is an Envelope, not a boundary surface; the two differ
    /// only in their namespace.
    #[test]
    fn a_gml_bounded_by_is_not_a_boundary_surface() {
        let (object, report) = read(&building(&format!(
            "<bldg:lod2Solid>{}</bldg:lod2Solid>\
             <gml:boundedBy><gml:Envelope srsName=\"EPSG:7415\">\
               <gml:lowerCorner>0 0 0</gml:lowerCorner>\
               <gml:upperCorner>1 1 1</gml:upperCorner>\
             </gml:Envelope></gml:boundedBy>",
            solid(&member(&polygon("f1", 0.0))),
        )));

        assert!(object.geometries[0].surfaces.is_empty());
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A `bldg:consistsOfBuildingPart` holding a `BuildingPart` with the
    /// given start-tag attributes and properties.
    fn consists_of(attrs: &str, properties: &str) -> String {
        format!(
            "<bldg:consistsOfBuildingPart><bldg:BuildingPart{attrs}>{properties}\
             </bldg:BuildingPart></bldg:consistsOfBuildingPart>"
        )
    }

    /// A `bldg:outerBuildingInstallation` holding a `BuildingInstallation`.
    fn installation(attrs: &str, properties: &str) -> String {
        format!(
            "<bldg:outerBuildingInstallation><bldg:BuildingInstallation{attrs}>{properties}\
             </bldg:BuildingInstallation></bldg:outerBuildingInstallation>"
        )
    }

    /// A `lod{lod}Solid` holding the cube-less one-face solid over `members`.
    fn lod_solid(lod: u8, members: &str) -> String {
        format!(
            "<bldg:lod{lod}Solid>{}</bldg:lod{lod}Solid>",
            solid(members)
        )
    }

    /// The id and type of each child, in document order.
    fn children_of(object: &IntermediateObject) -> Vec<(&str, &cjseq::CityObjectType)> {
        object
            .children
            .iter()
            .map(|child| (child.id.as_str(), &child.co_type))
            .collect()
    }

    /// Parts and installations become children of the building, in the order
    /// the document writes them, and each keeps its own CityJSON type.
    #[test]
    fn parts_and_installations_become_children_in_document_order() {
        let (object, report) = read(&building(&format!(
            "{}{}{}",
            consists_of(
                r#" gml:id="p1""#,
                &lod_solid(1, &member(&polygon("f1", 0.0)))
            ),
            installation(
                r#" gml:id="i1""#,
                &format!(
                    "<bldg:lod2Geometry>{}</bldg:lod2Geometry>",
                    multi_surface(&member(&polygon("i1-f", 5.0)))
                )
            ),
            consists_of(
                r#" gml:id="p2""#,
                &lod_solid(1, &member(&polygon("f2", 0.0)))
            ),
        )));

        assert_eq!(
            children_of(&object),
            vec![
                ("p1", &cjseq::CityObjectType::BuildingPart),
                ("i1", &cjseq::CityObjectType::BuildingInstallation),
                ("p2", &cjseq::CityObjectType::BuildingPart),
            ]
        );
        // The building's own geometry list is not its children's.
        assert!(object.geometries.is_empty());
        assert_eq!(object.children[0].geometries[0].lod, "1");
        // An installation states its geometry through `lodXGeometry`.
        assert_eq!(object.children[1].geometries[0].lod, "2");
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A child with no `gml:id` is named after its parent and its place among
    /// the children of its kind, counting from one.
    #[test]
    fn a_child_without_a_gml_id_is_named_after_its_parent() {
        let (object, _) = read(&building(&format!(
            "{}{}{}{}",
            consists_of("", &lod_solid(1, &member(&polygon("f1", 0.0)))),
            consists_of(
                r#" gml:id="named""#,
                &lod_solid(1, &member(&polygon("f2", 0.0)))
            ),
            consists_of("", &lod_solid(1, &member(&polygon("f3", 0.0)))),
            installation(
                "",
                &format!(
                    "<bldg:lod2Geometry>{}</bldg:lod2Geometry>",
                    multi_surface(&member(&polygon("f4", 5.0)))
                )
            ),
        )));

        // The counter runs over every child of that kind, so an id that is
        // present does not shift the ones that are generated after it.
        assert_eq!(
            children_of(&object)
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            vec!["b1-part-1", "named", "b1-part-3", "b1-inst-1"]
        );
    }

    /// A part is read by the same reader as the building it belongs to, so it
    /// may hold parts of its own — and the generated ids nest with them.
    #[test]
    fn a_part_may_hold_a_part_of_its_own() {
        let (object, _) = read(&building(&consists_of(
            "",
            &format!(
                "{}{}",
                lod_solid(1, &member(&polygon("f1", 0.0))),
                consists_of("", &lod_solid(2, &member(&polygon("f2", 0.0)))),
            ),
        )));

        let part = &object.children[0];
        assert_eq!(part.id, "b1-part-1");
        assert_eq!(part.geometries[0].lod, "1");
        let nested = &part.children[0];
        assert_eq!(nested.id, "b1-part-1-part-1");
        assert_eq!(nested.co_type, cjseq::CityObjectType::BuildingPart);
        assert_eq!(nested.geometries[0].lod, "2");
    }

    /// Everything the building reader does, the part reader does: attributes,
    /// boundary surfaces and the semantics they carry are all its own.
    #[test]
    fn a_part_reads_its_own_attributes_and_boundary_surfaces() {
        let (object, report) = read(&building(&consists_of(
            r#" gml:id="p1""#,
            &format!(
                "<bldg:measuredHeight uom=\"m\">9.5</bldg:measuredHeight>\
                 <bldg:lod2Solid>{}</bldg:lod2Solid>{}",
                solid(&member_ref("r1")),
                bounded_by("RoofSurface", 2, &member(&polygon("r1", 3.0))),
            ),
        )));

        let part = &object.children[0];
        assert_eq!(part.attributes["measuredHeight"], serde_json::json!(9.5));
        assert_eq!(stypes(&part.geometries[0]), vec!["RoofSurface"]);
        assert_eq!(sem_indices(&part.geometries[0]), vec![Some(0)]);
        // The part's attributes are not the building's.
        assert!(object.attributes.is_empty());
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// The xlink registry covers the whole `cityObjectMember`, so a part's
    /// solid resolves a polygon written under its parent building.
    #[test]
    fn a_parts_solid_resolves_a_polygon_written_on_its_parent() {
        let (object, report) = read(&building(&format!(
            "{}{}",
            // The polygon is written on the building, and named from the part.
            lod_solid(2, &member(&polygon("shared", 1.0))),
            consists_of(r#" gml:id="p1""#, &lod_solid(2, &member_ref("shared"))),
        )));

        let part = &object.children[0];
        assert_eq!(part.geometries[0].geometry.polygons().len(), 1);
        assert_eq!(
            part.geometries[0].geometry.polygons()[0].gml_id.as_deref(),
            Some("shared")
        );
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A child property this reader can take nothing from — a reference to a
    /// part written elsewhere — is content that was lost.
    #[test]
    fn a_child_property_holding_no_known_object_is_reported() {
        let (object, report) = read(&building(
            r##"<bldg:consistsOfBuildingPart xlink:href="#p9"/>
                <bldg:outerBuildingInstallation xlink:href="#i9"/>"##,
        ));

        assert!(object.children.is_empty());
        assert_eq!(report.skipped.len(), 2, "{report:?}");
        assert_eq!(report.skipped[0].element, CONSISTS_OF_BUILDING_PART);
        assert_eq!(report.skipped[1].element, OUTER_BUILDING_INSTALLATION);
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
}
