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
use std::collections::{HashMap, HashSet};

use cjseq::{
    GeographicalExtent, MaterialObject, MaterialReference, MaterialShell, MaterialValues, Metadata,
    ReferenceSystem, TextureObject, TextureReference, TextureValues, TexturedRing, TexturedShell,
    TexturedSurface,
};
use serde_json::Value;

use crate::appearance::SurfaceData;
use crate::crs::NormalizedCrs;
use crate::gml::{GmlGeometry, Polygon3, Ring};
use crate::model::{IntermediateGeometry, IntermediateObject, SemanticSurface};
use crate::{CityGmlDocument, ParseOptions, ParseReport};

/// Convert the intermediate model into CityJSONSeq structures.
///
/// `crs` is the document's reference system, if it named one that could be
/// normalised; `objects` are the top-level city objects in document order,
/// and the features come out in that same order.
///
/// `appearances` is the document's surface data, which reaches CityJSON only
/// here: CityGML states a material beside the geometry and names the polygons
/// it paints by `gml:id`, and turning that around into a palette plus one
/// index per surface needs the polygons, which only the converter has.
pub fn convert(
    mut objects: Vec<IntermediateObject>,
    crs: Option<NormalizedCrs>,
    appearances: Vec<SurfaceData>,
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

    let index = AppearanceIndex {
        materials: MaterialIndex::of(&appearances, report),
        textures: TextureIndex::of(&appearances, report),
    };
    let mut used = Used::default();
    let features = objects
        .iter()
        .map(|object| feature(object, &quantizer, &index, &mut used, report))
        .collect();
    index.materials.report_unused(&used.materials, report);
    index.textures.report_unused(&used.textures, report);

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

/// One top-level object and everything nested in it, as a CityJSONFeature
/// with its own vertex table.
///
/// A CityJSONFeature is one *tree* of City Objects, not one object: a building
/// and its parts and installations are written into the same feature and share
/// its vertex array, and the feature is identified by the root of that tree
/// (§ 7.2 of the CityJSON spec). Splitting them into a feature apiece would
/// break the `parents`/`children` links, which may only name objects of the
/// same feature.
///
/// An object with neither geometry nor attributes still becomes a feature:
/// it exists in the source, and a City Object with only a `type` is valid
/// CityJSON.
///
/// The materials, textures and texture vertices a feature uses are pooled
/// into that feature's own `appearance`, for the same reason its vertices are
/// its own: a
/// CityJSONFeature is read on its own line, and an index into a palette some
/// other line carries would resolve to nothing. `used` collects, across every
/// feature, the surface data that painted something, so that appearance which
/// painted nothing at all can be reported once at the end.
fn feature(
    object: &IntermediateObject,
    quantizer: &Quantizer,
    index: &AppearanceIndex,
    used: &mut Used,
    report: &mut ParseReport,
) -> cjseq::CityJSONFeature {
    let mut builder = FeatureBuilder {
        quantizer,
        vertices: VertexTable::default(),
        materials: MaterialPool::new(&index.materials),
        textures: TexturePool::new(&index.textures),
    };
    let mut city_objects = HashMap::new();
    add_city_object(object, None, &mut builder, &mut city_objects, report);
    used.materials
        .extend(builder.materials.used.iter().copied());
    used.textures.extend(builder.textures.used.iter().copied());

    cjseq::CityJSONFeature {
        thetype: cjseq::CityJSONFeatureType::CityJSONFeature,
        id: object.id.clone(),
        city_objects,
        vertices: builder.vertices.vertices,
        appearance: appearance(builder.materials, builder.textures),
    }
}

/// The feature's `appearance`, or `None` when nothing painted it: an
/// appearance holding empty palettes says less than no appearance at all.
///
/// The palettes are the *feature's*, as its vertices are — a
/// CityJSONFeature is read on its own line, and an index into a palette some
/// other line carries would resolve to nothing.
fn appearance(materials: MaterialPool, textures: TexturePool) -> Option<cjseq::Appearance> {
    let materials = (!materials.materials.is_empty()).then_some(materials.materials);
    let (textures, vertices_texture) = textures.palette();
    (materials.is_some() || textures.is_some()).then_some(cjseq::Appearance {
        materials,
        textures,
        vertices_texture,
        default_theme_texture: None,
        default_theme_material: None,
    })
}

/// What one feature is assembled against: the document's transform, which
/// every feature shares, and the vertex table and appearance palettes, which
/// are the feature's own.
struct FeatureBuilder<'a> {
    quantizer: &'a Quantizer,
    vertices: VertexTable,
    materials: MaterialPool<'a>,
    textures: TexturePool<'a>,
}

/// The document's surface data, indexed by the polygon each piece paints.
struct AppearanceIndex<'a> {
    materials: MaterialIndex<'a>,
    textures: TextureIndex<'a>,
}

/// The index entries that painted something, across every feature, so that
/// appearance which painted nothing at all can be reported once at the end.
/// The two indices number their entries separately, so the two sets are
/// separate too.
#[derive(Default)]
struct Used {
    materials: HashSet<usize>,
    textures: HashSet<usize>,
}

/// Write `object` into the feature's City Objects, then each of its children,
/// linking every pair from both ends.
///
/// Depth-first in document order, so the vertex table fills in the order the
/// source wrote the coordinates: an object's own geometry first, then that of
/// each child in turn.
///
/// `parent` is the id of the object this one is nested in, and `None` for the
/// root. CityJSON's `parents` is an array because a City Object may belong to
/// several groups; a building part belongs to exactly one building, so the
/// array a nested object gets holds exactly one id.
fn add_city_object(
    object: &IntermediateObject,
    parent: Option<&str>,
    builder: &mut FeatureBuilder,
    city_objects: &mut HashMap<String, cjseq::CityObject>,
    report: &mut ParseReport,
) {
    let mut city_object = cjseq::CityObject::new(object.co_type.clone());

    let geometry: Vec<cjseq::Geometry> = object
        .geometries
        .iter()
        .map(|geometry| convert_geometry(geometry, builder, report))
        .collect();
    // `geometry: []` and no `geometry` member differ, and the second is what
    // an object without geometry means. The same holds for `children`: an
    // empty array would claim the object was asked and had none.
    city_object.geometry = (!geometry.is_empty()).then_some(geometry);
    city_object.attributes =
        (!object.attributes.is_empty()).then(|| Value::Object(object.attributes.clone()));
    city_object.parents = parent.map(|parent| vec![parent.to_string()]);

    // A group's members are children too, and they are the ones that may name
    // an object of another feature: CityJSON § 2.5 lets a CityObjectGroup
    // refer to its members by id, and this converter keeps that id as it
    // stands. They follow the nested objects rather than replacing them, so
    // that an object which somehow had both keeps both, and `children_roles`
    // — which the spec requires to be one entry per child, in the same order —
    // is filled with `null` for the nested ones.
    let children: Vec<String> = object
        .children
        .iter()
        .map(|child| child.id.clone())
        .chain(object.group_members.iter().map(|(id, _)| id.clone()))
        .collect();
    city_object.children = (!children.is_empty()).then_some(children);
    city_object.children_roles = (!object.group_members.is_empty()).then(|| {
        object
            .children
            .iter()
            .map(|_| None)
            .chain(object.group_members.iter().map(|(_, role)| role.clone()))
            .collect()
    });

    // A CityJSON `CityObjects` member is keyed by id, so two objects of one
    // feature that share one cannot both be written. The first keeps it, as
    // everywhere else here that a source states one thing twice, and the
    // second is reported rather than silently overwriting it.
    match city_objects.entry(object.id.clone()) {
        Entry::Vacant(free) => {
            free.insert(city_object);
        }
        Entry::Occupied(taken) => report.warnings.push(format!(
            "more than one city object of this feature carries the id {:?}; the first one keeps \
             it and the later {:?} is left out",
            taken.key(),
            object.co_type,
        )),
    }
    for child in &object.children {
        add_city_object(child, Some(&object.id), builder, city_objects, report);
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
    builder: &mut FeatureBuilder,
    report: &mut ParseReport,
) -> cjseq::Geometry {
    let lod = Some(geometry.lod.clone());
    let common = cjseq::GeometryCommon {
        semantics: semantics(geometry),
        material: material(geometry, &mut builder.materials),
        texture: texture(geometry, &mut builder.textures, report),
    };
    let quantizer = builder.quantizer;
    let vertices = &mut builder.vertices;
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

/// The `semantics` member of a geometry, or `None` when the geometry has no
/// semantics to state.
///
/// CityJSON's `values` is the geometry's `boundaries` with the innermost level
/// — the rings of each surface — replaced by one index per surface, so it is
/// nested exactly one level less deeply than the boundaries are. A surface no
/// boundary surface claimed is `null` there, and a geometry where *every*
/// surface would be `null` says nothing at all, so it gets no `semantics`
/// member rather than an array of nulls.
fn semantics(geometry: &IntermediateGeometry) -> Option<cjseq::Semantics> {
    let polygons = geometry.geometry.polygons();
    if geometry.surfaces.is_empty() || !polygons.iter().any(|polygon| polygon.sem_idx.is_some()) {
        return None;
    }

    // An index that is not a surface of this geometry would name a surface
    // some other geometry owns; `null` is the honest answer.
    let count = geometry.surfaces.len();
    let indices = |polygons: &[Polygon3]| -> cjseq::SemanticsShell {
        polygons
            .iter()
            .map(|polygon| polygon.sem_idx.filter(|index| *index < count))
            .collect()
    };
    let shells = |shells: &[Vec<Polygon3>]| -> cjseq::SemanticsSolid {
        shells.iter().map(|shell| Some(indices(shell))).collect()
    };

    let values = match &geometry.geometry {
        GmlGeometry::MultiSurface(polygons) | GmlGeometry::CompositeSurface(polygons) => {
            cjseq::SemanticsValues::Surfaces(indices(polygons))
        }
        GmlGeometry::Solid(solid) => cjseq::SemanticsValues::Shells(shells(solid)),
        GmlGeometry::MultiSolid(solids) | GmlGeometry::CompositeSolid(solids) => {
            cjseq::SemanticsValues::Solids(solids.iter().map(|solid| Some(shells(solid))).collect())
        }
    };

    Some(cjseq::Semantics {
        surfaces: geometry.surfaces.iter().map(semantics_surface).collect(),
        values: Some(values),
        other: HashMap::new(),
    })
}

/// One intermediate semantic surface as CityJSON's Semantic Object: its type,
/// its place in the opening hierarchy, and its attributes as further members
/// of the same object.
fn semantics_surface(surface: &SemanticSurface) -> cjseq::SemanticsSurface {
    cjseq::SemanticsSurface {
        thetype: surface_type(&surface.stype),
        parent: surface.parent,
        // No opening is written as no member at all: an empty `children`
        // array would claim the surface was asked and had none.
        children: (!surface.children.is_empty()).then(|| surface.children.clone()),
        other: surface.attributes.clone().into_iter().collect(),
    }
}

/// A surface type name as cjseq spells it.
///
/// The known names deserialize into their own variants, and anything else is
/// an Extension type carried through verbatim. The readers only ever produce
/// names from the CityGML schema, so the fallback is unreachable today; it is
/// here because losing the type would be worse than passing an unknown one on.
fn surface_type(stype: &str) -> cjseq::SemanticSurfaceType {
    serde_json::from_value(Value::String(stype.to_string()))
        .unwrap_or_else(|_| cjseq::SemanticSurfaceType::Extension(stype.to_string()))
}

/// The `material` member of a geometry: one entry per theme that paints any
/// of this geometry's polygons, or `None` when no theme paints any of them.
///
/// CityJSON's `material.values` is the geometry's `boundaries` with the *two*
/// innermost levels — the rings of each surface, and the vertices of each ring
/// — replaced by one index per surface, so it is nested exactly two levels
/// less deeply than the boundaries are and one level less deeply than
/// `semantics.values`. A surface no material of that theme targets is `null`
/// there.
///
/// `values` is used rather than `value` even where every surface carries the
/// same material: `value` states that the material paints the whole object,
/// which is a different claim, and CityGML's per-polygon targets do not make
/// it.
///
/// A theme that paints nothing here gets no entry at all rather than an entry
/// of nulls, which is the same rule `semantics` follows.
fn material(
    geometry: &IntermediateGeometry,
    pool: &mut MaterialPool,
) -> Option<HashMap<String, MaterialReference>> {
    // A shared reference to the index, so that reading the themes does not
    // hold a borrow of the pool the values are then written into.
    let index = pool.index;
    let mut material = HashMap::new();
    for theme in &index.themes {
        if !geometry
            .geometry
            .polygons()
            .iter()
            .any(|polygon| index.entry_of(theme, polygon).is_some())
        {
            continue;
        }
        let values = match &geometry.geometry {
            GmlGeometry::MultiSurface(polygons) | GmlGeometry::CompositeSurface(polygons) => {
                MaterialValues::Surfaces(pool.shell(theme, polygons))
            }
            GmlGeometry::Solid(solid) => MaterialValues::Shells(pool.solid(theme, solid)),
            GmlGeometry::MultiSolid(solids) | GmlGeometry::CompositeSolid(solids) => {
                MaterialValues::Solids(
                    solids
                        .iter()
                        .map(|solid| Some(pool.solid(theme, solid)))
                        .collect(),
                )
            }
        };
        material.insert(
            (*theme).to_string(),
            MaterialReference {
                values: Some(Some(values)),
                value: None,
                other: HashMap::new(),
            },
        );
    }
    (!material.is_empty()).then_some(material)
}

/// The `texture` member of a geometry: one entry per theme that textures any
/// ring of this geometry, or `None` when no theme textures any of them.
///
/// CityJSON's `texture.values` is nested exactly as deeply as the geometry's
/// `boundaries`, with each *ring* replaced by `[texture index, one UV-vertex
/// index per point of the ring]` — one entry more than the ring has points.
/// A ring with no texture in that theme is `[null]`.
///
/// A theme is written only where it textured at least one ring here: a theme
/// whose targets all failed to match would otherwise contribute an entry of
/// nothing but `[null]`s, which says the same as no entry at all.
fn texture(
    geometry: &IntermediateGeometry,
    pool: &mut TexturePool,
    report: &mut ParseReport,
) -> Option<HashMap<String, TextureReference>> {
    // A shared reference to the index, so that reading the themes does not
    // hold a borrow of the pool the values are then written into.
    let index = pool.index;
    let mut texture = HashMap::new();
    for theme in &index.themes {
        if !geometry
            .geometry
            .polygons()
            .iter()
            .any(|polygon| index.painted(theme, polygon).is_some())
        {
            continue;
        }
        let textured = pool.textured;
        let values = match &geometry.geometry {
            GmlGeometry::MultiSurface(polygons) | GmlGeometry::CompositeSurface(polygons) => {
                TextureValues::Surface(pool.shell(theme, polygons, report))
            }
            GmlGeometry::Solid(solid) => TextureValues::Shell(pool.solid(theme, solid, report)),
            GmlGeometry::MultiSolid(solids) | GmlGeometry::CompositeSolid(solids) => {
                TextureValues::Solid(
                    solids
                        .iter()
                        .map(|solid| pool.solid(theme, solid, report))
                        .collect(),
                )
            }
        };
        // Every target of this theme matched a polygon and then failed at the
        // ring: the values hold no texture index at all.
        if pool.textured == textured {
            continue;
        }
        texture.insert(
            (*theme).to_string(),
            TextureReference {
                values: Some(values),
                other: HashMap::new(),
            },
        );
    }
    (!texture.is_empty()).then_some(texture)
}

/// The document's surface data, indexed by the polygon each piece paints.
///
/// The themes are kept in the order the document declares them, so that the
/// palette a feature builds is ordered by the source and not by a hash.
struct MaterialIndex<'a> {
    entries: Vec<MaterialEntry<'a>>,
    /// Every theme named, in first-declared order, without repeats.
    themes: Vec<&'a str>,
    /// Theme, then polygon `gml:id`, to the entry that paints it.
    by_target: HashMap<&'a str, HashMap<&'a str, usize>>,
}

/// One piece of surface data: the material, and the theme it was declared
/// under.
struct MaterialEntry<'a> {
    theme: &'a str,
    material: &'a MaterialObject,
}

impl<'a> MaterialIndex<'a> {
    /// Index the document's surface data.
    ///
    /// A polygon painted twice in one theme keeps the first material: CityJSON
    /// holds one index per surface per theme, so one of the two has to go, and
    /// the second is reported rather than dropped in silence.
    fn of(appearances: &'a [SurfaceData], report: &mut ParseReport) -> Self {
        let mut index = Self {
            entries: Vec::new(),
            themes: Vec::new(),
            by_target: HashMap::new(),
        };
        for data in appearances {
            let SurfaceData::Material {
                theme,
                material,
                targets,
            } = data
            else {
                continue;
            };
            let entry = index.entries.len();
            index.entries.push(MaterialEntry { theme, material });
            if !index.themes.contains(&theme.as_str()) {
                index.themes.push(theme);
            }
            let by_target = index.by_target.entry(theme).or_default();
            for target in targets {
                match by_target.entry(target) {
                    Entry::Vacant(unpainted) => {
                        unpainted.insert(entry);
                    }
                    Entry::Occupied(_) => report.warnings.push(format!(
                        "polygon {target:?} is targeted by more than one material in theme \
                         {theme:?}; the first one wins"
                    )),
                }
            }
        }
        index
    }

    /// The entry painting this polygon in this theme, if any does.
    ///
    /// A polygon with no `gml:id` can be targeted by nothing: an
    /// `app:target` names an id, so a polygon that has none is unreachable.
    fn entry_of(&self, theme: &str, polygon: &Polygon3) -> Option<usize> {
        let gml_id = polygon.gml_id.as_deref()?;
        self.by_target.get(theme)?.get(gml_id).copied()
    }

    /// Warn about every piece of surface data that painted no polygon of the
    /// document — one warning apiece, naming the material and its theme, so
    /// that a file whose targets do not match its geometry says which ones.
    fn report_unused(&self, used: &HashSet<usize>, report: &mut ParseReport) {
        for (entry, data) in self.entries.iter().enumerate() {
            if !used.contains(&entry) {
                report.warnings.push(format!(
                    "material {:?} of theme {:?} targets no polygon in the document; \
                     it is left out of the appearance",
                    data.material.name, data.theme
                ));
            }
        }
    }
}

/// One feature's material palette, filled in as the feature's geometries are
/// converted.
///
/// Indices are feature-local, because `CityJSONFeature.appearance` is: the
/// palette is written on the same line as the geometry that indexes it.
/// Materials are pooled in the order they are first used, and two entries
/// holding equal materials — the same colour declared once per theme, say —
/// share one palette entry.
struct MaterialPool<'a> {
    index: &'a MaterialIndex<'a>,
    materials: Vec<MaterialObject>,
    /// Index entry to palette index, for the entries this feature has used.
    interned: HashMap<usize, usize>,
    /// The index entries this feature used, for the document-wide report.
    used: HashSet<usize>,
}

impl<'a> MaterialPool<'a> {
    fn new(index: &'a MaterialIndex<'a>) -> Self {
        Self {
            index,
            materials: Vec::new(),
            interned: HashMap::new(),
            used: HashSet::new(),
        }
    }

    /// One shell's worth of material indices: one per surface, `None` where
    /// the theme paints none.
    fn shell(&mut self, theme: &str, polygons: &[Polygon3]) -> Vec<Option<usize>> {
        polygons
            .iter()
            .map(|polygon| self.index_of(theme, polygon))
            .collect()
    }

    /// One solid's worth: [`shell`](Self::shell) per shell. The shells
    /// themselves are always `Some` — a shell exists, whether or not anything
    /// paints it.
    fn solid(&mut self, theme: &str, shells: &[Vec<Polygon3>]) -> Vec<Option<MaterialShell>> {
        shells
            .iter()
            .map(|shell| Some(self.shell(theme, shell)))
            .collect()
    }

    /// The palette index for the material painting this polygon in this
    /// theme, adding it to the palette if this is its first use here.
    fn index_of(&mut self, theme: &str, polygon: &Polygon3) -> Option<usize> {
        let entry = self.index.entry_of(theme, polygon)?;
        self.used.insert(entry);
        Some(self.intern(entry))
    }

    /// The palette index of one index entry's material.
    fn intern(&mut self, entry: usize) -> usize {
        if let Some(&pooled) = self.interned.get(&entry) {
            return pooled;
        }
        let material = self.index.entries[entry].material;
        let pooled = match self
            .materials
            .iter()
            .position(|palette| palette == material)
        {
            Some(pooled) => pooled,
            None => {
                self.materials.push(material.clone());
                self.materials.len() - 1
            }
        };
        self.interned.insert(entry, pooled);
        pooled
    }
}

/// The document's textures, indexed by the polygon each one paints.
///
/// Shaped exactly as [`MaterialIndex`], one level deeper: a texture reaches
/// the *rings* of the polygons it targets, not the polygons alone.
struct TextureIndex<'a> {
    entries: Vec<TextureEntry<'a>>,
    /// Every theme named, in first-declared order, without repeats.
    themes: Vec<&'a str>,
    /// Theme, then polygon `gml:id`, to what textures it.
    by_target: HashMap<&'a str, HashMap<&'a str, PaintedPolygon<'a>>>,
}

/// One piece of surface data: the texture, and the theme it was declared
/// under.
struct TextureEntry<'a> {
    theme: &'a str,
    texture: &'a TextureObject,
}

/// One polygon as one texture paints it: which texture, and the coordinates
/// that texture states for each of the polygon's rings, as the document wrote
/// them.
struct PaintedPolygon<'a> {
    entry: usize,
    /// Ring `gml:id` to that ring's (u, v) pairs.
    rings: HashMap<&'a str, &'a [[f64; 2]]>,
}

impl<'a> TextureIndex<'a> {
    /// Index the document's textures.
    ///
    /// As with materials, a polygon textured twice in one theme keeps the
    /// first texture — CityJSON holds one texture index per ring per theme —
    /// and the second is reported rather than dropped in silence.
    fn of(appearances: &'a [SurfaceData], report: &mut ParseReport) -> Self {
        let mut index = Self {
            entries: Vec::new(),
            themes: Vec::new(),
            by_target: HashMap::new(),
        };
        for data in appearances {
            let SurfaceData::Texture {
                theme,
                texture,
                targets,
            } = data
            else {
                continue;
            };
            let entry = index.entries.len();
            index.entries.push(TextureEntry { theme, texture });
            if !index.themes.contains(&theme.as_str()) {
                index.themes.push(theme);
            }
            let by_target = index.by_target.entry(theme).or_default();
            for target in targets {
                match by_target.entry(&target.polygon_id) {
                    Entry::Vacant(untextured) => {
                        untextured.insert(PaintedPolygon {
                            entry,
                            rings: rings_of(target),
                        });
                    }
                    Entry::Occupied(_) => report.warnings.push(format!(
                        "polygon {:?} is targeted by more than one texture in theme {theme:?}; \
                         the first one wins",
                        target.polygon_id
                    )),
                }
            }
        }
        index
    }

    /// What textures this polygon in this theme, if anything does.
    ///
    /// A polygon with no `gml:id` can be targeted by nothing, exactly as in
    /// [`MaterialIndex::entry_of`].
    fn painted(&self, theme: &str, polygon: &Polygon3) -> Option<&PaintedPolygon<'a>> {
        let gml_id = polygon.gml_id.as_deref()?;
        self.by_target.get(theme)?.get(gml_id)
    }

    /// Warn about every texture that painted no polygon of the document.
    fn report_unused(&self, used: &HashSet<usize>, report: &mut ParseReport) {
        for (entry, data) in self.entries.iter().enumerate() {
            if !used.contains(&entry) {
                report.warnings.push(format!(
                    "texture {:?} of theme {:?} targets no polygon in the document; \
                     it is left out of the appearance",
                    data.texture.image.as_deref().unwrap_or_default(),
                    data.theme
                ));
            }
        }
    }
}

/// One target's rings, by `gml:id`.
///
/// A ring named twice by one target keeps the first list, which is the rule
/// the polygon level follows.
fn rings_of(target: &crate::appearance::TextureTarget) -> HashMap<&str, &[[f64; 2]]> {
    let mut rings = HashMap::new();
    for (ring, coords) in &target.ring_coords {
        rings.entry(ring.as_str()).or_insert(coords.as_slice());
    }
    rings
}

/// One feature's texture palette and texture-vertex table, filled in as the
/// feature's geometries are converted.
///
/// Both are feature-local, for the reason [`MaterialPool`]'s palette is:
/// `CityJSONFeature.appearance` is written on the same line as the geometry
/// that indexes it.
struct TexturePool<'a> {
    index: &'a TextureIndex<'a>,
    textures: Vec<TextureObject>,
    /// Index entry to palette index, for the entries this feature has used.
    interned: HashMap<usize, usize>,
    /// The (u, v) pairs this feature refers to, in first-seen order.
    uvs: Vec<[f64; 2]>,
    /// Each pooled pair by its bit pattern: `f64` is not `Hash`, and two
    /// coordinates are the same texture vertex exactly when the document
    /// wrote the same number twice.
    uv_index: HashMap<[u64; 2], usize>,
    /// How many rings have been given a texture, for the caller that decides
    /// whether a theme earned its `texture` entry at all.
    textured: usize,
    /// The index entries this feature used, for the document-wide report.
    used: HashSet<usize>,
}

impl<'a> TexturePool<'a> {
    fn new(index: &'a TextureIndex<'a>) -> Self {
        Self {
            index,
            textures: Vec::new(),
            interned: HashMap::new(),
            uvs: Vec::new(),
            uv_index: HashMap::new(),
            textured: 0,
            used: HashSet::new(),
        }
    }

    /// One shell's worth of texture values: one entry per surface, itself one
    /// entry per ring.
    fn shell(
        &mut self,
        theme: &str,
        polygons: &[Polygon3],
        report: &mut ParseReport,
    ) -> TexturedShell {
        polygons
            .iter()
            .map(|polygon| self.surface(theme, polygon, report))
            .collect()
    }

    /// One solid's worth: [`shell`](Self::shell) per shell.
    fn solid(
        &mut self,
        theme: &str,
        shells: &[Vec<Polygon3>],
        report: &mut ParseReport,
    ) -> Vec<TexturedShell> {
        shells
            .iter()
            .map(|shell| self.shell(theme, shell, report))
            .collect()
    }

    /// One polygon's worth: one entry per ring, exterior first, mirroring the
    /// surface in `boundaries`.
    fn surface(
        &mut self,
        theme: &str,
        polygon: &Polygon3,
        report: &mut ParseReport,
    ) -> TexturedSurface {
        polygon
            .rings
            .iter()
            .map(|ring| self.ring(theme, polygon, ring, report))
            .collect()
    }

    /// One ring: `[texture index, one UV-vertex index per point]`, or
    /// `[null]` where this theme does not texture it.
    ///
    /// The coordinates are matched against the ring the *reader repaired*,
    /// not the ring the document wrote: CityGML closes its rings and states
    /// one coordinate per written point, so a list one longer than the
    /// repaired ring is that closing point's coordinate and is dropped with
    /// it. Any other disagreement is a fault in the source — a texture drawn
    /// against a different geometry — and the ring is left untextured with a
    /// warning rather than textured from the wrong end.
    fn ring(
        &mut self,
        theme: &str,
        polygon: &Polygon3,
        ring: &Ring,
        report: &mut ParseReport,
    ) -> TexturedRing {
        let untextured = vec![None];
        let index = self.index;
        let Some(painted) = index.painted(theme, polygon) else {
            return untextured;
        };
        // The texture reached the polygon, so it painted something in this
        // document whatever the rings then say.
        self.used.insert(painted.entry);
        let coords = ring
            .gml_id
            .as_deref()
            .and_then(|gml_id| painted.rings.get(gml_id).copied());
        let Some(coords) = coords else {
            return untextured;
        };
        let Some(coords) = fit_to_ring(coords, ring.pts.len()) else {
            report.warnings.push(format!(
                "ring {:?} of polygon {:?} has {} texture coordinate(s) in theme {theme:?} for a \
                 ring of {} point(s); the ring is left untextured",
                ring.gml_id.as_deref().unwrap_or_default(),
                polygon.gml_id.as_deref().unwrap_or_default(),
                coords.len(),
                ring.pts.len()
            ));
            return untextured;
        };
        let texture = self.intern(painted.entry);
        self.textured += 1;
        let mut values = Vec::with_capacity(1 + coords.len());
        values.push(Some(texture));
        values.extend(coords.iter().map(|uv| Some(self.uv_index_of(*uv))));
        values
    }

    /// The palette index of one index entry's texture, adding it to the
    /// palette if this is its first use here. Two entries holding equal
    /// textures — the same image declared once per theme — share one entry.
    fn intern(&mut self, entry: usize) -> usize {
        if let Some(&pooled) = self.interned.get(&entry) {
            return pooled;
        }
        let texture = self.index.entries[entry].texture;
        let pooled = match self.textures.iter().position(|palette| palette == texture) {
            Some(pooled) => pooled,
            None => {
                self.textures.push(texture.clone());
                self.textures.len() - 1
            }
        };
        self.interned.insert(entry, pooled);
        pooled
    }

    /// The index of one (u, v) pair in the feature's texture-vertex table,
    /// appending it if this is its first appearance.
    fn uv_index_of(&mut self, uv: [f64; 2]) -> usize {
        match self.uv_index.entry([uv[0].to_bits(), uv[1].to_bits()]) {
            Entry::Occupied(seen) => *seen.get(),
            Entry::Vacant(unseen) => {
                self.uvs.push(uv);
                *unseen.insert(self.uvs.len() - 1)
            }
        }
    }

    /// The feature's `textures` and `vertices-texture`, or a pair of `None`s
    /// where nothing textured it.
    fn palette(self) -> (Option<Vec<TextureObject>>, Option<Vec<[f64; 2]>>) {
        if self.textures.is_empty() {
            return (None, None);
        }
        (Some(self.textures), Some(self.uvs))
    }
}

/// Texture coordinates cut to the length of the ring they belong to, or
/// `None` when they are not that ring's coordinates at all.
///
/// One coordinate per point is the match; one more is the closing point the
/// reader dropped, and its coordinate goes with it.
fn fit_to_ring(coords: &[[f64; 2]], points: usize) -> Option<&[[f64; 2]]> {
    (coords.len() == points || coords.len() == points + 1).then(|| &coords[..points])
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
            for polygon in geometry.geometry.polygons() {
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
            for polygon in geometry.geometry.polygons_mut() {
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
        convert(
            objects,
            crs,
            Vec::new(),
            &ParseOptions::default(),
            &mut report,
        )
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

    /// The same square, pointing at a semantic surface.
    fn semantic_polygon(pts: Vec<[f64; 3]>, sem_idx: Option<usize>) -> Polygon3 {
        Polygon3 {
            sem_idx,
            ..polygon(pts)
        }
    }

    /// A semantic surface of `stype` with nothing else on it.
    fn surface(stype: &str) -> SemanticSurface {
        SemanticSurface::new(stype.to_string())
    }

    /// The one geometry of the one city object of the one feature, as JSON.
    fn geometry_json(object: IntermediateObject) -> serde_json::Value {
        let doc = convert_one(vec![object], None);
        serde_json::to_value(
            &doc.features[0].city_objects["o1"]
                .geometry
                .as_ref()
                .unwrap()[0],
        )
        .unwrap()
    }

    /// `values` is `boundaries` with its innermost level — the rings — gone,
    /// so it is nested exactly one level less deeply, whatever the geometry.
    #[test]
    fn semantics_values_are_nested_one_level_less_deeply_than_boundaries() {
        let face = |sem_idx| {
            semantic_polygon(
                vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
                sem_idx,
            )
        };
        for (geometry, expected) in [
            (
                GmlGeometry::MultiSurface(vec![face(Some(0)), face(None)]),
                serde_json::json!([0, null]),
            ),
            (
                GmlGeometry::CompositeSurface(vec![face(Some(1))]),
                serde_json::json!([1]),
            ),
            (
                GmlGeometry::Solid(vec![vec![face(Some(0)), face(Some(1))]]),
                serde_json::json!([[0, 1]]),
            ),
            (
                GmlGeometry::MultiSolid(vec![vec![vec![face(Some(1))]]]),
                serde_json::json!([[[1]]]),
            ),
            (
                GmlGeometry::CompositeSolid(vec![vec![vec![face(Some(0))]]]),
                serde_json::json!([[[0]]]),
            ),
        ] {
            let mut object = object(geometry);
            object.geometries[0].surfaces = vec![surface("RoofSurface"), surface("GroundSurface")];
            let json = geometry_json(object);
            assert_eq!(json["semantics"]["values"], expected);
            assert_eq!(
                json["semantics"]["surfaces"],
                serde_json::json!([{"type": "RoofSurface"}, {"type": "GroundSurface"}])
            );
        }
    }

    /// A geometry no boundary surface described has no `semantics` member —
    /// not an empty one, and not an array of nulls.
    #[test]
    fn a_geometry_without_semantics_has_no_semantics_member() {
        let face = polygon(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]);
        // No surfaces at all.
        let json = geometry_json(object(GmlGeometry::MultiSurface(vec![face.clone()])));
        assert!(json.get("semantics").is_none(), "{json}");

        // Surfaces, but nothing pointing at them: the same thing, said
        // differently.
        let mut object = object(GmlGeometry::MultiSurface(vec![face]));
        object.geometries[0].surfaces = vec![surface("WallSurface")];
        let json = geometry_json(object);
        assert!(json.get("semantics").is_none(), "{json}");
    }

    /// An opening's `parent`, and the surface's `children`, reach the
    /// CityJSON document; a surface with no opening writes neither member.
    #[test]
    fn the_opening_hierarchy_is_written_out() {
        let mut wall = surface("WallSurface");
        wall.children = vec![1];
        let mut window = surface("Window");
        window.parent = Some(0);
        window
            .attributes
            .insert("direction".to_string(), Value::String("north".to_string()));

        let mut object = object(GmlGeometry::MultiSurface(vec![
            semantic_polygon(
                vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
                Some(0),
            ),
            semantic_polygon(
                vec![[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0]],
                Some(1),
            ),
        ]));
        object.geometries[0].surfaces = vec![wall, window];

        assert_eq!(
            geometry_json(object)["semantics"]["surfaces"],
            serde_json::json!([
                {"type": "WallSurface", "children": [1]},
                {"type": "Window", "parent": 0, "direction": "north"}
            ])
        );
    }

    /// An index that names no surface of this geometry is written as `null`:
    /// it would otherwise point into another geometry's list.
    #[test]
    fn a_semantic_index_out_of_range_is_written_as_null() {
        let mut object = object(GmlGeometry::MultiSurface(vec![
            semantic_polygon(
                vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
                Some(0),
            ),
            semantic_polygon(
                vec![[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0]],
                Some(7),
            ),
        ]));
        object.geometries[0].surfaces = vec![surface("RoofSurface")];
        assert_eq!(
            geometry_json(object)["semantics"]["values"],
            serde_json::json!([0, null])
        );
    }

    /// Two objects of one feature cannot share an id: the CityJSON member
    /// they are written into is keyed by it. The first keeps the key and the
    /// second is reported, rather than replacing it without a word.
    #[test]
    fn a_repeated_city_object_id_is_reported_and_the_first_object_kept() {
        let face = |z: f64| {
            GmlGeometry::MultiSurface(vec![polygon(vec![
                [0.0, 0.0, z],
                [1.0, 0.0, z],
                [1.0, 1.0, z],
            ])])
        };
        let mut root = object(face(0.0));
        let mut clash = object(face(1.0));
        clash.co_type = cjseq::CityObjectType::BuildingPart;
        root.children.push(clash);

        let mut report = ParseReport::default();
        let doc = convert(
            vec![root],
            None,
            Vec::new(),
            &ParseOptions::default(),
            &mut report,
        );

        let feature = &doc.features[0];
        assert_eq!(feature.city_objects.len(), 1);
        // The first object, not the child that came after it.
        assert_eq!(
            feature.city_objects["o1"].thetype,
            cjseq::CityObjectType::Building
        );
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
        assert!(
            report.warnings[0].contains("\"o1\""),
            "{}",
            report.warnings[0]
        );
    }

    /// A nested object tree is one feature: every object in it is a City
    /// Object of that feature, linked both ways, and every coordinate indexes
    /// the one vertex array they share.
    #[test]
    fn a_child_tree_becomes_one_feature_of_linked_city_objects() {
        let face = |z: f64| {
            GmlGeometry::MultiSurface(vec![polygon(vec![
                [0.0, 0.0, z],
                [1.0, 0.0, z],
                [1.0, 1.0, z],
            ])])
        };
        let named = |id: &str, co_type, geometry| {
            let mut object = object(geometry);
            object.id = id.to_string();
            object.co_type = co_type;
            object
        };

        let mut root = named("b1", cjseq::CityObjectType::Building, face(0.0));
        let mut part = named("p1", cjseq::CityObjectType::BuildingPart, face(1.0));
        part.children.push(named(
            "i1",
            cjseq::CityObjectType::BuildingInstallation,
            face(2.0),
        ));
        root.children.push(part);

        let doc = convert_one(vec![root], None);
        assert_eq!(doc.features.len(), 1);
        let feature = &doc.features[0];
        // The feature is named after the root, whatever it holds.
        assert_eq!(feature.id, "b1");
        assert_eq!(feature.city_objects.len(), 3);

        assert_eq!(
            feature.city_objects["b1"].children,
            Some(vec!["p1".to_string()])
        );
        assert!(feature.city_objects["b1"].parents.is_none());
        assert_eq!(
            feature.city_objects["p1"].parents,
            Some(vec!["b1".to_string()])
        );
        assert_eq!(
            feature.city_objects["p1"].children,
            Some(vec!["i1".to_string()])
        );
        assert_eq!(
            feature.city_objects["i1"].parents,
            Some(vec!["p1".to_string()])
        );
        assert!(feature.city_objects["i1"].children.is_none());

        // Nine distinct points, pooled feature-wide and in document order:
        // the root's, then the part's, then the grandchild's.
        assert_eq!(feature.vertices.len(), 9);
        let cjseq::Geometry::MultiSurface { boundaries, .. } =
            &feature.city_objects["i1"].geometry.as_ref().unwrap()[0]
        else {
            panic!("expected a MultiSurface");
        };
        assert_eq!(boundaries, &vec![vec![vec![6, 7, 8]]]);
    }

    //-------------------------------------------------------------------
    //-- the appearance join
    //-------------------------------------------------------------------

    /// The same square, with a `gml:id` an `app:target` can name.
    fn identified(gml_id: &str, pts: Vec<[f64; 3]>) -> Polygon3 {
        Polygon3 {
            gml_id: Some(gml_id.to_string()),
            ..polygon(pts)
        }
    }

    /// A unit square at height `z`, identified by `gml_id`.
    fn face(gml_id: &str, z: f64) -> Polygon3 {
        identified(gml_id, vec![[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z]])
    }

    /// One `app:X3DMaterial`'s worth of surface data.
    fn surface_data(theme: &str, name: &str, targets: &[&str]) -> SurfaceData {
        SurfaceData::Material {
            theme: theme.to_string(),
            material: MaterialObject {
                name: name.to_string(),
                ambient_intensity: None,
                diffuse_color: Some([0.5, 0.5, 0.5]),
                emissive_color: None,
                specular_color: None,
                shininess: None,
                transparency: None,
                is_smooth: None,
            },
            targets: targets.iter().map(|target| target.to_string()).collect(),
        }
    }

    /// Convert with appearance, keeping the report.
    fn convert_painted(
        objects: Vec<IntermediateObject>,
        appearances: Vec<SurfaceData>,
    ) -> (CityGmlDocument, ParseReport) {
        let mut report = ParseReport::default();
        let doc = convert(
            objects,
            None,
            appearances,
            &ParseOptions::default(),
            &mut report,
        );
        (doc, report)
    }

    /// `material.values` is `boundaries` with its two innermost levels — the
    /// rings, and their vertices — gone, so it is nested one level less
    /// deeply than `semantics.values` and two less than the boundaries.
    /// A polygon no material targets is `null` there.
    #[test]
    fn material_values_are_nested_two_levels_less_deeply_than_boundaries() {
        for (geometry, expected) in [
            (
                GmlGeometry::MultiSurface(vec![face("painted", 0.0), face("bare", 1.0)]),
                serde_json::json!([0, null]),
            ),
            (
                GmlGeometry::CompositeSurface(vec![face("painted", 0.0)]),
                serde_json::json!([0]),
            ),
            (
                GmlGeometry::Solid(vec![vec![face("painted", 0.0), face("bare", 1.0)]]),
                serde_json::json!([[0, null]]),
            ),
            (
                GmlGeometry::MultiSolid(vec![vec![vec![face("painted", 0.0)]]]),
                serde_json::json!([[[0]]]),
            ),
            (
                GmlGeometry::CompositeSolid(vec![vec![vec![face("painted", 0.0)]]]),
                serde_json::json!([[[0]]]),
            ),
        ] {
            let (doc, report) = convert_painted(
                vec![object(geometry)],
                vec![surface_data("summer", "red", &["painted"])],
            );
            let json = serde_json::to_value(
                &doc.features[0].city_objects["o1"]
                    .geometry
                    .as_ref()
                    .unwrap()[0],
            )
            .unwrap();
            assert_eq!(json["material"]["summer"]["values"], expected, "{json}");
            // The assignment is per surface, so it is `values` and never the
            // whole-object `value`.
            assert!(json["material"]["summer"].get("value").is_none(), "{json}");
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        }
    }

    /// Two themes over one geometry, and the palette they share: equal
    /// materials are one entry, and a theme that paints nothing here is not
    /// written at all.
    #[test]
    fn equal_materials_share_one_palette_entry_and_an_unused_theme_is_absent() {
        let (doc, report) = convert_painted(
            vec![object(GmlGeometry::MultiSurface(vec![
                face("a", 0.0),
                face("b", 1.0),
            ]))],
            vec![
                // The same grey, declared once per theme.
                surface_data("summer", "grey", &["a"]),
                surface_data("winter", "grey", &["b"]),
                // A third theme, painting a polygon this geometry does not
                // hold.
                surface_data("autumn", "grey", &["elsewhere"]),
            ],
        );
        let json = serde_json::to_value(
            &doc.features[0].city_objects["o1"]
                .geometry
                .as_ref()
                .unwrap()[0],
        )
        .unwrap();
        assert_eq!(
            json["material"],
            serde_json::json!({
                "summer": {"values": [0, null]},
                "winter": {"values": [null, 0]}
            })
        );
        assert_eq!(
            serde_json::to_value(&doc.features[0].appearance).unwrap(),
            serde_json::json!({"materials": [{"name": "grey", "diffuseColor": [0.5, 0.5, 0.5]}]})
        );
        // Only the appearance that painted nothing is reported.
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
        assert!(
            report.warnings[0].contains("autumn"),
            "{:?}",
            report.warnings
        );
    }

    /// The palette is the *feature's*, as its vertices are: a feature nothing
    /// paints carries no `appearance` at all, and one that is painted indexes
    /// its own palette from zero.
    #[test]
    fn each_feature_carries_its_own_palette() {
        let named = |id: &str, gml_id: &str| {
            let mut object = object(GmlGeometry::MultiSurface(vec![face(gml_id, 0.0)]));
            object.id = id.to_string();
            object
        };
        let (doc, report) = convert_painted(
            vec![named("o1", "first"), named("o2", "second")],
            vec![
                surface_data("summer", "grey-1", &["nothing"]),
                surface_data("summer", "grey-2", &["second"]),
            ],
        );
        assert!(doc.features[0].appearance.is_none());
        assert_eq!(
            serde_json::to_value(&doc.features[1].appearance).unwrap(),
            serde_json::json!({"materials": [{"name": "grey-2", "diffuseColor": [0.5, 0.5, 0.5]}]})
        );
        let json = serde_json::to_value(
            &doc.features[1].city_objects["o2"]
                .geometry
                .as_ref()
                .unwrap()[0],
        )
        .unwrap();
        // Feature-local: the second feature's only material is index 0, even
        // though it is the second the document declares.
        assert_eq!(json["material"]["summer"]["values"], serde_json::json!([0]));
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    }

    /// A geometry no theme paints has no `material` member — not an empty
    /// one, and not an array of nulls. Nor has a polygon without a `gml:id`
    /// any way of being targeted.
    #[test]
    fn an_unpainted_geometry_has_no_material_member() {
        for (geometry, appearances) in [
            // Nothing declared at all.
            (
                GmlGeometry::MultiSurface(vec![face("a", 0.0)]),
                Vec::<SurfaceData>::new(),
            ),
            // Declared, but naming another polygon.
            (
                GmlGeometry::MultiSurface(vec![face("a", 0.0)]),
                vec![surface_data("summer", "grey", &["b"])],
            ),
            // A polygon with no id cannot be the target of anything.
            (
                GmlGeometry::MultiSurface(vec![polygon(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [1.0, 1.0, 0.0],
                ])]),
                vec![surface_data("summer", "grey", &["a"])],
            ),
        ] {
            let (doc, _) = convert_painted(vec![object(geometry)], appearances);
            let json = serde_json::to_value(
                &doc.features[0].city_objects["o1"]
                    .geometry
                    .as_ref()
                    .unwrap()[0],
            )
            .unwrap();
            assert!(json.get("material").is_none(), "{json}");
            assert!(doc.features[0].appearance.is_none());
        }
    }

    /// One polygon, painted twice in one theme: CityJSON holds one index per
    /// surface per theme, so the first material wins and the second is
    /// reported rather than dropped in silence.
    #[test]
    fn a_polygon_painted_twice_in_one_theme_keeps_the_first_material() {
        let (doc, report) = convert_painted(
            vec![object(GmlGeometry::MultiSurface(vec![face("a", 0.0)]))],
            vec![
                surface_data("summer", "first", &["a"]),
                surface_data("summer", "second", &["a"]),
            ],
        );
        assert_eq!(
            serde_json::to_value(&doc.features[0].appearance).unwrap(),
            serde_json::json!({"materials": [{"name": "first", "diffuseColor": [0.5, 0.5, 0.5]}]})
        );
        // One warning for the conflict, one for the material it displaced.
        assert_eq!(report.warnings.len(), 2, "{:?}", report.warnings);
    }

    //-------------------------------------------------------------------
    //-- the texture join
    //-------------------------------------------------------------------

    /// A triangle whose polygon *and* exterior ring carry ids, which is what
    /// a texture needs to name it.
    fn textured_face(gml_id: &str, ring_id: &str, z: f64) -> Polygon3 {
        Polygon3 {
            gml_id: Some(gml_id.to_string()),
            rings: vec![Ring {
                gml_id: Some(ring_id.to_string()),
                pts: vec![[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z]],
            }],
            sem_idx: None,
        }
    }

    /// One `app:ParameterizedTexture`'s worth of surface data, targeting one
    /// ring of one polygon.
    fn texture_data(
        theme: &str,
        image: &str,
        polygon_id: &str,
        ring_id: &str,
        coords: &[[f64; 2]],
    ) -> SurfaceData {
        SurfaceData::Texture {
            theme: theme.to_string(),
            texture: TextureObject {
                thetype: Some(cjseq::TextureFormat::JPG),
                image: Some(image.to_string()),
                wrap_mode: None,
                texture_type: None,
                border_color: None,
            },
            targets: vec![crate::appearance::TextureTarget {
                polygon_id: polygon_id.to_string(),
                ring_coords: vec![(ring_id.to_string(), coords.to_vec())],
            }],
        }
    }

    /// The three coordinates of a [`textured_face`]'s ring.
    const UVS: [[f64; 2]; 3] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];

    /// `texture.values` is nested exactly as deeply as `boundaries`, one
    /// entry per *ring*: the texture index and then one UV-vertex index per
    /// point of the ring. A ring this theme does not texture is `[null]`.
    #[test]
    fn texture_values_are_nested_as_deeply_as_the_boundaries() {
        let textured = || textured_face("painted", "painted-ring", 0.0);
        for (geometry, expected) in [
            (
                GmlGeometry::MultiSurface(vec![textured(), face("bare", 1.0)]),
                serde_json::json!([[[0, 0, 1, 2]], [[null]]]),
            ),
            (
                GmlGeometry::CompositeSurface(vec![textured()]),
                serde_json::json!([[[0, 0, 1, 2]]]),
            ),
            (
                GmlGeometry::Solid(vec![vec![textured()]]),
                serde_json::json!([[[[0, 0, 1, 2]]]]),
            ),
            (
                GmlGeometry::MultiSolid(vec![vec![vec![textured()]]]),
                serde_json::json!([[[[[0, 0, 1, 2]]]]]),
            ),
            (
                GmlGeometry::CompositeSolid(vec![vec![vec![textured()]]]),
                serde_json::json!([[[[[0, 0, 1, 2]]]]]),
            ),
        ] {
            let (doc, report) = convert_painted(
                vec![object(geometry)],
                vec![texture_data(
                    "rgb",
                    "t/a.jpg",
                    "painted",
                    "painted-ring",
                    &UVS,
                )],
            );
            let json = serde_json::to_value(
                &doc.features[0].city_objects["o1"]
                    .geometry
                    .as_ref()
                    .unwrap()[0],
            )
            .unwrap();
            assert_eq!(json["texture"]["rgb"]["values"], expected, "{json}");
            assert_eq!(
                serde_json::to_value(&doc.features[0].appearance).unwrap(),
                serde_json::json!({
                    "textures": [{"type": "JPG", "image": "t/a.jpg"}],
                    "vertices-texture": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
                })
            );
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        }
    }

    /// GML closes its rings and states one coordinate per written point,
    /// where the reader drops the closing point: the coordinate that went
    /// with it is dropped too, and never reaches `vertices-texture`.
    #[test]
    fn the_closing_point_takes_its_texture_coordinate_with_it() {
        // A fourth pair that is *not* a repeat of the first, so that dropping
        // it is visible rather than hidden by the pooling.
        let closed = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [9.0, 9.0]];
        let (doc, report) = convert_painted(
            vec![object(GmlGeometry::MultiSurface(vec![textured_face(
                "painted",
                "painted-ring",
                0.0,
            )]))],
            vec![texture_data(
                "rgb",
                "t/a.jpg",
                "painted",
                "painted-ring",
                &closed,
            )],
        );
        let json = serde_json::to_value(
            &doc.features[0].city_objects["o1"]
                .geometry
                .as_ref()
                .unwrap()[0],
        )
        .unwrap();
        assert_eq!(
            json["texture"]["rgb"]["values"],
            serde_json::json!([[[0, 0, 1, 2]]])
        );
        assert_eq!(
            serde_json::to_value(&doc.features[0].appearance).unwrap()["vertices-texture"],
            serde_json::json!([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]])
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    /// Any other disagreement between the coordinates and the ring is a fault
    /// in the source: the ring is left untextured with a warning rather than
    /// textured from the wrong end. So is a ring the texture does not name,
    /// and a ring with no `gml:id` for it to name.
    #[test]
    fn coordinates_that_do_not_fit_the_ring_leave_it_untextured() {
        for (ring_id, coords) in [
            // Two coordinates too many, and one too few.
            ("painted-ring", vec![[0.0, 0.0]; 5]),
            ("painted-ring", vec![[0.0, 0.0]; 2]),
            // A ring of this polygon that the texture does not name.
            ("another-ring", UVS.to_vec()),
        ] {
            let (doc, report) = convert_painted(
                vec![object(GmlGeometry::MultiSurface(vec![textured_face(
                    "painted",
                    "painted-ring",
                    0.0,
                )]))],
                vec![texture_data("rgb", "t/a.jpg", "painted", ring_id, &coords)],
            );
            let json = serde_json::to_value(
                &doc.features[0].city_objects["o1"]
                    .geometry
                    .as_ref()
                    .unwrap()[0],
            )
            .unwrap();
            // No ring was textured, so the theme earned no entry at all and
            // the palette stayed empty.
            assert!(json.get("texture").is_none(), "{json}");
            assert!(doc.features[0].appearance.is_none(), "{ring_id}");
            // The count mismatch warns; the ring the texture never named is
            // no fault of the source and does not.
            let warnings = if ring_id == "painted-ring" { 1 } else { 0 };
            assert_eq!(report.warnings.len(), warnings, "{:?}", report.warnings);
        }
    }

    /// Equal textures share one palette entry, a coordinate written twice is
    /// one texture vertex, and a texture that painted no polygon of the
    /// document is reported rather than dropped in silence.
    #[test]
    fn equal_textures_and_repeated_coordinates_are_pooled() {
        let (doc, report) = convert_painted(
            vec![object(GmlGeometry::MultiSurface(vec![
                textured_face("a", "a-ring", 0.0),
                textured_face("b", "b-ring", 1.0),
            ]))],
            vec![
                // The same image, declared once per polygon, with one
                // coordinate in common.
                texture_data("rgb", "t/a.jpg", "a", "a-ring", &UVS),
                texture_data(
                    "rgb",
                    "t/a.jpg",
                    "b",
                    "b-ring",
                    &[[0.0, 0.0], [0.5, 0.0], [0.5, 0.5]],
                ),
                // A texture naming a polygon the document does not hold.
                texture_data("rgb", "t/b.jpg", "elsewhere", "no-ring", &UVS),
            ],
        );
        let json = serde_json::to_value(
            &doc.features[0].city_objects["o1"]
                .geometry
                .as_ref()
                .unwrap()[0],
        )
        .unwrap();
        assert_eq!(
            json["texture"]["rgb"]["values"],
            serde_json::json!([[[0, 0, 1, 2]], [[0, 0, 3, 4]]])
        );
        assert_eq!(
            serde_json::to_value(&doc.features[0].appearance).unwrap(),
            serde_json::json!({
                "textures": [{"type": "JPG", "image": "t/a.jpg"}],
                "vertices-texture": [
                    [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.5, 0.0], [0.5, 0.5]
                ]
            })
        );
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
        assert!(
            report.warnings[0].contains("t/b.jpg"),
            "{:?}",
            report.warnings
        );
    }

    /// Textures and materials share one `appearance`, each with its own
    /// palette, and each geometry carries both members.
    #[test]
    fn a_feature_that_is_painted_and_textured_carries_both_palettes() {
        let (doc, report) = convert_painted(
            vec![object(GmlGeometry::MultiSurface(vec![textured_face(
                "a", "a-ring", 0.0,
            )]))],
            vec![
                surface_data("rgb", "grey", &["a"]),
                texture_data("rgb", "t/a.jpg", "a", "a-ring", &UVS),
            ],
        );
        let json = serde_json::to_value(
            &doc.features[0].city_objects["o1"]
                .geometry
                .as_ref()
                .unwrap()[0],
        )
        .unwrap();
        assert_eq!(json["material"]["rgb"]["values"], serde_json::json!([0]));
        assert_eq!(
            json["texture"]["rgb"]["values"],
            serde_json::json!([[[0, 0, 1, 2]]])
        );
        assert_eq!(
            serde_json::to_value(&doc.features[0].appearance).unwrap(),
            serde_json::json!({
                "materials": [{"name": "grey", "diffuseColor": [0.5, 0.5, 0.5]}],
                "textures": [{"type": "JPG", "image": "t/a.jpg"}],
                "vertices-texture": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
            })
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
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
