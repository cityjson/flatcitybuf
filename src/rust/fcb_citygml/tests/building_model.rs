//! The streaming scan of a `CityModel`, down to the intermediate model.
//!
//! These tests drive `parse_to_model`, which is the half of the conversion
//! that reads CityGML; turning what it produces into CityJSON is the
//! converter's job and is tested against fixtures elsewhere.

use fcb_citygml::gml::GmlGeometry;
use fcb_citygml::model::IntermediateObject;
use fcb_citygml::{parse_to_model, ParseOptions, ParseReport};
use std::io::BufReader;

/// The six faces of a unit cube whose lower corner is at (1000, 2000, 0), as
/// closed GML rings. The offset keeps the coordinates far enough from the
/// origin that a converter that forgets to translate is obvious.
const CUBE_FACES: [&str; 6] = [
    "1000 2000 0 1001 2000 0 1001 2001 0 1000 2001 0 1000 2000 0",
    "1000 2000 1 1000 2001 1 1001 2001 1 1001 2000 1 1000 2000 1",
    "1000 2000 0 1000 2000 1 1001 2000 1 1001 2000 0 1000 2000 0",
    "1001 2001 0 1001 2001 1 1000 2001 1 1000 2001 0 1001 2001 0",
    "1000 2001 0 1000 2001 1 1000 2000 1 1000 2000 0 1000 2001 0",
    "1001 2000 0 1001 2000 1 1001 2001 1 1001 2001 0 1001 2000 0",
];

/// The cube as a `gml:Solid`, with `attrs` spliced into its start tag.
fn cube_solid(attrs: &str) -> String {
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
    format!(
        "<gml:Solid{attrs}><gml:exterior><gml:CompositeSurface>{members}\
         </gml:CompositeSurface></gml:exterior></gml:Solid>"
    )
}

/// A `CityModel` around `body`, with the namespace bindings every test needs.
fn city_model(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<core:CityModel xmlns:core="http://www.opengis.net/citygml/2.0"
                xmlns:bldg="http://www.opengis.net/citygml/building/2.0"
                xmlns:gen="http://www.opengis.net/citygml/generics/2.0"
                xmlns:gml="http://www.opengis.net/gml"
                xmlns:xlink="http://www.w3.org/1999/xlink">
{body}
</core:CityModel>"#
    )
}

/// A `gml:boundedBy` holding an `Envelope` named by `srs_name`.
fn bounded_by(srs_name: &str) -> String {
    format!(
        r#"<gml:boundedBy>
             <gml:Envelope srsName="{srs_name}" srsDimension="3">
               <gml:lowerCorner>1000 2000 0</gml:lowerCorner>
               <gml:upperCorner>1001 2001 1</gml:upperCorner>
             </gml:Envelope>
           </gml:boundedBy>"#
    )
}

/// A `core:cityObjectMember` holding a Building with the given start-tag
/// attributes and geometry properties.
fn building_member(attrs: &str, properties: &str) -> String {
    format!(
        "<core:cityObjectMember><bldg:Building{attrs}>{properties}\
         </bldg:Building></core:cityObjectMember>"
    )
}

/// Parse a whole document, failing the test on a hard error.
fn parse(
    xml: &str,
) -> (
    Vec<IntermediateObject>,
    Option<fcb_citygml::crs::NormalizedCrs>,
    ParseReport,
) {
    parse_to_model(BufReader::new(xml.as_bytes()), &ParseOptions::default())
        .unwrap_or_else(|err| panic!("parse failed: {err}"))
}

#[test]
fn lod1_building_to_intermediate_model() {
    let xml = city_model(&format!(
        "{}{}",
        bounded_by("EPSG:7415"),
        building_member(
            r#" gml:id="b1""#,
            &format!("<bldg:lod1Solid>{}</bldg:lod1Solid>", cube_solid(""))
        )
    ));

    let (objects, crs, report) = parse(&xml);

    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].id, "b1");
    assert_eq!(objects[0].co_type, cjseq::CityObjectType::Building);
    assert!(objects[0].attributes.is_empty());
    assert!(objects[0].children.is_empty());
    assert_eq!(objects[0].geometries.len(), 1);
    assert_eq!(objects[0].geometries[0].lod, "1");
    assert!(objects[0].geometries[0].surfaces.is_empty());
    let GmlGeometry::Solid(shells) = &objects[0].geometries[0].geometry else {
        panic!(
            "expected a Solid, got {:?}",
            objects[0].geometries[0].geometry
        );
    };
    assert_eq!(shells.len(), 1);
    assert_eq!(shells[0].len(), 6);
    assert_eq!(
        shells[0][0].rings[0].pts,
        vec![
            [1000., 2000., 0.],
            [1001., 2000., 0.],
            [1001., 2001., 0.],
            [1000., 2001., 0.]
        ]
    );

    let crs = crs.expect("the Envelope names a CRS");
    assert_eq!(
        crs.reference_system,
        "https://www.opengis.net/def/crs/EPSG/0/7415"
    );
    assert!(!crs.swap_axes);
    assert!(report.skipped.is_empty(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

#[test]
fn every_lod_property_becomes_its_own_geometry() {
    let xml = city_model(&format!(
        "{}{}",
        bounded_by("EPSG:7415"),
        building_member(
            r#" gml:id="b1""#,
            &format!(
                "<bldg:lod1Solid>{solid}</bldg:lod1Solid>\
                 <bldg:lod2MultiSurface><gml:MultiSurface>\
                   <gml:surfaceMember><gml:Polygon><gml:exterior><gml:LinearRing>\
                     <gml:posList>{face}</gml:posList>\
                   </gml:LinearRing></gml:exterior></gml:Polygon></gml:surfaceMember>\
                 </gml:MultiSurface></bldg:lod2MultiSurface>\
                 <bldg:lod0Geometry>{solid}</bldg:lod0Geometry>",
                solid = cube_solid(""),
                face = CUBE_FACES[0]
            )
        )
    ));

    let (objects, _, report) = parse(&xml);

    let lods: Vec<&str> = objects[0]
        .geometries
        .iter()
        .map(|g| g.lod.as_str())
        .collect();
    // Document order, one geometry per property.
    assert_eq!(lods, vec!["1", "2", "0"]);
    assert!(
        matches!(
            objects[0].geometries[1].geometry,
            GmlGeometry::MultiSurface(_)
        ),
        "{:?}",
        objects[0].geometries[1].geometry
    );
    assert!(report.skipped.is_empty(), "{report:?}");
}

#[test]
fn an_unknown_member_is_skipped_and_recorded() {
    let xml = city_model(&format!(
        "{}{}{}",
        bounded_by("EPSG:7415"),
        building_member(
            r#" gml:id="b1""#,
            &format!("<bldg:lod1Solid>{}</bldg:lod1Solid>", cube_solid(""))
        ),
        // A raster relief: valid CityGML, and a terrain component this
        // converter has no reader for — CityJSON can hold a TIN and nothing
        // else.
        r#"<core:cityObjectMember>
             <dem:RasterRelief xmlns:dem="http://www.opengis.net/citygml/relief/2.0"
                               gml:id="g1"/>
           </core:cityObjectMember>"#
    ));

    let (objects, _, report) = parse(&xml);

    // The building still comes through.
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].id, "b1");
    assert_eq!(report.skipped.len(), 1, "{report:?}");
    assert_eq!(report.skipped[0].element, "RasterRelief");
    assert_eq!(report.skipped[0].gml_id.as_deref(), Some("g1"));
    assert!(
        report.skipped[0].reason.contains("unsupported CityObject"),
        "{report:?}"
    );
}

#[test]
fn an_unknown_top_level_element_is_skipped_and_recorded() {
    let xml = city_model(r#"<core:someProperty><gml:Something gml:id="x1"/></core:someProperty>"#);

    let (objects, _, report) = parse(&xml);

    assert!(objects.is_empty());
    assert_eq!(report.skipped.len(), 1, "{report:?}");
    assert_eq!(report.skipped[0].element, "someProperty");
}

#[test]
fn a_building_without_a_gml_id_falls_back_to_its_member_index() {
    let xml = city_model(&format!(
        "{}{}{}",
        bounded_by("EPSG:7415"),
        building_member(
            "",
            &format!("<bldg:lod1Solid>{}</bldg:lod1Solid>", cube_solid(""))
        ),
        building_member(
            "",
            &format!("<bldg:lod1Solid>{}</bldg:lod1Solid>", cube_solid(""))
        )
    ));

    let (objects, _, _) = parse(&xml);

    let ids: Vec<&str> = objects.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["citygml-obj-0", "citygml-obj-1"]);
}

#[test]
fn a_geometry_property_holding_no_geometry_is_skipped_and_recorded() {
    let xml = city_model(&format!(
        "{}{}",
        bounded_by("EPSG:7415"),
        building_member(
            r#" gml:id="b1""#,
            "<bldg:lod2Solid><gml:Sphere/></bldg:lod2Solid>"
        )
    ));

    let (objects, _, report) = parse(&xml);

    assert_eq!(objects.len(), 1);
    assert!(objects[0].geometries.is_empty());
    assert_eq!(report.skipped.len(), 1, "{report:?}");
    assert_eq!(report.skipped[0].element, "lod2Solid");
}

#[test]
fn without_an_envelope_the_first_geometry_srs_name_is_used() {
    let xml = city_model(&building_member(
        r#" gml:id="b1""#,
        &format!(
            "<bldg:lod1Solid>{}</bldg:lod1Solid>",
            cube_solid(r#" srsName="urn:ogc:def:crs:EPSG::4326""#)
        ),
    ));

    let (_, crs, report) = parse(&xml);

    let crs = crs.expect("the Solid names a CRS");
    assert_eq!(
        crs.reference_system,
        "https://www.opengis.net/def/crs/EPSG/0/4326"
    );
    // The URN form carries the CRS's authoritative, latitude-first axis order.
    assert!(crs.swap_axes);
    assert!(report.warnings.is_empty(), "{report:?}");
}

#[test]
fn an_envelope_srs_name_wins_over_a_geometry_one() {
    let xml = city_model(&format!(
        "{}{}",
        bounded_by("EPSG:7415"),
        building_member(
            r#" gml:id="b1""#,
            &format!(
                "<bldg:lod1Solid>{}</bldg:lod1Solid>",
                cube_solid(r#" srsName="EPSG:28992""#)
            )
        )
    ));

    let (_, crs, _) = parse(&xml);

    assert_eq!(
        crs.unwrap().reference_system,
        "https://www.opengis.net/def/crs/EPSG/0/7415"
    );
}

#[test]
fn a_document_without_any_srs_name_warns_and_omits_the_crs() {
    let xml = city_model(&building_member(
        r#" gml:id="b1""#,
        &format!("<bldg:lod1Solid>{}</bldg:lod1Solid>", cube_solid("")),
    ));

    let (objects, crs, report) = parse(&xml);

    assert_eq!(objects.len(), 1);
    assert!(crs.is_none());
    assert_eq!(report.warnings.len(), 1, "{report:?}");
    assert!(
        report.warnings[0].contains("no srsName found"),
        "{report:?}"
    );
    assert!(report.skipped.is_empty(), "{report:?}");
}

#[test]
fn an_unrecognisable_srs_name_warns_and_omits_the_crs() {
    let xml = city_model(&bounded_by("urn:ogc:def:crs:OGC:1.3:CRS84"));

    let (_, crs, report) = parse(&xml);

    assert!(crs.is_none());
    assert_eq!(report.warnings.len(), 1, "{report:?}");
    assert!(report.warnings[0].contains("CRS84"), "{report:?}");
}

#[test]
fn a_compound_srs_name_warns_that_the_vertical_component_is_dropped() {
    let xml = city_model(&bounded_by(
        "urn:ogc:def:crs,crs:EPSG::28992,crs:EPSG::5709",
    ));

    let (_, crs, report) = parse(&xml);

    assert_eq!(
        crs.unwrap().reference_system,
        "https://www.opengis.net/def/crs/EPSG/0/28992"
    );
    assert_eq!(report.warnings.len(), 1, "{report:?}");
    assert!(report.warnings[0].contains("vertical"), "{report:?}");
}

#[test]
fn a_building_in_the_citygml_1_0_namespace_is_read_the_same() {
    let xml = format!(
        r#"<core:CityModel xmlns:core="http://www.opengis.net/citygml/1.0"
                           xmlns:bldg="http://www.opengis.net/citygml/building/1.0"
                           xmlns:gml="http://www.opengis.net/gml">
             {}
           </core:CityModel>"#,
        building_member(
            r#" gml:id="b1""#,
            &format!("<bldg:lod1Solid>{}</bldg:lod1Solid>", cube_solid(""))
        )
    );

    let (objects, _, _) = parse(&xml);

    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].id, "b1");
    assert_eq!(objects[0].geometries.len(), 1);
}

#[test]
fn a_solid_whose_faces_are_xlinks_into_the_member_resolves() {
    // The polygons live outside the solid, which is the shape a CityGML file
    // takes once its boundary surfaces carry the geometry: the registry has
    // to cover the whole member subtree, not just the geometry property.
    let definitions: String = CUBE_FACES
        .iter()
        .enumerate()
        .map(|(i, face)| {
            format!(
                r#"<bldg:boundedBy><bldg:WallSurface><bldg:lod2MultiSurface>
                     <gml:MultiSurface><gml:surfaceMember>
                       <gml:Polygon gml:id="p{i}"><gml:exterior><gml:LinearRing>
                         <gml:posList>{face}</gml:posList>
                       </gml:LinearRing></gml:exterior></gml:Polygon>
                     </gml:surfaceMember></gml:MultiSurface>
                   </bldg:lod2MultiSurface></bldg:WallSurface></bldg:boundedBy>"#
            )
        })
        .collect();
    let members: String = (0..CUBE_FACES.len())
        .map(|i| format!(r##"<gml:surfaceMember xlink:href="#p{i}"/>"##))
        .collect();
    let xml = city_model(&format!(
        "{}{}",
        bounded_by("EPSG:7415"),
        building_member(
            r#" gml:id="b1""#,
            &format!(
                "<bldg:lod2Solid><gml:Solid><gml:exterior><gml:CompositeSurface>{members}\
                 </gml:CompositeSurface></gml:exterior></gml:Solid></bldg:lod2Solid>{definitions}"
            )
        )
    ));

    let (objects, _, _) = parse(&xml);

    assert_eq!(objects[0].geometries.len(), 1);
    assert_eq!(objects[0].geometries[0].lod, "2");
    let GmlGeometry::Solid(shells) = &objects[0].geometries[0].geometry else {
        panic!("expected a Solid");
    };
    assert_eq!(shells[0].len(), 6);
    let ids: Vec<&str> = shells[0]
        .iter()
        .map(|p| p.gml_id.as_deref().unwrap())
        .collect();
    assert_eq!(ids, vec!["p0", "p1", "p2", "p3", "p4", "p5"]);
}
