//! CityGML implicit geometry, flattened into ordinary GML geometry.
//!
//! An `core:ImplicitGeometry` is a prototype — a tree, a bench, a traffic sign
//! — written once in a local coordinate system and *placed* by a 4x4
//! transformation matrix and a reference point. CityJSON has no such thing:
//! every geometry it holds is written in the document's own coordinates. So a
//! placement is flattened here, by running each point of the template through
//! the matrix and translating the result by the reference point.
//!
//! Flattening duplicates the template once per placement, which is exactly
//! what CityGML's compactness bought and CityJSON cannot keep. It is the price
//! of the format, not a shortcut.

use super::{gml_child, parse_geometry, GmlGeometry, XlinkRegistry};
use crate::xml::XmlNode;
use crate::{is_in, CityGmlError, ParseReport, Skipped, CORE_NS};

/// Local names of the core-module elements a placement is made of.
const IMPLICIT_GEOMETRY: &str = "ImplicitGeometry";
const TRANSFORMATION_MATRIX: &str = "transformationMatrix";
const REFERENCE_POINT: &str = "referencePoint";
const RELATIVE_GML_GEOMETRY: &str = "relativeGMLGeometry";
const LIBRARY_OBJECT: &str = "libraryObject";

/// Local name of the GML point the reference point is written as.
const POINT: &str = "Point";
const POS: &str = "pos";

/// A transformation matrix is a 4x4 written row by row.
const MATRIX_SIZE: usize = 4;
const MATRIX_LEN: usize = MATRIX_SIZE * MATRIX_SIZE;

/// Coordinates per position, as everywhere else in this crate.
const DIMS: usize = 3;

/// The placement that changes nothing, used when a document states no matrix.
const IDENTITY: [f64; MATRIX_LEN] = [
    1., 0., 0., 0., //
    0., 1., 0., 0., //
    0., 0., 1., 0., //
    0., 0., 0., 1., //
];

/// Flatten one `lodXImplicitRepresentation` property into the geometry it
/// places.
///
/// `property` is the *property* element — the `frn:lod1ImplicitRepresentation`
/// — rather than the `core:ImplicitGeometry` inside it, so that the caller can
/// hand over whatever the property holds and be told, through `report`, when
/// that is nothing this converter can place.
///
/// `member` is the whole `core:cityObjectMember` subtree, and is what an
/// `xlink:href` on the `core:relativeGMLGeometry` is resolved against. It is
/// not the same scope as `registry`, which indexes *polygons*: a template is
/// referenced as a whole geometry — a `gml:MultiSurface`, a `gml:Solid` — and
/// the element itself is needed, not its polygons. A reference to a template
/// written in *another* member is not resolved: this reader holds one member
/// at a time by design. That is the one thing this function cannot do that
/// real data asks for, and it is reported rather than dropped in silence.
///
/// `registry` is passed through to the geometry reader, so a template whose
/// own surfaces are `xlink:href`s to polygons of the member still resolves.
///
/// Returns `Ok(None)` — having recorded why in `report` — for a placement this
/// converter cannot flatten: no `core:ImplicitGeometry`, a matrix that is not
/// 16 numbers, a missing or malformed reference point, a template held in an
/// external `core:libraryObject`, and a reference that names nothing in the
/// member. None of those is malformed CityGML, so none of them fails the
/// conversion.
///
/// # Errors
///
/// Propagates the geometry reader's errors for the template itself: malformed
/// geometry, and `xlink:href`s that name no polygon in the member.
pub(crate) fn flatten_implicit(
    property: &XmlNode,
    member: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Option<GmlGeometry>, CityGmlError> {
    let Some(implicit) = core_child(property, IMPLICIT_GEOMETRY) else {
        skip(
            report,
            property,
            format!("<{}> holds no <{IMPLICIT_GEOMETRY}>", property.local),
        );
        return Ok(None);
    };

    let matrix = match transformation_matrix(implicit) {
        Ok(matrix) => matrix,
        Err(reason) => {
            skip(report, implicit, reason);
            return Ok(None);
        }
    };

    // The reference point is where the prototype is placed, and CityGML
    // requires it: a placement without one states no position at all, and a
    // default of the origin would put the object somewhere it is not.
    let reference = match reference_point(implicit) {
        Ok(reference) => reference,
        Err(reason) => {
            skip(report, implicit, reason);
            return Ok(None);
        }
    };

    let Some(mut geometry) = template(implicit, member, registry, report)? else {
        return Ok(None);
    };
    // A matrix that collapses the template — a zero scale — leaves rings with
    // no area, which the polygon reader would have rejected had they been
    // written that way. It is left as it stands: the document said so.
    for polygon in geometry.polygons_mut() {
        for ring in &mut polygon.rings {
            for point in &mut ring.pts {
                *point = place(&matrix, &reference, *point);
            }
        }
    }
    Ok(Some(geometry))
}

/// Place one template point: `M · [p, 1]`, then translated by the reference
/// point.
///
/// The matrix is row-major — CityGML 2.0 § 10.2 writes it row by row — so row
/// `i` starts at `i * 4` and its fourth entry is that row's translation. Only
/// the first three rows are read: the fourth is the homogeneous row, which is
/// `(0 0 0 1)` for the affine placements CityGML allows, and dividing by it
/// would be a projection no city model states.
fn place(matrix: &[f64; MATRIX_LEN], reference: &[f64; DIMS], point: [f64; DIMS]) -> [f64; DIMS] {
    std::array::from_fn(|index| {
        let row = &matrix[index * MATRIX_SIZE..];
        row[0] * point[0] + row[1] * point[1] + row[2] * point[2] + row[3] + reference[index]
    })
}

/// The transformation matrix of a placement, or the identity when it states
/// none.
///
/// A `core:transformationMatrix` is optional in the schema, and a placement
/// without one is still a placement: the prototype goes to the reference point
/// unrotated and unscaled. A matrix that is *present* and not 16 finite
/// numbers is a different thing — the document meant to say something and said
/// it wrong — so it is an error rather than a silent identity.
fn transformation_matrix(implicit: &XmlNode) -> Result<[f64; MATRIX_LEN], String> {
    let Some(node) = core_child(implicit, TRANSFORMATION_MATRIX) else {
        return Ok(IDENTITY);
    };
    numbers(&node.text, TRANSFORMATION_MATRIX)
}

/// The point a placement puts its prototype at.
fn reference_point(implicit: &XmlNode) -> Result<[f64; DIMS], String> {
    let point = core_child(implicit, REFERENCE_POINT)
        .and_then(|property| gml_child(property, POINT))
        .and_then(|point| gml_child(point, POS));
    let Some(pos) = point else {
        return Err(format!(
            "<{IMPLICIT_GEOMETRY}> has no <{REFERENCE_POINT}> holding a GML \
             <{POINT}> with a <{POS}>, so it states no position"
        ));
    };
    // The `srsDimension` a 2D point would carry is not honoured: a placement
    // in two dimensions has no height to give the object, and guessing one
    // would invent geometry.
    numbers(&pos.text, REFERENCE_POINT)
}

/// Exactly `N` finite numbers, from the whitespace-separated text of an
/// element, or why the text is not that.
///
/// `element` is the local name reported back, which is the property the reader
/// was after rather than necessarily the element the text sits in: a reference
/// point's numbers are written in a `gml:pos`, and what a report needs to name
/// is the `core:referencePoint`.
///
/// `NaN` and the infinities are rejected along with the tokens that are not
/// numbers at all: they parse happily, and would poison every coordinate they
/// reached.
fn numbers<const N: usize>(text: &str, element: &str) -> Result<[f64; N], String> {
    let tokens: Vec<&str> = text.split_ascii_whitespace().collect();
    if tokens.len() != N {
        return Err(format!(
            "<{element}> holds {} number(s), expected {N}",
            tokens.len()
        ));
    }
    let mut values = [0.0; N];
    for (slot, token) in values.iter_mut().zip(tokens) {
        *slot = token
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("<{element}> entry {token:?} is not a finite number"))?;
    }
    Ok(values)
}

/// The prototype a placement points at: the geometry inline under
/// `core:relativeGMLGeometry`, or the one its `xlink:href` names inside the
/// member.
fn template(
    implicit: &XmlNode,
    member: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Option<GmlGeometry>, CityGmlError> {
    let Some(property) = core_child(implicit, RELATIVE_GML_GEOMETRY) else {
        let reason = match core_child(implicit, LIBRARY_OBJECT) {
            // A library object is a prototype in another file — a VRML or
            // DXF model — which this converter does not fetch and could not
            // read as GML if it did.
            Some(_) => format!(
                "<{IMPLICIT_GEOMETRY}> states its prototype as an external \
                 <{LIBRARY_OBJECT}>, which is not read"
            ),
            None => format!("<{IMPLICIT_GEOMETRY}> has no <{RELATIVE_GML_GEOMETRY}>"),
        };
        skip(report, implicit, reason);
        return Ok(None);
    };

    if let Some(href) = property.href() {
        let Some(node) = href
            .strip_prefix('#')
            .and_then(|id| member.descendants().find(|node| node.gml_id() == Some(id)))
        else {
            skip(
                report,
                property,
                format!(
                    "<{RELATIVE_GML_GEOMETRY}> xlink:href {href} names no geometry in this \
                     cityObjectMember; a prototype shared between members is not resolved"
                ),
            );
            return Ok(None);
        };
        return referenced_template(node, property, registry, report);
    }

    for child in &property.children {
        if let Some(geometry) = parse_geometry(child, registry, report)? {
            return Ok(Some(geometry));
        }
    }
    skip(
        report,
        property,
        format!("no supported GML geometry in <{RELATIVE_GML_GEOMETRY}>"),
    );
    Ok(None)
}

/// One referenced element as a geometry, or a skip naming the property that
/// pointed at it.
fn referenced_template(
    node: &XmlNode,
    property: &XmlNode,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<Option<GmlGeometry>, CityGmlError> {
    match parse_geometry(node, registry, report)? {
        Some(geometry) => Ok(Some(geometry)),
        None => {
            skip(
                report,
                property,
                format!(
                    "<{RELATIVE_GML_GEOMETRY}> names <{}>, which is not a supported GML geometry",
                    node.local
                ),
            );
            Ok(None)
        }
    }
}

/// The first direct child that is the named element of the CityGML core
/// module, in either supported version.
fn core_child<'a>(node: &'a XmlNode, local: &str) -> Option<&'a XmlNode> {
    node.children
        .iter()
        .find(|child| is_in(child, &CORE_NS, local))
}

/// Record a placement this converter cannot flatten.
fn skip(report: &mut ParseReport, node: &XmlNode, reason: String) {
    report.skipped.push(Skipped {
        element: node.local.clone(),
        gml_id: node.gml_id().map(str::to_owned),
        reason,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(xml: &str) -> XmlNode {
        crate::xml::parse_str_for_tests(xml).unwrap()
    }

    /// The namespaces every fixture below binds.
    const NS: &str = r#"xmlns:core="http://www.opengis.net/citygml/2.0"
         xmlns:veg="http://www.opengis.net/citygml/vegetation/2.0"
         xmlns:gml="http://www.opengis.net/gml"
         xmlns:xlink="http://www.w3.org/1999/xlink""#;

    /// The six faces of the unit cube, as closed GML rings.
    const CUBE_FACES: [&str; 6] = [
        "0 0 0 1 0 0 1 1 0 0 1 0 0 0 0",
        "0 0 1 0 1 1 1 1 1 1 0 1 0 0 1",
        "0 0 0 0 0 1 1 0 1 1 0 0 0 0 0",
        "1 1 0 1 1 1 0 1 1 0 1 0 1 1 0",
        "0 1 0 0 1 1 0 0 1 0 0 0 0 1 0",
        "1 0 0 1 0 1 1 1 1 1 1 0 1 0 0",
    ];

    /// The unit cube as a `gml:MultiSurface`, carrying `gml_id` so that a
    /// reference can name it.
    fn cube(gml_id: &str) -> String {
        let members: String = CUBE_FACES
            .iter()
            .map(|face| {
                format!(
                    "<gml:surfaceMember><gml:Polygon><gml:exterior><gml:LinearRing>\
                     <gml:posList>{face}</gml:posList>\
                     </gml:LinearRing></gml:exterior></gml:Polygon></gml:surfaceMember>"
                )
            })
            .collect();
        format!(r#"<gml:MultiSurface gml:id="{gml_id}">{members}</gml:MultiSurface>"#)
    }

    /// A `veg:lod2ImplicitRepresentation` around the given content of its
    /// `core:ImplicitGeometry`.
    fn placement(content: &str) -> String {
        format!(
            "<veg:lod2ImplicitRepresentation {NS}>\
             <core:ImplicitGeometry gml:id=\"ig1\">{content}</core:ImplicitGeometry>\
             </veg:lod2ImplicitRepresentation>"
        )
    }

    /// The `core:transformationMatrix` and `core:referencePoint` of a
    /// placement, in the order CityGML writes them.
    fn matrix_and_point(matrix: &str, reference: &str) -> String {
        format!(
            "<core:transformationMatrix>{matrix}</core:transformationMatrix>\
             <core:referencePoint><gml:Point><gml:pos>{reference}</gml:pos></gml:Point>\
             </core:referencePoint>"
        )
    }

    /// A `core:relativeGMLGeometry` holding the unit cube inline.
    fn inline_cube() -> String {
        format!(
            "<core:relativeGMLGeometry>{}</core:relativeGMLGeometry>",
            cube("template")
        )
    }

    /// Flatten a placement, the property element standing in for the whole
    /// member — which is what it is, in a document that states its prototype
    /// inline.
    fn flatten(xml: &str) -> (Option<GmlGeometry>, ParseReport) {
        let root = node(xml);
        let registry = XlinkRegistry::collect(&root);
        let mut report = ParseReport::default();
        let geometry = flatten_implicit(&root, &root, &registry, &mut report)
            .unwrap_or_else(|err| panic!("flatten failed: {err}"));
        (geometry, report)
    }

    /// Every point of a geometry, in document order.
    fn points(geometry: &GmlGeometry) -> Vec<[f64; 3]> {
        geometry
            .polygons()
            .iter()
            .flat_map(|polygon| polygon.rings.iter())
            .flat_map(|ring| ring.pts.iter().copied())
            .collect()
    }

    /// The component-wise minimum and maximum of a geometry's points.
    fn bbox(geometry: &GmlGeometry) -> ([f64; 3], [f64; 3]) {
        let pts = points(geometry);
        let min = std::array::from_fn(|i| pts.iter().map(|p| p[i]).fold(f64::MAX, f64::min));
        let max = std::array::from_fn(|i| pts.iter().map(|p| p[i]).fold(f64::MIN, f64::max));
        (min, max)
    }

    /// The exterior ring of the first face, which is the cube's bottom.
    fn first_face(geometry: &GmlGeometry) -> Vec<[f64; 3]> {
        geometry.polygons()[0].rings[0].pts.clone()
    }

    #[test]
    fn the_identity_matrix_places_the_template_at_the_reference_point() {
        let (geometry, report) = flatten(&placement(&format!(
            "{}{}",
            matrix_and_point("1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1", "10 20 30"),
            inline_cube()
        )));
        let geometry = geometry.expect("the placement flattens");
        assert_eq!(geometry.polygons().len(), 6);
        assert_eq!(bbox(&geometry), ([10., 20., 30.], [11., 21., 31.]));
        assert_eq!(
            first_face(&geometry),
            vec![
                [10., 20., 30.],
                [11., 20., 30.],
                [11., 21., 30.],
                [10., 21., 30.]
            ]
        );
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    #[test]
    fn a_diagonal_scale_of_two_doubles_the_template() {
        let (geometry, report) = flatten(&placement(&format!(
            "{}{}",
            matrix_and_point("2 0 0 0 0 2 0 0 0 0 2 0 0 0 0 1", "10 20 30"),
            inline_cube()
        )));
        let geometry = geometry.expect("the placement flattens");
        // The cube is twice the size, and its local origin is still at the
        // reference point: the scale is applied before the translation.
        assert_eq!(bbox(&geometry), ([10., 20., 30.], [12., 22., 32.]));
        assert_eq!(
            first_face(&geometry),
            vec![
                [10., 20., 30.],
                [12., 20., 30.],
                [12., 22., 30.],
                [10., 22., 30.]
            ]
        );
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// The matrix is read row by row, so the fourth entry of each row is that
    /// row's translation. A column-major reading would put the translation in
    /// the last three entries instead, and this fixture would move by nothing.
    #[test]
    fn the_matrix_is_row_major_so_its_fourth_column_translates() {
        let (geometry, report) = flatten(&placement(&format!(
            "{}{}",
            matrix_and_point("1 0 0 5 0 1 0 6 0 0 1 7 0 0 0 1", "10 20 30"),
            inline_cube()
        )));
        let geometry = geometry.expect("the placement flattens");
        assert_eq!(bbox(&geometry), ([15., 26., 37.], [16., 27., 38.]));
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// A rotation is a rotation: a quarter turn about z maps (1,0,0) to
    /// (0,1,0), and the matrix says so row by row.
    #[test]
    fn a_rotation_matrix_turns_the_template() {
        let (geometry, _) = flatten(&placement(&format!(
            "{}{}",
            matrix_and_point("0 -1 0 0 1 0 0 0 0 0 1 0 0 0 0 1", "0 0 0"),
            inline_cube()
        )));
        let geometry = geometry.expect("the placement flattens");
        assert_eq!(
            first_face(&geometry),
            vec![[0., 0., 0.], [0., 1., 0.], [-1., 1., 0.], [-1., 0., 0.]]
        );
    }

    /// A placement without a matrix is still a placement: the prototype goes
    /// to the reference point as it stands.
    #[test]
    fn a_missing_matrix_is_the_identity() {
        let (geometry, report) = flatten(&placement(&format!(
            "<core:referencePoint><gml:Point><gml:pos>10 20 30</gml:pos></gml:Point>\
             </core:referencePoint>{}",
            inline_cube()
        )));
        let geometry = geometry.expect("the placement flattens");
        assert_eq!(bbox(&geometry), ([10., 20., 30.], [11., 21., 31.]));
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    #[test]
    fn a_matrix_that_is_not_sixteen_numbers_is_skipped() {
        for matrix in [
            "1 0 0 0 0 1 0 0 0 0 1 0 0 0 0",     // fifteen
            "1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 1", // seventeen
            "",
        ] {
            let (geometry, report) = flatten(&placement(&format!(
                "{}{}",
                matrix_and_point(matrix, "10 20 30"),
                inline_cube()
            )));
            assert!(geometry.is_none(), "{matrix:?}");
            assert_eq!(report.skipped.len(), 1, "{matrix:?}: {report:?}");
            assert_eq!(report.skipped[0].element, IMPLICIT_GEOMETRY);
            assert_eq!(report.skipped[0].gml_id.as_deref(), Some("ig1"));
            assert!(
                report.skipped[0].reason.contains(TRANSFORMATION_MATRIX),
                "{matrix:?}: {report:?}"
            );
        }
    }

    #[test]
    fn a_matrix_entry_that_is_not_a_number_is_skipped() {
        let (geometry, report) = flatten(&placement(&format!(
            "{}{}",
            matrix_and_point("1 0 0 0 0 1 0 0 0 0 1 0 0 0 nope 1", "10 20 30"),
            inline_cube()
        )));
        assert!(geometry.is_none());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert!(report.skipped[0].reason.contains("nope"), "{report:?}");
    }

    /// The reference point is where the object *is*. Defaulting it to the
    /// origin would place the prototype somewhere it is not, so a placement
    /// without one is dropped and reported.
    #[test]
    fn a_missing_reference_point_is_skipped() {
        for content in [
            String::new(),
            "<core:referencePoint/>".to_string(),
            "<core:referencePoint><gml:Point/></core:referencePoint>".to_string(),
        ] {
            let (geometry, report) = flatten(&placement(&format!(
                "<core:transformationMatrix>1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\
                 </core:transformationMatrix>{content}{}",
                inline_cube()
            )));
            assert!(geometry.is_none(), "{content:?}");
            assert_eq!(report.skipped.len(), 1, "{content:?}: {report:?}");
            assert_eq!(report.skipped[0].element, IMPLICIT_GEOMETRY);
            assert!(
                report.skipped[0].reason.contains(REFERENCE_POINT),
                "{content:?}: {report:?}"
            );
        }
    }

    #[test]
    fn a_reference_point_that_is_not_a_triple_is_skipped() {
        let (geometry, report) = flatten(&placement(&format!(
            "{}{}",
            matrix_and_point("1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1", "10 20"),
            inline_cube()
        )));
        assert!(geometry.is_none());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert!(
            report.skipped[0].reason.contains(REFERENCE_POINT),
            "{report:?}"
        );
    }

    /// A prototype named by reference is flattened like an inline one, as long
    /// as it is written in the same member.
    #[test]
    fn a_template_referenced_inside_the_member_is_resolved() {
        let member = node(&format!(
            r##"<core:cityObjectMember {NS}>
                  <veg:SolitaryVegetationObject gml:id="tree-1">
                    <veg:lod1ImplicitRepresentation><core:ImplicitGeometry>
                      {}
                      <core:relativeGMLGeometry>{}</core:relativeGMLGeometry>
                    </core:ImplicitGeometry></veg:lod1ImplicitRepresentation>
                    <veg:lod2ImplicitRepresentation><core:ImplicitGeometry>
                      {}
                      <core:relativeGMLGeometry xlink:href="#template"/>
                    </core:ImplicitGeometry></veg:lod2ImplicitRepresentation>
                  </veg:SolitaryVegetationObject>
                </core:cityObjectMember>"##,
            matrix_and_point("1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1", "10 20 30"),
            cube("template"),
            matrix_and_point("1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1", "50 60 70"),
        ));
        let property = member
            .descendants()
            .find(|node| node.local == "lod2ImplicitRepresentation")
            .unwrap();
        let registry = XlinkRegistry::collect(&member);
        let mut report = ParseReport::default();
        let geometry = flatten_implicit(property, &member, &registry, &mut report)
            .unwrap()
            .expect("the reference resolves");
        // The second placement puts the *same* prototype somewhere else: each
        // use is flattened independently, and the duplication is the point.
        assert_eq!(bbox(&geometry), ([50., 60., 70.], [51., 61., 71.]));
        assert!(report.skipped.is_empty(), "{report:?}");
    }

    /// The one limitation worth stating out loud: a prototype written in
    /// another `cityObjectMember` — which is what a document that shares one
    /// template between thousands of objects does — is out of scope here, and
    /// says so rather than vanishing.
    #[test]
    fn a_template_outside_the_member_is_skipped_with_a_reason() {
        let (geometry, report) = flatten(&placement(&format!(
            r##"{}<core:relativeGMLGeometry xlink:href="#elsewhere"/>"##,
            matrix_and_point("1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1", "10 20 30"),
        )));
        assert!(geometry.is_none());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].element, RELATIVE_GML_GEOMETRY);
        assert!(
            report.skipped[0].reason.contains("#elsewhere"),
            "{report:?}"
        );
        assert!(
            report.skipped[0].reason.contains("shared between members"),
            "{report:?}"
        );
    }

    #[test]
    fn a_library_object_is_skipped_by_name() {
        let (geometry, report) = flatten(&placement(&format!(
            "{}<core:libraryObject>tree.vrml</core:libraryObject>",
            matrix_and_point("1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1", "10 20 30"),
        )));
        assert!(geometry.is_none());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert!(
            report.skipped[0].reason.contains(LIBRARY_OBJECT),
            "{report:?}"
        );
    }

    #[test]
    fn a_relative_geometry_holding_nothing_readable_is_skipped() {
        let (geometry, report) = flatten(&placement(&format!(
            "{}<core:relativeGMLGeometry><gml:Sphere/></core:relativeGMLGeometry>",
            matrix_and_point("1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1", "10 20 30"),
        )));
        assert!(geometry.is_none());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].element, RELATIVE_GML_GEOMETRY);
    }

    #[test]
    fn a_property_without_an_implicit_geometry_is_skipped() {
        let (geometry, report) = flatten(&format!(
            r#"<veg:lod2ImplicitRepresentation {NS} gml:id="p1"/>"#
        ));
        assert!(geometry.is_none());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
        assert_eq!(report.skipped[0].element, "lod2ImplicitRepresentation");
        assert_eq!(report.skipped[0].gml_id.as_deref(), Some("p1"));
        assert!(
            report.skipped[0].reason.contains(IMPLICIT_GEOMETRY),
            "{report:?}"
        );
    }

    /// An `ImplicitGeometry` in another namespace is not the CityGML one: an
    /// application schema is free to define an element of that name.
    #[test]
    fn an_implicit_geometry_outside_the_core_namespace_is_not_one() {
        let (geometry, report) = flatten(&format!(
            r#"<veg:lod2ImplicitRepresentation {NS} xmlns:x="urn:example:other">
                 <x:ImplicitGeometry/>
               </veg:lod2ImplicitRepresentation>"#
        ));
        assert!(geometry.is_none());
        assert_eq!(report.skipped.len(), 1, "{report:?}");
    }
}
