//! GML surface collections and solids, and the xlink references between them.
//!
//! A CityGML geometry is a tree of aggregates over the polygons of
//! [`super::parse_polygon`], and its members are as often references as they
//! are inline elements: the same wall polygon is written once and pointed at
//! from the building's solid and from its `WallSurface`. [`XlinkRegistry`]
//! indexes a subtree's polygons up front so that both readings produce the
//! same [`Polygon3`].
//!
//! Every element is matched on its local name *and* [`GML_NS`], exactly as in
//! [`super`]: an application schema is free to define a `surfaceMember` of its
//! own, and it is not GML geometry.

use std::collections::HashMap;

use super::{
    element_context, gml_child, is_gml, parse_polygon, Polygon3, EXTERIOR, GML_NS, INTERIOR,
};
use crate::xml::XmlNode;
use crate::{CityGmlError, ParseReport, Skipped};

/// Local names of the aggregates this module reads.
const MULTI_SURFACE: &str = "MultiSurface";
const COMPOSITE_SURFACE: &str = "CompositeSurface";
const SURFACE: &str = "Surface";
const SOLID: &str = "Solid";
const MULTI_SOLID: &str = "MultiSolid";
const COMPOSITE_SOLID: &str = "CompositeSolid";

/// Local names of the member and patch properties inside them.
const SURFACE_MEMBER: &str = "surfaceMember";
const SURFACE_MEMBERS: &str = "surfaceMembers";
const SOLID_MEMBER: &str = "solidMember";
const SOLID_MEMBERS: &str = "solidMembers";
const PATCHES: &str = "patches";
const POLYGON: &str = "Polygon";

/// Surface patches that share the `Polygon` content model, and so parse with
/// [`parse_polygon`].
const PATCH_LOCAL_NAMES: [&str; 4] = [POLYGON, "PolygonPatch", "Triangle", "Rectangle"];

/// Local name of the XLink locator attribute. Attributes are matched on their
/// local name alone, so this reaches `xlink:href` under any prefix.
const HREF_ATTR: &str = "href";

/// Reason recorded for a polygon whose rings carry no area.
const DEGENERATE: &str = "degenerate ring";

/// A GML geometry aggregate, in the shapes CityJSON can express.
///
/// The distinction between a `MultiSurface` and a `CompositeSurface` — and
/// between a `MultiSolid` and a `CompositeSolid` — is kept because CityJSON
/// keeps it too: they are different geometry types, not different spellings.
#[derive(Debug, Clone, PartialEq)]
pub enum GmlGeometry {
    MultiSurface(Vec<Polygon3>),
    CompositeSurface(Vec<Polygon3>),
    /// Shells, the exterior first and any interiors after it.
    Solid(Vec<Vec<Polygon3>>),
    MultiSolid(Vec<Vec<Vec<Polygon3>>>),
    CompositeSolid(Vec<Vec<Vec<Polygon3>>>),
}

/// Parse a GML geometry aggregate rooted at `node`.
///
/// Returns `Ok(None)` when `node` is not one of `gml:MultiSurface`,
/// `gml:CompositeSurface`, `gml:Solid`, `gml:MultiSolid` or
/// `gml:CompositeSolid` — including when it carries one of those local names
/// in another namespace — so a caller can offer it every child of a geometry
/// property and let this function pick the one that is geometry.
///
/// Content that is valid GML but has no CityJSON counterpart does not fail
/// the parse: a member this reader cannot follow, and a polygon that
/// collapses to fewer than three distinct points, are dropped and recorded in
/// `report`, leaving the surrounding collection intact.
///
/// # Errors
///
/// Returns [`CityGmlError::UnresolvableXlink`] when a `surfaceMember` points
/// at an `xlink:href` that names no polygon in `registry`, and
/// [`CityGmlError::InvalidGeometry`] when a solid has no exterior shell, or
/// one that holds no surfaces at all — a solid without a boundary is
/// structurally wrong rather than merely unrepresentable.
///
/// # Examples
///
/// ```
/// use fcb_citygml::gml::{parse_geometry, GmlGeometry, XlinkRegistry};
/// use fcb_citygml::xml::XmlNode;
/// use fcb_citygml::ParseReport;
///
/// // <gml:MultiSurface><gml:surfaceMember><gml:Polygon><gml:exterior>
/// //   <gml:LinearRing><gml:posList>0 0 0 1 0 0 1 1 0 0 0 0</gml:posList>
/// // </gml:LinearRing></gml:exterior></gml:Polygon></gml:surfaceMember></gml:MultiSurface>
/// fn el(local: &str, text: &str, children: Vec<XmlNode>) -> XmlNode {
///     XmlNode {
///         ns: "http://www.opengis.net/gml".to_string(),
///         local: local.to_string(),
///         attrs: Vec::new(),
///         text: text.to_string(),
///         children,
///     }
/// }
/// let ring = el("LinearRing", "", vec![el("posList", "0 0 0 1 0 0 1 1 0 0 0 0", vec![])]);
/// let polygon = el("Polygon", "", vec![el("exterior", "", vec![ring])]);
/// let member = el("surfaceMember", "", vec![polygon]);
/// let node = el("MultiSurface", "", vec![member]);
///
/// let registry = XlinkRegistry::collect(&node);
/// let mut report = ParseReport::default();
/// let geometry = parse_geometry(&node, &registry, &mut report)?.expect("a MultiSurface");
/// match geometry {
///     GmlGeometry::MultiSurface(polygons) => assert_eq!(polygons.len(), 1),
///     other => panic!("unexpected geometry: {other:?}"),
/// }
/// # Ok::<(), fcb_citygml::CityGmlError>(())
/// ```
pub fn parse_geometry(
    node: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Option<GmlGeometry>, CityGmlError> {
    if node.ns != GML_NS {
        return Ok(None);
    }
    let geometry = match node.local.as_str() {
        MULTI_SURFACE => GmlGeometry::MultiSurface(parse_surfaces(node, registry, report)?),
        COMPOSITE_SURFACE => GmlGeometry::CompositeSurface(parse_surfaces(node, registry, report)?),
        SOLID => GmlGeometry::Solid(parse_solid(node, registry, report)?),
        MULTI_SOLID => GmlGeometry::MultiSolid(parse_solids(node, registry, report)?),
        COMPOSITE_SOLID => GmlGeometry::CompositeSolid(parse_solids(node, registry, report)?),
        _ => return Ok(None),
    };
    Ok(Some(geometry))
}

/// Collect the polygons of a surface collection: the `surfaceMember`
/// properties of a `MultiSurface` or `CompositeSurface`, or the patches of a
/// `Surface`.
fn parse_surfaces(
    container: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Vec<Polygon3>, CityGmlError> {
    let mut polygons = Vec::new();
    for child in &container.children {
        if child.ns != GML_NS {
            continue;
        }
        match child.local.as_str() {
            SURFACE_MEMBER => {
                if let Some(polygon) = parse_surface_member(child, registry, report)? {
                    polygons.push(polygon);
                }
            }
            PATCHES => parse_patches(child, report, &mut polygons)?,
            // The plural properties hold their members directly rather than
            // one per property. They are rare, and dropping them silently
            // would lose surfaces without a trace.
            SURFACE_MEMBERS => report.skipped.push(unsupported(
                child,
                format!(
                    "<{SURFACE_MEMBERS}> is not supported; use one <{SURFACE_MEMBER}> per surface"
                ),
            )),
            _ => {}
        }
    }
    Ok(polygons)
}

/// Resolve one `gml:surfaceMember`, inline or by reference.
///
/// Returns `Ok(None)` for a member this reader cannot turn into a polygon,
/// having recorded why in `report`.
fn parse_surface_member(
    member: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Option<Polygon3>, CityGmlError> {
    if let Some(href) = member.attr(HREF_ATTR) {
        let context = element_context(member);
        return match registry.lookup(href, &context)? {
            Some(polygon) => Ok(Some(polygon.clone())),
            // Indexed, but degenerate: the reference resolves, the polygon
            // just carries no area, so it is skipped like an inline one.
            None => {
                report
                    .skipped
                    .push(degenerate(href.strip_prefix('#').map(str::to_owned)));
                Ok(None)
            }
        };
    }

    let Some(node) = gml_child(member, POLYGON) else {
        report.skipped.push(unsupported(
            member,
            format!("no inline GML <{POLYGON}> and no xlink:href"),
        ));
        return Ok(None);
    };
    let Some(polygon) = parse_polygon(node)? else {
        report
            .skipped
            .push(degenerate(node.gml_id().map(str::to_owned)));
        return Ok(None);
    };
    Ok(Some(polygon))
}

/// Collect the polygons of a `gml:patches` property of a `gml:Surface`.
///
/// A `PolygonPatch` has the same content model as a `Polygon`, so the patches
/// parse with [`parse_polygon`]; a patch kind that does not, such as a
/// `Cone` or a `Sphere`, is recorded and dropped.
fn parse_patches(
    patches: &XmlNode,
    report: &mut ParseReport,
    polygons: &mut Vec<Polygon3>,
) -> Result<(), CityGmlError> {
    for patch in &patches.children {
        if patch.ns != GML_NS {
            continue;
        }
        if !PATCH_LOCAL_NAMES.contains(&patch.local.as_str()) {
            report.skipped.push(unsupported(
                patch,
                format!("surface patch <{}> is not supported", patch.local),
            ));
            continue;
        }
        match parse_polygon(patch)? {
            Some(polygon) => polygons.push(polygon),
            None => report
                .skipped
                .push(degenerate(patch.gml_id().map(str::to_owned))),
        }
    }
    Ok(())
}

/// Parse a `gml:Solid` into its shells, the exterior first.
///
/// A second `gml:exterior` is not valid GML; as in [`super::parse_polygon`],
/// keeping it as an interior shell is closer to the author's intent than
/// discarding it.
fn parse_solid(
    solid: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Vec<Vec<Polygon3>>, CityGmlError> {
    let mut exterior = None;
    let mut interiors = Vec::new();
    for boundary in &solid.children {
        let is_exterior = if is_gml(boundary, EXTERIOR) {
            true
        } else if is_gml(boundary, INTERIOR) {
            false
        } else {
            continue;
        };
        let shell = parse_shell(boundary, registry, report)?;
        if is_exterior && exterior.is_none() {
            exterior = Some(shell);
        } else if shell.is_empty() {
            // An interior shell with nothing left in it is a hole that would
            // enclose no volume, and CityJSON has no way to write it.
            report
                .skipped
                .push(unsupported(boundary, "shell has no surfaces".to_string()));
        } else {
            interiors.push(shell);
        }
    }

    let Some(exterior) = exterior else {
        return Err(CityGmlError::InvalidGeometry {
            context: element_context(solid),
            reason: format!("solid has no <{EXTERIOR}> shell"),
        });
    };
    if exterior.is_empty() {
        return Err(CityGmlError::InvalidGeometry {
            context: element_context(solid),
            reason: format!("solid's <{EXTERIOR}> shell has no surfaces"),
        });
    }

    let mut shells = Vec::with_capacity(1 + interiors.len());
    shells.push(exterior);
    shells.append(&mut interiors);
    Ok(shells)
}

/// Collect the polygons of the shell inside a `gml:exterior` or
/// `gml:interior` of a solid.
fn parse_shell(
    boundary: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Vec<Polygon3>, CityGmlError> {
    let mut polygons = Vec::new();
    for child in &boundary.children {
        if child.ns != GML_NS {
            continue;
        }
        match child.local.as_str() {
            COMPOSITE_SURFACE | SURFACE => {
                polygons.extend(parse_surfaces(child, registry, report)?)
            }
            _ => report.skipped.push(unsupported(
                child,
                format!("shell surface <{}> is not supported", child.local),
            )),
        }
    }
    Ok(polygons)
}

/// Parse the `solidMember` properties of a `MultiSolid` or `CompositeSolid`.
///
/// Only inline solids are followed. A solid reached by `xlink:href` would
/// need an index of solids rather than of polygons, so it is recorded and
/// dropped instead of resolved.
fn parse_solids(
    container: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Vec<Vec<Vec<Polygon3>>>, CityGmlError> {
    let mut solids = Vec::new();
    for child in &container.children {
        if child.ns != GML_NS {
            continue;
        }
        match child.local.as_str() {
            SOLID_MEMBER => {
                if let Some(href) = child.attr(HREF_ATTR) {
                    report.skipped.push(unsupported(
                        child,
                        format!("xlink:href {href} to a <{SOLID}> is not supported"),
                    ));
                    continue;
                }
                match gml_child(child, SOLID) {
                    Some(solid) => solids.push(parse_solid(solid, registry, report)?),
                    None => report.skipped.push(unsupported(
                        child,
                        format!("no inline GML <{SOLID}> and no xlink:href"),
                    )),
                }
            }
            SOLID_MEMBERS => report.skipped.push(unsupported(
                child,
                format!("<{SOLID_MEMBERS}> is not supported; use one <{SOLID_MEMBER}> per solid"),
            )),
            _ => {}
        }
    }
    Ok(solids)
}

/// A skip for content that is valid GML but has no CityJSON counterpart.
fn unsupported(node: &XmlNode, reason: String) -> Skipped {
    Skipped {
        element: node.local.clone(),
        gml_id: node.gml_id().map(str::to_owned),
        reason,
    }
}

/// A skip for a polygon whose rings carry no area.
fn degenerate(gml_id: Option<String>) -> Skipped {
    Skipped {
        element: POLYGON.to_string(),
        gml_id,
        reason: DEGENERATE.to_string(),
    }
}

/// Every `gml:Polygon` of a subtree that carries a `gml:id`, so that an
/// `xlink:href` elsewhere in that subtree can be followed.
///
/// CityGML shares surfaces by reference — a wall polygon is written once
/// under the solid and pointed at from the `WallSurface`, or the other way
/// round — and the reference may point backwards or forwards. The whole
/// subtree is therefore indexed before any geometry is read.
#[derive(Debug, Default)]
pub struct XlinkRegistry {
    /// Polygons by `gml:id`. `None` marks one that parsed as degenerate: it
    /// is kept so that a reference to it can be told apart from a reference
    /// to nothing at all.
    polygons: HashMap<String, Option<Polygon3>>,
}

impl XlinkRegistry {
    /// Index every `gml:Polygon` under `subtree`, itself included.
    ///
    /// Indexing cannot fail: a polygon that is structurally invalid is left
    /// out, and the error surfaces as an unresolvable reference if — and only
    /// if — something points at it. Where an id is used twice, which is not
    /// valid XML, the first polygon in document order wins.
    pub fn collect(subtree: &XmlNode) -> Self {
        let mut polygons = HashMap::new();
        for node in subtree.descendants() {
            if !is_gml(node, POLYGON) {
                continue;
            }
            let Some(id) = node.gml_id() else {
                continue;
            };
            if let Ok(polygon) = parse_polygon(node) {
                polygons.entry(id.to_owned()).or_insert(polygon);
            }
        }
        Self { polygons }
    }

    /// Resolve an `xlink:href` to the polygon it names.
    ///
    /// `context` describes the element that carries the reference, and is
    /// reported back in the error.
    ///
    /// # Errors
    ///
    /// Returns [`CityGmlError::UnresolvableXlink`] when `href` is not a
    /// same-document fragment — this converter reads one document and does
    /// not fetch another — when no polygon in the indexed subtree carries
    /// that `gml:id`, and when the polygon it names is degenerate and so has
    /// no rings to hand back.
    ///
    /// # Examples
    ///
    /// ```
    /// use fcb_citygml::gml::XlinkRegistry;
    ///
    /// // Nothing is indexed, so every reference is unresolvable.
    /// let registry = XlinkRegistry::default();
    /// let err = registry.resolve("#p1", "<surfaceMember>").unwrap_err();
    /// assert!(err.to_string().contains("#p1"));
    /// ```
    pub fn resolve(&self, href: &str, context: &str) -> Result<Polygon3, CityGmlError> {
        self.lookup(href, context)?
            .cloned()
            .ok_or_else(|| unresolvable(href, context))
    }

    /// Look a reference up, keeping "indexed but degenerate" (`Ok(None)`)
    /// apart from "not indexed at all" (an error), which is the distinction
    /// between a skip and a failed conversion.
    fn lookup(&self, href: &str, context: &str) -> Result<Option<&Polygon3>, CityGmlError> {
        let id = href
            .strip_prefix('#')
            .ok_or_else(|| unresolvable(href, context))?;
        self.polygons
            .get(id)
            .map(Option::as_ref)
            .ok_or_else(|| unresolvable(href, context))
    }
}

/// The error for a reference that names no polygon.
fn unresolvable(href: &str, context: &str) -> CityGmlError {
    CityGmlError::UnresolvableXlink {
        href: href.to_string(),
        context: context.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CityGmlError;

    fn node(xml: &str) -> XmlNode {
        crate::xml::parse_str_for_tests(xml).unwrap()
    }

    /// The six faces of the unit cube, as closed GML rings.
    const CUBE_FACES: [&str; 6] = [
        "0 0 0 1 0 0 1 1 0 0 1 0 0 0 0",
        "0 0 1 0 1 1 1 1 1 1 0 1 0 0 1",
        "0 0 0 0 0 1 1 0 1 1 0 0 0 0 0",
        "1 1 0 1 1 1 0 1 1 0 1 0 1 1 0",
        "0 1 0 0 1 1 0 0 1 0 0 0 0 1 0",
        "1 0 0 1 0 1 1 1 1 1 1 0 1 0 0",
    ];

    /// A `gml:Polygon` with one exterior ring holding `pos_list`.
    fn polygon(pos_list: &str) -> String {
        format!(
            "<gml:Polygon><gml:exterior><gml:LinearRing>\
             <gml:posList>{pos_list}</gml:posList>\
             </gml:LinearRing></gml:exterior></gml:Polygon>"
        )
    }

    /// The cube's faces, each wrapped in its own `gml:surfaceMember`.
    fn cube_members() -> String {
        CUBE_FACES
            .iter()
            .map(|face| format!("<gml:surfaceMember>{}</gml:surfaceMember>", polygon(face)))
            .collect()
    }

    /// A `gml:Solid` whose single shell is the unit cube.
    fn cube_solid() -> String {
        format!(
            "<gml:Solid><gml:exterior><gml:CompositeSurface>{}\
             </gml:CompositeSurface></gml:exterior></gml:Solid>",
            cube_members()
        )
    }

    /// Parse `xml` with a registry collected from that same tree.
    fn parse(xml: &str) -> (Option<GmlGeometry>, ParseReport) {
        let root = node(xml);
        let registry = XlinkRegistry::collect(&root);
        let mut report = ParseReport::default();
        let geometry = parse_geometry(&root, &registry, &mut report).unwrap();
        (geometry, report)
    }

    #[test]
    fn multi_surface_of_two_inline_polygons() {
        let (geometry, report) = parse(&format!(
            r#"<gml:MultiSurface xmlns:gml="http://www.opengis.net/gml" gml:id="ms">
                 <gml:surfaceMember>{}</gml:surfaceMember>
                 <gml:surfaceMember>{}</gml:surfaceMember>
               </gml:MultiSurface>"#,
            polygon("0 0 0 1 0 0 1 1 0 0 0 0"),
            polygon("0 0 5 1 0 5 1 1 5 0 0 5"),
        ));
        let GmlGeometry::MultiSurface(polygons) = geometry.unwrap() else {
            panic!("expected a MultiSurface");
        };
        assert_eq!(polygons.len(), 2);
        assert_eq!(
            polygons[0].rings[0].pts,
            vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.]]
        );
        assert_eq!(
            polygons[1].rings[0].pts,
            vec![[0., 0., 5.], [1., 0., 5.], [1., 1., 5.]]
        );
        assert!(report.skipped.is_empty());
    }

    #[test]
    fn composite_surface_of_inline_polygons() {
        let (geometry, _) = parse(&format!(
            r#"<gml:CompositeSurface xmlns:gml="http://www.opengis.net/gml">
                 <gml:surfaceMember>{}</gml:surfaceMember>
               </gml:CompositeSurface>"#,
            polygon("0 0 0 1 0 0 1 1 0 0 0 0"),
        ));
        let GmlGeometry::CompositeSurface(polygons) = geometry.unwrap() else {
            panic!("expected a CompositeSurface");
        };
        assert_eq!(polygons.len(), 1);
    }

    #[test]
    fn solid_with_xlinked_members() {
        // The brief's snippet verbatim, save for the raw-string delimiter:
        // `"#p1"` closes an `r#"…"#` literal, so it needs `r##"…"##`.
        let root = node(
            r##"
    <root xmlns:gml="http://www.opengis.net/gml" xmlns:xlink="http://www.w3.org/1999/xlink">
      <defs><gml:Polygon gml:id="p1"><gml:exterior><gml:LinearRing>
        <gml:posList>0 0 0 1 0 0 1 1 0 0 1 0</gml:posList>
      </gml:LinearRing></gml:exterior></gml:Polygon></defs>
      <gml:MultiSurface gml:id="ms">
        <gml:surfaceMember xlink:href="#p1"/>
      </gml:MultiSurface>
    </root>"##,
        );
        let reg = XlinkRegistry::collect(&root);
        let ms_node = root
            .descendants()
            .find(|n| n.local == "MultiSurface")
            .unwrap();
        let mut report = crate::ParseReport::default();
        let g = parse_geometry(ms_node, &reg, &mut report).unwrap().unwrap();
        match g {
            GmlGeometry::MultiSurface(ps) => {
                assert_eq!(ps.len(), 1);
                assert_eq!(ps[0].gml_id.as_deref(), Some("p1"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn solid_with_a_composite_surface_exterior_keeps_every_face() {
        let (geometry, report) = parse(&format!(
            r#"<gml:Solid xmlns:gml="http://www.opengis.net/gml" gml:id="s1">
                 <gml:exterior><gml:CompositeSurface>{}</gml:CompositeSurface></gml:exterior>
               </gml:Solid>"#,
            cube_members()
        ));
        let GmlGeometry::Solid(shells) = geometry.unwrap() else {
            panic!("expected a Solid");
        };
        assert_eq!(shells.len(), 1);
        assert_eq!(shells[0].len(), 6);
        // The coordinates survive: the first face is the closed bottom ring
        // with its closing point dropped.
        assert_eq!(
            shells[0][0].rings[0].pts,
            vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]]
        );
        assert_eq!(
            shells[0][5].rings[0].pts,
            vec![[1., 0., 0.], [1., 0., 1.], [1., 1., 1.], [1., 1., 0.]]
        );
        assert!(report.skipped.is_empty());
    }

    #[test]
    fn solid_interior_shells_follow_the_exterior() {
        let one_face = format!(
            "<gml:surfaceMember>{}</gml:surfaceMember>",
            polygon(CUBE_FACES[0])
        );
        let (geometry, _) = parse(&format!(
            r#"<gml:Solid xmlns:gml="http://www.opengis.net/gml">
                 <gml:interior><gml:CompositeSurface>{one_face}</gml:CompositeSurface></gml:interior>
                 <gml:exterior><gml:CompositeSurface>{}</gml:CompositeSurface></gml:exterior>
               </gml:Solid>"#,
            cube_members()
        ));
        let GmlGeometry::Solid(shells) = geometry.unwrap() else {
            panic!("expected a Solid");
        };
        assert_eq!(shells.len(), 2);
        assert_eq!(shells[0].len(), 6, "the exterior shell comes first");
        assert_eq!(shells[1].len(), 1);
    }

    #[test]
    fn solid_shell_may_be_a_surface_with_patches() {
        let (geometry, _) = parse(&format!(
            r#"<gml:Solid xmlns:gml="http://www.opengis.net/gml">
                 <gml:exterior><gml:Surface><gml:patches>
                   <gml:PolygonPatch><gml:exterior><gml:LinearRing>
                     <gml:posList>{}</gml:posList>
                   </gml:LinearRing></gml:exterior></gml:PolygonPatch>
                 </gml:patches></gml:Surface></gml:exterior>
               </gml:Solid>"#,
            CUBE_FACES[0]
        ));
        let GmlGeometry::Solid(shells) = geometry.unwrap() else {
            panic!("expected a Solid");
        };
        assert_eq!(shells.len(), 1);
        assert_eq!(shells[0].len(), 1);
        assert_eq!(shells[0][0].rings[0].pts.len(), 4);
    }

    #[test]
    fn solid_with_an_empty_exterior_is_invalid_geometry() {
        let root = node(
            r#"<gml:Solid xmlns:gml="http://www.opengis.net/gml" gml:id="s9">
                 <gml:exterior><gml:CompositeSurface/></gml:exterior>
               </gml:Solid>"#,
        );
        let err = parse_geometry(
            &root,
            &XlinkRegistry::default(),
            &mut ParseReport::default(),
        )
        .unwrap_err();
        assert!(
            matches!(err, CityGmlError::InvalidGeometry { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn solid_without_an_exterior_is_invalid_geometry() {
        let root = node(r#"<gml:Solid xmlns:gml="http://www.opengis.net/gml"/>"#);
        let err = parse_geometry(
            &root,
            &XlinkRegistry::default(),
            &mut ParseReport::default(),
        )
        .unwrap_err();
        assert!(
            matches!(err, CityGmlError::InvalidGeometry { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn multi_solid_of_two_cubes() {
        let (geometry, report) = parse(&format!(
            r#"<gml:MultiSolid xmlns:gml="http://www.opengis.net/gml">
                 <gml:solidMember>{cube}</gml:solidMember>
                 <gml:solidMember>{cube}</gml:solidMember>
               </gml:MultiSolid>"#,
            cube = cube_solid()
        ));
        let GmlGeometry::MultiSolid(solids) = geometry.unwrap() else {
            panic!("expected a MultiSolid");
        };
        assert_eq!(solids.len(), 2);
        for solid in &solids {
            assert_eq!(solid.len(), 1);
            assert_eq!(solid[0].len(), 6);
        }
        assert!(report.skipped.is_empty());
    }

    #[test]
    fn composite_solid_of_two_cubes() {
        let (geometry, _) = parse(&format!(
            r#"<gml:CompositeSolid xmlns:gml="http://www.opengis.net/gml">
                 <gml:solidMember>{cube}</gml:solidMember>
                 <gml:solidMember>{cube}</gml:solidMember>
               </gml:CompositeSolid>"#,
            cube = cube_solid()
        ));
        let GmlGeometry::CompositeSolid(solids) = geometry.unwrap() else {
            panic!("expected a CompositeSolid");
        };
        assert_eq!(solids.len(), 2);
    }

    #[test]
    fn degenerate_polygon_in_a_collection_is_skipped_not_fatal() {
        let (geometry, report) = parse(&format!(
            r#"<gml:MultiSurface xmlns:gml="http://www.opengis.net/gml">
                 <gml:surfaceMember>{}</gml:surfaceMember>
                 <gml:surfaceMember><gml:Polygon gml:id="flat">
                   <gml:exterior><gml:LinearRing>
                     <gml:posList>0 0 0 1 0 0 0 0 0</gml:posList>
                   </gml:LinearRing></gml:exterior>
                 </gml:Polygon></gml:surfaceMember>
               </gml:MultiSurface>"#,
            polygon("0 0 0 1 0 0 1 1 0 0 0 0"),
        ));
        let GmlGeometry::MultiSurface(polygons) = geometry.unwrap() else {
            panic!("expected a MultiSurface");
        };
        assert_eq!(polygons.len(), 1);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].element, "Polygon");
        assert_eq!(report.skipped[0].gml_id.as_deref(), Some("flat"));
        assert_eq!(report.skipped[0].reason, "degenerate ring");
    }

    #[test]
    fn a_referenced_degenerate_polygon_is_skipped_with_a_report_entry() {
        let root = node(
            r##"<root xmlns:gml="http://www.opengis.net/gml"
                      xmlns:xlink="http://www.w3.org/1999/xlink">
                  <defs><gml:Polygon gml:id="flat"><gml:exterior><gml:LinearRing>
                    <gml:posList>0 0 0 1 0 0 0 0 0</gml:posList>
                  </gml:LinearRing></gml:exterior></gml:Polygon></defs>
                  <gml:MultiSurface>
                    <gml:surfaceMember xlink:href="#flat"/>
                  </gml:MultiSurface>
                </root>"##,
        );
        let registry = XlinkRegistry::collect(&root);
        let ms = root
            .descendants()
            .find(|n| n.local == "MultiSurface")
            .unwrap();
        let mut report = ParseReport::default();
        let geometry = parse_geometry(ms, &registry, &mut report).unwrap().unwrap();
        let GmlGeometry::MultiSurface(polygons) = geometry else {
            panic!("expected a MultiSurface");
        };
        assert!(polygons.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].element, "Polygon");
        assert_eq!(report.skipped[0].gml_id.as_deref(), Some("flat"));
        assert_eq!(report.skipped[0].reason, "degenerate ring");
    }

    #[test]
    fn an_unresolvable_href_names_the_href() {
        let root = node(
            r##"<gml:MultiSurface xmlns:gml="http://www.opengis.net/gml"
                                  xmlns:xlink="http://www.w3.org/1999/xlink" gml:id="ms">
                  <gml:surfaceMember xlink:href="#missing-42"/>
                </gml:MultiSurface>"##,
        );
        let err = parse_geometry(
            &root,
            &XlinkRegistry::default(),
            &mut ParseReport::default(),
        )
        .unwrap_err();
        assert!(
            matches!(&err, CityGmlError::UnresolvableXlink { href, .. } if href == "#missing-42"),
            "{err:?}"
        );
        assert!(err.to_string().contains("#missing-42"), "{err}");
    }

    #[test]
    fn an_href_without_a_fragment_is_unresolvable() {
        let registry = XlinkRegistry::default();
        let err = registry
            .resolve("http://example.com/other.gml#p1", "<surfaceMember>")
            .unwrap_err();
        assert!(
            matches!(err, CityGmlError::UnresolvableXlink { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn resolve_returns_the_indexed_polygon() {
        let root = node(
            r#"<root xmlns:gml="http://www.opengis.net/gml">
                 <gml:Polygon gml:id="p1"><gml:exterior><gml:LinearRing>
                   <gml:posList>0 0 0 1 0 0 1 1 0 0 0 0</gml:posList>
                 </gml:LinearRing></gml:exterior></gml:Polygon>
               </root>"#,
        );
        let registry = XlinkRegistry::collect(&root);
        let polygon = registry.resolve("#p1", "<surfaceMember>").unwrap();
        assert_eq!(polygon.gml_id.as_deref(), Some("p1"));
        assert_eq!(polygon.rings[0].pts.len(), 3);
    }

    #[test]
    fn the_registry_ignores_polygons_outside_the_gml_namespace() {
        let root = node(
            r#"<root xmlns:gml="http://www.opengis.net/gml" xmlns:other="urn:example:other">
                 <other:Polygon gml:id="p1"><gml:exterior><gml:LinearRing>
                   <gml:posList>0 0 0 1 0 0 1 1 0 0 0 0</gml:posList>
                 </gml:LinearRing></gml:exterior></other:Polygon>
               </root>"#,
        );
        let registry = XlinkRegistry::collect(&root);
        assert!(registry.resolve("#p1", "<surfaceMember>").is_err());
    }

    #[test]
    fn a_surface_member_with_neither_polygon_nor_href_is_skipped() {
        let (geometry, report) = parse(
            r#"<gml:MultiSurface xmlns:gml="http://www.opengis.net/gml">
                 <gml:surfaceMember gml:id="m1"><gml:OrientableSurface/></gml:surfaceMember>
               </gml:MultiSurface>"#,
        );
        let GmlGeometry::MultiSurface(polygons) = geometry.unwrap() else {
            panic!("expected a MultiSurface");
        };
        assert!(polygons.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].element, "surfaceMember");
        assert_eq!(report.skipped[0].gml_id.as_deref(), Some("m1"));
    }

    #[test]
    fn a_solid_member_reached_by_href_is_skipped() {
        let (geometry, report) = parse(
            r##"<gml:MultiSolid xmlns:gml="http://www.opengis.net/gml"
                                xmlns:xlink="http://www.w3.org/1999/xlink">
                  <gml:solidMember xlink:href="#s1"/>
                </gml:MultiSolid>"##,
        );
        let GmlGeometry::MultiSolid(solids) = geometry.unwrap() else {
            panic!("expected a MultiSolid");
        };
        assert!(solids.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].element, "solidMember");
        assert!(report.skipped[0].reason.contains("#s1"), "{report:?}");
    }

    #[test]
    fn surface_members_in_the_plural_is_reported_rather_than_dropped() {
        let (geometry, report) = parse(&format!(
            r#"<gml:MultiSurface xmlns:gml="http://www.opengis.net/gml">
                 <gml:surfaceMembers>{}</gml:surfaceMembers>
               </gml:MultiSurface>"#,
            polygon("0 0 0 1 0 0 1 1 0 0 0 0"),
        ));
        let GmlGeometry::MultiSurface(polygons) = geometry.unwrap() else {
            panic!("expected a MultiSurface");
        };
        assert!(polygons.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].element, "surfaceMembers");
    }

    #[test]
    fn an_element_that_is_not_a_geometry_is_none() {
        for xml in [
            r#"<bldg:Building xmlns:bldg="http://www.opengis.net/citygml/building/2.0"/>"#,
            r#"<gml:Polygon xmlns:gml="http://www.opengis.net/gml"/>"#,
            // GML 3.2 is not this reader's namespace.
            r#"<gml:MultiSurface xmlns:gml="http://www.opengis.net/gml/3.2"/>"#,
            // Same local name, no namespace at all.
            r#"<MultiSurface/>"#,
        ] {
            let root = node(xml);
            let geometry = parse_geometry(
                &root,
                &XlinkRegistry::default(),
                &mut ParseReport::default(),
            )
            .unwrap();
            assert!(geometry.is_none(), "{xml}");
        }
    }

    #[test]
    fn members_outside_the_gml_namespace_are_not_members() {
        let (geometry, report) = parse(&format!(
            r#"<gml:MultiSurface xmlns:gml="http://www.opengis.net/gml"
                                 xmlns:other="urn:example:other">
                 <other:surfaceMember>{}</other:surfaceMember>
               </gml:MultiSurface>"#,
            polygon("0 0 0 1 0 0 1 1 0 0 0 0"),
        ));
        let GmlGeometry::MultiSurface(polygons) = geometry.unwrap() else {
            panic!("expected a MultiSurface");
        };
        assert!(polygons.is_empty());
        assert!(report.skipped.is_empty());
    }
}
