//! Flattening of a CityJSON geometry into the arrays FlatCityBuf stores.
//!
//! Every function here is **type-driven**: the nesting depth of `boundaries`,
//! of `material.values`, of `texture.values` and of `semantics.values` is read
//! off the `cjseq` variant, never inferred from which arrays happen to be
//! non-empty. See `crate::reader::geom_decoder` for the matching decode side
//! and for the table of depths per geometry type.

use cjseq::{
    Geometry as CjGeometry, MaterialReference as CjMaterialReference,
    MaterialValues as CjMaterialValues, Ring, Semantics as CjSemantics,
    SemanticsSurface as CjSemanticsSurface, SemanticsValues as CjSemanticsValues, Shell, Surface,
    TextureReference as CjTextureReference, TextureValues as CjTextureValues, TexturedShell,
    TexturedSurface,
};
use std::collections::HashMap;

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

    let common = geometry.common();
    let semantics = common
        .and_then(|c| c.semantics.as_ref())
        .map(|s| encode_semantics(s, &boundaries));
    let textures = common.and_then(|c| c.texture.as_ref()).map(encode_texture);
    let materials = common
        .and_then(|c| c.material.as_ref())
        .map(encode_material);

    EncodedGeometry {
        boundaries,
        semantics,
        materials,
        textures,
    }
}

// ---------------------------------------------------------------------------
// boundaries
// ---------------------------------------------------------------------------

fn push_ring(ring: &Ring, b: &mut GMBoundaries) {
    b.strings.push(ring.len() as u32);
    b.indices.extend(ring.iter().map(|&i| i as u32));
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
/// outermost count is simply `solids.len()`. This is the shape the format has
/// always had; it is reproduced here without the depth inference that used to
/// produce it.
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
// material
// ---------------------------------------------------------------------------

fn material_index(i: Option<usize>) -> u32 {
    i.map_or(NULL, |v| v as u32)
}

/// Flattens one theme's `material.values` at the depth its variant declares.
///
/// A `null` shell or solid — legal at every level of `material.values` — is
/// written as a `NULL` count in `shells`/`solids` rather than being dropped,
/// so it decodes back as `null` and not as an empty array (finding #7).
pub(crate) fn encode_material(
    materials: &HashMap<String, CjMaterialReference>,
) -> Vec<MaterialMapping> {
    let mut material_mappings = Vec::new();
    for (theme, material) in materials {
        if let Some(value) = material.value {
            material_mappings.push(MaterialMapping::Value(MaterialValue {
                theme: theme.clone(),
                value: value as u32,
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

        match values {
            // MultiSurface, CompositeSurface: one index per surface.
            CjMaterialValues::Surfaces(surfaces) => {
                mv.vertices
                    .extend(surfaces.iter().copied().map(material_index));
            }
            // Solid: one index per surface, per shell.
            CjMaterialValues::Shells(shells) => {
                mv.solids.push(shells.len() as u32);
                for shell in shells {
                    push_material_shell(shell.as_deref(), &mut mv);
                }
            }
            // MultiSolid, CompositeSolid: ... per solid.
            CjMaterialValues::Solids(solids) => {
                for solid in solids {
                    match solid {
                        Some(shells) => {
                            mv.solids.push(shells.len() as u32);
                            for shell in shells {
                                push_material_shell(shell.as_deref(), &mut mv);
                            }
                        }
                        None => mv.solids.push(NULL),
                    }
                }
            }
        }

        material_mappings.push(MaterialMapping::Values(mv));
    }
    material_mappings
}

fn push_material_shell(shell: Option<&[Option<usize>]>, mv: &mut MaterialValues) {
    match shell {
        Some(indices) => {
            mv.shells.push(indices.len() as u32);
            mv.vertices
                .extend(indices.iter().copied().map(material_index));
        }
        None => mv.shells.push(NULL),
    }
}

// ---------------------------------------------------------------------------
// texture
// ---------------------------------------------------------------------------

pub(crate) fn encode_texture(
    texture_map: &HashMap<String, CjTextureReference>,
) -> Vec<TextureMapping> {
    let mut texture_mappings = Vec::new();

    for (theme, texture) in texture_map {
        let mut mapping = TextureMapping {
            theme: theme.clone(),
            ..Default::default()
        };

        // A theme may legally carry no `values` at all, which flattens to
        // nothing but still records the theme.
        if let Some(values) = texture.values.as_ref() {
            mapping.has_values = true;
            encode_texture_values(values, &mut mapping);
        }

        texture_mappings.push(mapping);
    }

    texture_mappings
}

fn push_textured_surface(surface: &TexturedSurface, m: &mut TextureMapping) {
    for ring in surface {
        m.strings.push(ring.len() as u32);
        m.vertices.extend(ring.iter().copied().map(material_index));
    }
    m.surfaces.push(surface.len() as u32);
}

fn push_textured_shell(shell: &TexturedShell, m: &mut TextureMapping) {
    for surface in shell {
        push_textured_surface(surface, m);
    }
    m.shells.push(shell.len() as u32);
}

/// Flattens one theme's `texture.values` at the depth its variant declares.
/// `texture.values` is nested exactly as deeply as the geometry's boundaries,
/// so this mirrors [`encode_boundaries`] level for level.
fn encode_texture_values(values: &CjTextureValues, m: &mut TextureMapping) {
    match values {
        CjTextureValues::Surface(surfaces) => {
            for surface in surfaces {
                push_textured_surface(surface, m);
            }
            m.shells.push(surfaces.len() as u32);
        }
        CjTextureValues::Shell(shells) => {
            for shell in shells {
                push_textured_shell(shell, m);
            }
            m.solids.push(shells.len() as u32);
        }
        CjTextureValues::Solid(solids) => {
            for solid in solids {
                for shell in solid {
                    push_textured_shell(shell, m);
                }
                m.solids.push(solid.len() as u32);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// semantics
// ---------------------------------------------------------------------------

fn semantics_index(i: Option<usize>) -> u32 {
    i.map_or(NULL, |v| v as u32)
}

/// Flattens `semantics.values` into one entry per surface, in document order.
///
/// The depth comes from the `SemanticsValues` variant. Unlike a material
/// mapping, a semantics mapping has no count arrays of its own — the decoder
/// regroups the flat run using the *boundary* counts — so a `null` shell or
/// solid cannot be stored as such. It is expanded to a run of `null` surface
/// values of the right length instead, which says the same thing (nothing in
/// here is semantically tagged) but does not round-trip its spelling.
fn encode_semantics_values(
    values: &CjSemanticsValues,
    boundaries: &GMBoundaries,
    flattened: &mut Vec<u32>,
) {
    match values {
        CjSemanticsValues::Surfaces(surfaces) => {
            flattened.extend(surfaces.iter().copied().map(semantics_index));
        }
        CjSemanticsValues::Shells(shells) => {
            let mut shell_cursor = 0;
            for shell in shells {
                push_semantics_shell(shell.as_deref(), boundaries, &mut shell_cursor, flattened);
            }
        }
        CjSemanticsValues::Solids(solids) => {
            let mut shell_cursor = 0;
            for (i, solid) in solids.iter().enumerate() {
                let shell_count = boundaries.solids.get(i).copied().unwrap_or(0) as usize;
                match solid {
                    Some(shells) => {
                        for shell in shells {
                            push_semantics_shell(
                                shell.as_deref(),
                                boundaries,
                                &mut shell_cursor,
                                flattened,
                            );
                        }
                    }
                    None => {
                        for _ in 0..shell_count {
                            push_semantics_shell(None, boundaries, &mut shell_cursor, flattened);
                        }
                    }
                }
            }
        }
    }
}

fn push_semantics_shell(
    shell: Option<&[Option<usize>]>,
    boundaries: &GMBoundaries,
    shell_cursor: &mut usize,
    flattened: &mut Vec<u32>,
) {
    let surface_count = boundaries.shells.get(*shell_cursor).copied().unwrap_or(0) as usize;
    *shell_cursor += 1;
    match shell {
        Some(indices) => flattened.extend(indices.iter().copied().map(semantics_index)),
        None => flattened.extend(std::iter::repeat_n(NULL, surface_count)),
    }
}

/// Flattens a `semantics` member: its surfaces verbatim, its values one entry
/// per surface — or no vector at all for `"values": null`.
pub(crate) fn encode_semantics(semantics: &CjSemantics, boundaries: &GMBoundaries) -> GMSemantics {
    let values = semantics.values.as_ref().map(|v| {
        let mut values = Vec::new();
        encode_semantics_values(v, boundaries, &mut values);
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

    /// A `MultiSurface` and a `CompositeSurface` with identical boundaries
    /// flatten identically, and so do a `MultiSolid` and a `CompositeSolid`:
    /// nothing about the encoding distinguishes the members of a depth pair.
    /// The type is what tells them apart, and it is stored separately.
    #[test]
    fn types_of_equal_depth_flatten_identically() {
        let surface_boundaries = json!([[[0, 1, 2]], [[3, 4, 5]]]);
        let ms = encode(&geom(
            json!({"type": "MultiSurface", "boundaries": surface_boundaries}),
        ));
        let cs = encode(&geom(
            json!({"type": "CompositeSurface", "boundaries": surface_boundaries}),
        ));
        assert_eq!(ms.boundaries.shells, cs.boundaries.shells);
        assert_eq!(ms.boundaries.surfaces, cs.boundaries.surfaces);
        assert_eq!(ms.boundaries.indices, cs.boundaries.indices);

        let solid_boundaries = json!([[[[[0, 1, 2]]]], [[[[3, 4, 5]]]]]);
        let msol = encode(&geom(
            json!({"type": "MultiSolid", "boundaries": solid_boundaries}),
        ));
        let csol = encode(&geom(
            json!({"type": "CompositeSolid", "boundaries": solid_boundaries}),
        ));
        assert_eq!(msol.boundaries.solids, csol.boundaries.solids);
        assert_eq!(msol.boundaries.shells, csol.boundaries.shells);
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
                children: Some(vec![2]),
                other: HashMap::from([("slope".to_string(), json!(33.4))]),
            },
            CjSemanticsSurface {
                thetype: SemanticSurfaceType::RoofSurface,
                parent: None,
                children: None,
                other: HashMap::from([("slope".to_string(), json!(66.6))]),
            },
            CjSemanticsSurface {
                thetype: SemanticSurfaceType::OuterCeilingSurface,
                parent: Some(0),
                children: None,
                other: HashMap::from([("colour".to_string(), json!("blue"))]),
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
                other: HashMap::new(),
            },
            CjSemanticsSurface {
                thetype: SemanticSurfaceType::WallSurface,
                parent: None,
                children: None,
                other: HashMap::new(),
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
            "type": "Solid",
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
    fn test_encode_material() -> Result<()> {
        // Test case 1: Single material value
        let materials = HashMap::from([(
            "theme1".to_string(),
            serde_json::from_value::<CjMaterialReference>(json!({"value": 5}))?,
        )]);

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 1);
        match &encoded[0] {
            MaterialMapping::Value(value) => {
                assert_eq!(value.theme, "theme1");
                assert_eq!(value.value, 5);
            }
            _ => panic!("Expected MaterialMapping::Value"),
        }

        // Test case 2: MultiSurface material values
        let materials = HashMap::from([(
            "theme2".to_string(),
            serde_json::from_value::<CjMaterialReference>(json!({"values": [0, 1, null, 2]}))?,
        )]);

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 1);
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.theme, "theme2");
                assert_eq!(values.vertices, vec![0, 1, NULL, 2]);
                assert!(values.shells.is_empty());
                assert!(values.solids.is_empty());
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        // Test case 3: Solid material values
        let materials = HashMap::from([(
            "theme3".to_string(),
            serde_json::from_value::<CjMaterialReference>(
                json!({"values": [[0, 1, null], [2, 3, 4]]}),
            )?,
        )]);

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 1);
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.theme, "theme3");
                assert_eq!(values.solids, vec![2]); // 1 solid with 2 shells
                assert_eq!(values.shells, vec![3, 3]); // Each shell has 3 surfaces
                assert_eq!(values.vertices, vec![0, 1, NULL, 2, 3, 4]);
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        // Test case 4: Multiple themes
        let materials = HashMap::from([
            (
                "theme4".to_string(),
                serde_json::from_value::<CjMaterialReference>(json!({"value": 7}))?,
            ),
            (
                "theme5".to_string(),
                serde_json::from_value::<CjMaterialReference>(json!({"values": [8, 9]}))?,
            ),
        ]);

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 2);

        // Find and verify each mapping by theme name instead of relying on order
        let theme4_mapping = encoded
            .iter()
            .find(|m| theme_of(m) == "theme4")
            .expect("Should have theme4 mapping");

        let theme5_mapping = encoded
            .iter()
            .find(|m| theme_of(m) == "theme5")
            .expect("Should have theme5 mapping");

        match theme4_mapping {
            MaterialMapping::Value(value) => {
                assert_eq!(value.theme, "theme4");
                assert_eq!(value.value, 7);
            }
            _ => panic!("Expected MaterialMapping::Value for theme4"),
        }

        match theme5_mapping {
            MaterialMapping::Values(values) => {
                assert_eq!(values.theme, "theme5");
                assert_eq!(values.vertices, vec![8, 9]);
                assert!(values.shells.is_empty());
                assert!(values.solids.is_empty());
            }
            _ => panic!("Expected MaterialMapping::Values for theme5"),
        }

        // Test case 5: CompositeSolid material values
        let materials = HashMap::from([(
            "theme6".to_string(),
            serde_json::from_value::<CjMaterialReference>(json!({
                "values": [[[0, 1, null], [2, null, null]], [[3, 4, null]]]
            }))?,
        )]);

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 1);
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.theme, "theme6");
                assert_eq!(values.solids, vec![2, 1]); // Two solids: 2 shells, then 1
                assert_eq!(values.shells, vec![3, 3, 3]); // Each shell has 3 surfaces
                assert_eq!(values.vertices, vec![0, 1, NULL, 2, NULL, NULL, 3, 4, NULL]);
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        Ok(())
    }

    /// `material.values` is nullable at *every* level, not only at the leaf.
    /// A whole `null` shell or solid is recorded as a `NULL` count so that it
    /// decodes back as `null` rather than as an empty array.
    #[test]
    fn a_null_material_shell_or_solid_is_recorded_as_a_null_count() -> Result<()> {
        let materials = HashMap::from([(
            "t".to_string(),
            serde_json::from_value::<CjMaterialReference>(json!({"values": [[0, 1], null]}))?,
        )]);
        match &encode_material(&materials)[0] {
            MaterialMapping::Values(v) => {
                assert_eq!(v.solids, vec![2]);
                assert_eq!(v.shells, vec![2, NULL]);
                assert_eq!(v.vertices, vec![0, 1]);
            }
            _ => panic!("expected Values"),
        }

        let materials = HashMap::from([(
            "t".to_string(),
            serde_json::from_value::<CjMaterialReference>(json!({"values": [[[0, 1]], null]}))?,
        )]);
        match &encode_material(&materials)[0] {
            MaterialMapping::Values(v) => {
                assert_eq!(v.solids, vec![1, NULL]);
                assert_eq!(v.shells, vec![2]);
                assert_eq!(v.vertices, vec![0, 1]);
            }
            _ => panic!("expected Values"),
        }
        Ok(())
    }

    #[test]
    fn test_encode_texture() -> Result<()> {
        let theme = "test-theme".to_string();
        let texture = |v: serde_json::Value| -> HashMap<String, CjTextureReference> {
            HashMap::from([(
                theme.clone(),
                serde_json::from_value::<CjTextureReference>(json!({"values": v}))
                    .expect("texture values must parse"),
            )])
        };

        // MultiSurface-like texture values (the shallowest a texture can be)
        let encoded = encode_texture(&texture(json!([
            [[0, 10, 20, 30]],
            [[1, 11, 21, null]],
            [[2, 12, null, 32]]
        ])));
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].theme, theme);
        assert_eq!(
            encoded[0].vertices,
            vec![0, 10, 20, 30, 1, 11, 21, NULL, 2, 12, NULL, 32]
        );
        assert_eq!(encoded[0].strings, vec![4, 4, 4]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![3]);
        assert!(encoded[0].solids.is_empty());

        // Solid-like texture values
        let encoded = encode_texture(&texture(json!([
            [[[0, 10, 20, 30]], [[1, 11, 21, null]], [[2, 12, null, 32]]],
            [[[3, 13, 23, 33]], [[4, 14, 24, null]]]
        ])));
        assert_eq!(encoded.len(), 1);
        assert_eq!(
            encoded[0].vertices,
            vec![0, 10, 20, 30, 1, 11, 21, NULL, 2, 12, NULL, 32, 3, 13, 23, 33, 4, 14, 24, NULL]
        );
        assert_eq!(encoded[0].strings, vec![4, 4, 4, 4, 4]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![3, 2]);
        assert_eq!(encoded[0].solids, vec![2]);

        // CompositeSolid-like texture values
        let encoded = encode_texture(&texture(json!([
            [
                [[[0, 10, 20]], [[1, 11, null]]],
                [[[2, 12, 22]], [[3, null, 23]]]
            ],
            [[[[4, 14, 24]], [[5, 15, 25]]]]
        ])));
        assert_eq!(encoded.len(), 1);
        assert_eq!(
            encoded[0].vertices,
            vec![0, 10, 20, 1, 11, NULL, 2, 12, 22, 3, NULL, 23, 4, 14, 24, 5, 15, 25]
        );
        assert_eq!(encoded[0].strings, vec![3, 3, 3, 3, 3, 3]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1, 1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![2, 2, 2]);
        assert_eq!(encoded[0].solids, vec![2, 1]);

        // Multiple themes
        let textures = HashMap::from([
            (
                "winter".to_string(),
                serde_json::from_value::<CjTextureReference>(json!({"values": [[[0, 10, 20]]]}))?,
            ),
            (
                "summer".to_string(),
                serde_json::from_value::<CjTextureReference>(json!({"values": [[[1, 11, null]]]}))?,
            ),
        ]);

        let encoded = encode_texture(&textures);
        assert_eq!(encoded.len(), 2);

        let winter_mapping = encoded
            .iter()
            .find(|m| m.theme == "winter")
            .expect("Should have winter mapping");
        let summer_mapping = encoded
            .iter()
            .find(|m| m.theme == "summer")
            .expect("Should have summer mapping");

        assert_eq!(winter_mapping.vertices, vec![0, 10, 20]);
        assert_eq!(winter_mapping.strings, vec![3]);

        assert_eq!(summer_mapping.vertices, vec![1, 11, NULL]);
        assert_eq!(summer_mapping.strings, vec![3]);

        Ok(())
    }
}
