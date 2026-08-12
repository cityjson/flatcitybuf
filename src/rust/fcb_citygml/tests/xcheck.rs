//! Cross-check against citygml-tools: the same CityGML, converted by the
//! reference implementation, must describe the same city.
//!
//! The fixtures in `tests/fixtures` are hand-written, and a hand-written
//! expectation can only be as complete as the hand that wrote it. These
//! samples are the other half: real files from real cities, converted by
//! citygml4j's citygml-tools — the implementation the CityGML/CityJSON
//! community treats as the reference — and committed beside the input.
//! `tests/xcheck/README.md` records where each came from and how to make it
//! again.
//!
//! The comparison is deliberately *structural* and not textual. Two correct
//! converters do not agree byte for byte, and demanding that they did would
//! only teach this suite to imitate citygml-tools' arbitrary choices. What is
//! compared is what a CityGML document actually says: which objects exist,
//! what each is, how they nest, what attributes they carry, how many
//! geometries at which levels of detail, of what type, with how many polygons
//! and shells, what each polygon *is* semantically, where the corners are in
//! the real world, and which materials and texture images the appearance
//! names.
//!
//! Everywhere the two implementations legitimately differ there is a named
//! constant below with the reason. Nothing is loosened wholesale: an
//! unexplained difference fails.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use fcb_citygml::{parse_citygml, CityGmlDocument, ParseOptions};
use serde_json::{Map, Value};

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

/// KIT's FZK-Haus, LoD 2, CityGML 2.0: one building, a `bldg:lod2Solid` whose
/// faces are the `bldg:boundedBy` surfaces, generic attributes of every kind,
/// and an `xAL` address.
#[test]
fn fzk_haus_lod2() {
    assert_sample("fzk-haus-lod2");
}

/// KIT's LoD 3 railway scene, CityGML 2.0, cut down to eight members: a
/// building and a tunnel with installations nested in them, two bridges, a
/// tree carrying an implicit geometry, city furniture, a generic object and a
/// railway — with X3D materials and parameterised textures over all of it.
#[test]
fn railway_lod3() {
    assert_sample("railway-lod3");
}

/// A Den Haag tile, CityGML **1.0**, cut down to 25 buildings: LoD 2 solids
/// assembled by `xlink` from `bldg:boundedBy` surfaces, building parts, and an
/// `app:appearance` on each object rather than on the `CityModel`. The 1.0
/// namespaces are part of what this checks.
#[test]
fn denhaag_lod2() {
    assert_sample("denhaag-lod2");
}

// ---------------------------------------------------------------------------
// Where the two implementations legitimately differ
// ---------------------------------------------------------------------------

/// Attributes citygml-tools writes that this converter does not, each because
/// the property is outside the mapping this converter defines.
///
/// The mapping is a closed list — `gml:name` plus the module properties and
/// the `gen:` generic attributes — so a property that is not on it is not
/// silently dropped so much as never claimed. Adding any of these would be a
/// feature, not a bug fix, which is why they are listed rather than fixed.
const REFERENCE_ONLY_ATTRIBUTES: &[&str] = &[
    // `gml:description`. `gml:name` is mapped; the description is not.
    "description",
    // `core:creationDate` / `core:terminationDate`: a CityGML feature's
    // lifespan, which CityJSON has no member for either.
    "creationDate",
    "terminationDate",
    // `core:relativeToTerrain` / `core:relativeToWater`.
    "relativeToTerrain",
    "relativeToWater",
    // citygml-tools renames CityGML 2.0's `bldg:yearOfConstruction` to
    // CityGML 3.0's `dateOfConstruction` and pads the year out to a full
    // date ("2020" becomes "2020-01-01"). This converter keeps the 2.0
    // spelling and the 2.0 value — see `OURS_ONLY_ATTRIBUTES`.
    "dateOfConstruction",
    "dateOfDemolition",
    // Not in the source at all: citygml-tools reads CityGML 2.0 through
    // citygml4j's CityGML 3.0 model, where the choice between
    // `bldg:outerBuildingInstallation` and `bldg:interiorBuildingInstallation`
    // has become an attribute of the installation, so every outer one it
    // writes carries `"relationToConstruction": "outside"`. It says nothing
    // the property it was derived from did not.
    "relationToConstruction",
];

/// Attributes this converter writes that citygml-tools does not, under that
/// name. See `dateOfConstruction` above.
const OURS_ONLY_ATTRIBUTES: &[&str] = &["yearOfConstruction", "yearOfDemolition"];

/// City Object members citygml-tools writes that this converter does not.
///
/// `address` is the `xAL` address of a building. Parsing `xAL` is outside this
/// converter's scope, so a CityGML address reaches CityJSON in neither the
/// `address` member nor the attributes.
///
/// `geographicalExtent` is a per-object bounding box, which citygml-tools
/// writes only when asked to; it is listed so that turning the option on while
/// regenerating the corpus does not fail the suite.
const REFERENCE_ONLY_MEMBERS: &[&str] = &["address", "geographicalExtent"];

/// City Object members this converter writes that citygml-tools does not.
///
/// `children_roles` is CityJSON's array of `grp:groupMember` roles, which
/// citygml-tools does not carry over.
const OURS_ONLY_MEMBERS: &[&str] = &["children_roles"];

/// The members this comparison reads itself; anything else is checked against
/// the two lists above.
const COMPARED_MEMBERS: &[&str] = &["type", "attributes", "geometry", "parents", "children"];

/// How far apart two implementations' idea of the same corner may be.
///
/// Both sides quantise to millimetres — this converter through the CityJSON
/// `transform`, citygml-tools by rounding to `--vertex-precision` decimals —
/// so each coordinate may move by half a step on either side. Two steps of the
/// coarser scale is comfortably above that and far below anything that would
/// hide a real displacement.
fn vertex_tolerance(ours: &Transform, reference: &Transform) -> f64 {
    let coarsest = ours
        .scale
        .iter()
        .chain(reference.scale.iter())
        .fold(0.0_f64, |a, b| a.max(*b));
    2.0 * coarsest
}

/// How many of an object's vertices to check. Whole scenes have hundreds of
/// thousands of them and every one is quantised the same way, so a sample
/// spread across the object says what checking all of them would.
const VERTEX_SAMPLE: usize = 500;

// ---------------------------------------------------------------------------
// One sample
// ---------------------------------------------------------------------------

/// Parse `tests/xcheck/<name>.gml` and hold it against
/// `tests/xcheck/<name>.citygml-tools.city.json`.
fn assert_sample(name: &str) {
    let dir = corpus_dir();
    let gml = File::open(dir.join(format!("{name}.gml")))
        .unwrap_or_else(|err| panic!("opening {name}.gml: {err}"));
    let (ours, _report) = parse_citygml(BufReader::new(gml), &ParseOptions::default())
        .unwrap_or_else(|err| panic!("converting {name}.gml: {err}"));

    let path = dir.join(format!("{name}.citygml-tools.city.json"));
    let reference: Value = serde_json::from_reader(BufReader::new(
        File::open(&path).unwrap_or_else(|err| panic!("opening {}: {err}", path.display())),
    ))
    .unwrap_or_else(|err| panic!("parsing {}: {err}", path.display()));

    assert_structural_match(&ours, &reference);
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/xcheck")
}

/// Fail unless `ours` and `reference` describe the same city.
///
/// `reference` is one whole CityJSON document — citygml-tools' `to-cityjson`
/// output, not a sequence: every City Object in one map, one shared vertex
/// list, one transform — while `ours` is CityJSONSeq, a metadata line and a
/// feature per top-level object with vertices of its own. The two shapes are
/// reconciled here rather than compared.
pub fn assert_structural_match(ours: &CityGmlDocument, reference: &Value) {
    let our_transform = Transform::of(
        &ours.metadata.transform.scale,
        &ours.metadata.transform.translate,
    );
    let ref_transform = Transform::from_json(&reference["transform"]);
    let tolerance = vertex_tolerance(&our_transform, &ref_transform);

    let our_objects = our_city_objects(ours);
    let ref_objects = ref_city_objects(reference);
    let ref_vertices = vertex_table(&reference["vertices"], &ref_transform);

    let pairing = Pairing::of(&our_objects, &ref_objects, &ref_vertices);

    for (our_id, ref_id) in &pairing.ours_to_reference {
        let ours = &our_objects[our_id];
        let theirs = &ref_objects[ref_id];
        let at = format!("object {our_id:?}");

        compare_members(&ours.object, theirs, &at);
        compare_type(&ours.object, theirs, &at);
        compare_attributes(&ours.object, theirs, &at);
        compare_hierarchy(&ours.object, theirs, &pairing, &at);
        compare_geometries(&ours.object, theirs, &at);
        compare_vertices(ours, theirs, &ref_vertices, tolerance, &at);
    }

    compare_appearance(ours, reference);
}

// ---------------------------------------------------------------------------
// Reconciling CityJSONSeq with one CityJSON document
// ---------------------------------------------------------------------------

/// One of this converter's City Objects, with the feature-local vertex table
/// its boundaries index into.
struct OurObject {
    object: Value,
    /// The feature's vertices, already dequantised.
    vertices: Vec<[f64; 3]>,
}

/// Every City Object of every feature, by id.
fn our_city_objects(document: &CityGmlDocument) -> BTreeMap<String, OurObject> {
    let transform = Transform::of(
        &document.metadata.transform.scale,
        &document.metadata.transform.translate,
    );
    let mut objects = BTreeMap::new();
    for feature in &document.features {
        let feature = serde_json::to_value(feature).expect("a feature serialises");
        let vertices = vertex_table(&feature["vertices"], &transform);
        let city_objects = feature["CityObjects"]
            .as_object()
            .expect("a feature has CityObjects");
        for (id, object) in city_objects {
            let previous = objects.insert(
                id.clone(),
                OurObject {
                    object: object.clone(),
                    vertices: vertices.clone(),
                },
            );
            assert!(previous.is_none(), "id {id:?} appears in two features");
        }
    }
    objects
}

/// citygml-tools' City Objects, by id.
fn ref_city_objects(reference: &Value) -> BTreeMap<String, Value> {
    reference["CityObjects"]
        .as_object()
        .expect("the reference has CityObjects")
        .iter()
        .map(|(id, object)| (id.clone(), object.clone()))
        .collect()
}

/// A CityJSON `transform`.
struct Transform {
    scale: [f64; 3],
    translate: [f64; 3],
}

impl Transform {
    fn of(scale: &[f64], translate: &[f64]) -> Self {
        Self {
            scale: [scale[0], scale[1], scale[2]],
            translate: [translate[0], translate[1], translate[2]],
        }
    }

    fn from_json(transform: &Value) -> Self {
        let axis = |key: &str| -> [f64; 3] {
            let values = transform[key].as_array().expect("transform is an object");
            let at = |i: usize| values[i].as_f64().expect("transform holds numbers");
            [at(0), at(1), at(2)]
        };
        Self {
            scale: axis("scale"),
            translate: axis("translate"),
        }
    }

    fn dequantize(&self, vertex: &Value) -> [f64; 3] {
        let values = vertex.as_array().expect("a vertex is an array");
        let at = |i: usize| values[i].as_f64().expect("a vertex holds numbers");
        [
            at(0) * self.scale[0] + self.translate[0],
            at(1) * self.scale[1] + self.translate[1],
            at(2) * self.scale[2] + self.translate[2],
        ]
    }
}

/// A whole `vertices` array, in real-world coordinates.
fn vertex_table(vertices: &Value, transform: &Transform) -> Vec<[f64; 3]> {
    vertices
        .as_array()
        .expect("vertices is an array")
        .iter()
        .map(|vertex| transform.dequantize(vertex))
        .collect()
}

// ---------------------------------------------------------------------------
// Pairing the objects
// ---------------------------------------------------------------------------

/// Which of this converter's objects is which of citygml-tools'.
///
/// Almost always the id: both carry the source's `gml:id` through. But
/// CityGML lets a nested object — a `bldg:BuildingInstallation`, a
/// `tun:TunnelInstallation` — be written without one, and then each
/// implementation invents a name of its own: this converter names it after its
/// parent (`{parent}-inst-1`), citygml-tools mints a UUID. Neither is more
/// right than the other, so an unmatched object is paired by what it *is* —
/// its type and the shape of its geometry — rather than by what it is called.
struct Pairing {
    ours_to_reference: BTreeMap<String, String>,
}

impl Pairing {
    fn of(
        ours: &BTreeMap<String, OurObject>,
        theirs: &BTreeMap<String, Value>,
        ref_vertices: &[[f64; 3]],
    ) -> Self {
        let mut ours_to_reference = BTreeMap::new();
        let mut our_rest = Vec::new();
        for (id, object) in ours {
            if theirs.contains_key(id) {
                ours_to_reference.insert(id.clone(), id.clone());
            } else {
                our_rest.push((id.clone(), object));
            }
        }

        // Bucket what is left by what it is: same type, same geometries. Two
        // objects of the same kind and size — a tunnel's two identical
        // portals — land in one bucket, and are told apart by where they are.
        let mut their_buckets: BTreeMap<String, Vec<(String, Option<Bounds>)>> = BTreeMap::new();
        for (id, object) in theirs.iter().filter(|(id, _)| !ours.contains_key(*id)) {
            their_buckets
                .entry(anonymous_signature(object))
                .or_default()
                .push((id.clone(), Bounds::of(object, ref_vertices)));
        }
        for (id, object) in &our_rest {
            let signature = anonymous_signature(&object.object);
            let candidates = their_buckets
                .get_mut(&signature)
                .filter(|candidates| !candidates.is_empty());
            let Some(candidates) = candidates else {
                panic!(
                    "object {id:?} has no counterpart in the citygml-tools output, by id or by \
                     shape ({signature})"
                );
            };
            let bounds = Bounds::of(&object.object, &object.vertices);
            let nearest = candidates
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let distance = |candidate: &Option<Bounds>| {
                        Bounds::distance(bounds.as_ref(), candidate.as_ref())
                    };
                    distance(&a.1).total_cmp(&distance(&b.1))
                })
                .map(|(index, _)| index)
                .expect("the bucket is not empty");
            ours_to_reference.insert(id.clone(), candidates.remove(nearest).0);
        }
        let unpaired: Vec<&String> = their_buckets.values().flatten().map(|(id, _)| id).collect();
        assert!(
            unpaired.is_empty(),
            "citygml-tools has objects this converter does not: {unpaired:?}"
        );
        Self { ours_to_reference }
    }
}

/// What an object *is*, for pairing two anonymous ones: its type and the
/// type, LoD and polygon count of each of its geometries.
fn anonymous_signature(object: &Value) -> String {
    let mut signature = format!("type={}", object["type"]);
    for geometry in geometries(object) {
        signature.push_str(&format!(
            " [{} lod={} polygons={}]",
            geometry["type"],
            geometry["lod"],
            polygons(geometry).len()
        ));
    }
    signature
}

/// Where an object is: the corners of its bounding box, in real-world
/// coordinates.
#[derive(Clone, Copy)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

impl Bounds {
    fn of(object: &Value, vertices: &[[f64; 3]]) -> Option<Self> {
        let mut bounds: Option<Self> = None;
        for index in used_vertices(object) {
            let vertex = vertices[index];
            bounds = Some(match bounds {
                None => Self {
                    min: vertex,
                    max: vertex,
                },
                Some(mut bounds) => {
                    for (axis, coordinate) in vertex.iter().enumerate() {
                        bounds.min[axis] = bounds.min[axis].min(*coordinate);
                        bounds.max[axis] = bounds.max[axis].max(*coordinate);
                    }
                    bounds
                }
            });
        }
        bounds
    }

    /// How far apart two boxes are, for choosing between candidates. Two
    /// objects with no geometry at all are as close as they can be.
    fn distance(ours: Option<&Self>, theirs: Option<&Self>) -> f64 {
        match (ours, theirs) {
            (None, None) => 0.0,
            (Some(ours), Some(theirs)) => (0..3)
                .map(|axis| {
                    (ours.min[axis] - theirs.min[axis]).abs()
                        + (ours.max[axis] - theirs.max[axis]).abs()
                })
                .sum(),
            _ => f64::INFINITY,
        }
    }
}

// ---------------------------------------------------------------------------
// The comparisons
// ---------------------------------------------------------------------------

/// Neither side may carry a City Object member the other knows nothing about,
/// except the ones named above.
fn compare_members(ours: &Value, theirs: &Value, at: &str) {
    let members = |object: &Value| -> BTreeSet<String> {
        object
            .as_object()
            .expect("a City Object is an object")
            .keys()
            .filter(|key| !COMPARED_MEMBERS.contains(&key.as_str()))
            .cloned()
            .collect()
    };
    for member in members(theirs).difference(&members(ours)) {
        assert!(
            REFERENCE_ONLY_MEMBERS.contains(&member.as_str()),
            "{at}: citygml-tools writes a {member:?} member and this converter does not"
        );
    }
    for member in members(ours).difference(&members(theirs)) {
        assert!(
            OURS_ONLY_MEMBERS.contains(&member.as_str()),
            "{at}: this converter writes a {member:?} member and citygml-tools does not"
        );
    }
}

fn compare_type(ours: &Value, theirs: &Value, at: &str) {
    assert_eq!(
        ours["type"], theirs["type"],
        "{at}: this converter says {}, citygml-tools says {}",
        ours["type"], theirs["type"]
    );
}

fn compare_attributes(ours: &Value, theirs: &Value, at: &str) {
    let empty = Map::new();
    let attributes = |object: &Value| -> Map<String, Value> {
        object["attributes"].as_object().unwrap_or(&empty).clone()
    };
    let ours = attributes(ours);
    let theirs = attributes(theirs);

    for (key, our_value) in &ours {
        let Some(their_value) = theirs.get(key) else {
            assert!(
                OURS_ONLY_ATTRIBUTES.contains(&key.as_str()),
                "{at}: this converter writes attribute {key:?} and citygml-tools does not"
            );
            continue;
        };
        assert!(
            values_match(our_value, their_value),
            "{at}: attribute {key:?} is {our_value} here and {their_value} in citygml-tools"
        );
    }
    for key in theirs.keys() {
        assert!(
            ours.contains_key(key) || REFERENCE_ONLY_ATTRIBUTES.contains(&key.as_str()),
            "{at}: citygml-tools writes attribute {key:?} and this converter does not"
        );
    }
}

/// Whether two attribute values say the same thing.
///
/// Numbers are compared with a tolerance because both sides parsed the same
/// decimal text into a `f64` and wrote it back out; everything else is
/// compared exactly.
///
/// The one shape difference allowed is a `gen:measureAttribute`: CityGML
/// states a value and a `uom`, and citygml-tools keeps both in an object
/// (`{"value": 120.0, "uom": "m2"}`) where this converter writes the bare
/// number. CityJSON says nothing about how a unit of measure should be
/// carried, so neither is wrong; the number itself must still agree.
fn values_match(ours: &Value, theirs: &Value) -> bool {
    if let (Some(ours), Some(theirs)) = (ours.as_f64(), theirs.as_f64()) {
        let tolerance = 1e-9_f64.max(1e-9 * ours.abs().max(theirs.abs()));
        return (ours - theirs).abs() <= tolerance;
    }
    if let (Some(number), Some(measure)) = (ours.as_f64(), theirs.as_object()) {
        if let Some(value) = measure.get("value").and_then(Value::as_f64) {
            return values_match(&Value::from(number), &Value::from(value));
        }
    }
    if let (Some(ours), Some(theirs)) = (ours.as_array(), theirs.as_array()) {
        return ours.len() == theirs.len()
            && ours.iter().zip(theirs).all(|(a, b)| values_match(a, b));
    }
    ours == theirs
}

/// `parents` and `children` as sets, with this converter's ids translated
/// into citygml-tools' where the two invented different ones.
fn compare_hierarchy(ours: &Value, theirs: &Value, pairing: &Pairing, at: &str) {
    for member in ["parents", "children"] {
        let ids = |object: &Value| -> BTreeSet<String> {
            object[member]
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .map(|id| id.as_str().expect("an id is a string").to_string())
                        .collect()
                })
                .unwrap_or_default()
        };
        let translated: BTreeSet<String> = ids(ours)
            .into_iter()
            .map(|id| pairing.ours_to_reference.get(&id).cloned().unwrap_or(id))
            .collect();
        assert_eq!(
            translated,
            ids(theirs),
            "{at}: {member} differ (this converter's ids shown as citygml-tools')"
        );
    }
}

/// The same geometries, level of detail by level of detail.
///
/// Grouped by LoD rather than compared in order: a geometry's place in the
/// array means nothing, and the two implementations read the `lodX…`
/// properties of an object in a different order.
fn compare_geometries(ours: &Value, theirs: &Value, at: &str) {
    let by_lod = |object: &Value| -> BTreeMap<String, Vec<Value>> {
        let mut by_lod: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for geometry in geometries(object) {
            let lod = geometry["lod"].as_str().unwrap_or("").to_string();
            by_lod.entry(lod).or_default().push(geometry.clone());
        }
        by_lod
    };
    let ours = by_lod(ours);
    let theirs = by_lod(theirs);
    assert_eq!(
        ours.keys().collect::<Vec<_>>(),
        theirs.keys().collect::<Vec<_>>(),
        "{at}: the levels of detail differ"
    );
    for (lod, our_geometries) in &ours {
        let their_geometries = &theirs[lod];
        assert_eq!(
            our_geometries.len(),
            their_geometries.len(),
            "{at}: {} geometries at LoD {lod} here, {} in citygml-tools",
            our_geometries.len(),
            their_geometries.len()
        );
        for (index, (ours, theirs)) in our_geometries.iter().zip(their_geometries).enumerate() {
            let at = &format!("{at}, LoD {lod} geometry {index}");
            compare_geometry(ours, theirs, at);
        }
    }
}

fn compare_geometry(ours: &Value, theirs: &Value, at: &str) {
    assert_eq!(
        ours["type"], theirs["type"],
        "{at}: this converter says {}, citygml-tools says {}",
        ours["type"], theirs["type"]
    );
    assert_eq!(
        polygons(ours).len(),
        polygons(theirs).len(),
        "{at}: polygon counts differ"
    );
    assert_eq!(
        shell_count(ours),
        shell_count(theirs),
        "{at}: shell counts differ"
    );
    compare_semantics(ours, theirs, at);
}

/// The same surface types, over the same polygons.
///
/// A geometry may have semantics on one side and not the other only if
/// neither side has any: a semantic surface is read from the source, not
/// invented. Where both have them, the check is twofold — the same *multiset*
/// of surface types, which catches a lost or duplicated surface however the
/// two implementations ordered their `surfaces` arrays, and then the type
/// assigned to each polygon in turn, which catches a surface attached to the
/// wrong face.
fn compare_semantics(ours: &Value, theirs: &Value, at: &str) {
    let (our_semantics, their_semantics) = (&ours["semantics"], &theirs["semantics"]);
    assert_eq!(
        our_semantics.is_null(),
        their_semantics.is_null(),
        "{at}: one side has semantics and the other does not \
         (this converter: {}, citygml-tools: {})",
        !our_semantics.is_null(),
        !their_semantics.is_null()
    );
    if our_semantics.is_null() {
        return;
    }

    let our_types = surface_types(our_semantics);
    let their_types = surface_types(their_semantics);
    assert_eq!(
        counted(&our_types),
        counted(&their_types),
        "{at}: the semantic surfaces differ"
    );

    let ours = assigned_types(ours, &our_types);
    let theirs = assigned_types(theirs, &their_types);
    assert_eq!(
        ours, theirs,
        "{at}: the semantics of the individual polygons differ"
    );
}

/// The `type` of each entry of a `semantics.surfaces` array, in index order.
fn surface_types(semantics: &Value) -> Vec<String> {
    semantics["surfaces"]
        .as_array()
        .expect("semantics has surfaces")
        .iter()
        .map(|surface| {
            surface["type"]
                .as_str()
                .expect("a semantic surface has a type")
                .to_string()
        })
        .collect()
}

/// The surface type of each polygon of a geometry, in the order the polygons
/// are written, with `None` where the polygon has no semantics.
fn assigned_types(geometry: &Value, types: &[String]) -> Vec<Option<String>> {
    collect_at_depth(&geometry["semantics"]["values"], polygon_depth(geometry))
        .into_iter()
        .map(|value| {
            value
                .as_u64()
                .map(|index| types[index as usize].clone())
                .or_else(|| {
                    assert!(value.is_null(), "a semantics value is an index or null");
                    None
                })
        })
        .collect()
}

fn counted(types: &[String]) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for stype in types {
        *counts.entry(stype.as_str()).or_insert(0) += 1;
    }
    counts
}

/// Every dequantised vertex of ours must be a vertex of citygml-tools'.
///
/// One-sided on purpose. citygml-tools may hold vertices this converter does
/// not — its vertex table is the whole document's, and this one's is the
/// feature's — but a corner of ours that is nowhere in theirs is a coordinate
/// this converter got wrong.
fn compare_vertices(
    ours: &OurObject,
    theirs: &Value,
    ref_vertices: &[[f64; 3]],
    tolerance: f64,
    at: &str,
) {
    let our_indices = used_vertices(&ours.object);
    let theirs: Vec<[f64; 3]> = used_vertices(theirs)
        .iter()
        .map(|index| ref_vertices[*index])
        .collect();
    let grid = VertexGrid::of(&theirs, tolerance);

    let stride = (our_indices.len() / VERTEX_SAMPLE).max(1);
    for index in our_indices.iter().step_by(stride) {
        let vertex = ours.vertices[*index];
        assert!(
            grid.holds(vertex),
            "{at}: vertex {vertex:?} is nowhere within {tolerance} of a citygml-tools vertex \
             of the same object"
        );
    }
}

/// The reference's vertices in buckets a tolerance wide, so that finding the
/// counterpart of a vertex costs the 27 buckets around it rather than the
/// whole table.
struct VertexGrid {
    tolerance: f64,
    cells: HashMap<[i64; 3], Vec<[f64; 3]>>,
}

impl VertexGrid {
    fn of(vertices: &[[f64; 3]], tolerance: f64) -> Self {
        let mut grid = Self {
            tolerance,
            cells: HashMap::new(),
        };
        for vertex in vertices {
            grid.cells.entry(grid_cell(*vertex, tolerance)).or_default();
            grid.cells
                .get_mut(&grid_cell(*vertex, tolerance))
                .expect("just inserted")
                .push(*vertex);
        }
        grid
    }

    fn holds(&self, vertex: [f64; 3]) -> bool {
        let [x, y, z] = grid_cell(vertex, self.tolerance);
        let squared = self.tolerance * self.tolerance;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(candidates) = self.cells.get(&[x + dx, y + dy, z + dz]) else {
                        continue;
                    };
                    if candidates
                        .iter()
                        .any(|candidate| distance_squared(vertex, *candidate) <= squared)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn grid_cell(vertex: [f64; 3], tolerance: f64) -> [i64; 3] {
    [
        (vertex[0] / tolerance).floor() as i64,
        (vertex[1] / tolerance).floor() as i64,
        (vertex[2] / tolerance).floor() as i64,
    ]
}

fn distance_squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

// ---------------------------------------------------------------------------
// Appearance
// ---------------------------------------------------------------------------

/// The same palette, described the same way.
///
/// Two differences are expected and neither is a loss.
///
/// *Names.* citygml-tools names a material after the `gml:id` of the
/// `app:X3DMaterial` it came from; this converter, per its own specification,
/// names it `material-{n}` unless the source gave it a `gml:name`. CityJSON
/// requires a name and says nothing about what it should be, so the names are
/// left out of the comparison and the definitions are compared instead.
///
/// *Defaults, and how many entries.* citygml-tools writes the CityGML schema
/// default for every property the source left out (`--material-defaults`,
/// on by default) and collapses materials that end up identical into one
/// palette entry; this converter writes what the source stated and one entry
/// per `app:X3DMaterial`. So each of this converter's *distinct* definitions
/// must be a subset of one of citygml-tools', and the two must have the same
/// number of distinct definitions — which is the thing that matters: the same
/// palette, however it is spelled and however often it is repeated.
fn compare_appearance(ours: &CityGmlDocument, reference: &Value) {
    let their_materials = reference_appearance(reference, "materials");
    let their_textures = reference_appearance(reference, "textures");
    let mut our_materials = BTreeSet::new();
    let mut our_textures = BTreeSet::new();
    for feature in &ours.features {
        let feature = serde_json::to_value(feature).expect("a feature serialises");
        our_materials.extend(definitions(&feature["appearance"]["materials"]));
        our_textures.extend(definitions(&feature["appearance"]["textures"]));
    }

    compare_surface_data(&our_materials, &their_materials, "material");
    compare_surface_data(&our_textures, &their_textures, "texture");

    let images = |definitions: &BTreeSet<String>| -> BTreeSet<String> {
        definitions
            .iter()
            .filter_map(|definition| {
                let definition: Value = serde_json::from_str(definition).expect("round-trips");
                definition["image"].as_str().map(str::to_string)
            })
            .collect()
    };
    assert_eq!(
        images(&our_textures),
        images(&their_textures),
        "the texture images differ"
    );
}

/// The reference's `appearance.<kind>`, as definitions without their names.
fn reference_appearance(reference: &Value, kind: &str) -> BTreeSet<String> {
    definitions(&reference["appearance"][kind])
}

/// Every entry of a materials or textures array, without its `name`, as
/// canonical JSON — so that a set of them is a set of distinct definitions.
fn definitions(entries: &Value) -> BTreeSet<String> {
    entries
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    let mut entry = entry.as_object().expect("an entry is an object").clone();
                    entry.remove("name");
                    let sorted: BTreeMap<&String, &Value> = entry.iter().collect();
                    serde_json::to_string(&sorted).expect("an entry serialises")
                })
                .collect()
        })
        .unwrap_or_default()
}

fn compare_surface_data(ours: &BTreeSet<String>, theirs: &BTreeSet<String>, kind: &str) {
    assert_eq!(
        ours.len(),
        theirs.len(),
        "{kind}s: {} distinct definitions here, {} in citygml-tools",
        ours.len(),
        theirs.len()
    );
    for definition in ours {
        let ours: Value = serde_json::from_str(definition).expect("round-trips");
        let matched = theirs.iter().any(|theirs| {
            let theirs: Value = serde_json::from_str(theirs).expect("round-trips");
            states_no_less(&theirs, &ours)
        });
        assert!(
            matched,
            "{kind} {definition} matches no citygml-tools {kind}; \
             citygml-tools has {theirs:?}"
        );
    }
}

/// Whether `whole` states everything `part` states, and agrees on all of it.
fn states_no_less(whole: &Value, part: &Value) -> bool {
    let (whole, part) = (
        whole.as_object().expect("an object"),
        part.as_object().expect("an object"),
    );
    part.iter().all(|(key, value)| {
        whole
            .get(key)
            .is_some_and(|theirs| values_match(value, theirs))
    })
}

// ---------------------------------------------------------------------------
// Reading a CityJSON geometry
// ---------------------------------------------------------------------------

/// Every vertex index an object's boundaries name, without repeats.
fn used_vertices(object: &Value) -> BTreeSet<usize> {
    let mut indices = BTreeSet::new();
    for geometry in geometries(object) {
        for polygon in polygons(geometry) {
            for ring in polygon.as_array().expect("a polygon is an array of rings") {
                for index in ring.as_array().expect("a ring is an array of indices") {
                    indices.insert(index.as_u64().expect("an index is a number") as usize);
                }
            }
        }
    }
    indices
}

fn geometries(object: &Value) -> Vec<&Value> {
    object["geometry"]
        .as_array()
        .map(|geometries| geometries.iter().collect())
        .unwrap_or_default()
}

/// How deeply a polygon is nested inside a geometry's `boundaries`, which is
/// what its type says (CityJSON 2.0 § 3.4).
fn polygon_depth(geometry: &Value) -> usize {
    match geometry["type"].as_str().expect("a geometry has a type") {
        "MultiSurface" | "CompositeSurface" => 0,
        "Solid" => 1,
        "MultiSolid" | "CompositeSolid" => 2,
        other => panic!("unexpected geometry type {other:?}"),
    }
}

/// Every polygon of a geometry — each an array of rings — in document order.
fn polygons(geometry: &Value) -> Vec<&Value> {
    collect_at_depth(&geometry["boundaries"], polygon_depth(geometry))
}

/// How many shells the geometry has, over all its solids; `None` for a
/// geometry that has none.
fn shell_count(geometry: &Value) -> Option<usize> {
    match geometry["type"].as_str().expect("a geometry has a type") {
        "Solid" => Some(geometry["boundaries"].as_array().map_or(0, Vec::len)),
        "MultiSolid" | "CompositeSolid" => Some(collect_at_depth(&geometry["boundaries"], 1).len()),
        _ => None,
    }
}

/// The elements `depth` levels of nesting down, in document order: `depth` 0
/// is the array's own elements.
fn collect_at_depth(value: &Value, depth: usize) -> Vec<&Value> {
    let Some(elements) = value.as_array() else {
        return Vec::new();
    };
    if depth == 0 {
        return elements.iter().collect();
    }
    elements
        .iter()
        .flat_map(|element| collect_at_depth(element, depth - 1))
        .collect()
}
