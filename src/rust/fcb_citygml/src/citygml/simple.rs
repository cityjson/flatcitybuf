//! The simple thematic modules: vegetation, transportation, water, land use,
//! city furniture, generic objects and groups.
//!
//! What these have in common is what they lack. None of them nests a city
//! object the way a building nests its parts, so each is read into one
//! [`IntermediateObject`] with no recursion: attributes, `lodX…` geometries,
//! and — for a road and a water body — the thematic surfaces that say what
//! each polygon of those geometries is. That makes the whole family a table
//! rather than a reader apiece; [`KINDS`] is the table, and everything a
//! module does differently is a field of it.
//!
//! Two things are not shared with the building module. A road states its
//! semantics under `tran:trafficArea` and `tran:auxiliaryTrafficArea` rather
//! than under a `boundedBy`, which is a [`SurfaceSpec`] with two properties
//! instead of one; and a `grp:CityObjectGroup` holds *references* to city
//! objects rather than objects, which is the one thing in this crate that
//! points out of the feature it is written in.

use super::attributes::read_common_attributes;
use super::semantics::{read_semantic_surfaces, SurfaceProperty, SurfaceSpec};
use super::{member_object_id, read_lod_geometries};
use crate::gml::XlinkRegistry;
use crate::model::IntermediateObject;
use crate::xml::XmlNode;
use crate::{is_in, CityGmlError, ParseReport, Skipped};

/// Namespace URIs of each module this file reads, 2.0 and 1.0.
const VEGETATION_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/vegetation/2.0",
    "http://www.opengis.net/citygml/vegetation/1.0",
];
const TRANSPORTATION_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/transportation/2.0",
    "http://www.opengis.net/citygml/transportation/1.0",
];
const WATERBODY_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/waterbody/2.0",
    "http://www.opengis.net/citygml/waterbody/1.0",
];
const LANDUSE_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/landuse/2.0",
    "http://www.opengis.net/citygml/landuse/1.0",
];
const CITYFURNITURE_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/cityfurniture/2.0",
    "http://www.opengis.net/citygml/cityfurniture/1.0",
];
const GENERICS_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/generics/2.0",
    "http://www.opengis.net/citygml/generics/1.0",
];
const CITYOBJECTGROUP_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/cityobjectgroup/2.0",
    "http://www.opengis.net/citygml/cityobjectgroup/1.0",
];

/// The properties of a road, a railway or a square that carry semantics, and
/// the areas each holds.
///
/// Transportation is the module that does not use `boundedBy`: it names the
/// property after the kind of area, and the two areas are two CityJSON
/// semantic surface types spelled the same way. They are still boundary
/// surfaces in every way that matters here — the road's `lodXMultiSurface`
/// names their polygons by `gml:id`, exactly as a building's solid names the
/// polygons of its walls.
static TRAFFIC_SURFACES: SurfaceSpec = SurfaceSpec {
    namespaces: &TRANSPORTATION_NS,
    properties: &[
        SurfaceProperty {
            property: "trafficArea",
            elements: &["TrafficArea"],
        },
        SurfaceProperty {
            property: "auxiliaryTrafficArea",
            elements: &["AuxiliaryTrafficArea"],
        },
    ],
    openings: &[],
    container: "trafficArea",
};

/// The thematic surfaces of a water body, written under `wtr:boundedBy` as a
/// building's are.
static WATER_SURFACES: SurfaceSpec = SurfaceSpec {
    namespaces: &WATERBODY_NS,
    properties: &[SurfaceProperty {
        property: "boundedBy",
        elements: &["WaterSurface", "WaterGroundSurface", "WaterClosureSurface"],
    }],
    openings: &[],
    container: "boundedBy",
};

/// What a city object element becomes in CityJSON.
enum CityJsonType {
    /// One of the types CityJSON names itself, which is the usual case: the
    /// two standards agree on the spelling.
    Known(cjseq::CityObjectType),
    /// A CityJSON Extension type, carrying the name verbatim. CityJSON has no
    /// type for a `gen:GenericCityObject` — an object that is deliberately
    /// nothing in particular — and § 8 requires an Extension name to start
    /// with `+`.
    Extension(&'static str),
}

impl CityJsonType {
    /// The cjseq type this stands for.
    fn co_type(&self) -> cjseq::CityObjectType {
        match self {
            Self::Known(co_type) => co_type.clone(),
            Self::Extension(name) => cjseq::CityObjectType::Extension((*name).to_string()),
        }
    }
}

/// One city object element of a simple thematic module.
pub(crate) struct SimpleKind {
    /// The namespaces of the module that defines it, 2.0 and 1.0.
    namespaces: &'static [&'static str],
    /// The element's local name.
    element: &'static str,
    /// What the object becomes in CityJSON.
    co_type: CityJsonType,
    /// Where its thematic surfaces are written, if it has any.
    surfaces: Option<&'static SurfaceSpec>,
}

/// Every element this file reads.
///
/// The geometry property names are deliberately absent: `lodX…` is scanned by
/// name in [`super::read_lod_geometries`], so a `veg:lod1Geometry`, a
/// `luse:lod1MultiSurface` and a `wtr:lod2Solid` all arrive without an entry
/// here saying which module spells it which way.
static KINDS: [SimpleKind; 11] = [
    SimpleKind {
        namespaces: &VEGETATION_NS,
        element: "SolitaryVegetationObject",
        co_type: CityJsonType::Known(cjseq::CityObjectType::SolitaryVegetationObject),
        surfaces: None,
    },
    SimpleKind {
        namespaces: &VEGETATION_NS,
        element: "PlantCover",
        co_type: CityJsonType::Known(cjseq::CityObjectType::PlantCover),
        surfaces: None,
    },
    SimpleKind {
        namespaces: &TRANSPORTATION_NS,
        element: "Road",
        co_type: CityJsonType::Known(cjseq::CityObjectType::Road),
        surfaces: Some(&TRAFFIC_SURFACES),
    },
    SimpleKind {
        namespaces: &TRANSPORTATION_NS,
        element: "Railway",
        co_type: CityJsonType::Known(cjseq::CityObjectType::Railway),
        surfaces: Some(&TRAFFIC_SURFACES),
    },
    // CityGML spells a square `Square` and CityJSON spells the same thing
    // `TransportSquare`. Both element names are accepted: the CityJSON
    // spelling is what a document round-tripped through a CityJSON tool comes
    // back as, and reading it costs one row.
    SimpleKind {
        namespaces: &TRANSPORTATION_NS,
        element: "Square",
        co_type: CityJsonType::Known(cjseq::CityObjectType::TransportSquare),
        surfaces: Some(&TRAFFIC_SURFACES),
    },
    SimpleKind {
        namespaces: &TRANSPORTATION_NS,
        element: "TransportSquare",
        co_type: CityJsonType::Known(cjseq::CityObjectType::TransportSquare),
        surfaces: Some(&TRAFFIC_SURFACES),
    },
    SimpleKind {
        namespaces: &WATERBODY_NS,
        element: "WaterBody",
        co_type: CityJsonType::Known(cjseq::CityObjectType::WaterBody),
        surfaces: Some(&WATER_SURFACES),
    },
    SimpleKind {
        namespaces: &LANDUSE_NS,
        element: "LandUse",
        co_type: CityJsonType::Known(cjseq::CityObjectType::LandUse),
        surfaces: None,
    },
    SimpleKind {
        namespaces: &CITYFURNITURE_NS,
        element: "CityFurniture",
        co_type: CityJsonType::Known(cjseq::CityObjectType::CityFurniture),
        surfaces: None,
    },
    SimpleKind {
        namespaces: &GENERICS_NS,
        element: "GenericCityObject",
        co_type: CityJsonType::Extension(GENERIC_CITY_OBJECT),
        surfaces: None,
    },
    SimpleKind {
        namespaces: &CITYOBJECTGROUP_NS,
        element: "CityObjectGroup",
        co_type: CityJsonType::Known(cjseq::CityObjectType::CityObjectGroup),
        surfaces: None,
    },
];

/// The CityJSON Extension type a `gen:GenericCityObject` becomes.
const GENERIC_CITY_OBJECT: &str = "+GenericCityObject";

/// Local name of the property naming a member of a group, and of the
/// attribute stating what that member is to the group.
const GROUP_MEMBER: &str = "groupMember";
const ROLE_ATTR: &str = "role";

/// Local name of the XLink locator attribute. Attributes are matched on their
/// local name alone, so this reaches `xlink:href` under any prefix.
const HREF_ATTR: &str = "href";

/// The kind of object this node is, if it is one this file reads.
pub(crate) fn kind_of(node: &XmlNode) -> Option<&'static SimpleKind> {
    KINDS
        .iter()
        .find(|kind| node.local == kind.element && kind.namespaces.contains(&node.ns.as_str()))
}

/// Read one simple thematic city object into the intermediate model.
///
/// `registry` indexes the polygons of the whole `cityObjectMember`, so a
/// road's surface resolves the polygons written under its traffic areas.
/// `member_index` names the object when it carries no `gml:id`.
///
/// The order of the steps is not free: the geometries must be read before the
/// semantics, because the semantics pass deduplicates its diagnostics against
/// what the first one recorded. See [`read_semantic_surfaces`].
///
/// # Errors
///
/// Propagates the geometry reader's errors: malformed geometry, and
/// `xlink:href`s that name nothing in the member.
pub(crate) fn read_simple_object(
    node: &XmlNode,
    kind: &SimpleKind,
    member: &XmlNode,
    registry: &XlinkRegistry,
    member_index: usize,
    report: &mut ParseReport,
) -> Result<IntermediateObject, CityGmlError> {
    let mut object =
        IntermediateObject::new(member_object_id(node, member_index), kind.co_type.co_type());
    read_common_attributes(node, &mut object.attributes, report);
    object.geometries = read_lod_geometries(node, member, registry, report)?;
    if let Some(spec) = kind.surfaces {
        read_semantic_surfaces(node, spec, registry, &mut object.geometries, report)?;
    }
    object.group_members = read_group_members(node, report);
    Ok(object)
}

/// The members of a `grp:CityObjectGroup`: the id each names, and the role it
/// gives that member.
///
/// A group's members are references, and they are the one place this converter
/// keeps an id it cannot resolve. A CityJSONSeq document is a feature per
/// line, so a group and the objects it groups are ordinarily in *different*
/// features, and the group names them by id across those lines. Following the
/// reference to check it would mean holding the whole document, which is
/// exactly what this reader is written not to do, so the id is kept verbatim.
///
/// A member written inline — a city object nested in the group rather than
/// referenced from it — is legal CityGML and lost here: it would have to
/// become a city object of its own, and a group is not the feature its members
/// belong to. It is reported rather than dropped in silence.
///
/// Scanning for the property costs nothing on an object of another module: no
/// module but `cityobjectgroup` defines a `groupMember`, so the loop finds
/// none.
fn read_group_members(node: &XmlNode, report: &mut ParseReport) -> Vec<(String, Option<String>)> {
    let mut members = Vec::new();
    for property in &node.children {
        if !is_in(property, &CITYOBJECTGROUP_NS, GROUP_MEMBER) {
            continue;
        }
        // A reference within the document is written `#id`; anything else is
        // a reference this converter cannot follow either way, and keeping it
        // as it stands says more than dropping it.
        let id = property
            .attr(HREF_ATTR)
            .map(|href| href.strip_prefix('#').unwrap_or(href))
            .filter(|id| !id.is_empty());
        let Some(id) = id else {
            report.skipped.push(Skipped {
                element: property.local.clone(),
                gml_id: property.gml_id().map(str::to_owned),
                reason: format!(
                    "<{GROUP_MEMBER}> names no city object with an xlink:href; \
                     the member is dropped"
                ),
            });
            continue;
        };
        members.push((id.to_string(), property.attr(ROLE_ATTR).map(str::to_owned)));
    }
    members
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::IntermediateGeometry;

    fn node(xml: &str) -> XmlNode {
        crate::xml::parse_str_for_tests(xml).unwrap()
    }

    /// The namespaces every fixture below binds.
    const NS: &str = r#"xmlns:veg="http://www.opengis.net/citygml/vegetation/2.0"
         xmlns:tran="http://www.opengis.net/citygml/transportation/2.0"
         xmlns:wtr="http://www.opengis.net/citygml/waterbody/2.0"
         xmlns:grp="http://www.opengis.net/citygml/cityobjectgroup/2.0"
         xmlns:gen="http://www.opengis.net/citygml/generics/2.0"
         xmlns:gml="http://www.opengis.net/gml"
         xmlns:xlink="http://www.w3.org/1999/xlink""#;

    /// Read one city object, with the xlink registry the member scan would
    /// have collected for it.
    fn read(xml: &str) -> (IntermediateObject, ParseReport) {
        let node = node(xml);
        let kind = kind_of(&node).unwrap_or_else(|| panic!("no reader for <{}>", node.local));
        let registry = XlinkRegistry::collect(&node);
        let mut report = ParseReport::default();
        let object = read_simple_object(&node, kind, &node, &registry, 0, &mut report)
            .unwrap_or_else(|err| panic!("read failed: {err}"));
        (object, report)
    }

    /// A unit square at height `z`, as a `gml:Polygon` carrying `gml:id`.
    fn polygon(gml_id: &str, z: f64) -> String {
        format!(
            r#"<gml:surfaceMember><gml:Polygon gml:id="{gml_id}"><gml:exterior><gml:LinearRing>
                 <gml:posList>0 0 {z} 1 0 {z} 1 1 {z} 0 0 {z}</gml:posList>
               </gml:LinearRing></gml:exterior></gml:Polygon></gml:surfaceMember>"#
        )
    }

    /// A `gml:surfaceMember` holding an `xlink:href` to `gml_id`.
    fn member_ref(gml_id: &str) -> String {
        format!(r##"<gml:surfaceMember xlink:href="#{gml_id}"/>"##)
    }

    /// A `gml:MultiSurface` around `members`.
    fn multi_surface(members: &str) -> String {
        format!("<gml:MultiSurface>{members}</gml:MultiSurface>")
    }

    /// The types of a geometry's semantic surfaces, in index order.
    fn stypes(geometry: &IntermediateGeometry) -> Vec<&str> {
        geometry
            .surfaces
            .iter()
            .map(|surface| surface.stype.as_str())
            .collect()
    }

    /// The semantic surface each polygon of a geometry points at, in document
    /// order.
    fn sem_indices(geometry: &IntermediateGeometry) -> Vec<Option<usize>> {
        geometry
            .geometry
            .polygons()
            .iter()
            .map(|polygon| polygon.sem_idx)
            .collect()
    }

    /// Every element in the table is recognised in both module versions, and
    /// becomes the CityJSON type the table names.
    #[test]
    fn each_element_is_recognised_in_both_module_versions() {
        for kind in &KINDS {
            for ns in kind.namespaces {
                let element = node(&format!(r#"<m:{} xmlns:m="{ns}"/>"#, kind.element));
                let found = kind_of(&element).unwrap_or_else(|| panic!("{ns} {}", kind.element));
                assert_eq!(found.co_type.co_type(), kind.co_type.co_type());
            }
        }
    }

    /// The local name alone is not a city object: an application schema is
    /// free to define a `Road` of its own, and it is not the CityGML one.
    #[test]
    fn the_local_name_alone_is_not_a_city_object() {
        assert!(kind_of(&node(r#"<Road/>"#)).is_none());
        assert!(kind_of(&node(r#"<x:Road xmlns:x="urn:example:other"/>"#)).is_none());
        // A traffic area is a semantic surface, not a city object of its own.
        assert!(kind_of(&node(&format!("<tran:TrafficArea {NS}/>"))).is_none());
    }

    /// A `gen:GenericCityObject` has no CityJSON type of its own, so it takes
    /// an Extension type — which the spec requires to start with `+`.
    #[test]
    fn a_generic_city_object_becomes_an_extension_type() {
        let (object, report) = read(&format!(
            "<gen:GenericCityObject {NS} gml:id=\"g1\">\
               <gml:name>Retaining wall</gml:name>\
               <gen:lod1Geometry>{}</gen:lod1Geometry>\
             </gen:GenericCityObject>",
            multi_surface(&polygon("g1-p1", 0.0))
        ));

        assert_eq!(
            object.co_type,
            cjseq::CityObjectType::Extension(GENERIC_CITY_OBJECT.to_string())
        );
        assert!(matches!(
            object.co_type,
            cjseq::CityObjectType::Extension(ref name) if name.starts_with('+')
        ));
        assert_eq!(
            object.attributes["name"],
            serde_json::json!("Retaining wall")
        );
        assert_eq!(object.geometries[0].lod, "1");
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// The traffic areas of a road become the semantic surfaces of the
    /// geometry at their own level of detail, in document order, and each
    /// keeps its own attributes.
    #[test]
    fn traffic_areas_become_the_semantic_surfaces_of_the_road() {
        let (object, report) = read(&format!(
            "<tran:Road {NS} gml:id=\"r1\">\
               <tran:lod2MultiSurface>{}</tran:lod2MultiSurface>\
               <tran:trafficArea><tran:TrafficArea gml:id=\"ta1\">\
                 <tran:function>1</tran:function>\
                 <tran:lod2MultiSurface>{}</tran:lod2MultiSurface>\
               </tran:TrafficArea></tran:trafficArea>\
               <tran:auxiliaryTrafficArea><tran:AuxiliaryTrafficArea gml:id=\"ata1\">\
                 <tran:lod2MultiSurface>{}</tran:lod2MultiSurface>\
               </tran:AuxiliaryTrafficArea></tran:auxiliaryTrafficArea>\
             </tran:Road>",
            multi_surface(&format!("{}{}", member_ref("ta-p1"), member_ref("ata-p1"))),
            multi_surface(&polygon("ta-p1", 0.0)),
            multi_surface(&polygon("ata-p1", 1.0)),
        ));

        let geometry = &object.geometries[0];
        assert_eq!(object.co_type, cjseq::CityObjectType::Road);
        assert_eq!(geometry.lod, "2");
        assert_eq!(
            stypes(geometry),
            vec!["TrafficArea", "AuxiliaryTrafficArea"]
        );
        assert_eq!(sem_indices(geometry), vec![Some(0), Some(1)]);
        assert_eq!(
            geometry.surfaces[0].attributes["function"],
            serde_json::json!("1")
        );
        // A traffic area's attributes are not the road's.
        assert!(object.attributes.is_empty(), "{:?}", object.attributes);
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A water body states its surfaces under `boundedBy`, as a building
    /// does, and the geometry may be written after them.
    #[test]
    fn a_water_body_reads_its_bounded_by_surfaces() {
        let (object, report) = read(&format!(
            "<wtr:WaterBody {NS} gml:id=\"w1\">\
               <wtr:boundedBy><wtr:WaterSurface>\
                 <wtr:lod2MultiSurface>{}</wtr:lod2MultiSurface>\
               </wtr:WaterSurface></wtr:boundedBy>\
               <wtr:boundedBy><wtr:WaterGroundSurface>\
                 <wtr:lod2MultiSurface>{}</wtr:lod2MultiSurface>\
               </wtr:WaterGroundSurface></wtr:boundedBy>\
               <wtr:lod2MultiSurface>{}</wtr:lod2MultiSurface>\
             </wtr:WaterBody>",
            multi_surface(&polygon("top", 1.0)),
            multi_surface(&polygon("bottom", 0.0)),
            multi_surface(&format!("{}{}", member_ref("bottom"), member_ref("top"))),
        ));

        let geometry = &object.geometries[0];
        assert_eq!(stypes(geometry), vec!["WaterSurface", "WaterGroundSurface"]);
        // The geometry's own order, not the order the surfaces were written.
        assert_eq!(sem_indices(geometry), vec![Some(1), Some(0)]);
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A group's members are references: the `#` is a same-document locator
    /// and not part of the id, and a member with no role gets `None` rather
    /// than a shorter list.
    #[test]
    fn group_members_become_ids_and_roles() {
        let (object, report) = read(&format!(
            r##"<grp:CityObjectGroup {NS} gml:id="g1">
                  <gml:name>Green corridor</gml:name>
                  <grp:groupMember xlink:href="#tree-1" role="part"/>
                  <grp:groupMember xlink:href="#road-1"/>
                </grp:CityObjectGroup>"##
        ));

        assert_eq!(object.co_type, cjseq::CityObjectType::CityObjectGroup);
        assert_eq!(
            object.group_members,
            vec![
                ("tree-1".to_string(), Some("part".to_string())),
                ("road-1".to_string(), None),
            ]
        );
        assert!(object.children.is_empty());
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A member of a group in another document is kept as it stands: this
    /// reader cannot follow it either way, and an id says more than nothing.
    #[test]
    fn a_group_member_without_a_fragment_is_kept_verbatim() {
        let (object, report) = read(&format!(
            r##"<grp:CityObjectGroup {NS} gml:id="g1">
                  <grp:groupMember xlink:href="other.gml#b1"/>
                </grp:CityObjectGroup>"##
        ));

        assert_eq!(
            object.group_members,
            vec![("other.gml#b1".to_string(), None)]
        );
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A member written inline rather than referenced is content this
    /// converter loses, so it says so.
    #[test]
    fn an_inline_group_member_is_reported() {
        let (object, report) = read(&format!(
            r##"<grp:CityObjectGroup {NS} gml:id="g1">
                  <grp:groupMember>
                    <veg:PlantCover gml:id="cover-1"/>
                  </grp:groupMember>
                  <grp:groupMember xlink:href="#tree-1"/>
                </grp:CityObjectGroup>"##
        ));

        // The member that could be read still is.
        assert_eq!(object.group_members, vec![("tree-1".to_string(), None)]);
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].element, GROUP_MEMBER);
    }

    /// Only a group has members: a `groupMember` is not a property any other
    /// module defines, so the scan costs nothing on the objects that have
    /// none.
    #[test]
    fn an_object_of_another_module_has_no_group_members() {
        let (object, _) = read(&format!(
            "<veg:PlantCover {NS} gml:id=\"c1\">\
               <veg:averageHeight>2.5</veg:averageHeight>\
               <veg:lod1MultiSurface>{}</veg:lod1MultiSurface>\
             </veg:PlantCover>",
            multi_surface(&polygon("c1-p1", 0.0))
        ));

        assert_eq!(object.co_type, cjseq::CityObjectType::PlantCover);
        assert_eq!(object.attributes["averageHeight"], serde_json::json!(2.5));
        assert!(object.group_members.is_empty());
    }
}
