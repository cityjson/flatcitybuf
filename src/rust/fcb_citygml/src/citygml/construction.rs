//! The construction family: the reader shared by buildings, bridges and
//! tunnels, and the bridge and tunnel modules themselves.
//!
//! CityGML describes the three the same way. Each has a root feature with
//! attributes, `lodX…` geometry properties and thematic boundary surfaces
//! under `boundedBy`; each nests objects of its own — parts, installations,
//! and for a bridge its construction elements — and a part may hold parts, so
//! the reading is recursive; and the whole tree becomes one CityJSON feature.
//! Only the names differ: `bldg:consistsOfBuildingPart` against
//! `brid:consistsOfBridgePart`, and one namespace against another.
//!
//! So the reader is one reader, parameterised by a [`ConstructionSpec`] per
//! module. [`super::building`] holds the building's spec, which is where the
//! building module's own tests live; the bridge's and the tunnel's are here.
//!
//! Each addition is additive: a property this reader does not recognise is
//! passed over silently rather than reported, because at this stage nearly
//! every property of a real feature is one of those. A property that *is*
//! recognised and still yields nothing — a `boundedBy` or a
//! `consistsOfBridgePart` that only references an object elsewhere — is
//! reported, because that is content this converter lost.

use super::attributes::read_common_attributes;
use super::semantics::{read_semantic_surfaces, SurfaceProperty, SurfaceSpec};
use super::{member_object_id, read_lod_geometries};
use crate::gml::XlinkRegistry;
use crate::model::IntermediateObject;
use crate::xml::XmlNode;
use crate::{is_in, CityGmlError, ParseReport, Skipped};

/// Local name of the property holding a thematic boundary surface, and of the
/// property holding one of that surface's openings. All three modules spell
/// them the same way, each in its own namespace.
pub(crate) const BOUNDED_BY: &str = "boundedBy";
pub(crate) const OPENING: &str = "opening";

/// The thematic boundary surfaces of the construction family, each of which is
/// a CityJSON semantic surface type spelled the same way. The building, bridge
/// and tunnel modules each declare this same set.
///
/// The interior surfaces — `InteriorWallSurface`, `CeilingSurface`,
/// `FloorSurface` — are deliberately absent: they are boundaries of a
/// `bldg:Room` or a `tun:HollowSpace`, not of the construction itself, and
/// they arrive with the reader that reads those.
pub(crate) const BOUNDARY_SURFACES: [&str; 6] = [
    "RoofSurface",
    "WallSurface",
    "GroundSurface",
    "ClosureSurface",
    "OuterCeilingSurface",
    "OuterFloorSurface",
];

/// The openings a boundary surface may hold. Each becomes a semantic surface
/// of its own, pointing at the surface it opens.
pub(crate) const OPENINGS: [&str; 2] = ["Window", "Door"];

/// Namespace URIs of the CityGML bridge and tunnel modules, 2.0 and 1.0.
const BRIDGE_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/bridge/2.0",
    "http://www.opengis.net/citygml/bridge/1.0",
];
const TUNNEL_NS: [&str; 2] = [
    "http://www.opengis.net/citygml/tunnel/2.0",
    "http://www.opengis.net/citygml/tunnel/1.0",
];

/// One kind of nested object a construction may hold.
pub(crate) struct ChildKind {
    /// The property that carries the object.
    pub property: &'static str,
    /// The element that property holds.
    pub element: &'static str,
    /// The CityJSON type the object becomes.
    pub co_type: cjseq::CityObjectType,
    /// The word a generated id is built from: `b1-part-2`, `b1-inst-1`,
    /// `bridge-1-const-1`.
    pub id_word: &'static str,
}

/// One module of the construction family: what its root feature is called,
/// what it becomes, what it may nest and where it writes its semantics.
pub(crate) struct ConstructionSpec {
    /// The namespaces of the module that defines it, 2.0 and 1.0.
    pub namespaces: &'static [&'static str],
    /// The local name of the root feature: `Building`, `Bridge`, `Tunnel`.
    pub element: &'static str,
    /// The CityJSON type that root feature becomes.
    pub co_type: cjseq::CityObjectType,
    /// The nested objects this module knows, in no particular order: a
    /// document may write them in any, and it is the document's order that is
    /// kept.
    pub children: &'static [ChildKind],
    /// Where the module writes its thematic surfaces.
    pub surfaces: &'static SurfaceSpec,
}

/// Where the bridge and tunnel modules write their thematic surfaces: under
/// `boundedBy`, with the openings of each under `opening`, exactly as the
/// building module does.
static BRIDGE_SURFACES: SurfaceSpec = SurfaceSpec {
    namespaces: &BRIDGE_NS,
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

static TUNNEL_SURFACES: SurfaceSpec = SurfaceSpec {
    namespaces: &TUNNEL_NS,
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

/// The objects a bridge nests.
///
/// CityGML spells the third element `BridgeConstructionElement` and CityJSON
/// spells the same thing `BridgeConstructiveElement`; the property that holds
/// it is `outerBridgeConstruction`, which is why a generated id for one reads
/// `{parent}-const-{n}` rather than borrowing the element's longer name.
///
/// `brid:interiorBridgeInstallation` is deliberately absent, as its building
/// counterpart is: it is a property of a `brid:BridgeRoom`, and rooms arrive
/// with the reader that reads them.
static BRIDGE_CHILDREN: [ChildKind; 3] = [
    ChildKind {
        property: "consistsOfBridgePart",
        element: "BridgePart",
        co_type: cjseq::CityObjectType::BridgePart,
        id_word: "part",
    },
    ChildKind {
        property: "outerBridgeInstallation",
        element: "BridgeInstallation",
        co_type: cjseq::CityObjectType::BridgeInstallation,
        id_word: "inst",
    },
    ChildKind {
        property: "outerBridgeConstruction",
        element: "BridgeConstructionElement",
        co_type: cjseq::CityObjectType::BridgeConstructiveElement,
        id_word: "const",
    },
];

/// The objects a tunnel nests. A tunnel has no construction elements in
/// CityGML 2.0, and its `tun:interiorTunnelInstallation` belongs to a
/// `tun:HollowSpace` rather than to the tunnel.
static TUNNEL_CHILDREN: [ChildKind; 2] = [
    ChildKind {
        property: "consistsOfTunnelPart",
        element: "TunnelPart",
        co_type: cjseq::CityObjectType::TunnelPart,
        id_word: "part",
    },
    ChildKind {
        property: "outerTunnelInstallation",
        element: "TunnelInstallation",
        co_type: cjseq::CityObjectType::TunnelInstallation,
        id_word: "inst",
    },
];

static BRIDGE: ConstructionSpec = ConstructionSpec {
    namespaces: &BRIDGE_NS,
    element: "Bridge",
    co_type: cjseq::CityObjectType::Bridge,
    children: &BRIDGE_CHILDREN,
    surfaces: &BRIDGE_SURFACES,
};

static TUNNEL: ConstructionSpec = ConstructionSpec {
    namespaces: &TUNNEL_NS,
    element: "Tunnel",
    co_type: cjseq::CityObjectType::Tunnel,
    children: &TUNNEL_CHILDREN,
    surfaces: &TUNNEL_SURFACES,
};

/// Every module this reader reads, the building's spec included.
static SPECS: [&ConstructionSpec; 3] = [&super::building::BUILDING, &BRIDGE, &TUNNEL];

/// The module this node is the root feature of, if it is one this reader
/// reads.
///
/// The local name alone is not enough: an application schema may define a
/// `Bridge` of its own, and it is not the CityGML one.
pub(crate) fn spec_of(node: &XmlNode) -> Option<&'static ConstructionSpec> {
    SPECS
        .into_iter()
        .find(|spec| is_in(node, spec.namespaces, spec.element))
}

/// Read a construction's root feature — a `bldg:Building`, a `brid:Bridge`, a
/// `tun:Tunnel` — into the intermediate model.
///
/// `registry` indexes the polygons of the whole `cityObjectMember`, so a solid
/// whose faces are `xlink:href`s to polygons written elsewhere in the feature
/// resolves. `member_index` names the object when it carries no `gml:id`; the
/// generated id is stable for a given document, which matters because it ends
/// up as a CityJSON object key.
///
/// # Errors
///
/// Propagates the geometry reader's errors: malformed geometry, and
/// references that name no polygon in the member.
pub(crate) fn read_construction(
    node: &XmlNode,
    spec: &ConstructionSpec,
    member: &XmlNode,
    registry: &XlinkRegistry,
    member_index: usize,
    report: &mut ParseReport,
) -> Result<IntermediateObject, CityGmlError> {
    let id = member_object_id(node, member_index);
    read_construction_object(
        node,
        id,
        spec.co_type.clone(),
        spec,
        member,
        registry,
        report,
    )
}

/// Read one object of a construction family — a `Bridge`, a `BridgePart`, a
/// `TunnelInstallation` — into the intermediate model.
///
/// A root feature and the objects nested in it are read alike because CityGML
/// describes them alike: each is an `_AbstractBuilding`, an
/// `_AbstractBridge` or a feature shaped like one, with its own attributes,
/// its own `lodX…` geometry properties, its own boundary surfaces and — for
/// the parts — nested objects of its own. That is what makes this recursive.
///
/// `id` is settled by the caller, because what names an object without a
/// `gml:id` differs: a root feature is named after its member's position, a
/// nested object after its parent and its place among that parent's children.
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
fn read_construction_object(
    node: &XmlNode,
    id: String,
    co_type: cjseq::CityObjectType,
    spec: &ConstructionSpec,
    member: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<IntermediateObject, CityGmlError> {
    let mut object = IntermediateObject::new(id, co_type);
    read_common_attributes(node, &mut object.attributes, report);
    object.geometries = read_lod_geometries(node, member, registry, report)?;
    read_semantic_surfaces(
        node,
        spec.surfaces,
        registry,
        &mut object.geometries,
        report,
    )?;
    object.children = read_children(node, spec, &object.id, member, registry, report)?;
    Ok(object)
}

/// Read the objects nested in one construction object, in document order.
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
/// Propagates the nested objects' errors, as [`read_construction_object`]
/// does.
fn read_children(
    node: &XmlNode,
    spec: &ConstructionSpec,
    parent_id: &str,
    member: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Vec<IntermediateObject>, CityGmlError> {
    let mut children = Vec::new();
    // One counter per kind, so that the parts and the installations of one
    // parent are numbered independently of each other.
    let mut counts = vec![0usize; spec.children.len()];
    for property in &node.children {
        let Some((index, kind)) = spec
            .children
            .iter()
            .enumerate()
            .find(|(_, kind)| is_in(property, spec.namespaces, kind.property))
        else {
            continue;
        };
        let mut read_any = false;
        for child in &property.children {
            if !is_in(child, spec.namespaces, kind.element) {
                continue;
            }
            counts[index] += 1;
            let id = child
                .gml_id()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{parent_id}-{}-{}", kind.id_word, counts[index]));
            children.push(read_construction_object(
                child,
                id,
                kind.co_type.clone(),
                spec,
                member,
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
    use crate::model::IntermediateGeometry;

    fn node(xml: &str) -> XmlNode {
        crate::xml::parse_str_for_tests(xml).unwrap()
    }

    /// The namespaces every fixture below binds.
    const NS: &str = r#"xmlns:brid="http://www.opengis.net/citygml/bridge/2.0"
         xmlns:tun="http://www.opengis.net/citygml/tunnel/2.0"
         xmlns:gml="http://www.opengis.net/gml"
         xmlns:xlink="http://www.w3.org/1999/xlink""#;

    /// Read one root feature, with the xlink registry the member scan would
    /// have collected for it.
    fn read(xml: &str) -> (IntermediateObject, ParseReport) {
        let root = node(xml);
        let spec = spec_of(&root).unwrap_or_else(|| panic!("no reader for <{}>", root.local));
        let registry = XlinkRegistry::collect(&root);
        let mut report = ParseReport::default();
        let object = read_construction(&root, spec, &root, &registry, 0, &mut report)
            .unwrap_or_else(|err| panic!("read failed: {err}"));
        (object, report)
    }

    /// A `gml:surfaceMember` holding a unit square at height `z`, with the
    /// given `gml:id` on its polygon.
    fn member(gml_id: &str, z: f64) -> String {
        format!(
            r#"<gml:surfaceMember><gml:Polygon gml:id="{gml_id}"><gml:exterior><gml:LinearRing>
                 <gml:posList>0 0 {z} 1 0 {z} 1 1 {z} 0 0 {z}</gml:posList>
               </gml:LinearRing></gml:exterior></gml:Polygon></gml:surfaceMember>"#
        )
    }

    /// A `gml:MultiSurface` around `members`.
    fn multi_surface(members: &str) -> String {
        format!("<gml:MultiSurface>{members}</gml:MultiSurface>")
    }

    /// The id and type of each child, in document order.
    fn children_of(object: &IntermediateObject) -> Vec<(&str, &cjseq::CityObjectType)> {
        object
            .children
            .iter()
            .map(|child| (child.id.as_str(), &child.co_type))
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

    /// Every module's root feature is recognised in both module versions, and
    /// nothing else is.
    #[test]
    fn each_root_feature_is_recognised_in_both_module_versions() {
        for spec in SPECS {
            for ns in spec.namespaces {
                let element = node(&format!(r#"<m:{} xmlns:m="{ns}"/>"#, spec.element));
                let found = spec_of(&element).unwrap_or_else(|| panic!("{ns} {}", spec.element));
                assert_eq!(found.co_type, spec.co_type);
            }
        }
        // The local name alone is not a city object, and a nested element is
        // not a root feature.
        assert!(spec_of(&node(r#"<Bridge/>"#)).is_none());
        assert!(spec_of(&node(r#"<x:Bridge xmlns:x="urn:example:other"/>"#)).is_none());
        assert!(spec_of(&node(&format!("<brid:BridgePart {NS}/>"))).is_none());
    }

    /// A bridge's three kinds of nested object each become a child of their
    /// own type, in the order the document writes them.
    #[test]
    fn a_bridge_nests_parts_installations_and_construction_elements() {
        let (object, report) = read(&format!(
            r#"<brid:Bridge {NS} gml:id="br1">
                 <gml:name>Foot bridge</gml:name>
                 <brid:consistsOfBridgePart>
                   <brid:BridgePart gml:id="bp1">
                     <brid:lod2MultiSurface>{}</brid:lod2MultiSurface>
                   </brid:BridgePart>
                 </brid:consistsOfBridgePart>
                 <brid:outerBridgeConstruction>
                   <brid:BridgeConstructionElement>
                     <brid:lod2Geometry>{}</brid:lod2Geometry>
                   </brid:BridgeConstructionElement>
                 </brid:outerBridgeConstruction>
                 <brid:outerBridgeInstallation>
                   <brid:BridgeInstallation>
                     <brid:lod2Geometry>{}</brid:lod2Geometry>
                   </brid:BridgeInstallation>
                 </brid:outerBridgeInstallation>
               </brid:Bridge>"#,
            multi_surface(&member("bp-p1", 0.0)),
            multi_surface(&member("bce-p1", 1.0)),
            multi_surface(&member("bi-p1", 2.0)),
        ));

        assert_eq!(object.co_type, cjseq::CityObjectType::Bridge);
        assert_eq!(object.attributes["name"], serde_json::json!("Foot bridge"));
        assert_eq!(
            children_of(&object),
            vec![
                ("bp1", &cjseq::CityObjectType::BridgePart),
                // CityGML's BridgeConstructionElement is CityJSON's
                // BridgeConstructiveElement, and a generated id for one is
                // named after the property that holds it.
                (
                    "br1-const-1",
                    &cjseq::CityObjectType::BridgeConstructiveElement
                ),
                ("br1-inst-1", &cjseq::CityObjectType::BridgeInstallation),
            ]
        );
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A tunnel nests parts and installations, and a part is read by the same
    /// reader, so it may hold parts of its own.
    #[test]
    fn a_tunnel_nests_parts_that_may_nest_parts() {
        let (object, report) = read(&format!(
            r#"<tun:Tunnel {NS} gml:id="t1">
                 <tun:consistsOfTunnelPart>
                   <tun:TunnelPart>
                     <tun:lod2MultiSurface>{}</tun:lod2MultiSurface>
                     <tun:consistsOfTunnelPart>
                       <tun:TunnelPart>
                         <tun:lod3MultiSurface>{}</tun:lod3MultiSurface>
                       </tun:TunnelPart>
                     </tun:consistsOfTunnelPart>
                   </tun:TunnelPart>
                 </tun:consistsOfTunnelPart>
                 <tun:outerTunnelInstallation>
                   <tun:TunnelInstallation gml:id="ti1">
                     <tun:lod2Geometry>{}</tun:lod2Geometry>
                   </tun:TunnelInstallation>
                 </tun:outerTunnelInstallation>
               </tun:Tunnel>"#,
            multi_surface(&member("tp-p1", 0.0)),
            multi_surface(&member("tp-p2", 1.0)),
            multi_surface(&member("ti-p1", 2.0)),
        ));

        assert_eq!(object.co_type, cjseq::CityObjectType::Tunnel);
        assert_eq!(
            children_of(&object),
            vec![
                ("t1-part-1", &cjseq::CityObjectType::TunnelPart),
                ("ti1", &cjseq::CityObjectType::TunnelInstallation),
            ]
        );
        let nested = &object.children[0].children[0];
        assert_eq!(nested.id, "t1-part-1-part-1");
        assert_eq!(nested.co_type, cjseq::CityObjectType::TunnelPart);
        assert_eq!(nested.geometries[0].lod, "3");
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A bridge states its boundary surfaces exactly as a building does, in
    /// its own namespace — openings included.
    #[test]
    fn a_bridge_reads_its_bounded_by_surfaces_and_their_openings() {
        let (object, report) = read(&format!(
            r#"<brid:Bridge {NS} gml:id="br1">
                 <brid:lod3MultiSurface>{}</brid:lod3MultiSurface>
                 <brid:boundedBy>
                   <brid:WallSurface>
                     <gml:name>Parapet</gml:name>
                     <brid:lod3MultiSurface>{}</brid:lod3MultiSurface>
                     <brid:opening>
                       <brid:Door>
                         <brid:lod3MultiSurface>{}</brid:lod3MultiSurface>
                       </brid:Door>
                     </brid:opening>
                   </brid:WallSurface>
                 </brid:boundedBy>
               </brid:Bridge>"#,
            multi_surface(
                r##"<gml:surfaceMember xlink:href="#w1"/>
                    <gml:surfaceMember xlink:href="#d1"/>"##
            ),
            multi_surface(&member("w1", 0.0)),
            multi_surface(&member("d1", 1.0)),
        ));

        let geometry = &object.geometries[0];
        assert_eq!(stypes(geometry), vec!["WallSurface", "Door"]);
        assert_eq!(
            geometry.surfaces[0].attributes["name"],
            serde_json::json!("Parapet")
        );
        assert_eq!(geometry.surfaces[0].children, vec![1]);
        assert_eq!(geometry.surfaces[1].parent, Some(0));
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A tunnel's boundary surfaces are the tunnel module's: the same local
    /// names in another namespace.
    #[test]
    fn a_tunnel_reads_its_bounded_by_surfaces() {
        let (object, report) = read(&format!(
            r#"<tun:Tunnel {NS} gml:id="t1">
                 <tun:lod2MultiSurface>{}</tun:lod2MultiSurface>
                 <tun:boundedBy>
                   <tun:GroundSurface>
                     <tun:lod2MultiSurface>{}</tun:lod2MultiSurface>
                   </tun:GroundSurface>
                 </tun:boundedBy>
               </tun:Tunnel>"#,
            multi_surface(r##"<gml:surfaceMember xlink:href="#g1"/>"##),
            multi_surface(&member("g1", 0.0)),
        ));

        assert_eq!(stypes(&object.geometries[0]), vec!["GroundSurface"]);
        assert_eq!(object.geometries[0].geometry.polygons()[0].sem_idx, Some(0));
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A boundary surface of the *building* module written inside a bridge is
    /// not the bridge's: each spec matches its own namespace, so the surface
    /// is not read and the property it sits under is reported.
    #[test]
    fn a_surface_of_another_module_is_not_this_modules_surface() {
        let (object, report) = read(&format!(
            r#"<brid:Bridge {NS}
                            xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                            gml:id="br1">
                 <brid:lod2MultiSurface>{}</brid:lod2MultiSurface>
                 <brid:boundedBy>
                   <bldg:WallSurface>
                     <bldg:lod2MultiSurface>{}</bldg:lod2MultiSurface>
                   </bldg:WallSurface>
                 </brid:boundedBy>
               </brid:Bridge>"#,
            multi_surface(&member("p1", 0.0)),
            multi_surface(&member("w1", 1.0)),
        ));

        assert!(object.geometries[0].surfaces.is_empty());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].element, BOUNDED_BY);
    }

    /// A child property this reader can take nothing from — a reference to a
    /// part written elsewhere — is content that was lost.
    #[test]
    fn a_child_property_holding_no_known_object_is_reported() {
        let (object, report) = read(&format!(
            r##"<brid:Bridge {NS} gml:id="br1">
                  <brid:consistsOfBridgePart xlink:href="#bp9"/>
                </brid:Bridge>"##
        ));

        assert!(object.children.is_empty());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].element, "consistsOfBridgePart");
    }
}
