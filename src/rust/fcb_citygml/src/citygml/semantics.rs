//! Thematic boundary surfaces: what each polygon of a geometry *is*.
//!
//! Several modules state semantics the same way and differ only in where they
//! write them. A building writes a `bldg:boundedBy` holding a
//! `bldg:RoofSurface`; a water body writes a `wtr:boundedBy` holding a
//! `wtr:WaterSurface`; a road writes a `tran:trafficArea` holding a
//! `tran:TrafficArea`. Underneath, all three are the same arrangement — a
//! property, a surface element with attributes of its own, and `lodX…`
//! geometry naming the polygons the surface claims — so the reader is one
//! reader, parameterised by a [`SurfaceSpec`] per module.
//!
//! The two halves are read separately because CityGML states them separately:
//! `bldg:boundedBy` says what each polygon *is*, and `bldg:lod2Solid` says how
//! the polygons make up the object — usually by `xlink:href`, in either
//! direction. Joining the two by `gml:id` is what makes both spellings work,
//! and it is why the order the two properties are written in does not matter.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value};

use super::attributes::read_common_attributes;
use super::{lod_of, read_geometry_property};
use crate::gml::XlinkRegistry;
use crate::model::{IntermediateGeometry, SemanticSurface};
use crate::xml::XmlNode;
use crate::{is_in, CityGmlError, ParseReport, Skipped};

/// Local name of a GML polygon, for the report.
pub(crate) const POLYGON: &str = "Polygon";

/// One property that may hold thematic surfaces, and the elements it may
/// hold.
pub(crate) struct SurfaceProperty {
    /// The property's local name, e.g. `boundedBy`.
    pub property: &'static str,
    /// The elements it may hold, each of which is a CityJSON semantic surface
    /// type spelled the same way.
    pub elements: &'static [&'static str],
}

/// Where one module writes its thematic surfaces, and what they may be.
pub(crate) struct SurfaceSpec {
    /// The namespaces of the module that defines them, 2.0 and 1.0.
    pub namespaces: &'static [&'static str],
    /// The properties that may hold a surface. Most modules have exactly one,
    /// `boundedBy`; transportation names one property per kind of area.
    pub properties: &'static [SurfaceProperty],
    /// The properties of a surface that hold an opening, and the openings
    /// themselves. Empty for every module but building: only a wall has
    /// windows.
    pub openings: &'static [SurfaceProperty],
    /// The property named in a report about surfaces that could not be
    /// placed — the property they are written under, or the first of them
    /// when a module has several.
    pub container: &'static str,
}

/// Read an object's thematic surfaces, and label the polygons of `geometries`
/// with the semantics they state.
///
/// This takes the object's node and its already-read geometries rather than
/// belonging to any one reader: every object that carries boundary surfaces
/// carries them the same way, whether it is a building, one of its parts, or
/// a road.
///
/// The order matters, and it is the caller's to keep: the geometries must
/// have been read *before* this runs, because this pass deduplicates its
/// diagnostics against the entries that one recorded. Reversed, one lost
/// polygon would be reported twice — which is also why this pass reports
/// through a scratch report; see [`merge_diagnostics`].
///
/// # Errors
///
/// Propagates the geometry reader's errors: malformed geometry, and
/// `xlink:href`s that name nothing in the member.
pub(crate) fn read_semantic_surfaces(
    node: &XmlNode,
    spec: &SurfaceSpec,
    registry: &XlinkRegistry,
    geometries: &mut [IntermediateGeometry],
    report: &mut ParseReport,
) -> Result<(), CityGmlError> {
    let mut boundary_report = ParseReport::default();
    let boundaries = read_boundary_surfaces(node, spec, registry, &mut boundary_report)?;
    merge_diagnostics(boundary_report, report);
    attach_semantics(boundaries, spec, geometries, report);
    Ok(())
}

/// Merge the boundary pass's diagnostics into the object's, leaving out the
/// ones already recorded.
///
/// The same polygon is parsed twice — once where it is defined, under the
/// boundary surface, and once where the object's geometry names it — and each
/// parse drops a degenerate or unsupported one on its own account. Both are
/// right, and both are the *same* loss: a report that named it twice would
/// count one lost polygon as two.
///
/// Only an entry that names a `gml:id` can be shown to be a repeat, because a
/// `gml:id` is unique within a document; two anonymous polygons dropped for
/// the same reason are two losses, and stay two entries. Warnings are appended
/// unchanged for the same reason — two surfaces may raise the same one
/// honestly, and there is no id to tell them apart.
///
/// The check is against the entries kept so far, so a repeat *within* the
/// boundary pass — the same polygon claimed by two boundary surfaces — is
/// caught too. It is quadratic in the number of skipped entries, which is a
/// count of what a document got *wrong*; a document where that is large has a
/// bigger problem than this loop.
fn merge_diagnostics(from: ParseReport, into: &mut ParseReport) {
    for skipped in from.skipped {
        let already_recorded = skipped.gml_id.is_some()
            && into.skipped.iter().any(|seen| {
                seen.gml_id == skipped.gml_id
                    && seen.element == skipped.element
                    && seen.reason == skipped.reason
            });
        if !already_recorded {
            into.skipped.push(skipped);
        }
    }
    into.warnings.extend(from.warnings);
}

/// The semantic surfaces an object's boundary surfaces state, grouped by the
/// level of detail whose geometry they describe.
///
/// The grouping is per LoD because a CityJSON geometry carries its own
/// `surfaces` list: a `WallSurface` with both a `lod2MultiSurface` and a
/// `lod3MultiSurface` describes two geometries, and each needs its own entry
/// and its own index.
#[derive(Debug, Default)]
struct BoundarySurfaces {
    /// LoD → what was read at that LoD. Ordered so that the report reads the
    /// same way twice for the same document.
    by_lod: BTreeMap<String, LodSurfaces>,
}

/// The semantic surfaces of one level of detail, and the polygons that carry
/// them.
#[derive(Debug, Default)]
struct LodSurfaces {
    /// The surfaces in the order they were written, which is the order their
    /// indices are in.
    surfaces: Vec<SemanticSurface>,
    /// `gml:id` of a polygon → the surface it belongs to. A polygon reaches
    /// the object's geometry by that id, whichever side of the reference it
    /// was written on.
    surface_of_polygon: HashMap<String, usize>,
}

impl BoundarySurfaces {
    /// Append `surface` to the list for `lod` and answer the index it took.
    fn push(&mut self, lod: &str, surface: SemanticSurface) -> usize {
        let lod_surfaces = self.by_lod.entry(lod.to_string()).or_default();
        lod_surfaces.surfaces.push(surface);
        lod_surfaces.surfaces.len() - 1
    }

    /// Record that the polygon with this `gml:id` belongs to surface `index`
    /// at `lod`.
    fn label(&mut self, lod: &str, polygon_id: &str, index: usize) {
        self.by_lod
            .entry(lod.to_string())
            .or_default()
            .surface_of_polygon
            .insert(polygon_id.to_string(), index);
    }

    /// Note that surface `child` opens surface `parent`.
    fn add_child(&mut self, lod: &str, parent: usize, child: usize) {
        if let Some(surface) = self
            .by_lod
            .get_mut(lod)
            .and_then(|lod_surfaces| lod_surfaces.surfaces.get_mut(parent))
        {
            surface.children.push(child);
        }
    }
}

/// Whether a node is one of the named elements of `spec`'s module.
///
/// The local name alone is not enough: an application schema may define a
/// `WallSurface` of its own, and it is not the CityGML one.
fn is_surface_element(node: &XmlNode, spec: &SurfaceSpec, locals: &[&str]) -> bool {
    spec.namespaces.contains(&node.ns.as_str()) && locals.contains(&node.local.as_str())
}

/// Read every thematic-surface property of an object.
fn read_boundary_surfaces(
    node: &XmlNode,
    spec: &SurfaceSpec,
    registry: &XlinkRegistry,
    report: &mut ParseReport,
) -> Result<BoundarySurfaces, CityGmlError> {
    let mut boundaries = BoundarySurfaces::default();
    for property in &node.children {
        let Some(kind) = spec
            .properties
            .iter()
            .find(|kind| is_in(property, spec.namespaces, kind.property))
        else {
            continue;
        };
        let mut read_any = false;
        for surface in &property.children {
            if !is_surface_element(surface, spec, kind.elements) {
                continue;
            }
            read_boundary_surface(surface, spec, registry, &mut boundaries, report)?;
            read_any = true;
        }
        if !read_any {
            // A property this reader took nothing from is content that was
            // lost: a surface type it does not know, or — the common case — a
            // reference to a boundary surface shared with another object,
            // which this converter does not follow.
            report.skipped.push(Skipped {
                element: property.local.clone(),
                gml_id: property.gml_id().map(str::to_owned),
                reason: format!(
                    "<{}> holds no boundary surface this reader knows",
                    property.local
                ),
            });
        }
    }
    Ok(boundaries)
}

/// One thematic surface or opening as it is being read: what it will become,
/// and where it has already become it.
///
/// The index is per LoD and not one index at all, because a single
/// `<bldg:WallSurface>` may hold geometry at several levels of detail, and
/// each of those is a CityJSON geometry with a `surfaces` list of its own.
struct PendingSurface {
    /// The CityJSON semantic surface type: the element's local name, which
    /// CityGML and CityJSON spell the same way.
    stype: String,
    attributes: Map<String, Value>,
    /// LoD → the index this surface took in that LoD's list.
    indices: HashMap<String, usize>,
}

impl PendingSurface {
    /// Read a thematic surface's or an opening's own attributes.
    fn read(node: &XmlNode, report: &mut ParseReport) -> Self {
        let mut attributes = Map::new();
        read_common_attributes(node, &mut attributes, report);
        Self {
            stype: node.local.clone(),
            attributes,
            indices: HashMap::new(),
        }
    }

    /// The index this surface takes at `lod`, creating its entry on first
    /// use — and, for an opening, linking it to the surface it opens from
    /// both ends.
    fn index_at(
        &mut self,
        lod: &str,
        parent: Option<usize>,
        boundaries: &mut BoundarySurfaces,
    ) -> usize {
        if let Some(index) = self.indices.get(lod) {
            return *index;
        }
        let mut surface = SemanticSurface::new(self.stype.clone());
        surface.attributes = self.attributes.clone();
        surface.parent = parent;
        let index = boundaries.push(lod, surface);
        if let Some(parent) = parent {
            boundaries.add_child(lod, parent, index);
        }
        self.indices.insert(lod.to_string(), index);
        index
    }
}

/// Read one thematic surface: the surface itself, then its openings.
fn read_boundary_surface(
    surface: &XmlNode,
    spec: &SurfaceSpec,
    registry: &XlinkRegistry,
    boundaries: &mut BoundarySurfaces,
    report: &mut ParseReport,
) -> Result<(), CityGmlError> {
    let mut pending = PendingSurface::read(surface, report);

    for property in &surface.children {
        let Some(lod) = lod_of(property) else {
            continue;
        };
        let index = pending.index_at(lod, None, boundaries);
        label_polygons(property, lod, index, registry, boundaries, report)?;
    }

    for property in &surface.children {
        let Some(kind) = spec
            .openings
            .iter()
            .find(|kind| is_in(property, spec.namespaces, kind.property))
        else {
            continue;
        };
        for opening in &property.children {
            if !is_surface_element(opening, spec, kind.elements) {
                continue;
            }
            read_opening(opening, &mut pending, registry, boundaries, report)?;
        }
    }
    Ok(())
}

/// Read one `bldg:Window` or `bldg:Door` into a semantic surface of its own.
///
/// An opening's surfaces are not the wall's: CityJSON gives each opening its
/// own entry and links the two with `parent`/`children` (§ 3.3). `opened` is
/// the surface it opens, and it is created at the opening's own LoD if it has
/// no entry there yet, so that the link always points inside the same
/// `surfaces` list.
fn read_opening(
    opening: &XmlNode,
    opened: &mut PendingSurface,
    registry: &XlinkRegistry,
    boundaries: &mut BoundarySurfaces,
    report: &mut ParseReport,
) -> Result<(), CityGmlError> {
    let mut pending = PendingSurface::read(opening, report);

    for property in &opening.children {
        let Some(lod) = lod_of(property) else {
            continue;
        };
        let parent = opened.index_at(lod, None, boundaries);
        let index = pending.index_at(lod, Some(parent), boundaries);
        label_polygons(property, lod, index, registry, boundaries, report)?;
    }
    Ok(())
}

/// Record every polygon of one `lodX…` property of a boundary surface as
/// belonging to semantic surface `index`.
///
/// The polygons themselves are not kept: they reach CityJSON through the
/// object's own geometry, and this only has to be able to recognise them there
/// — which is what a `gml:id` is for. A polygon written without one cannot be
/// recognised, so its semantics are lost and the loss is reported.
fn label_polygons(
    property: &XmlNode,
    lod: &str,
    index: usize,
    registry: &XlinkRegistry,
    boundaries: &mut BoundarySurfaces,
    report: &mut ParseReport,
) -> Result<(), CityGmlError> {
    let Some(geometry) = read_geometry_property(property, registry, report)? else {
        report.skipped.push(Skipped {
            element: property.local.clone(),
            gml_id: property.gml_id().map(str::to_owned),
            reason: format!("no supported GML geometry in <{}>", property.local),
        });
        return Ok(());
    };
    for polygon in geometry.polygons() {
        match &polygon.gml_id {
            Some(id) => boundaries.label(lod, id, index),
            None => report.skipped.push(Skipped {
                element: POLYGON.to_string(),
                gml_id: None,
                reason: format!(
                    "a polygon of <{}> has no gml:id, so no geometry can be matched to it; \
                     its semantics are dropped",
                    property.local
                ),
            }),
        }
    }
    Ok(())
}

/// Hand each LoD's semantic surfaces to the geometries at that LoD, and point
/// every polygon at the surface it belongs to.
///
/// A polygon whose `gml:id` no boundary surface claimed keeps
/// [`sem_idx`](crate::gml::Polygon3::sem_idx) `None`, which is CityJSON's
/// `null`: a surface with no semantics, not a surface with the wrong ones.
///
/// Boundary surfaces at an LoD the object has no geometry for describe nothing
/// this converter can write — CityJSON has no place for semantics without a
/// geometry to hang them on — so they are dropped and reported.
fn attach_semantics(
    boundaries: BoundarySurfaces,
    spec: &SurfaceSpec,
    geometries: &mut [IntermediateGeometry],
    report: &mut ParseReport,
) {
    for (lod, lod_surfaces) in boundaries.by_lod {
        let mut carried = false;
        for geometry in geometries.iter_mut().filter(|geometry| geometry.lod == lod) {
            for polygon in geometry.geometry.polygons_mut() {
                polygon.sem_idx = polygon
                    .gml_id
                    .as_deref()
                    .and_then(|id| lod_surfaces.surface_of_polygon.get(id))
                    .copied();
            }
            geometry.surfaces = lod_surfaces.surfaces.clone();
            carried = true;
        }
        if !carried {
            report.skipped.push(Skipped {
                element: spec.container.to_string(),
                gml_id: None,
                reason: format!(
                    "{} boundary surface(s) at LoD {lod}: the object has no LoD {lod} geometry \
                     to carry them, so their semantics are dropped",
                    lod_surfaces.surfaces.len()
                ),
            });
        }
    }
}
