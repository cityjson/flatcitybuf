//! GML geometry primitives shared by every CityGML module reader.
//!
//! Only the pieces CityJSON can express are modelled: a polygon is a list of
//! rings, the first exterior and the rest interior, each ring a list of 3D
//! points.

mod geometry;
mod implicit;

pub use geometry::GmlGeometry;
pub(crate) use geometry::{parse_geometry, parse_triangles, XlinkRegistry};
pub(crate) use implicit::flatten_implicit;

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
pub(crate) const GML_NS: &str = "http://www.opengis.net/gml";

/// Local names of the elements this module reads. A local name alone never
/// identifies an element — an application schema is free to define its own
/// `posList` — so every match pairs the local name with [`GML_NS`].
const EXTERIOR: &str = "exterior";
const INTERIOR: &str = "interior";
const LINEAR_RING: &str = "LinearRing";
const POS: &str = "pos";
const POS_LIST: &str = "posList";

/// Coordinates per position when nothing says otherwise. CityGML geometry is
/// 3D, and CityJSON has no other shape to write it in.
const DIMS: usize = 3;

/// Name of the GML attribute stating how many numbers make one position.
///
/// It may sit on the position element itself or on any element above it, and
/// the nearest declaration wins (GML 3.1.1, `SRSReferenceGroup`). CityGML
/// files put it on the `gml:posList` as a rule, and on the document's
/// `gml:Envelope` when they state it once for the whole file.
pub(crate) const SRS_DIMENSION_ATTR: &str = "srsDimension";

/// Reason recorded for a polygon whose rings collapse to no area.
pub(crate) const DEGENERATE: &str = "degenerate ring";

/// A `gml:Polygon` as this converter reads it: the polygon, or the reason
/// there is no CityJSON surface to write.
///
/// The two reasons are a ring that carries no area and coordinates stated in
/// a dimension CityJSON cannot hold. Both are valid GML that simply cannot be
/// written, so both are *reported* and neither is fatal — and the reason
/// travels with the outcome because the caller cannot tell the two apart.
pub(crate) type MaybePolygon = Result<Polygon3, String>;

/// Parse a `gml:Polygon` element into its repaired rings.
///
/// The boundary, ring and position elements inside it are recognised only in
/// the GML namespace; an element with a matching local name in any other one
/// is not GML geometry and is passed over, which leaves the polygon looking
/// as though that part were absent. Which element *is* the polygon is the
/// caller's decision, so `node` itself is not checked — `gml:Triangle` and
/// `gml:Rectangle` have the same content model and parse the same way.
///
/// Returns the reason instead of a polygon when one of its rings collapses to
/// fewer than three distinct points, and when the coordinates are stated in a
/// `srsDimension` other than three: neither can be written as a CityJSON
/// surface, and the caller records the reason as a skip.
///
/// # Errors
///
/// Returns [`CityGmlError::InvalidGeometry`] when the polygon is structurally
/// wrong rather than merely unwritable: no exterior ring, a boundary with no
/// `LinearRing`, a coordinate that is not a number, or a coordinate count
/// that is not a multiple of the dimension the document declared.
pub(crate) fn parse_polygon(node: &XmlNode) -> Result<MaybePolygon, CityGmlError> {
    let gml_id = node.gml_id().map(str::to_owned);
    let dims = srs_dimension(node, DIMS);

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
        let ring = match parse_ring(boundary, dims)? {
            Ok(ring) => ring,
            Err(reason) => return Ok(Err(reason)),
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
    Ok(Ok(Polygon3 {
        gml_id,
        rings,
        sem_idx: None,
    }))
}

/// Parse the `LinearRing` inside a `gml:exterior` or `gml:interior`.
///
/// `inherited` is the coordinate dimension in force outside this boundary;
/// the boundary and the ring may each override it.
///
/// Returns the reason instead of a ring when the ring is degenerate or is
/// stated in a dimension this converter cannot write.
fn parse_ring(boundary: &XmlNode, inherited: usize) -> Result<Result<Ring, String>, CityGmlError> {
    let dims = srs_dimension(boundary, inherited);
    let Some(linear_ring) = gml_child(boundary, LINEAR_RING) else {
        return Err(CityGmlError::InvalidGeometry {
            context: element_context(boundary),
            reason: format!("no GML <{LINEAR_RING}> child"),
        });
    };
    let pts = match parse_positions(linear_ring, srs_dimension(linear_ring, dims))? {
        Ok(pts) => pts,
        Err(reason) => return Ok(Err(reason)),
    };
    Ok(repair_ring(pts)
        .map(|pts| Ring {
            gml_id: linear_ring.gml_id().map(str::to_owned),
            pts,
        })
        .ok_or_else(|| DEGENERATE.to_string()))
}

/// Collect the points of a `LinearRing` from its `pos` and `posList` children.
///
/// `inherited` is the coordinate dimension in force outside each position,
/// which the position itself may override.
fn parse_positions(
    ring: &XmlNode,
    inherited: usize,
) -> Result<Result<Vec<[f64; 3]>, String>, CityGmlError> {
    let mut pts = Vec::new();
    for child in &ring.children {
        if child.ns != GML_NS {
            continue;
        }
        let dims = srs_dimension(child, inherited);
        match child.local.as_str() {
            POS => {
                let point = match parse_coords(&child.text, child, dims)? {
                    Ok(point) => point,
                    Err(reason) => return Ok(Err(reason)),
                };
                if point.len() != 1 {
                    return Err(CityGmlError::InvalidGeometry {
                        context: element_context(child),
                        reason: format!(
                            "<{POS}> holds {} coordinates, expected {dims}",
                            point.len() * dims
                        ),
                    });
                }
                pts.extend(point);
            }
            POS_LIST => match parse_coords(&child.text, child, dims)? {
                Ok(points) => pts.extend(points),
                Err(reason) => return Ok(Err(reason)),
            },
            _ => {}
        }
    }
    Ok(Ok(pts))
}

/// Parse whitespace-separated coordinates into 3D points.
///
/// Points are assembled as the tokens arrive rather than via an intermediate
/// list of scalars: a single `posList` can hold tens of thousands of them.
///
/// Returns the reason instead of the points when `dims` is not three: the
/// coordinates are perfectly good, and there is simply no CityJSON to write
/// them as. Grouping them into threes anyway is the failure this guards
/// against — a 2D ring of six points divides by three as readily as a 3D ring
/// of four, and the result is four points nowhere near the building.
fn parse_coords(
    text: &str,
    owner: &XmlNode,
    dims: usize,
) -> Result<Result<Vec<[f64; 3]>, String>, CityGmlError> {
    if dims != DIMS {
        let count = text.split_ascii_whitespace().count();
        return match count % dims {
            0 => Ok(Err(format!(
                "srsDimension {dims} is not supported; CityJSON holds 3D geometry alone"
            ))),
            _ => Err(not_a_multiple(owner, count, dims)),
        };
    }

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
        return Err(not_a_multiple(owner, pts.len() * DIMS + filled, dims));
    }
    Ok(Ok(pts))
}

/// The error for a coordinate count that does not divide into whole
/// positions, which is malformed geometry whatever the dimension.
fn not_a_multiple(owner: &XmlNode, count: usize, dims: usize) -> CityGmlError {
    CityGmlError::InvalidGeometry {
        context: element_context(owner),
        reason: format!("{count} coordinates is not a multiple of the srsDimension {dims}"),
    }
}

/// The coordinate dimension an element declares, or `inherited` where it
/// declares none.
///
/// A value that is not a positive whole number says nothing about the
/// geometry, so the dimension already in force stands rather than a guess
/// being made from it.
fn srs_dimension(node: &XmlNode, inherited: usize) -> usize {
    node.attr(SRS_DIMENSION_ATTR)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|dims| *dims > 0)
        .unwrap_or(inherited)
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
pub(crate) fn is_gml(node: &XmlNode, local: &str) -> bool {
    node.local == local && node.ns == GML_NS
}

/// The first direct child that is the named GML element.
pub(crate) fn gml_child<'a>(node: &'a XmlNode, local: &str) -> Option<&'a XmlNode> {
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
        .is_err());
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
        .is_err());
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
        .is_err());
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

    /// A ring stated in two dimensions is content this converter cannot
    /// write, not content that is wrong: the polygon is dropped with a reason
    /// and the document survives.
    #[test]
    fn a_two_dimensional_ring_is_dropped_with_a_reason() {
        let reason = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList srsDimension="2">0 0 10 0 10 10 0 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#,
        ))
        .unwrap()
        .unwrap_err();
        assert!(reason.contains("srsDimension 2"), "{reason}");
    }

    /// The case that used to pass silently: twelve coordinates divide by
    /// three, so a 2D ring of six points was regrouped into four 3D points
    /// that were nowhere near the building. The declared dimension is read
    /// before the coordinates are grouped, so it cannot happen again.
    #[test]
    fn a_two_dimensional_ring_whose_count_divides_by_three_is_still_dropped() {
        let polygon = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList srsDimension="2">0 0 10 0 10 10 5 15 0 10 0 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#,
        ))
        .unwrap();
        assert!(polygon.is_err(), "{polygon:?}");
    }

    /// `srsDimension` is inherited from the nearest GML element above the
    /// position that states one.
    #[test]
    fn srs_dimension_is_inherited_from_an_ancestor() {
        for xml in [
            r#"<gml:Polygon xmlns:gml="http://www.opengis.net/gml" srsDimension="2">
                 <gml:exterior><gml:LinearRing>
                   <gml:posList>0 0 10 0 10 10 0 0</gml:posList>
                 </gml:LinearRing></gml:exterior></gml:Polygon>"#,
            r#"<gml:Polygon xmlns:gml="http://www.opengis.net/gml">
                 <gml:exterior><gml:LinearRing srsDimension="2">
                   <gml:posList>0 0 10 0 10 10 0 0</gml:posList>
                 </gml:LinearRing></gml:exterior></gml:Polygon>"#,
            // The nearest declaration wins: 3D geometry inside a 2D document.
            r#"<gml:Polygon xmlns:gml="http://www.opengis.net/gml" srsDimension="2">
                 <gml:exterior><gml:LinearRing>
                   <gml:posList srsDimension="3">0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
                 </gml:LinearRing></gml:exterior></gml:Polygon>"#,
        ] {
            let outcome = parse_polygon(&node(xml)).unwrap();
            assert_eq!(
                outcome.is_ok(),
                xml.contains(r#"posList srsDimension="3""#),
                "{xml}"
            );
        }
    }

    /// A count that does not divide by the *declared* dimension is malformed
    /// geometry however many dimensions were declared, and stays fatal.
    #[test]
    fn a_count_that_does_not_divide_by_the_declared_dimension_is_invalid_geometry() {
        for pos_list in [
            // No srsDimension: three, and five coordinates is not a multiple.
            r#"<gml:posList>0 0 0 10 0</gml:posList>"#,
            // Declared two, and five is not a multiple of that either.
            r#"<gml:posList srsDimension="2">0 0 10 0 10</gml:posList>"#,
        ] {
            let err = parse_polygon(&node(&format!(
                r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          {pos_list}
        </gml:LinearRing></gml:exterior></gml:Polygon>"#
            )))
            .unwrap_err();
            assert!(
                matches!(err, CityGmlError::InvalidGeometry { .. }),
                "{pos_list}: {err:?}"
            );
        }
    }

    /// A `gml:pos` follows the same rule as a `gml:posList`.
    #[test]
    fn a_two_dimensional_pos_is_dropped_with_a_reason() {
        let reason = parse_polygon(&node(
            r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:pos srsDimension="2">0 0</gml:pos>
          <gml:pos srsDimension="2">1 0</gml:pos>
          <gml:pos srsDimension="2">1 1</gml:pos>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#,
        ))
        .unwrap()
        .unwrap_err();
        assert!(reason.contains("srsDimension 2"), "{reason}");
    }

    /// An `srsDimension` that is not a positive number says nothing, and the
    /// inherited dimension stands.
    #[test]
    fn an_unreadable_srs_dimension_leaves_the_inherited_one_alone() {
        for value in ["", "three", "0", "-2"] {
            let polygon = parse_polygon(&node(&format!(
                r#"
        <gml:Polygon xmlns:gml="http://www.opengis.net/gml"><gml:exterior><gml:LinearRing>
          <gml:posList srsDimension="{value}">0 0 0 10 0 0 10 10 0 0 0 0</gml:posList>
        </gml:LinearRing></gml:exterior></gml:Polygon>"#
            )))
            .unwrap();
            assert!(polygon.is_ok(), "{value}: {polygon:?}");
        }
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
