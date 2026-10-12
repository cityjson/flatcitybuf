//! Flattening of a CityJSON geometry into the arrays FlatCityBuf stores.
//!
//! Every function here is **type-driven**: the nesting depth of `boundaries`,
//! of `material.values`, of `texture.values` and of `semantics.values` is read
//! off the `cjseq` variant, never inferred from which arrays happen to be
//! non-empty. See `crate::reader::geom_decoder` for the matching decode side
//! and for the table of depths per geometry type.

use cjseq::{
    Geometry as CjGeometry, Index, MaterialIndex, MaterialReference as CjMaterialReference,
    MultiSolidValues, PrimitiveValues, Ring, Semantics as CjSemantics,
    SemanticsSurface as CjSemanticsSurface, Shell, SolidValues, Surface, SurfaceIndex,
    TextureReference as CjTextureReference, TexturedRing, TexturedShell, TexturedSurface, Themes,
};

/// The wire spelling of "no value here": a `null` index in CityJSON, and — in
/// the `solids`/`shells` count arrays of a material mapping — a whole `null`
/// shell or solid, which `material.values` permits at every level.
const NULL: u32 = u32::MAX;

#[derive(Debug, Clone, Default)]
pub(crate) struct GMBoundaries {
    pub(crate) solids: Vec<u32>,   // Number of shells per solid
    pub(crate) shells: Vec<u32>,   // Number of surfaces per shell
    pub(crate) surfaces: Vec<u32>, // Number of rings per surface
    pub(crate) strings: Vec<u32>,  // Number of indices per ring
    pub(crate) indices: Vec<u32>,  // Flattened list of all indices
}

#[derive(Debug, Clone, Default)]
pub struct MaterialValues {
    pub(crate) theme: String,
    /// One entry per solid, holding that solid's shell count; `NULL` for a
    /// solid that is `null` in `material.values`.
    pub(crate) solids: Vec<u32>,
    /// One entry per shell, holding that shell's surface count; `NULL` for a
    /// shell that is `null` in `material.values`.
    pub(crate) shells: Vec<u32>,
    /// One material index per surface, `NULL` for a surface with no material.
    pub(crate) vertices: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct MaterialValue {
    pub(crate) theme: String,
    pub(crate) value: u32,
}

#[derive(Debug, Clone)]
pub enum MaterialMapping {
    Value(MaterialValue),
    Values(MaterialValues),
    /// `"values": null` — distinct from an absent `values`, which the schema
    /// treats as a different (and, on its own, invalid) document. Stored as a
    /// mapping that names its theme and carries no arrays at all.
    NullValues(String),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TextureMapping {
    pub(crate) theme: String,
    /// Whether the theme has a `values` member at all. A theme without one is
    /// valid CityJSON (the per-theme texture object has no `required`), and is
    /// stored as a mapping carrying no arrays.
    pub(crate) has_values: bool,
    pub(crate) solids: Vec<u32>,   // Number of shells per solid
    pub(crate) shells: Vec<u32>,   // Number of surfaces per shell
    pub(crate) surfaces: Vec<u32>, // Number of rings per surface
    pub(crate) strings: Vec<u32>,  // Number of indices per ring
    pub(crate) vertices: Vec<u32>, // Flattened list of all indices
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GMSemantics {
    pub(crate) surfaces: Vec<CjSemanticsSurface>, // List of semantic surfaces
    /// One semantic value per surface, or `None` for `"values": null`.
    ///
    /// `semantics.values` is required-but-nullable, so an explicit `null` is
    /// valid CityJSON and is *not* the same as an empty array: re-emitting it
    /// as `[]` (or, for a two-shell `Solid`, as `[[], []]`) produces a document
    /// the schema rejects. `None` is stored as an absent vector, exactly as
    /// `MaterialMapping::NullValues` and `TextureMapping::has_values` do.
    pub(crate) values: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Default)]
#[doc(hidden)]
pub(crate) struct EncodedGeometry {
    pub(crate) boundaries: GMBoundaries,
    pub(crate) semantics: Option<GMSemantics>,
    pub(crate) textures: Option<Vec<TextureMapping>>,
    pub(crate) materials: Option<Vec<MaterialMapping>>,
}

/// Flattens one geometry — its boundaries and whatever semantics, material and
/// texture it carries — into the FlatCityBuf arrays.
///
/// A [`CjGeometry::GeometryInstance`] carries none of these; it is encoded by
/// `serializer::to_geometry_instance` instead, and yields empty arrays here.
pub(crate) fn encode(geometry: &CjGeometry) -> EncodedGeometry {
    let boundaries = encode_boundaries(geometry);
    let (semantics, materials, textures) = match geometry {
        CjGeometry::MultiPoint { semantics, .. }
        | CjGeometry::MultiLineString { semantics, .. } => (
            semantics.as_ref().map(|s| encode_semantics(s, &boundaries)),
            None,
            None,
        ),
        CjGeometry::MultiSurface {
            semantics,
            material,
            texture,
            ..
        }
        | CjGeometry::CompositeSurface {
            semantics,
            material,
            texture,
            ..
        } => encode_appearance(semantics, material, texture, &boundaries),
        CjGeometry::Solid {
            semantics,
            material,
            texture,
            ..
        } => encode_appearance(semantics, material, texture, &boundaries),
        CjGeometry::MultiSolid {
            semantics,
            material,
            texture,
            ..
        }
        | CjGeometry::CompositeSolid {
            semantics,
            material,
            texture,
            ..
        } => encode_appearance(semantics, material, texture, &boundaries),
        CjGeometry::GeometryInstance { .. } => (None, None, None),
    };

    EncodedGeometry {
        boundaries,
        semantics,
        materials,
        textures,
    }
}

type EncodedAppearance = (
    Option<GMSemantics>,
    Option<Vec<MaterialMapping>>,
    Option<Vec<TextureMapping>>,
);

/// The semantics, material and texture of a surface or solid geometry, whose
/// value shapes `S`, `M` and `T` are fixed by its variant.
fn encode_appearance<S, M, T>(
    semantics: &Option<CjSemantics<S>>,
    material: &Option<Themes<CjMaterialReference<M>>>,
    texture: &Option<Themes<CjTextureReference<T>>>,
    boundaries: &GMBoundaries,
) -> EncodedAppearance
where
    S: NestedValues<SurfaceIndex>,
    M: NestedValues<MaterialIndex>,
    T: NestedTextures,
{
    (
        semantics.as_ref().map(|s| encode_semantics(s, boundaries)),
        material.as_ref().map(encode_material),
        texture.as_ref().map(encode_texture),
    )
}

// ---------------------------------------------------------------------------
// boundaries
// ---------------------------------------------------------------------------

fn push_ring(ring: &Ring, b: &mut GMBoundaries) {
    b.strings.push(ring.len() as u32);
    b.indices.extend(ring.iter().map(|i| i.get() as u32));
}

fn push_surface(surface: &Surface, b: &mut GMBoundaries) {
    for ring in surface {
        push_ring(ring, b);
    }
    b.surfaces.push(surface.len() as u32);
}

fn push_shell(shell: &Shell, b: &mut GMBoundaries) {
    for surface in shell {
        push_surface(surface, b);
    }
    b.shells.push(shell.len() as u32);
}

fn push_solid(solid: &[Shell], b: &mut GMBoundaries) {
    for shell in solid {
        push_shell(shell, b);
    }
    b.solids.push(solid.len() as u32);
}

/// Flattens `boundaries` at exactly the depth the geometry's type implies.
///
/// The outermost level is recorded as a single count one array up from where
/// the type sits — a `MultiSurface` writes its surface count into `shells`, a
/// `Solid` its shell count into `solids` — except for the 5-deep types, whose
/// outermost count is simply `solids.len()`.
pub(crate) fn encode_boundaries(geometry: &CjGeometry) -> GMBoundaries {
    let mut b = GMBoundaries::default();
    match geometry {
        CjGeometry::MultiPoint { boundaries, .. } => push_ring(boundaries, &mut b),
        CjGeometry::MultiLineString { boundaries, .. } => {
            for ring in boundaries {
                push_ring(ring, &mut b);
            }
            b.surfaces.push(boundaries.len() as u32);
        }
        CjGeometry::MultiSurface { boundaries, .. }
        | CjGeometry::CompositeSurface { boundaries, .. } => {
            for surface in boundaries {
                push_surface(surface, &mut b);
            }
            b.shells.push(boundaries.len() as u32);
        }
        CjGeometry::Solid { boundaries, .. } => push_solid(boundaries, &mut b),
        CjGeometry::MultiSolid { boundaries, .. }
        | CjGeometry::CompositeSolid { boundaries, .. } => {
            for solid in boundaries {
                push_solid(solid, &mut b);
            }
        }
        // Encoded by `to_geometry_instance`, which writes the single reference
        // vertex index directly.
        CjGeometry::GeometryInstance { .. } => {}
    }
    b
}

// ---------------------------------------------------------------------------
// semantics and material values
// ---------------------------------------------------------------------------

fn leaf<I: Index>(i: Option<I>) -> u32 {
    i.map_or(NULL, |v| v.get() as u32)
}

/// The three shapes `semantics.values` and `material.values` come in —
/// per surface, per shell, per solid — and how each one flattens. `I` is the
/// kind of index at the leaves.
pub(crate) trait NestedValues<I> {
    /// The material layout: a count per solid and per shell, `NULL` for a
    /// solid or shell that is `null`, and one leaf per surface.
    fn flatten_counted(&self, solids: &mut Vec<u32>, shells: &mut Vec<u32>, leaves: &mut Vec<u32>);

    /// The semantics layout: leaves only, regrouped on decode by the boundary
    /// counts. A semantics mapping has no count arrays of its own, so a `null`
    /// shell or solid cannot be stored as such; it is expanded to a run of
    /// `NULL` leaves as long as the boundaries say, which means the same thing
    /// (nothing in it is semantically tagged) but does not round-trip its
    /// spelling.
    fn flatten_by_boundaries(&self, boundaries: &GMBoundaries, leaves: &mut Vec<u32>);
}

impl<I: Index> NestedValues<I> for PrimitiveValues<I> {
    fn flatten_counted(&self, _: &mut Vec<u32>, _: &mut Vec<u32>, leaves: &mut Vec<u32>) {
        leaves.extend(self.iter().copied().map(leaf));
    }

    fn flatten_by_boundaries(&self, _: &GMBoundaries, leaves: &mut Vec<u32>) {
        leaves.extend(self.iter().copied().map(leaf));
    }
}

impl<I: Index> NestedValues<I> for SolidValues<I> {
    fn flatten_counted(&self, solids: &mut Vec<u32>, shells: &mut Vec<u32>, leaves: &mut Vec<u32>) {
        solids.push(self.len() as u32);
        for shell in self {
            push_counted_shell(shell.as_deref(), shells, leaves);
        }
    }

    fn flatten_by_boundaries(&self, boundaries: &GMBoundaries, leaves: &mut Vec<u32>) {
        let mut shell_cursor = 0;
        for shell in self {
            push_boundary_shell(shell.as_deref(), boundaries, &mut shell_cursor, leaves);
        }
    }
}

impl<I: Index> NestedValues<I> for MultiSolidValues<I> {
    fn flatten_counted(&self, solids: &mut Vec<u32>, shells: &mut Vec<u32>, leaves: &mut Vec<u32>) {
        for solid in self {
            match solid {
                Some(solid) => {
                    solids.push(solid.len() as u32);
                    for shell in solid {
                        push_counted_shell(shell.as_deref(), shells, leaves);
                    }
                }
                None => solids.push(NULL),
            }
        }
    }

    fn flatten_by_boundaries(&self, boundaries: &GMBoundaries, leaves: &mut Vec<u32>) {
        let mut shell_cursor = 0;
        for (i, solid) in self.iter().enumerate() {
            match solid {
                Some(solid) => {
                    for shell in solid {
                        push_boundary_shell(
                            shell.as_deref(),
                            boundaries,
                            &mut shell_cursor,
                            leaves,
                        );
                    }
                }
                None => {
                    let shell_count = boundaries.solids.get(i).copied().unwrap_or(0);
                    for _ in 0..shell_count {
                        push_boundary_shell::<I>(None, boundaries, &mut shell_cursor, leaves);
                    }
                }
            }
        }
    }
}

fn push_counted_shell<I: Index>(
    shell: Option<&[Option<I>]>,
    shells: &mut Vec<u32>,
    leaves: &mut Vec<u32>,
) {
    match shell {
        Some(indices) => {
            shells.push(indices.len() as u32);
            leaves.extend(indices.iter().copied().map(leaf));
        }
        None => shells.push(NULL),
    }
}

fn push_boundary_shell<I: Index>(
    shell: Option<&[Option<I>]>,
    boundaries: &GMBoundaries,
    shell_cursor: &mut usize,
    leaves: &mut Vec<u32>,
) {
    let surface_count = boundaries.shells.get(*shell_cursor).copied().unwrap_or(0) as usize;
    *shell_cursor += 1;
    match shell {
        Some(indices) => leaves.extend(indices.iter().copied().map(leaf)),
        None => leaves.extend(std::iter::repeat_n(NULL, surface_count)),
    }
}

// ---------------------------------------------------------------------------
// material
// ---------------------------------------------------------------------------

/// Flattens each theme's `material`: a `value`, or `values` at the depth its
/// shape declares.
///
/// A `null` shell or solid — legal at every level of `material.values` — is
/// written as a `NULL` count in `shells`/`solids` rather than being dropped,
/// so it decodes back as `null` and not as an empty array (finding #7).
pub(crate) fn encode_material<V: NestedValues<MaterialIndex>>(
    materials: &Themes<CjMaterialReference<V>>,
) -> Vec<MaterialMapping> {
    let mut material_mappings = Vec::new();
    for (theme, material) in sorted_themes(materials) {
        if let Some(value) = material.value {
            material_mappings.push(MaterialMapping::Value(MaterialValue {
                theme: theme.clone(),
                value: value.get() as u32,
            }));
            continue;
        }

        // The outer `Option` is present-vs-absent, the inner `null`-vs-array.
        let values = match material.values.as_ref() {
            Some(Some(values)) => values,
            Some(None) => {
                material_mappings.push(MaterialMapping::NullValues(theme.clone()));
                continue;
            }
            // Neither `value` nor `values`: nothing to store, and nothing the
            // schema would accept either.
            None => continue,
        };

        let mut mv = MaterialValues {
            theme: theme.clone(),
            ..Default::default()
        };
        values.flatten_counted(&mut mv.solids, &mut mv.shells, &mut mv.vertices);
        material_mappings.push(MaterialMapping::Values(mv));
    }
    material_mappings
}

/// Themes in name order: the mappings are written in this order whatever
/// order the document lists its themes in.
fn sorted_themes<T>(themes: &Themes<T>) -> Vec<(&String, &T)> {
    let mut sorted: Vec<(&String, &T)> = themes.iter().collect();
    sorted.sort_unstable_by(|a, b| a.0.cmp(b.0));
    sorted
}

// ---------------------------------------------------------------------------
// texture
// ---------------------------------------------------------------------------

/// The three shapes `texture.values` comes in, nested exactly as deeply as
/// the boundaries, and how each one flattens. This mirrors
/// [`encode_boundaries`] level for level.
pub(crate) trait NestedTextures {
    fn flatten(&self, m: &mut TextureMapping);
}

impl NestedTextures for Vec<TexturedSurface> {
    fn flatten(&self, m: &mut TextureMapping) {
        for surface in self {
            push_textured_surface(surface, m);
        }
        m.shells.push(self.len() as u32);
    }
}

impl NestedTextures for Vec<TexturedShell> {
    fn flatten(&self, m: &mut TextureMapping) {
        for shell in self {
            push_textured_shell(shell, m);
        }
        m.solids.push(self.len() as u32);
    }
}

impl NestedTextures for Vec<Vec<TexturedShell>> {
    fn flatten(&self, m: &mut TextureMapping) {
        for solid in self {
            for shell in solid {
                push_textured_shell(shell, m);
            }
            m.solids.push(solid.len() as u32);
        }
    }
}

pub(crate) fn encode_texture<V: NestedTextures>(
    texture_map: &Themes<CjTextureReference<V>>,
) -> Vec<TextureMapping> {
    sorted_themes(texture_map)
        .into_iter()
        .map(|(theme, texture)| {
            let mut mapping = TextureMapping {
                theme: theme.clone(),
                ..Default::default()
            };
            // A theme may legally carry no `values` at all, which flattens to
            // nothing but still records the theme.
            if let Some(values) = texture.values.as_ref() {
                mapping.has_values = true;
                values.flatten(&mut mapping);
            }
            mapping
        })
        .collect()
}

/// A ring is stored as it is spelled: `[null]`, or `[texture, uv, ...]`.
fn push_textured_ring(ring: &TexturedRing, m: &mut TextureMapping) {
    match ring {
        TexturedRing::Untextured => {
            m.strings.push(1);
            m.vertices.push(NULL);
        }
        TexturedRing::Textured { texture, uvs } => {
            m.strings.push(1 + uvs.len() as u32);
            m.vertices.push(texture.get() as u32);
            m.vertices.extend(uvs.iter().map(|uv| uv.get() as u32));
        }
    }
}

fn push_textured_surface(surface: &TexturedSurface, m: &mut TextureMapping) {
    for ring in surface {
        push_textured_ring(ring, m);
    }
    m.surfaces.push(surface.len() as u32);
}

fn push_textured_shell(shell: &TexturedShell, m: &mut TextureMapping) {
    for surface in shell {
        push_textured_surface(surface, m);
    }
    m.shells.push(shell.len() as u32);
}

// ---------------------------------------------------------------------------
// semantics
// ---------------------------------------------------------------------------

/// Flattens a `semantics` member: its surfaces verbatim, its values one entry
/// per surface — or no vector at all for `"values": null`.
pub(crate) fn encode_semantics<V: NestedValues<SurfaceIndex>>(
    semantics: &CjSemantics<V>,
    boundaries: &GMBoundaries,
) -> GMSemantics {
    let values = semantics.values.as_ref().map(|v| {
        let mut values = Vec::new();
        v.flatten_by_boundaries(boundaries, &mut values);
        values
    });

    GMSemantics {
        surfaces: semantics.surfaces.to_vec(),
        values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use cjseq::SemanticSurfaceType;
    use pretty_assertions::assert_eq;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn theme_of(m: &MaterialMapping) -> &str {
        match m {
            MaterialMapping::Value(v) => &v.theme,
            MaterialMapping::Values(v) => &v.theme,
            MaterialMapping::NullValues(theme) => theme,
        }
    }

    fn geom(v: serde_json::Value) -> CjGeometry {
        serde_json::from_value(v).expect("test geometry must parse")
    }

    /// A theme map of material or texture references, from its JSON.
    fn themes<T: DeserializeOwned>(v: serde_json::Value) -> Themes<T> {
        serde_json::from_value(v).expect("test themes must parse")
    }

    fn object(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        v.as_object().expect("an object").clone()
    }

    #[test]
    fn test_encode_boundaries() -> Result<()> {
        // MultiPoint
        let encoded = encode(&geom(json!({
            "type": "MultiPoint", "lod": "1", "boundaries": [2, 44, 0, 7]
        })));
        assert_eq!(vec![2, 44, 0, 7], encoded.boundaries.indices);
        assert_eq!(vec![4], encoded.boundaries.strings);
        assert!(encoded.boundaries.surfaces.is_empty());
        assert!(encoded.boundaries.shells.is_empty());
        assert!(encoded.boundaries.solids.is_empty());

        // MultiLineString
        let encoded = encode(&geom(json!({
            "type": "MultiLineString", "lod": "1", "boundaries": [[2, 3, 5], [77, 55, 212]]
        })));
        assert_eq!(vec![2, 3, 5, 77, 55, 212], encoded.boundaries.indices);
        assert_eq!(vec![3, 3], encoded.boundaries.strings);
        assert_eq!(vec![2], encoded.boundaries.surfaces);
        assert!(encoded.boundaries.shells.is_empty());
        assert!(encoded.boundaries.solids.is_empty());

        // MultiSurface
        let encoded = encode(&geom(json!({
            "type": "MultiSurface", "lod": "1",
            "boundaries": [[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]]
        })));
        assert_eq!(
            vec![0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4],
            encoded.boundaries.indices
        );
        assert_eq!(vec![4, 4, 4], encoded.boundaries.strings);
        assert_eq!(vec![1, 1, 1], encoded.boundaries.surfaces);
        assert_eq!(vec![3], encoded.boundaries.shells);
        assert!(encoded.boundaries.solids.is_empty());

        // Solid
        let encoded = encode(&geom(json!({
            "type": "Solid", "lod": "1",
            "boundaries": [
                [
                    [[0, 3, 2, 1, 22], [1, 2, 3, 4]],
                    [[4, 5, 6, 7]],
                    [[0, 1, 5, 4]],
                    [[1, 2, 6, 5]]
                ],
                [
                    [[240, 243, 124]],
                    [[244, 246, 724]],
                    [[34, 414, 45]],
                    [[111, 246, 5]]
                ]
            ]
        })));
        assert_eq!(
            vec![
                0, 3, 2, 1, 22, 1, 2, 3, 4, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244,
                246, 724, 34, 414, 45, 111, 246, 5
            ],
            encoded.boundaries.indices
        );
        assert_eq!(vec![5, 4, 4, 4, 4, 3, 3, 3, 3], encoded.boundaries.strings);
        assert_eq!(vec![2, 1, 1, 1, 1, 1, 1, 1], encoded.boundaries.surfaces);
        assert_eq!(vec![4, 4], encoded.boundaries.shells);
        assert_eq!(vec![2], encoded.boundaries.solids);

        // CompositeSolid
        let encoded = encode(&geom(json!({
            "type": "CompositeSolid", "lod": "1",
            "boundaries": [
                [
                    [
                        [[0, 3, 2, 1, 22]],
                        [[4, 5, 6, 7]],
                        [[0, 1, 5, 4]],
                        [[1, 2, 6, 5]]
                    ],
                    [
                        [[240, 243, 124]],
                        [[244, 246, 724]],
                        [[34, 414, 45]],
                        [[111, 246, 5]]
                    ]
                ],
                [[
                    [[666, 667, 668]],
                    [[74, 75, 76]],
                    [[880, 881, 885]],
                    [[111, 122, 226]]
                ]]
            ]
        })));
        assert_eq!(
            vec![
                0, 3, 2, 1, 22, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244, 246, 724,
                34, 414, 45, 111, 246, 5, 666, 667, 668, 74, 75, 76, 880, 881, 885, 111, 122, 226
            ],
            encoded.boundaries.indices
        );
        assert_eq!(
            encoded.boundaries.strings,
            vec![5, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3]
        );
        assert_eq!(
            encoded.boundaries.surfaces,
            vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]
        );
        assert_eq!(encoded.boundaries.shells, vec![4, 4, 4]);
        assert_eq!(encoded.boundaries.solids, vec![2, 1]);

        Ok(())
    }

    #[test]
    fn test_encode_semantics() -> Result<()> {
        //MultiSurface
        let multi_surface = geom(json!({
            "type": "MultiSurface",
            "lod": "2",
            "boundaries": [
              [[0, 3, 2, 1]],
              [[4, 5, 6, 7]],
              [[0, 1, 5, 4]],
              [[0, 2, 3, 8]],
              [[10, 12, 23, 48]]
            ],
            "semantics": {
              "surfaces": [
                {"type": "WallSurface", "slope": 33.4, "children": [2]},
                {"type": "RoofSurface", "slope": 66.6},
                {"type": "OuterCeilingSurface", "parent": 0, "colour": "blue"}
              ],
              "values": [0, 0, null, 1, 2]
            }
        }));
        let encoded = encode(&multi_surface);
        let encoded_semantics = encoded.semantics.expect("semantics must be encoded");

        let expected_semantics_surfaces = vec![
            CjSemanticsSurface {
                thetype: SemanticSurfaceType::WallSurface,
                parent: None,
                children: Some(vec![SurfaceIndex(2)]),
                other: object(json!({"slope": 33.4})),
            },
            CjSemanticsSurface {
                thetype: SemanticSurfaceType::RoofSurface,
                parent: None,
                children: None,
                other: object(json!({"slope": 66.6})),
            },
            CjSemanticsSurface {
                thetype: SemanticSurfaceType::OuterCeilingSurface,
                parent: Some(SurfaceIndex(0)),
                children: None,
                other: object(json!({"colour": "blue"})),
            },
        ];

        assert_eq!(expected_semantics_surfaces, encoded_semantics.surfaces);
        assert_eq!(Some(vec![0, 0, NULL, 1, 2]), encoded_semantics.values);

        //CompositeSolid
        let composite_solid = geom(json!({
          "type": "CompositeSolid",
          "lod": "2.2",
          "boundaries": [
            [[
              [[0, 3, 2, 1, 22]],
              [[4, 5, 6, 7]],
              [[0, 1, 5, 4]],
              [[1, 2, 6, 5]]
            ]],
            [[
              [[666, 667, 668]],
              [[74, 75, 76]],
              [[880, 881, 885]]
            ]]
          ],
          "semantics": {
            "surfaces": [{"type": "RoofSurface"}, {"type": "WallSurface"}],
            "values": [[[0, 1, 1, null]], [[null, null, null]]]
          }
        }));
        let encoded = encode(&composite_solid);
        let encoded_semantics = encoded.semantics.expect("semantics must be encoded");

        let expected_semantics_surfaces = vec![
            CjSemanticsSurface {
                thetype: SemanticSurfaceType::RoofSurface,
                parent: None,
                children: None,
                other: serde_json::Map::new(),
            },
            CjSemanticsSurface {
                thetype: SemanticSurfaceType::WallSurface,
                parent: None,
                children: None,
                other: serde_json::Map::new(),
            },
        ];

        assert_eq!(expected_semantics_surfaces, encoded_semantics.surfaces);
        assert_eq!(
            Some(vec![0, 1, 1, NULL, NULL, NULL, NULL]),
            encoded_semantics.values
        );
        Ok(())
    }

    /// A `null` shell in `semantics.values` cannot be stored as such — the
    /// decoder regroups the flat run using the boundary counts — so it is
    /// expanded to one `null` per surface of the corresponding boundary shell.
    #[test]
    fn a_null_semantics_shell_expands_to_one_null_per_surface() {
        let solid = geom(json!({
            "type": "Solid", "lod": "2",
            "boundaries": [
                [[[0, 1, 2]], [[3, 4, 5]]],
                [[[6, 7, 8]]]
            ],
            "semantics": {
                "surfaces": [{"type": "RoofSurface"}],
                "values": [[0, 0], null]
            }
        }));
        let encoded = encode(&solid);
        let semantics = encoded.semantics.expect("semantics must be encoded");
        // Two surfaces in the first shell, one in the second.
        assert_eq!(semantics.values, Some(vec![0, 0, NULL]));
    }

    #[test]
    fn test_encode_material() {
        type Surfaces = CjMaterialReference<PrimitiveValues<MaterialIndex>>;
        type Shells = CjMaterialReference<SolidValues<MaterialIndex>>;
        type Solids = CjMaterialReference<MultiSolidValues<MaterialIndex>>;

        // A `value` colours the whole geometry.
        let encoded = encode_material(&themes::<Shells>(json!({"theme1": {"value": 5}})));
        assert_eq!(encoded.len(), 1);
        match &encoded[0] {
            MaterialMapping::Value(value) => {
                assert_eq!(value.theme, "theme1");
                assert_eq!(value.value, 5);
            }
            _ => panic!("Expected MaterialMapping::Value"),
        }

        // MultiSurface: one index per surface, no counts.
        let encoded = encode_material(&themes::<Surfaces>(
            json!({"theme2": {"values": [0, 1, null, 2]}}),
        ));
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.vertices, vec![0, 1, NULL, 2]);
                assert!(values.shells.is_empty());
                assert!(values.solids.is_empty());
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        // Solid: one solid of two shells of three surfaces.
        let encoded = encode_material(&themes::<Shells>(
            json!({"theme3": {"values": [[0, 1, null], [2, 3, 4]]}}),
        ));
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.solids, vec![2]);
                assert_eq!(values.shells, vec![3, 3]);
                assert_eq!(values.vertices, vec![0, 1, NULL, 2, 3, 4]);
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        // CompositeSolid: two solids, of two shells and of one.
        let encoded = encode_material(&themes::<Solids>(json!({"theme6": {
            "values": [[[0, 1, null], [2, null, null]], [[3, 4, null]]]
        }})));
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.solids, vec![2, 1]);
                assert_eq!(values.shells, vec![3, 3, 3]);
                assert_eq!(values.vertices, vec![0, 1, NULL, 2, NULL, NULL, 3, 4, NULL]);
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        // Themes are written in name order, whatever order the document gives.
        let encoded = encode_material(&themes::<Surfaces>(json!({
            "winter": {"value": 7},
            "summer": {"values": [8, 9]}
        })));
        let names: Vec<&str> = encoded.iter().map(theme_of).collect();
        assert_eq!(names, vec!["summer", "winter"]);
    }

    /// `material.values` is nullable at *every* level, not only at the leaf.
    /// A whole `null` shell or solid is recorded as a `NULL` count so that it
    /// decodes back as `null` rather than as an empty array.
    #[test]
    fn a_null_material_shell_or_solid_is_recorded_as_a_null_count() {
        let materials = themes::<CjMaterialReference<SolidValues<MaterialIndex>>>(
            json!({"t": {"values": [[0, 1], null]}}),
        );
        match &encode_material(&materials)[0] {
            MaterialMapping::Values(v) => {
                assert_eq!(v.solids, vec![2]);
                assert_eq!(v.shells, vec![2, NULL]);
                assert_eq!(v.vertices, vec![0, 1]);
            }
            _ => panic!("expected Values"),
        }

        let materials = themes::<CjMaterialReference<MultiSolidValues<MaterialIndex>>>(
            json!({"t": {"values": [[[0, 1]], null]}}),
        );
        match &encode_material(&materials)[0] {
            MaterialMapping::Values(v) => {
                assert_eq!(v.solids, vec![1, NULL]);
                assert_eq!(v.shells, vec![2]);
                assert_eq!(v.vertices, vec![0, 1]);
            }
            _ => panic!("expected Values"),
        }
    }

    /// Each ring is stored as spelled, `[texture, uv, ...]` or `[null]`, with
    /// the counts mirroring the boundaries level for level.
    #[test]
    fn test_encode_texture() {
        // MultiSurface
        let encoded = encode_texture(&themes::<CjTextureReference<Vec<TexturedSurface>>>(
            json!({"t": {"values": [[[0, 10, 20, 30]], [[null]], [[2, 12, 22, 32]]]}}),
        ));
        assert_eq!(
            encoded[0].vertices,
            vec![0, 10, 20, 30, NULL, 2, 12, 22, 32]
        );
        assert_eq!(encoded[0].strings, vec![4, 1, 4]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![3]);
        assert!(encoded[0].solids.is_empty());

        // Solid
        let encoded = encode_texture(&themes::<CjTextureReference<Vec<TexturedShell>>>(
            json!({"t": {"values": [[[[0, 10, 20, 30]], [[null]]], [[[3, 13, 23, 33]]]]}}),
        ));
        assert_eq!(
            encoded[0].vertices,
            vec![0, 10, 20, 30, NULL, 3, 13, 23, 33]
        );
        assert_eq!(encoded[0].strings, vec![4, 1, 4]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![2, 1]);
        assert_eq!(encoded[0].solids, vec![2]);

        // CompositeSolid
        let encoded = encode_texture(&themes::<CjTextureReference<Vec<Vec<TexturedShell>>>>(
            json!({"t": {"values": [[[[[0, 10, 20]]], [[[null]]]], [[[[4, 14, 24]]]]]}}),
        ));
        assert_eq!(encoded[0].vertices, vec![0, 10, 20, NULL, 4, 14, 24]);
        assert_eq!(encoded[0].strings, vec![3, 1, 3]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![1, 1, 1]);
        assert_eq!(encoded[0].solids, vec![2, 1]);
    }
}
