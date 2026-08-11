//! The converting half: intermediate model in, CityJSONSeq out.
//!
//! Everything the reader produced is in real-world coordinates, because the
//! CityJSON `transform` cannot be chosen until the last coordinate has been
//! seen. So this runs two passes. The first finds the component-wise minimum
//! and maximum of every coordinate in the document, which fixes `translate`
//! and the geographical extent; the second quantises against that transform
//! and builds one feature per top-level object.
//!
//! Vertices are deduplicated **per feature**, not per document: a
//! CityJSONFeature carries its own `vertices` array and its boundaries index
//! into that array alone (§ 7.2 of the CityJSON spec). A shared table would
//! produce indices no reader could resolve.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use cjseq::{GeographicalExtent, Metadata, ReferenceSystem};
use serde_json::Value;

use crate::crs::NormalizedCrs;
use crate::gml::{GmlGeometry, Polygon3};
use crate::model::{IntermediateGeometry, IntermediateObject};
use crate::{CityGmlDocument, ParseOptions, ParseReport};

/// Convert the intermediate model into CityJSONSeq structures.
///
/// `crs` is the document's reference system, if it named one that could be
/// normalised; `objects` are the top-level city objects in document order,
/// and the features come out in that same order.
///
/// Appearances are not a parameter yet. The plan's signature has a
/// `Vec<appearance::SurfaceData>` here, but that module does not exist until
/// materials and textures are read, and a parameter that can only ever be
/// given an empty vector documents nothing while forcing a type into
/// existence for no reader. It is added when there is something to pass.
pub fn convert(
    mut objects: Vec<IntermediateObject>,
    crs: Option<NormalizedCrs>,
    opts: &ParseOptions,
    report: &mut ParseReport,
) -> CityGmlDocument {
    // Before anything measures a coordinate: a latitude-first source has to
    // be reordered, and both the extent and the quantisation must see the
    // reordered values.
    if crs.as_ref().is_some_and(|crs| crs.swap_axes) {
        swap_axes(&mut objects);
    }

    let extent = Extent::of(&objects);
    let quantizer = Quantizer {
        scale: opts.scale,
        translate: extent.map_or([0.0; 3], |extent| extent.min),
    };

    let features = objects
        .iter()
        .map(|object| feature(object, &quantizer))
        .collect();

    CityGmlDocument {
        metadata: metadata_line(&quantizer, extent, crs.as_ref(), report),
        features,
    }
}

/// The CityJSONSeq "first line": the transform and metadata every feature
/// after it is read against.
fn metadata_line(
    quantizer: &Quantizer,
    extent: Option<Extent>,
    crs: Option<&NormalizedCrs>,
    report: &mut ParseReport,
) -> cjseq::CityJSON {
    let mut metadata = cjseq::CityJSON::new();
    metadata.transform.scale = quantizer.scale.to_vec();
    metadata.transform.translate = quantizer.translate.to_vec();
    metadata.metadata = document_metadata(extent, crs, report);
    metadata
}

/// The `metadata` member, or `None` when there would be nothing in it.
///
/// An empty object is legal CityJSON but says less than no object at all, and
/// a document with neither coordinates nor a CRS — an empty `CityModel` — has
/// nothing to put there.
fn document_metadata(
    extent: Option<Extent>,
    crs: Option<&NormalizedCrs>,
    report: &mut ParseReport,
) -> Option<Metadata> {
    let reference_system = crs.and_then(|crs| reference_system(crs, report));
    if extent.is_none() && reference_system.is_none() {
        return None;
    }
    Some(Metadata {
        geographical_extent: extent.map(Extent::geographical_extent),
        identifier: None,
        point_of_contact: None,
        reference_date: None,
        reference_system,
        title: None,
        other: HashMap::new(),
    })
}

/// Turn a normalised `srsName` into cjseq's reference system.
///
/// [`crate::crs::normalize_srs`] only ever emits the OGC URL form, so the
/// parse cannot fail today; it is still a `Result`, and losing the CRS with a
/// warning beats panicking if that ever stops being true.
fn reference_system(crs: &NormalizedCrs, report: &mut ParseReport) -> Option<ReferenceSystem> {
    match ReferenceSystem::from_url(&crs.reference_system) {
        Ok(reference_system) => Some(reference_system),
        Err(err) => {
            report.warnings.push(format!(
                "reference system {:?} is not a CityJSON CRS URL ({err}); referenceSystem omitted",
                crs.reference_system
            ));
            None
        }
    }
}

/// One top-level object as a CityJSONFeature, with its own vertex table.
///
/// An object with neither geometry nor attributes still becomes a feature:
/// it exists in the source, and a City Object with only a `type` is valid
/// CityJSON.
fn feature(object: &IntermediateObject, quantizer: &Quantizer) -> cjseq::CityJSONFeature {
    let mut vertices = VertexTable::default();
    let mut city_object = cjseq::CityObject::new(object.co_type.clone());

    let geometry: Vec<cjseq::Geometry> = object
        .geometries
        .iter()
        .map(|geometry| convert_geometry(geometry, quantizer, &mut vertices))
        .collect();
    // `geometry: []` and no `geometry` member differ, and the second is what
    // an object without geometry means.
    city_object.geometry = (!geometry.is_empty()).then_some(geometry);
    city_object.attributes =
        (!object.attributes.is_empty()).then(|| Value::Object(object.attributes.clone()));

    let mut city_objects = HashMap::with_capacity(1);
    city_objects.insert(object.id.clone(), city_object);

    cjseq::CityJSONFeature {
        thetype: cjseq::CityJSONFeatureType::CityJSONFeature,
        id: object.id.clone(),
        city_objects,
        vertices: vertices.vertices,
        appearance: None,
    }
}

/// One intermediate geometry as a CityJSON geometry.
///
/// The mapping is by nesting depth and nothing else: a CityJSON
/// `MultiSurface` is a list of surfaces, a `Solid` a list of shells of
/// surfaces, a `MultiSolid` a list of those. The GML side is already
/// flattened to exactly those shapes.
fn convert_geometry(
    geometry: &IntermediateGeometry,
    quantizer: &Quantizer,
    vertices: &mut VertexTable,
) -> cjseq::Geometry {
    let lod = Some(geometry.lod.clone());
    // Semantics, materials and textures are later tasks; nothing here can
    // fill them in yet.
    let common = cjseq::GeometryCommon::default();
    let surfaces = |polygons: &[Polygon3], vertices: &mut VertexTable| -> Vec<cjseq::Surface> {
        polygons
            .iter()
            .map(|polygon| convert_polygon(polygon, quantizer, vertices))
            .collect()
    };
    let shells = |shells: &[Vec<Polygon3>], vertices: &mut VertexTable| -> Vec<cjseq::Shell> {
        shells
            .iter()
            .map(|shell| surfaces(shell, vertices))
            .collect()
    };

    match &geometry.geometry {
        GmlGeometry::MultiSurface(polygons) => cjseq::Geometry::MultiSurface {
            lod,
            boundaries: surfaces(polygons, vertices),
            common,
        },
        GmlGeometry::CompositeSurface(polygons) => cjseq::Geometry::CompositeSurface {
            lod,
            boundaries: surfaces(polygons, vertices),
            common,
        },
        GmlGeometry::Solid(solid) => cjseq::Geometry::Solid {
            lod,
            boundaries: shells(solid, vertices),
            common,
        },
        GmlGeometry::MultiSolid(solids) => cjseq::Geometry::MultiSolid {
            lod,
            boundaries: solids.iter().map(|solid| shells(solid, vertices)).collect(),
            common,
        },
        GmlGeometry::CompositeSolid(solids) => cjseq::Geometry::CompositeSolid {
            lod,
            boundaries: solids.iter().map(|solid| shells(solid, vertices)).collect(),
            common,
        },
    }
}

/// A polygon as a CityJSON surface: exterior ring first, then the interior
/// ones, each a list of indices into the feature's vertex table.
fn convert_polygon(
    polygon: &Polygon3,
    quantizer: &Quantizer,
    vertices: &mut VertexTable,
) -> cjseq::Surface {
    polygon
        .rings
        .iter()
        .map(|ring| {
            ring.pts
                .iter()
                .map(|pt| vertices.index_of(quantizer.quantize(pt)))
                .collect()
        })
        .collect()
}

/// The CityJSON `transform`, in the one direction this crate needs it.
struct Quantizer {
    scale: [f64; 3],
    translate: [f64; 3],
}

impl Quantizer {
    /// A real-world point as the integer triple CityJSON stores.
    ///
    /// Rounding, not truncation: truncating biases every coordinate towards
    /// the translate corner by up to one scale unit.
    fn quantize(&self, pt: &[f64; 3]) -> [i64; 3] {
        std::array::from_fn(|i| ((pt[i] - self.translate[i]) / self.scale[i]).round() as i64)
    }
}

/// A feature's vertex array, and the index of each vertex already in it.
///
/// First-seen order, so the array reads in document order and two runs over
/// the same input agree.
#[derive(Default)]
struct VertexTable {
    index: HashMap<[i64; 3], usize>,
    vertices: Vec<Vec<i64>>,
}

impl VertexTable {
    /// The index of `vertex`, appending it if this is its first appearance.
    fn index_of(&mut self, vertex: [i64; 3]) -> usize {
        match self.index.entry(vertex) {
            Entry::Occupied(seen) => *seen.get(),
            Entry::Vacant(unseen) => {
                self.vertices.push(vertex.to_vec());
                *unseen.insert(self.vertices.len() - 1)
            }
        }
    }
}

/// The component-wise bounding box of a set of real-world coordinates.
#[derive(Debug, Clone, Copy)]
struct Extent {
    min: [f64; 3],
    max: [f64; 3],
}

impl Extent {
    /// The extent of every coordinate in `objects`, or `None` when they hold
    /// no coordinate at all.
    fn of(objects: &[IntermediateObject]) -> Option<Self> {
        let mut extent: Option<Self> = None;
        visit_points(objects, &mut |pt| match &mut extent {
            Some(extent) => extent.grow(pt),
            none => *none = Some(Self { min: *pt, max: *pt }),
        });
        extent
    }

    fn grow(&mut self, pt: &[f64; 3]) {
        for ((min, max), coord) in self.min.iter_mut().zip(&mut self.max).zip(pt) {
            *min = min.min(*coord);
            *max = max.max(*coord);
        }
    }

    /// CityJSON's `geographicalExtent`: `[min x, min y, min z, max x, max y,
    /// max z]`, in real-world coordinates.
    fn geographical_extent(self) -> GeographicalExtent {
        [
            self.min[0],
            self.min[1],
            self.min[2],
            self.max[0],
            self.max[1],
            self.max[2],
        ]
    }
}

/// Call `f` on every coordinate of every object, its children included.
fn visit_points(objects: &[IntermediateObject], f: &mut impl FnMut(&[f64; 3])) {
    for object in objects {
        for geometry in &object.geometries {
            for polygon in polygons(&geometry.geometry) {
                for ring in &polygon.rings {
                    ring.pts.iter().for_each(&mut *f);
                }
            }
        }
        visit_points(&object.children, f);
    }
}

/// Reorder every coordinate from latitude, longitude to CityJSON's x, y.
fn swap_axes(objects: &mut [IntermediateObject]) {
    for object in objects.iter_mut() {
        for geometry in &mut object.geometries {
            for polygon in polygons_mut(&mut geometry.geometry) {
                for ring in &mut polygon.rings {
                    for pt in &mut ring.pts {
                        pt.swap(0, 1);
                    }
                }
            }
        }
        swap_axes(&mut object.children);
    }
}

/// Every polygon of a geometry, whatever its nesting.
fn polygons(geometry: &GmlGeometry) -> Vec<&Polygon3> {
    match geometry {
        GmlGeometry::MultiSurface(polygons) | GmlGeometry::CompositeSurface(polygons) => {
            polygons.iter().collect()
        }
        GmlGeometry::Solid(shells) => shells.iter().flatten().collect(),
        GmlGeometry::MultiSolid(solids) | GmlGeometry::CompositeSolid(solids) => {
            solids.iter().flatten().flatten().collect()
        }
    }
}

/// [`polygons`], mutably.
fn polygons_mut(geometry: &mut GmlGeometry) -> Vec<&mut Polygon3> {
    match geometry {
        GmlGeometry::MultiSurface(polygons) | GmlGeometry::CompositeSurface(polygons) => {
            polygons.iter_mut().collect()
        }
        GmlGeometry::Solid(shells) => shells.iter_mut().flatten().collect(),
        GmlGeometry::MultiSolid(solids) | GmlGeometry::CompositeSolid(solids) => {
            solids.iter_mut().flatten().flatten().collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gml::Ring;

    fn polygon(pts: Vec<[f64; 3]>) -> Polygon3 {
        Polygon3 {
            gml_id: None,
            rings: vec![Ring { gml_id: None, pts }],
            sem_idx: None,
        }
    }

    fn object(geometry: GmlGeometry) -> IntermediateObject {
        let mut object = IntermediateObject::new("o1".to_string(), cjseq::CityObjectType::Building);
        object.geometries.push(IntermediateGeometry {
            lod: "2".to_string(),
            geometry,
            surfaces: Vec::new(),
        });
        object
    }

    fn convert_one(
        objects: Vec<IntermediateObject>,
        crs: Option<NormalizedCrs>,
    ) -> CityGmlDocument {
        let mut report = ParseReport::default();
        convert(objects, crs, &ParseOptions::default(), &mut report)
    }

    /// Half a scale unit either side of a coordinate must land on the same
    /// integer it does; truncation would send the lower one down.
    #[test]
    fn quantisation_rounds_rather_than_truncates() {
        let quantizer = Quantizer {
            scale: [0.001, 0.001, 0.001],
            translate: [0.0, 0.0, 0.0],
        };
        assert_eq!(
            quantizer.quantize(&[1.0006, 1.0004, -1.0006]),
            [1001, 1000, -1001]
        );
    }

    /// The same point twice is one vertex, and the array is in first-seen
    /// order.
    #[test]
    fn repeated_points_share_one_vertex() {
        let doc = convert_one(
            vec![object(GmlGeometry::MultiSurface(vec![
                polygon(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]),
                polygon(vec![[1.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]]),
            ]))],
            None,
        );
        assert_eq!(doc.features[0].vertices.len(), 3);
        let cjseq::Geometry::MultiSurface { boundaries, .. } = &doc.features[0].city_objects["o1"]
            .geometry
            .as_ref()
            .unwrap()[0]
        else {
            panic!("expected a MultiSurface");
        };
        assert_eq!(boundaries, &vec![vec![vec![0, 1, 2]], vec![vec![2, 1, 0]]]);
    }

    /// Each feature indexes its own vertex array, so the second feature
    /// starts again at 0 even for the very same point.
    #[test]
    fn each_feature_has_its_own_vertex_table() {
        let face = || {
            GmlGeometry::MultiSurface(vec![polygon(vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
            ])])
        };
        let doc = convert_one(vec![object(face()), object(face())], None);
        assert_eq!(doc.features.len(), 2);
        for feature in &doc.features {
            assert_eq!(
                feature.vertices,
                vec![vec![0, 0, 0], vec![1000, 0, 0], vec![1000, 1000, 0]]
            );
        }
    }

    /// `translate` is the minimum over the *whole* document, not per feature,
    /// or the features would not share a coordinate system.
    #[test]
    fn translate_is_the_minimum_over_every_object() {
        let doc = convert_one(
            vec![
                object(GmlGeometry::MultiSurface(vec![polygon(vec![
                    [10.0, 20.0, 30.0],
                    [11.0, 20.0, 30.0],
                    [11.0, 21.0, 30.0],
                ])])),
                object(GmlGeometry::MultiSurface(vec![polygon(vec![
                    [5.0, 25.0, 35.0],
                    [6.0, 25.0, 35.0],
                    [6.0, 26.0, 35.0],
                ])])),
            ],
            None,
        );
        assert_eq!(doc.metadata.transform.translate, vec![5.0, 20.0, 30.0]);
        assert_eq!(
            doc.metadata.metadata.unwrap().geographical_extent,
            Some([5.0, 20.0, 30.0, 11.0, 26.0, 35.0])
        );
    }

    /// A latitude-first CRS is reordered before anything measures it, so the
    /// extent and the transform are in x, y order too.
    #[test]
    fn a_lat_lon_crs_swaps_every_coordinate_first() {
        let crs = NormalizedCrs {
            reference_system: "https://www.opengis.net/def/crs/EPSG/0/4326".to_string(),
            swap_axes: true,
        };
        let doc = convert_one(
            vec![object(GmlGeometry::MultiSurface(vec![polygon(vec![
                [52.0, 4.0, 0.0],
                [52.0, 5.0, 0.0],
                [53.0, 5.0, 0.0],
            ])]))],
            Some(crs),
        );
        assert_eq!(doc.metadata.transform.translate, vec![4.0, 52.0, 0.0]);
        assert_eq!(
            doc.metadata.metadata.unwrap().geographical_extent,
            Some([4.0, 52.0, 0.0, 5.0, 53.0, 0.0])
        );
    }

    /// An object with nothing in it is still a feature — but its City Object
    /// carries no `geometry` member rather than an empty one.
    #[test]
    fn an_object_without_geometry_is_still_a_feature() {
        let object = IntermediateObject::new("empty".to_string(), cjseq::CityObjectType::Building);
        let doc = convert_one(vec![object], None);
        assert_eq!(doc.features.len(), 1);
        let city_object = &doc.features[0].city_objects["empty"];
        assert!(city_object.geometry.is_none());
        assert!(city_object.attributes.is_none());
        assert!(doc.features[0].vertices.is_empty());
        // No coordinates anywhere, so nothing fixes a translate.
        assert_eq!(doc.metadata.transform.translate, vec![0.0, 0.0, 0.0]);
    }

    /// A document with neither coordinates nor a CRS has an empty metadata
    /// object to write, so it writes none.
    #[test]
    fn a_document_with_nothing_to_describe_has_no_metadata() {
        let doc = convert_one(Vec::new(), None);
        assert!(doc.metadata.metadata.is_none());
        assert!(doc.features.is_empty());
    }

    /// A CRS alone is enough to earn a metadata object.
    #[test]
    fn a_crs_without_coordinates_still_writes_a_reference_system() {
        let crs = NormalizedCrs {
            reference_system: "https://www.opengis.net/def/crs/EPSG/0/7415".to_string(),
            swap_axes: false,
        };
        let doc = convert_one(Vec::new(), Some(crs));
        let metadata = doc.metadata.metadata.unwrap();
        assert!(metadata.geographical_extent.is_none());
        assert_eq!(
            metadata.reference_system.unwrap().to_url(),
            "https://www.opengis.net/def/crs/EPSG/0/7415"
        );
    }

    /// Each geometry kind keeps its own boundary depth.
    #[test]
    fn every_geometry_kind_maps_to_its_citygml_depth() {
        let face = || polygon(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]);
        for (geometry, depth) in [
            (GmlGeometry::MultiSurface(vec![face()]), 3),
            (GmlGeometry::CompositeSurface(vec![face()]), 3),
            (GmlGeometry::Solid(vec![vec![face()]]), 4),
            (GmlGeometry::MultiSolid(vec![vec![vec![face()]]]), 5),
            (GmlGeometry::CompositeSolid(vec![vec![vec![face()]]]), 5),
        ] {
            let doc = convert_one(vec![object(geometry)], None);
            let geometry = &doc.features[0].city_objects["o1"]
                .geometry
                .as_ref()
                .unwrap()[0];
            assert_eq!(geometry.lod(), Some("2"));
            let boundaries = serde_json::to_value(geometry).unwrap()["boundaries"].clone();
            assert_eq!(nesting_depth(&boundaries), depth, "{geometry:?}");
        }
    }

    /// Array levels between the outermost array and a vertex index.
    fn nesting_depth(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::Array(items) => {
                1 + items.iter().map(nesting_depth).max().unwrap_or(0)
            }
            _ => 0,
        }
    }
}
