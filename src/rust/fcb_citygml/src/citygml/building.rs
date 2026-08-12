//! The building module: `bldg:Building`, and the objects nested in it.
//!
//! A building, its `bldg:BuildingPart`s and its `bldg:BuildingInstallation`s
//! are read by [`super::construction`], which is the reader the building, the
//! bridge and the tunnel modules share: CityGML describes all three families
//! the same way and only the names differ. What is left here is the building's
//! own half of that description — its namespaces, the objects it nests, and
//! where it writes its thematic surfaces — and the tests that pin the shared
//! reader's behaviour on the family it was first written for.

use super::construction::{
    ChildKind, ConstructionSpec, BOUNDARY_SURFACES, BOUNDED_BY, OPENING, OPENINGS,
};
use super::semantics::{SurfaceProperty, SurfaceSpec};

/// Namespace URIs of the CityGML building module, 2.0 and 1.0.
const BUILDING_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/building/2.0",
    "http://www.opengis.net/citygml/building/1.0",
];

/// The properties holding a nested city object, and the element each of them
/// holds.
const CONSISTS_OF_BUILDING_PART: &str = "consistsOfBuildingPart";
const BUILDING_PART: &str = "BuildingPart";
const OUTER_BUILDING_INSTALLATION: &str = "outerBuildingInstallation";
const BUILDING_INSTALLATION: &str = "BuildingInstallation";

/// The nested objects this module knows, in no particular order: a document
/// may write them in any, and it is the document's order that is kept.
///
/// `bldg:interiorBuildingInstallation` is deliberately absent. It is a
/// property of a `bldg:Room`, and rooms — with their interior boundary
/// surfaces — arrive with the reader that reads them.
static BUILDING_CHILDREN: [ChildKind; 2] = [
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

/// Where the building module writes its thematic surfaces: under
/// `bldg:boundedBy`, with the openings of each under `bldg:opening`.
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

/// The building module, as the shared construction reader takes it: the one
/// element it claims as a root feature, and everything that element implies.
pub(crate) static BUILDING: ConstructionSpec = ConstructionSpec {
    namespaces: &BUILDING_NS,
    element: "Building",
    co_type: cjseq::CityObjectType::Building,
    children: &BUILDING_CHILDREN,
    surfaces: &BUILDING_SURFACES,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::citygml::construction::{read_construction, spec_of};
    use crate::citygml::semantics::POLYGON;
    use crate::gml::XlinkRegistry;
    use crate::model::{IntermediateGeometry, IntermediateObject};
    use crate::xml::XmlNode;
    use crate::{ParseReport, Skipped};

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
        let object = read_construction(&building, &BUILDING, &building, &registry, 0, &mut report)
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

    /// The building is a member of the construction family, so it is the
    /// shared dispatch that has to recognise it — in both module versions,
    /// and by namespace as well as by name.
    #[test]
    fn a_building_is_recognised_in_both_module_versions() {
        for ns in BUILDING_NS {
            let building = node(&format!(r#"<bldg:Building xmlns:bldg="{ns}"/>"#));
            let spec = spec_of(&building).unwrap_or_else(|| panic!("{ns}"));
            assert_eq!(spec.co_type, cjseq::CityObjectType::Building);
        }
        // The local name alone is not a building.
        assert!(spec_of(&node(r#"<Building/>"#)).is_none());
        assert!(spec_of(&node(r#"<b:Building xmlns:b="urn:example:other"/>"#)).is_none());
        // A part is not a root feature: it is read through the building that
        // holds it.
        assert!(spec_of(&bldg("BuildingPart")).is_none());
    }
}
