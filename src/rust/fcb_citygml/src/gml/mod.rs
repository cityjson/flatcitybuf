//! GML geometry primitives shared by every CityGML module reader.
//!
//! Only the pieces CityJSON can express are modelled: a polygon is a list of
//! rings, the first exterior and the rest interior, each ring a list of 3D
//! points.

mod geometry;

pub use geometry::{parse_geometry, GmlGeometry, XlinkRegistry};

use crate::xml::XmlNode;
use crate::CityGmlError;

/// A GML `LinearRing`, already repaired: no closing point, no consecutive
/// duplicates, at least three points.
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    pub gml_id: Option<String>,
    pub pts: Vec<[f64; 3]>,
}

/// A GML `Polygon` with its rings.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon3 {
    pub gml_id: Option<String>,
    /// `rings[0]` is the exterior ring; the rest are interior.
    pub rings: Vec<Ring>,
    /// Index into the owning surface's semantic-surface list. Filled in by
    /// the module readers, which are the only ones that know the semantics.
    pub sem_idx: Option<usize>,
}

/// The GML namespace this reader accepts.
///
/// CityGML 2.0 is built on GML 3.1.1, which is bound to this URI. GML 3.2
/// (`http://www.opengis.net/gml/3.2`, used by CityGML 3.0) is deliberately
/// *not* accepted: recognising its elements here would claim a conformance
/// the rest of the converter does not have.
const GML_NS: &str = "http://www.opengis.net/gml";

/// Local names of the elements this module reads. A local name alone never
/// identifies an element — an application schema is free to define its own
/// `posList` — so every match pairs the local name with [`GML_NS`].
const EXTERIOR: &str = "exterior";
const INTERIOR: &str = "interior";
const LINEAR_RING: &str = "LinearRing";
const POS: &str = "pos";
const POS_LIST: &str = "posList";

/// Coordinates per position. CityGML geometry is 3D; a `srsDimension` of 2
/// is not supported, and shows up as a coordinate count that is not a
/// multiple of three.
const DIMS: usize = 3;

/// Parse a `gml:Polygon` element into its repaired rings.
///
/// The boundary, ring and position elements inside it are recognised only in
/// [`GML_NS`]; an element with a matching local name in any other namespace
/// is not GML geometry and is passed over, which leaves the polygon looking
/// as though that part were absent. Which element *is* the polygon is the
/// caller's decision, so `node` itself is not checked — `gml:Triangle` and
/// `gml:Rectangle` have the same content model and parse the same way.
///
/// Returns `Ok(None)` when any of the polygon's rings collapses to fewer
/// than three distinct points — the polygon carries no area and cannot be
/// written as a CityJSON surface, so the caller records a skip.
///
/// # Errors
///
/// Returns [`CityGmlError::InvalidGeometry`] when the polygon is structurally
/// wrong rather than merely degenerate: no exterior ring, a boundary with no
/// `LinearRing`, a coordinate that is not a number, or a coordinate count
/// that is not a multiple of three.
///
/// # Examples
///
/// ```
/// use fcb_citygml::gml::parse_polygon;
/// use fcb_citygml::xml::XmlNode;
///
/// // <gml:Polygon><gml:exterior><gml:LinearRing>
/// //   <gml:posList>0 0 0 1 0 0 1 1 0 0 0 0</gml:posList>
/// // </gml:LinearRing></gml:exterior></gml:Polygon>
/// fn el(local: &str, text: &str, children: Vec<XmlNode>) -> XmlNode {
///     XmlNode {
///         ns: "http://www.opengis.net/gml".to_string(),
///         local: local.to_string(),
///         attrs: Vec::new(),
///         text: text.to_string(),
///         children,
///     }
/// }
/// let pos_list = el("posList", "0 0 0 1 0 0 1 1 0 0 0 0", Vec::new());
/// let ring = el("LinearRing", "", vec![pos_list]);
/// let node = el("Polygon", "", vec![el("exterior", "", vec![ring])]);
///
/// let polygon = parse_polygon(&node)?.expect("the ring is not degenerate");
/// // The closing point is dropped: CityJSON rings are not closed.
/// assert_eq!(polygon.rings[0].pts, vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.]]);
/// # Ok::<(), fcb_citygml::CityGmlError>(())
/// ```
pub fn parse_polygon(node: &XmlNode) -> Result<Option<Polygon3>, CityGmlError> {
    let gml_id = node.gml_id().map(str::to_owned);

    let mut exterior = None;
    let mut interiors = Vec::new();
    for boundary in &node.children {
        let is_exterior = if is_gml(boundary, EXTERIOR) {
            true
        } else if is_gml(boundary, INTERIOR) {
            false
        } else {
            continue;
        };
        let Some(ring) = parse_ring(boundary)? else {
            return Ok(None);
        };
        if is_exterior && exterior.is_none() {
            exterior = Some(ring);
        } else {
            // A second exterior ring is not valid GML; keeping it as a hole
            // is closer to the author's intent than discarding it.
            interiors.push(ring);
        }
    }

    let Some(exterior) = exterior else {
        return Err(CityGmlError::InvalidGeometry {
            context: element_context(node),
            reason: "polygon has no exterior ring".to_string(),
        });
    };

    let mut rings = Vec::with_capacity(1 + interiors.len());
    rings.push(exterior);
    rings.append(&mut interiors);
    Ok(Some(Polygon3 {
        gml_id,
        rings,
        sem_idx: None,
    }))
}

/// Parse the `LinearRing` inside a `gml:exterior` or `gml:interior`.
///
/// Returns `Ok(None)` when the ring is degenerate.
fn parse_ring(boundary: &XmlNode) -> Result<Option<Ring>, CityGmlError> {
    let Some(linear_ring) = gml_child(boundary, LINEAR_RING) else {
        return Err(CityGmlError::InvalidGeometry {
            context: element_context(boundary),
            reason: format!("no GML <{LINEAR_RING}> child"),
        });
    };
    let pts = parse_positions(linear_ring)?;
    Ok(repair_ring(pts).map(|pts| Ring {
        gml_id: linear_ring.gml_id().map(str::to_owned),
        pts,
    }))
}

/// Collect the points of a `LinearRing` from its `pos` and `posList` children.
fn parse_positions(ring: &XmlNode) -> Result<Vec<[f64; 3]>, CityGmlError> {
    let mut pts = Vec::new();
    for child in &ring.children {
        if child.ns != GML_NS {
            continue;
        }
        match child.local.as_str() {
            POS => {
                let point = parse_coords(&child.text, child)?;
                if point.len() != 1 {
                    return Err(CityGmlError::InvalidGeometry {
                        context: element_context(child),
                        reason: format!(
                            "<{POS}> holds {} coordinates, expected {DIMS}",
                            point.len() * DIMS
                        ),
                    });
                }
                pts.extend(point);
            }
            POS_LIST => pts.extend(parse_coords(&child.text, child)?),
            _ => {}
        }
    }
    Ok(pts)
}

/// Parse whitespace-separated coordinates into 3D points.
///
/// Points are assembled as the tokens arrive rather than via an intermediate
/// list of scalars: a single `posList` can hold tens of thousands of them.
fn parse_coords(text: &str, owner: &XmlNode) -> Result<Vec<[f64; 3]>, CityGmlError> {
    let mut pts = Vec::new();
    let mut point = [0.0; DIMS];
    let mut filled = 0;
    for token in text.split_ascii_whitespace() {
        point[filled] = parse_f64(token, owner)?;
        filled += 1;
        if filled == DIMS {
            pts.push(point);
            filled = 0;
        }
    }
    if filled != 0 {
        return Err(CityGmlError::InvalidGeometry {
            context: element_context(owner),
            reason: format!(
                "{} coordinates is not a multiple of {DIMS}; only 3D geometry is supported",
                pts.len() * DIMS + filled
            ),
        });
    }
    Ok(pts)
}

/// Parse one coordinate, rejecting anything that is not a finite number.
///
/// `NaN` and the infinities parse happily out of `"NaN"` and `"inf"` but are
/// not coordinates, and would poison every bounding box they reach.
fn parse_f64(token: &str, owner: &XmlNode) -> Result<f64, CityGmlError> {
    token
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| CityGmlError::InvalidGeometry {
            context: element_context(owner),
            reason: format!("coordinate {token:?} is not a finite number"),
        })
}

/// Repair a raw ring into the CityJSON convention: not closed, no repeated
/// point next to its twin, and at least three points.
///
/// Returns `None` for a ring that has too few distinct points left to bound
/// an area.
fn repair_ring(pts: Vec<[f64; 3]>) -> Option<Vec<[f64; 3]>> {
    let mut repaired: Vec<[f64; 3]> = Vec::with_capacity(pts.len());
    for pt in pts {
        // Exact equality is deliberate: these are the same decimal literal
        // repeated in the source, not the result of a computation.
        if repaired.last() != Some(&pt) {
            repaired.push(pt);
        }
    }
    // GML closes its rings, CityJSON does not.
    if repaired.len() > 1 && repaired.first() == repaired.last() {
        repaired.pop();
    }
    (repaired.len() >= 3).then_some(repaired)
}

/// Whether a node is the named GML element — local name *and* namespace.
fn is_gml(node: &XmlNode, local: &str) -> bool {
    node.local == local && node.ns == GML_NS
}

/// The first direct child that is the named GML element.
fn gml_child<'a>(node: &'a XmlNode, local: &str) -> Option<&'a XmlNode> {
    node.children.iter().find(|child| is_gml(child, local))
}

/// A human-readable identifier for an element, for error messages.
fn element_context(node: &XmlNode) -> String {
    match node.gml_id() {
        Some(id) => format!("<{}> gml:id={id}", node.local),
        None => format!("<{}>", node.local),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(xml: &str) -> crate::xml::XmlNode {
        crate::xml::parse_str_for_tests(xml).unwrap()
    }

    #[test]
    fn polygon_with_poslist_exterior_and_interior() {
        let p = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml" gml:id="p1">
          <gml:exterior><gml:LinearRing gml:id="r1">
            <gml:posList>0 0 0 10 0 0 10 10 0 0 10 0 0 0 0</gml:posList>
          </gml:LinearRing></gml:exterior>
          <gml:interior><gml:LinearRing>
            <gml:pos>2 2 0</gml:pos><gml:pos>4 2 0</gml:pos><gml:pos>4 4 0</gml:pos><gml:pos>2 2 0</gml:pos>
          </gml:LinearRing></gml:interior>
        </gml:Polygon>"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(p.gml_id.as_deref(), Some("p1"));
        assert_eq!(p.rings.len(), 2);
        assert_eq!(p.rings[0].gml_id.as_deref(), Some("r1"));
        assert_eq!(
            p.rings[0].pts,
            vec![[0., 0., 0.], [10., 0., 0.], [10., 10., 0.], [0., 10., 0.]]
        ); // closure dropped
        assert_eq!(p.rings[1].pts.len(), 3);
    }
    #[test]
    fn consecutive_duplicates_dropped() {
        let p = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0 0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(p.rings[0].pts.len(), 3);
    }
    #[test]
    fn degenerate_ring_is_none() {
        assert!(parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0 10 0 0 0 0 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#
        ))
        .unwrap()
        .is_none());
    }
    #[test]
    fn odd_coordinate_count_is_invalid_geometry() {
        assert!(parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0 10 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#
        ))
        .is_err());
    }

    #[test]
    fn polygon_without_a_prefix_is_parsed_the_same() {
        // The GML namespace may be the document default; nothing may depend
        // on the `gml:` prefix being present.
        let p = parse_polygon(&node(
            r#"
        <Polygon xmlns="http://www.opengis.net/gml"><exterior><LinearRing>
          <posList>0 0 0 10 0 0 10 10 0 0 0 0</posList>
        </LinearRing></exterior></Polygon>"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(p.gml_id, None);
        assert_eq!(p.rings.len(), 1);
        assert_eq!(p.rings[0].pts.len(), 3);
        assert_eq!(p.rings[0].gml_id, None);
        assert_eq!(p.sem_idx, None);
    }

    #[test]
    fn interior_rings_keep_document_order_after_the_exterior() {
        let p = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml">
          <gml:interior><gml:LinearRing gml:id="i1">
            <gml:posList>1 1 0 2 1 0 2 2 0 1 1 0</gml:posList>
          </gml:LinearRing></gml:interior>
          <gml:exterior><gml:LinearRing gml:id="e">
            <gml:posList>0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
          </gml:LinearRing></gml:exterior>
          <gml:interior><gml:LinearRing gml:id="i2">
            <gml:posList>3 3 0 4 3 0 4 4 0 3 3 0</gml:posList>
          </gml:LinearRing></gml:interior>
        </gml:Polygon>"#,
        ))
        .unwrap()
        .unwrap();
        let ids: Vec<&str> = p
            .rings
            .iter()
            .map(|r| r.gml_id.as_deref().unwrap())
            .collect();
        assert_eq!(ids, vec!["e", "i1", "i2"]);
    }

    #[test]
    fn a_degenerate_interior_ring_skips_the_whole_polygon() {
        assert!(parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml">
          <gml:exterior><gml:LinearRing>
            <gml:posList>0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
          </gml:LinearRing></gml:exterior>
          <gml:interior><gml:LinearRing>
            <gml:posList>1 1 0 2 1 0 1 1 0</gml:posList>
          </gml:LinearRing></gml:interior>
        </gml:Polygon>"#
        ))
        .unwrap()
        .is_none());
    }

    #[test]
    fn non_numeric_coordinate_is_invalid_geometry() {
        let err = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0 10 nan-ish 0 10 10 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#,
        ))
        .unwrap_err();
        assert!(
            matches!(err, CityGmlError::InvalidGeometry { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn a_pos_that_is_not_a_triple_is_invalid_geometry() {
        let err = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:pos>0 0</gml:pos><gml:pos>1 0 0</gml:pos><gml:pos>1 1 0</gml:pos>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#,
        ))
        .unwrap_err();
        assert!(
            matches!(err, CityGmlError::InvalidGeometry { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn a_pos_list_outside_the_gml_namespace_is_not_a_pos_list() {
        // Same local name, different namespace: an application schema is
        // free to define its own <posList>, and it is not GML geometry.
        // With no GML positions left the ring has no points at all, so the
        // polygon is degenerate rather than silently half-parsed.
        assert!(parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"
                     xmlns:other="urn:example:other">
          <gml:exterior><gml:LinearRing>
            <other:posList>0 0 0 10 0 0 10 10 0 0 0 0</other:posList>
          </gml:LinearRing></gml:exterior>
        </gml:Polygon>"#
        ))
        .unwrap()
        .is_none());
    }

    #[test]
    fn a_pos_outside_the_gml_namespace_is_ignored() {
        // The GML positions are kept and the foreign ones do not join them.
        let p = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"
                     xmlns:other="urn:example:other">
          <gml:exterior><gml:LinearRing>
            <gml:pos>0 0 0</gml:pos>
            <other:pos>99 99 99</other:pos>
            <gml:pos>10 0 0</gml:pos>
            <gml:pos>10 10 0</gml:pos>
          </gml:LinearRing></gml:exterior>
        </gml:Polygon>"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            p.rings[0].pts,
            vec![[0., 0., 0.], [10., 0., 0.], [10., 10., 0.]]
        );
    }

    #[test]
    fn a_linear_ring_outside_the_gml_namespace_is_invalid_geometry() {
        let err = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"
                     xmlns:other="urn:example:other">
          <gml:exterior><other:LinearRing>
            <gml:posList>0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
          </other:LinearRing></gml:exterior>
        </gml:Polygon>"#,
        ))
        .unwrap_err();
        assert!(
            matches!(err, CityGmlError::InvalidGeometry { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn boundaries_outside_the_gml_namespace_are_not_boundaries() {
        let err = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"
                     xmlns:other="urn:example:other">
          <other:exterior><gml:LinearRing>
            <gml:posList>0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
          </gml:LinearRing></other:exterior>
        </gml:Polygon>"#,
        ))
        .unwrap_err();
        assert!(
            matches!(err, CityGmlError::InvalidGeometry { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn the_gml_3_2_namespace_is_not_accepted() {
        // This reader targets CityGML 2.0, which binds GML 3.1.1. Accepting
        // the GML 3.2 namespace here would claim a conformance the rest of
        // the converter does not have.
        assert!(parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml/3.2">
          <gml:exterior><gml:LinearRing>
            <gml:posList>0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
          </gml:LinearRing></gml:exterior>
        </gml:Polygon>"#
        ))
        .is_err());
    }

    #[test]
    fn non_finite_coordinates_are_invalid_geometry() {
        // "NaN" and "inf" parse as f64 but are not positions, and would
        // poison every bounding box they reached.
        for value in ["NaN", "inf", "-inf"] {
            let err = parse_polygon(&node(&format!(
                r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0 10 {value} 0 10 10 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#
            )))
            .unwrap_err();
            assert!(
                matches!(err, CityGmlError::InvalidGeometry { .. }),
                "{value}: {err:?}"
            );
        }
    }

    #[test]
    fn boundary_without_a_linear_ring_is_invalid_geometry() {
        let err = parse_polygon(&node(
            r#"<gml:Polygon xmlns:gml="http://www.opengis.net/gml">
                 <gml:exterior/>
               </gml:Polygon>"#,
        ))
        .unwrap_err();
        assert!(
            matches!(err, CityGmlError::InvalidGeometry { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn polygon_without_an_exterior_ring_is_invalid_geometry() {
        let err = parse_polygon(&node(
            r#"<gml:Polygon xmlns:gml="http://www.opengis.net/gml" gml:id="p9"/>"#,
        ))
        .unwrap_err();
        assert!(
            matches!(err, CityGmlError::InvalidGeometry { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn coordinates_may_be_separated_by_newlines_and_be_exponential() {
        let p = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList>0 0 0
            1e1 0 0
            10 10 0
            0 0 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            p.rings[0].pts,
            vec![[0., 0., 0.], [10., 0., 0.], [10., 10., 0.]]
        );
    }

    #[test]
    fn repair_ring_rules() {
        // Closing point dropped.
        assert_eq!(
            repair_ring(vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 0., 0.]]),
            Some(vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.]])
        );
        // Consecutive duplicates dropped, non-consecutive repeats kept.
        assert_eq!(
            repair_ring(vec![
                [0., 0., 0.],
                [0., 0., 0.],
                [1., 0., 0.],
                [1., 1., 0.],
                [1., 0., 0.],
            ]),
            Some(vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [1., 0., 0.],])
        );
        // Fewer than three points left over.
        assert_eq!(repair_ring(vec![]), None);
        assert_eq!(repair_ring(vec![[0., 0., 0.], [0., 0., 0.]]), None);
        assert_eq!(repair_ring(vec![[0., 0., 0.], [1., 0., 0.]]), None);
    }
}
