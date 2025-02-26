use cjseq::{
    Boundaries as CjBoundaries, GeometryType as CjGeometryType,
    MaterialReference as CjMaterialReference, MaterialValues as CjMaterialValues, Semantics,
    SemanticsSurface, SemanticsValues, TextureReference as CjTextureReference,
    TextureValues as CjTextureValues,
};

use crate::fb::{
    GeometryType, MaterialMapping, SemanticObject, SemanticSurfaceType, TextureMapping,
};
use std::collections::HashMap;

/// For semantics decoding, we only care about solids and shells.
/// We stop recursing at d <= 2 which are surfaces, rings and points (meaning we just return semantic_indices).
struct PartLists<'a> {
    solids: &'a [u32],
    shells: &'a [u32],
    starts: [usize; 5], // parallel "start" indices
}

/// Decodes the flattened arrays back into a nested CityJSON boundaries structure.
///
/// Uses cursor indices to track position in each array while rebuilding the
/// hierarchical structure of solids, shells, surfaces and rings.
///
/// # Returns
///
/// The reconstructed CityJSON boundaries structure
pub(crate) fn decode(
    solids: &[u32],
    shells: &[u32],
    surfaces: &[u32],
    strings: &[u32],
    indices: &[u32],
) -> CjBoundaries {
    let mut shell_cursor = 0;
    let mut surface_cursor = 0;
    let mut ring_cursor = 0;
    let mut index_cursor = 0;

    if !solids.is_empty() {
        let mut solids_vec = Vec::new();
        for &shell_count in solids.iter() {
            let mut shell_vec = Vec::new();
            for _ in 0..shell_count {
                let surfaces_in_shell = shells[shell_cursor] as usize;
                shell_cursor += 1;

                let mut surface_vec = Vec::new();
                for _ in 0..surfaces_in_shell {
                    let rings_in_surface = surfaces[surface_cursor] as usize;
                    surface_cursor += 1;

                    let mut ring_vec = Vec::new();
                    for _ in 0..rings_in_surface {
                        let ring_size = strings[ring_cursor] as usize;
                        ring_cursor += 1;

                        let ring_indices = indices[index_cursor..index_cursor + ring_size]
                            .iter()
                            .map(|x| *x as usize)
                            .collect::<Vec<_>>();
                        index_cursor += ring_size;

                        let ring_indices = ring_indices
                            .into_iter()
                            .map(|x| x as u32)
                            .collect::<Vec<_>>();
                        ring_vec.push(CjBoundaries::Indices(ring_indices));
                    }

                    surface_vec.push(CjBoundaries::Nested(ring_vec));
                }

                shell_vec.push(CjBoundaries::Nested(surface_vec));
            }

            solids_vec.push(CjBoundaries::Nested(shell_vec));
        }

        if solids_vec.len() == 1 {
            solids_vec.into_iter().next().unwrap()
        } else {
            CjBoundaries::Nested(solids_vec)
        }
    } else if !shells.is_empty() {
        let mut shell_vec = Vec::new();
        for &surface_count in shells.iter() {
            let mut surface_vec = Vec::new();
            for _ in 0..surface_count {
                let rings_in_surface = surfaces[surface_cursor] as usize;
                surface_cursor += 1;

                let mut ring_vec = Vec::new();
                for _ in 0..rings_in_surface {
                    let ring_size = strings[ring_cursor] as usize;
                    ring_cursor += 1;
                    let ring_indices = indices[index_cursor..index_cursor + ring_size]
                        .iter()
                        .map(|x| *x as usize)
                        .collect::<Vec<_>>();
                    index_cursor += ring_size;

                    ring_vec.push(CjBoundaries::Indices(
                        ring_indices.into_iter().map(|x| x as u32).collect(),
                    ));
                }
                surface_vec.push(CjBoundaries::Nested(ring_vec));
            }
            shell_vec.push(CjBoundaries::Nested(surface_vec));
        }
        if shell_vec.len() == 1 {
            shell_vec.into_iter().next().unwrap()
        } else {
            CjBoundaries::Nested(shell_vec)
        }
    } else if !surfaces.is_empty() {
        let mut surface_vec = Vec::new();
        for &rings_count in surfaces.iter() {
            let mut ring_vec = Vec::new();
            for _ in 0..rings_count {
                let ring_size = strings[ring_cursor] as usize;
                ring_cursor += 1;
                let ring_indices = indices[index_cursor..index_cursor + ring_size]
                    .iter()
                    .map(|x| *x as usize)
                    .collect::<Vec<_>>();
                index_cursor += ring_size;

                ring_vec.push(CjBoundaries::Indices(
                    ring_indices.into_iter().map(|x| x as u32).collect(),
                ));
            }
            surface_vec.push(CjBoundaries::Nested(ring_vec));
        }
        if surface_vec.len() == 1 {
            surface_vec.into_iter().next().unwrap()
        } else {
            CjBoundaries::Nested(surface_vec)
        }
    } else if !strings.is_empty() {
        let mut ring_vec = Vec::new();
        for &ring_size in strings.iter() {
            let ring_indices = indices[index_cursor..index_cursor + ring_size as usize]
                .iter()
                .map(|x| *x as usize)
                .collect::<Vec<_>>();
            index_cursor += ring_size as usize;
            ring_vec.push(CjBoundaries::Indices(
                ring_indices.into_iter().map(|x| x as u32).collect(),
            ));
        }
        if ring_vec.len() == 1 {
            ring_vec.into_iter().next().unwrap()
        } else {
            CjBoundaries::Nested(ring_vec)
        }
    } else {
        CjBoundaries::Indices(indices.to_vec())
    }
}

/// Converts FlatBuffers semantic surface objects into CityJSON semantic surfaces.
///
/// # Arguments
///
/// * `semantics_objects` - Slice of FlatBuffers semantic surface objects
///
/// # Returns
///
/// Vector of CityJSON semantic surface definitions
pub(crate) fn decode_semantics_surfaces(
    semantics_objects: &[SemanticObject],
) -> Vec<SemanticsSurface> {
    let surfaces = semantics_objects.iter().map(|s| {
        let surface_type_str = match s.type_() {
            SemanticSurfaceType::RoofSurface => "RoofSurface",
            SemanticSurfaceType::GroundSurface => "GroundSurface",
            SemanticSurfaceType::WallSurface => "WallSurface",
            SemanticSurfaceType::ClosureSurface => "ClosureSurface",
            SemanticSurfaceType::OuterCeilingSurface => "OuterCeilingSurface",
            SemanticSurfaceType::OuterFloorSurface => "OuterFloorSurface",
            SemanticSurfaceType::Window => "Window",
            SemanticSurfaceType::Door => "Door",
            SemanticSurfaceType::InteriorWallSurface => "InteriorWallSurface",
            SemanticSurfaceType::CeilingSurface => "CeilingSurface",
            SemanticSurfaceType::FloorSurface => "FloorSurface",
            SemanticSurfaceType::WaterSurface => "WaterSurface",
            SemanticSurfaceType::WaterGroundSurface => "WaterGroundSurface",
            SemanticSurfaceType::WaterClosureSurface => "WaterClosureSurface",
            SemanticSurfaceType::TrafficArea => "TrafficArea",
            SemanticSurfaceType::AuxiliaryTrafficArea => "AuxiliaryTrafficArea",
            SemanticSurfaceType::TransportationMarking => "TransportationMarking",
            SemanticSurfaceType::TransportationHole => "TransportationHole",
            _ => unreachable!(),
        };

        let children = s.children().map(|c| c.iter().collect::<Vec<_>>());

        // let attributes = None; // FIXME

        SemanticsSurface {
            thetype: surface_type_str.to_string(),
            parent: s.parent(),
            children,
            // TODO: Think how to handle `other`
            other: serde_json::Value::default(),
        }
    });
    surfaces.collect()
}

/// Helper function for recursively decoding semantic values.
///
/// # Arguments
///
/// * `d` - Current depth in geometry hierarchy (4=solids, 3=shells, <=2=surfaces)
/// * `start` - Starting index in current array level
/// * `n` - Number of elements to process at current level
/// * `part_lists` - References to solids/shells arrays and cursor positions
/// * `semantic_indices` - Flattened array of semantic value indices
///
/// # Returns
///
/// Nested structure of semantic values matching geometry hierarchy
fn decode_semantics_(
    d: i32,
    start: Option<usize>,
    n: Option<usize>,
    part_lists: &mut PartLists,
    semantic_indices: &[u32],
) -> SemanticsValues {
    // 1) If top-level call (start==None, n==None)
    if start.is_none() || n.is_none() {
        if d > 2 {
            // example: d=4 => part_lists[4] = self.solids, d=3 => shells
            let arr = match d {
                4 => &part_lists.solids,
                3 => &part_lists.shells,
                _ => unreachable!(),
            };

            let mut results = Vec::new();
            // loop over each 'gn' in part_lists[d]
            for &gn in *arr {
                // decode_semantics_(d-1, self.starts[d], gn)
                let st = part_lists.starts[d as usize];
                // decode subarray
                let subvals = decode_semantics_(
                    d - 1,
                    Some(st),
                    Some(gn as usize),
                    part_lists,
                    semantic_indices,
                );
                part_lists.starts[d as usize] += gn as usize;
                results.push(subvals);
            }

            SemanticsValues::Nested(results)
        } else {
            // d <= 2 => "return self.semantic_indices"
            // as a single Indices array
            let mut leaf = Vec::new();
            for &val in semantic_indices {
                leaf.push(if val == u32::MAX { None } else { Some(val) });
            }
            SemanticsValues::Indices(leaf)
        }
    } else {
        // 2) If subsequent recursive call (start,n are Some)
        let s = start.unwrap();
        let length = n.unwrap();
        if d <= 2 {
            let slice = &semantic_indices[s..s + length];
            let mut leaf = Vec::with_capacity(slice.len());
            for &val in slice {
                leaf.push(if val == u32::MAX { None } else { Some(val) });
            }
            SemanticsValues::Indices(leaf)
        } else {
            // d>2 => we iterate subarray part_lists[d][start..start+n]
            let arr = match d {
                4 => &part_lists.solids,
                3 => &part_lists.shells,
                _ => unreachable!(),
            };

            let mut results = Vec::new();
            // for gn in part_lists[d][start..start+n]
            for &gn in &arr[s..(s + length)] {
                let st = part_lists.starts[d as usize];
                let subvals = decode_semantics_(
                    d - 1,
                    Some(st),
                    Some(gn as usize),
                    part_lists,
                    semantic_indices,
                );
                part_lists.starts[d as usize] += gn as usize;
                results.push(subvals);
            }

            SemanticsValues::Nested(results)
        }
    }
}

/// Decodes FlatBuffers semantic data into CityJSON semantics structure.
///
/// # Arguments
///
/// * `geometry_type` - Type of geometry (determines nesting depth)
/// * `semantics_objects` - Vector of semantic surface definitions
/// * `semantics_values` - Vector of semantic value indices
///
/// # Returns
///
/// Complete CityJSON semantics structure with surfaces and values
pub(crate) fn decode_semantics(
    solids: &[u32],
    shells: &[u32],
    geometry_type: GeometryType,
    semantics_objects: Vec<SemanticObject>,
    semantics_values: Vec<u32>,
) -> Semantics {
    let surfaces = decode_semantics_surfaces(&semantics_objects);

    let mut part_lists = PartLists {
        solids,
        shells,
        starts: [0; 5],
    };

    let d = match geometry_type {
        GeometryType::MultiSolid | GeometryType::CompositeSolid => 4,
        GeometryType::Solid => 3,
        GeometryType::MultiSurface
        | GeometryType::CompositeSurface
        | GeometryType::MultiLineString
        | GeometryType::MultiPoint => 2,
        // fallback
        _ => 2,
    };

    if d <= 2 {
        // Flatten entire semantics_values into Indices
        let mut leaf = Vec::new();
        for &val in &semantics_values {
            leaf.push(if val == u32::MAX { None } else { Some(val) });
        }
        return Semantics {
            surfaces,
            values: SemanticsValues::Indices(leaf),
        };
    }

    let result = decode_semantics_(d, None, None, &mut part_lists, &semantics_values);

    Semantics {
        surfaces,
        values: result,
    }
}

impl GeometryType {
    pub fn to_string(self) -> &'static str {
        match self {
            Self::MultiPoint => "MultiPoint",
            Self::MultiLineString => "MultiLineString",
            Self::MultiSurface => "MultiSurface",
            Self::CompositeSurface => "CompositeSurface",
            Self::Solid => "Solid",
            Self::MultiSolid => "MultiSolid",
            Self::CompositeSolid => "CompositeSolid",
            _ => "Solid",
        }
    }

    pub fn to_cj(self) -> CjGeometryType {
        match self {
            Self::MultiPoint => CjGeometryType::MultiPoint,
            Self::MultiLineString => CjGeometryType::MultiLineString,
            Self::MultiSurface => CjGeometryType::MultiSurface,
            Self::CompositeSurface => CjGeometryType::CompositeSurface,
            Self::Solid => CjGeometryType::Solid,
            Self::MultiSolid => CjGeometryType::MultiSolid,
            Self::CompositeSolid => CjGeometryType::CompositeSolid,
            _ => CjGeometryType::Solid,
        }
    }
}

/// Decodes FlatBuffers material mappings into CityJSON material references.
///
/// # Arguments
///
/// * `material_mappings` - Vector of FlatBuffers material mappings
///
/// # Returns
///
/// HashMap of theme names to CityJSON material references
pub(crate) fn decode_materials(
    material_mappings: &[MaterialMapping],
) -> Option<HashMap<String, CjMaterialReference>> {
    if material_mappings.is_empty() {
        return None;
    }

    let mut materials = HashMap::new();

    for mapping in material_mappings {
        let theme = mapping.theme().unwrap_or("theme").to_string();

        // Check if this is a single value material reference
        if let Some(value) = mapping.value() {
            materials.insert(
                theme,
                CjMaterialReference {
                    value: Some(value as usize),
                    values: None,
                },
            );
            continue;
        }

        // Otherwise, it's a material values mapping
        let solids = mapping.solids().map(|s| s.iter().collect::<Vec<_>>());
        let shells = mapping.shells().map(|s| s.iter().collect::<Vec<_>>());
        let vertices = mapping.vertices().map(|v| v.iter().collect::<Vec<_>>());

        if solids.is_none() || vertices.is_none() {
            continue;
        }

        let solids = solids.unwrap();
        let shells = shells.unwrap_or_default();
        let vertices = vertices.unwrap();

        // Determine the structure based on the presence of solids and shells
        let values = if !solids.is_empty() {
            // For MultiSolid/CompositeSolid: values is nested array of shells
            let mut nested_values = Vec::new();
            let mut vertex_index = 0;

            for &solid_size in &solids {
                let mut solid_values = Vec::new();

                for _ in 0..solid_size {
                    if shells.is_empty() {
                        // For Solid: values is array of surface indices
                        let mut shell_values = Vec::new();
                        for &vertex in &vertices[vertex_index..] {
                            shell_values.push(if vertex == u32::MAX {
                                None
                            } else {
                                Some(vertex as usize)
                            });
                            vertex_index += 1;
                        }
                        solid_values.push(CjMaterialValues::Indices(shell_values));
                    } else {
                        // For MultiSolid/CompositeSolid: values is nested array of shells
                        let mut shell_values = Vec::new();
                        for &shell_size in &shells {
                            let mut surface_values = Vec::new();
                            for _ in 0..shell_size {
                                if vertex_index < vertices.len() {
                                    let vertex = vertices[vertex_index];
                                    surface_values.push(if vertex == u32::MAX {
                                        None
                                    } else {
                                        Some(vertex as usize)
                                    });
                                    vertex_index += 1;
                                }
                            }
                            shell_values.push(CjMaterialValues::Indices(surface_values));
                        }
                        solid_values.push(CjMaterialValues::Nested(shell_values));
                    }
                }

                nested_values.push(CjMaterialValues::Nested(solid_values));
            }

            CjMaterialValues::Nested(nested_values)
        } else {
            // For MultiSurface/CompositeSurface: values is array of indices
            let indices = vertices
                .iter()
                .map(|&v| {
                    if v == u32::MAX {
                        None
                    } else {
                        Some(v as usize)
                    }
                })
                .collect();

            CjMaterialValues::Indices(indices)
        };

        materials.insert(
            theme,
            CjMaterialReference {
                value: None,
                values: Some(values),
            },
        );
    }

    Some(materials)
}

/// Decodes FlatBuffers texture mappings into CityJSON texture references.
///
/// # Arguments
///
/// * `texture_mappings` - Vector of FlatBuffers texture mappings
///
/// # Returns
///
/// HashMap of theme names to CityJSON texture references
pub(crate) fn decode_textures(
    texture_mappings: &[TextureMapping],
) -> Option<HashMap<String, CjTextureReference>> {
    if texture_mappings.is_empty() {
        return None;
    }

    let mut textures = HashMap::new();

    for mapping in texture_mappings {
        let theme = mapping.theme().unwrap_or("theme").to_string();

        // Get all the arrays from the mapping
        let solids = mapping
            .solids()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let shells = mapping
            .shells()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let surfaces = mapping
            .surfaces()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let strings = mapping
            .strings()
            .map(|s| s.iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let vertices = mapping
            .vertices()
            .map(|v| v.iter().collect::<Vec<_>>())
            .unwrap_or_default();

        if vertices.is_empty() {
            continue;
        }

        // Determine the structure based on the presence of solids, shells, surfaces, and strings
        let values = if !solids.is_empty() {
            // For Solid/MultiSolid/CompositeSolid
            let mut solid_values = Vec::new();
            let mut shell_index = 0;
            let mut surface_index = 0;
            let mut string_index = 0;
            let mut vertex_index = 0;

            for &solid_size in &solids {
                let mut shell_values = Vec::new();

                for _ in 0..solid_size {
                    if shell_index < shells.len() {
                        let shell_size = shells[shell_index];
                        shell_index += 1;

                        let mut surface_values = Vec::new();
                        for _ in 0..shell_size {
                            if surface_index < surfaces.len() {
                                let surface_size = surfaces[surface_index];
                                surface_index += 1;

                                let mut string_values = Vec::new();
                                for _ in 0..surface_size {
                                    if string_index < strings.len() {
                                        let string_size = strings[string_index];
                                        string_index += 1;

                                        let mut indices = Vec::new();
                                        for _ in 0..string_size {
                                            if vertex_index < vertices.len() {
                                                let vertex = vertices[vertex_index];
                                                indices.push(if vertex == u32::MAX {
                                                    None
                                                } else {
                                                    Some(vertex as usize)
                                                });
                                                vertex_index += 1;
                                            }
                                        }

                                        string_values.push(CjTextureValues::Indices(indices));
                                    }
                                }

                                surface_values.push(CjTextureValues::Nested(string_values));
                            }
                        }

                        shell_values.push(CjTextureValues::Nested(surface_values));
                    }
                }

                solid_values.push(CjTextureValues::Nested(shell_values));
            }

            CjTextureValues::Nested(solid_values)
        } else if !shells.is_empty() {
            // For Solid
            let mut shell_values = Vec::new();
            let mut surface_index = 0;
            let mut string_index = 0;
            let mut vertex_index = 0;

            for &shell_size in &shells {
                let mut surface_values = Vec::new();

                for _ in 0..shell_size {
                    if surface_index < surfaces.len() {
                        let surface_size = surfaces[surface_index];
                        surface_index += 1;

                        let mut string_values = Vec::new();
                        for _ in 0..surface_size {
                            if string_index < strings.len() {
                                let string_size = strings[string_index];
                                string_index += 1;

                                let mut indices = Vec::new();
                                for _ in 0..string_size {
                                    if vertex_index < vertices.len() {
                                        let vertex = vertices[vertex_index];
                                        indices.push(if vertex == u32::MAX {
                                            None
                                        } else {
                                            Some(vertex as usize)
                                        });
                                        vertex_index += 1;
                                    }
                                }

                                string_values.push(CjTextureValues::Indices(indices));
                            }
                        }

                        surface_values.push(CjTextureValues::Nested(string_values));
                    }
                }

                shell_values.push(CjTextureValues::Nested(surface_values));
            }

            CjTextureValues::Nested(shell_values)
        } else if !surfaces.is_empty() {
            // For MultiSurface/CompositeSurface
            let mut surface_values = Vec::new();
            let mut string_index = 0;
            let mut vertex_index = 0;

            for &surface_size in &surfaces {
                let mut string_values = Vec::new();

                for _ in 0..surface_size {
                    if string_index < strings.len() {
                        let string_size = strings[string_index];
                        string_index += 1;

                        let mut indices = Vec::new();
                        for _ in 0..string_size {
                            if vertex_index < vertices.len() {
                                let vertex = vertices[vertex_index];
                                indices.push(if vertex == u32::MAX {
                                    None
                                } else {
                                    Some(vertex as usize)
                                });
                                vertex_index += 1;
                            }
                        }

                        string_values.push(CjTextureValues::Indices(indices));
                    }
                }

                surface_values.push(CjTextureValues::Nested(string_values));
            }

            CjTextureValues::Nested(surface_values)
        } else if !strings.is_empty() {
            // For MultiLineString
            let mut string_values = Vec::new();
            let mut vertex_index = 0;

            for &string_size in &strings {
                let mut indices = Vec::new();

                for _ in 0..string_size {
                    if vertex_index < vertices.len() {
                        let vertex = vertices[vertex_index];
                        indices.push(if vertex == u32::MAX {
                            None
                        } else {
                            Some(vertex as usize)
                        });
                        vertex_index += 1;
                    }
                }

                string_values.push(CjTextureValues::Indices(indices));
            }

            CjTextureValues::Nested(string_values)
        } else {
            // For MultiPoint or simple indices
            let indices = vertices
                .iter()
                .map(|&v| {
                    if v == u32::MAX {
                        None
                    } else {
                        Some(v as usize)
                    }
                })
                .collect();

            CjTextureValues::Indices(indices)
        };

        textures.insert(theme, CjTextureReference { values });
    }

    Some(textures)
}

#[cfg(test)]
mod tests {
    use crate::{
        fb::feature_generated::{
            root_as_city_feature, CityFeature, CityFeatureArgs, CityObject, CityObjectArgs,
            GeometryType, MaterialMapping, MaterialMappingArgs, TextureMapping, TextureMappingArgs,
        },
        serializer::to_geometry,
    };
    use pretty_assertions::assert_eq;

    use super::*;
    use anyhow::Result;
    use cjseq::Geometry as CjGeometry;
    use flatbuffers::FlatBufferBuilder;
    use serde_json::json;

    #[test]
    fn test_decode_boundaries() -> Result<()> {
        // MultiPoint
        let boundaries_value = json!([2, 44, 0, 7]);
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
        let indices = vec![2, 44, 0, 7];
        let strings = vec![4];
        let boundaries = decode(&[], &[], &[], &strings, &indices);
        assert_eq!(expected, boundaries);

        // MultiLineString
        let boundaries_value = json!([[2, 3, 5], [77, 55, 212]]);
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
        let indices = vec![2, 3, 5, 77, 55, 212];
        let strings = vec![3, 3];
        let boundaries = decode(&[], &[], &[], &strings, &indices);
        assert_eq!(expected, boundaries);

        // MultiSurface
        let boundaries_value = json!([[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]]);
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
        let indices = vec![0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5];
        let strings = vec![4, 4, 4];
        let surfaces = vec![1, 1, 1];
        let boundaries = decode(&[], &[], &surfaces, &strings, &indices);
        assert_eq!(expected, boundaries);

        // Solid
        let boundaries_value = json!([
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
        ]);
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
        let indices = vec![
            0, 3, 2, 1, 22, 1, 2, 3, 4, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244,
            246, 724, 34, 414, 45, 111, 246, 5,
        ];
        let strings = vec![5, 4, 4, 4, 4, 3, 3, 3, 3];
        let surfaces = vec![2, 1, 1, 1, 1, 1, 1, 1];
        let shells = vec![4, 4];
        let solids = vec![2];
        let boundaries = decode(&solids, &shells, &surfaces, &strings, &indices);
        assert_eq!(expected, boundaries);

        // CompositeSolid
        let boundaries_value = json!([
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
        ]);
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
        let indices = vec![
            0, 3, 2, 1, 22, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244, 246, 724, 34,
            414, 45, 111, 246, 5, 666, 667, 668, 74, 75, 76, 880, 881, 885, 111, 122, 226,
        ];
        let strings = vec![5, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3];
        let surfaces = vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
        let shells = vec![4, 4, 4, 4];
        let solids = vec![2, 1];
        let boundaries = decode(&solids, &shells, &surfaces, &strings, &indices);
        assert_eq!(expected, boundaries);

        Ok(())
    }

    #[test]
    fn test_decode_semantics() -> Result<()> {
        // Test Case 1: MultiSurface
        {
            let mut fbb = FlatBufferBuilder::new();
            let multi_surfaces_gem_json = json!({
                "type": "MultiSurface",
                "lod": "2",
                "boundaries": [
                  [
                    [
                      0,
                      3,
                      2,
                      1
                    ]
                  ],
                  [
                    [
                      4,
                      5,
                      6,
                      7
                    ]
                  ],
                  [
                    [
                      0,
                      1,
                      5,
                      4
                    ]
                  ],
                  [
                    [
                      0,
                      2,
                      3,
                      8
                    ]
                  ],
                  [
                    [
                      10,
                      12,
                      23,
                      48
                    ]
                  ]
                ],
                "semantics": {
                  "surfaces": [
                    {
                      "type": "WallSurface",
                      "slope": 33.4,
                      "children": [
                        2
                      ]
                    },
                    {
                      "type": "RoofSurface",
                      "slope": 66.6
                    },
                    {
                      "type": "OuterCeilingSurface",
                      "parent": 0,
                      "colour": "blue"
                    }
                  ],
                  "values": [
                    0,
                    0,
                    null,
                    1,
                    2
                  ]
                }
            });
            let multi_sufaces_geom: CjGeometry = serde_json::from_value(multi_surfaces_gem_json)?;
            let city_feature = {
                let id = fbb.create_string("test");

                let geometry = to_geometry(&mut fbb, &multi_sufaces_geom);
                let geometries = fbb.create_vector(&[geometry]);
                let city_object = CityObject::create(
                    &mut fbb,
                    &CityObjectArgs {
                        geometry: Some(geometries),
                        id: Some(id),
                        ..Default::default()
                    },
                );
                let city_objects = fbb.create_vector(&[city_object]);
                CityFeature::create(
                    &mut fbb,
                    &CityFeatureArgs {
                        id: Some(id),
                        vertices: None,
                        objects: Some(city_objects),
                        appearance: None,
                    },
                )
            };
            fbb.finish(city_feature, None);
            let buf = fbb.finished_data();
            let city_feature = root_as_city_feature(buf);
            let geometry = city_feature
                .unwrap()
                .objects()
                .unwrap()
                .get(0)
                .geometry()
                .unwrap()
                .get(0);

            let solids = geometry
                .solids()
                .unwrap_or_default()
                .iter()
                .collect::<Vec<_>>();
            let shells = geometry
                .shells()
                .unwrap_or_default()
                .iter()
                .collect::<Vec<_>>();

            let decoded = decode_semantics(
                &solids,
                &shells,
                GeometryType::MultiSurface,
                geometry.semantics_objects().unwrap().iter().collect(),
                geometry.semantics().unwrap().iter().collect(),
            );

            // Verify decoded surfaces
            assert_eq!(3, decoded.surfaces.len());
            assert_eq!("WallSurface", decoded.surfaces[0].thetype);
            assert_eq!(Some(vec![2]), decoded.surfaces[0].children);
            assert_eq!("RoofSurface", decoded.surfaces[1].thetype);
            assert_eq!(None, decoded.surfaces[1].children);
            assert_eq!("OuterCeilingSurface", decoded.surfaces[2].thetype);
            assert_eq!(Some(0), decoded.surfaces[2].parent);

            assert_eq!(
                SemanticsValues::Indices(vec![Some(0), Some(0), None, Some(1), Some(2)]),
                decoded.values
            );
        }

        // Test Case 2: CompositeSolid
        {
            let mut fbb = FlatBufferBuilder::new();
            let composite_solid_gem_json = json!({
              "type": "CompositeSolid",
              "lod": "2.2",
              "boundaries": [
                [ //-- 1st Solid
                [
                  [
                    [
                      0,
                      3,
                      2,
                      1,
                      22
                    ]
                  ],
                  [
                    [
                      4,
                      5,
                      6,
                      7
                    ]
                  ],
                  [
                    [
                      0,
                      1,
                      5,
                      4
                    ]
                  ],
                  [
                    [
                      1,
                      2,
                      6,
                      5
                    ]
                  ]
                ]
              ],
              [ //-- 2nd Solid
                [
                  [
                    [
                      666,
                      667,
                      668
                    ]
                  ],
                  [
                    [
                      74,
                      75,
                      76
                    ]
                  ],
                  [
                    [
                      880,
                      881,
                      885
                    ]
                  ]
                ]
              ]
                ],
              "semantics": {
                "surfaces" : [
                  {
                    "type": "RoofSurface"
                  },
                  {
                    "type": "WallSurface"
                  }
                ],
                "values": [
                  [
                    [0, 1, 1, null]
                  ],
                  [
                    [null, null, null]
                  ]
                ]
              }
            }  );
            let composite_solid_geom: CjGeometry =
                serde_json::from_value(composite_solid_gem_json)?;
            let city_feature = {
                let id = fbb.create_string("test");

                let geometry = to_geometry(&mut fbb, &composite_solid_geom);
                let geometries = fbb.create_vector(&[geometry]);
                let city_object = CityObject::create(
                    &mut fbb,
                    &CityObjectArgs {
                        geometry: Some(geometries),
                        id: Some(id),
                        ..Default::default()
                    },
                );
                let city_objects = fbb.create_vector(&[city_object]);
                CityFeature::create(
                    &mut fbb,
                    &CityFeatureArgs {
                        id: Some(id),
                        vertices: None,
                        objects: Some(city_objects),
                        appearance: None,
                    },
                )
            };

            fbb.finish(city_feature, None);
            let buf = fbb.finished_data();
            let city_feature = root_as_city_feature(buf);
            let geometry = city_feature
                .unwrap()
                .objects()
                .unwrap()
                .get(0)
                .geometry()
                .unwrap()
                .get(0);

            let solids = geometry
                .solids()
                .unwrap_or_default()
                .iter()
                .collect::<Vec<_>>();
            let shells = geometry
                .shells()
                .unwrap_or_default()
                .iter()
                .collect::<Vec<_>>();
            let decoded = decode_semantics(
                &solids,
                &shells,
                GeometryType::CompositeSolid,
                geometry.semantics_objects().unwrap().iter().collect(),
                geometry.semantics().unwrap().iter().collect(),
            );

            // Verify decoded surfaces
            assert_eq!(decoded.surfaces.len(), 2);
            assert_eq!(decoded.surfaces[0].thetype, "RoofSurface");
            assert_eq!(decoded.surfaces[0].children, None);
            assert_eq!(decoded.surfaces[1].thetype, "WallSurface");
            assert_eq!(decoded.surfaces[1].children, None);

            match &decoded.values {
                SemanticsValues::Nested(solids) => {
                    assert_eq!(solids.len(), 2);
                    // First solid
                    match &solids[0] {
                        SemanticsValues::Nested(shells) => {
                            assert_eq!(shells.len(), 1);
                            match &shells[0] {
                                SemanticsValues::Indices(values) => {
                                    assert_eq!(values, &vec![Some(0), Some(1), Some(1), None]);
                                }
                                _ => panic!("Expected Indices for shell values"),
                            }
                        }
                        _ => panic!("Expected Nested for solid values"),
                    }
                    // Second solid
                    match &solids[1] {
                        SemanticsValues::Nested(shells) => {
                            assert_eq!(shells.len(), 1);
                            match &shells[0] {
                                SemanticsValues::Indices(values) => {
                                    assert_eq!(values, &vec![None, None, None]);
                                }
                                _ => panic!("Expected Indices for shell values"),
                            }
                        }
                        _ => panic!("Expected Nested for solid values"),
                    }
                }
                _ => panic!("Expected Nested values for CompositeSolid"),
            }
            Ok(())
        }
    }

    #[test]
    fn test_decode_materials() -> Result<()> {
        let mut fbb = FlatBufferBuilder::new();

        // Test case 1: Single material value
        {
            let theme = fbb.create_string("theme1");
            let mapping = MaterialMapping::create(
                &mut fbb,
                &MaterialMappingArgs {
                    theme: Some(theme),
                    solids: None,
                    shells: None,
                    vertices: None,
                    value: Some(5),
                },
            );

            fbb.finish(mapping, None);
            let buf = fbb.finished_data();
            let material_mapping = unsafe { flatbuffers::root_unchecked::<MaterialMapping>(buf) };

            let decoded = decode_materials(&[material_mapping]);

            assert!(decoded.is_some());
            let materials = decoded.unwrap();
            assert_eq!(materials.len(), 1);
            assert!(materials.contains_key("theme1"));

            let material_ref = &materials["theme1"];
            assert_eq!(material_ref.value, Some(5));
            assert!(material_ref.values.is_none());
        }

        // Test case 2: MultiSurface material values
        {
            let mut fbb = FlatBufferBuilder::new();
            let theme = fbb.create_string("theme2");

            // Create vertices for MultiSurface
            let solids = fbb.create_vector(&[1u32]);
            let vertices = fbb.create_vector(&[0u32, 1, u32::MAX, 2]);

            let mapping = MaterialMapping::create(
                &mut fbb,
                &MaterialMappingArgs {
                    theme: Some(theme),
                    solids: Some(solids),
                    shells: None,
                    vertices: Some(vertices),
                    value: None,
                },
            );

            fbb.finish(mapping, None);
            let buf = fbb.finished_data();
            let material_mapping = unsafe { flatbuffers::root_unchecked::<MaterialMapping>(buf) };

            let decoded = decode_materials(&[material_mapping]);

            assert!(decoded.is_some());
            let materials = decoded.unwrap();
            assert_eq!(materials.len(), 1);
            assert!(materials.contains_key("theme2"));

            let material_ref = &materials["theme2"];
            assert!(material_ref.value.is_none());
            assert!(material_ref.values.is_some());

            if let Some(CjMaterialValues::Nested(nested)) = &material_ref.values {
                assert_eq!(nested.len(), 1); // One solid

                if let CjMaterialValues::Nested(solid_values) = &nested[0] {
                    assert_eq!(solid_values.len(), 1); // One shell

                    if let CjMaterialValues::Indices(indices) = &solid_values[0] {
                        assert_eq!(indices.len(), 4);
                        assert_eq!(indices[0], Some(0));
                        assert_eq!(indices[1], Some(1));
                        assert_eq!(indices[2], None);
                        assert_eq!(indices[3], Some(2));
                    } else {
                        panic!("Expected Indices for shell values");
                    }
                } else {
                    panic!("Expected Nested for solid values");
                }
            } else {
                panic!("Expected Nested values");
            }
        }

        // Test case 3: Solid material values with shells
        {
            let mut fbb = FlatBufferBuilder::new();
            let theme = fbb.create_string("theme3");

            // Create vertices for Solid with shells
            let vertices = fbb.create_vector(&[0u32, 1, u32::MAX, 2, 3, 4]);
            let solids = fbb.create_vector(&[2u32]); // One solid
            let shells = fbb.create_vector(&[3u32, 3u32]); // Two shells

            let mapping = MaterialMapping::create(
                &mut fbb,
                &MaterialMappingArgs {
                    theme: Some(theme),
                    solids: Some(solids),
                    shells: Some(shells),
                    vertices: Some(vertices),
                    value: None,
                },
            );

            fbb.finish(mapping, None);
            let buf = fbb.finished_data();
            let material_mapping = unsafe { flatbuffers::root_unchecked::<MaterialMapping>(buf) };

            let decoded = decode_materials(&[material_mapping]);

            assert!(decoded.is_some());
            let materials = decoded.unwrap();
            assert_eq!(materials.len(), 1);
            assert!(materials.contains_key("theme3"));

            let material_ref = &materials["theme3"];
            assert!(material_ref.value.is_none());
            assert!(material_ref.values.is_some());

            println!("material_ref.values: {:?}", material_ref.values);
            if let Some(CjMaterialValues::Nested(nested)) = &material_ref.values {
                assert_eq!(nested.len(), 1); // One solid

                if let CjMaterialValues::Nested(shell_values) = &nested[0] {
                    assert_eq!(shell_values.len(), 2); // Two shells

                    // First shell
                    if let CjMaterialValues::Indices(indices) = &shell_values[0] {
                        assert_eq!(indices.len(), 3);
                        assert_eq!(indices[0], Some(0));
                        assert_eq!(indices[1], Some(1));
                        assert_eq!(indices[2], None);
                    } else {
                        panic!("Expected Indices for first shell");
                    }

                    // Second shell
                    if let CjMaterialValues::Indices(indices) = &shell_values[1] {
                        assert_eq!(indices.len(), 3);
                        assert_eq!(indices[0], Some(2));
                        assert_eq!(indices[1], Some(3));
                        assert_eq!(indices[2], Some(4));
                    } else {
                        panic!("Expected Indices for second shell");
                    }
                } else {
                    panic!("Expected Nested for shell values");
                }
            } else {
                panic!("Expected Nested for solid values");
            }
        }

        // Test case 4: Multiple material mappings
        {
            // First mapping: single value
            let mut fbb1 = FlatBufferBuilder::new();
            let theme1 = fbb1.create_string("theme4");
            let mapping1 = MaterialMapping::create(
                &mut fbb1,
                &MaterialMappingArgs {
                    theme: Some(theme1),
                    solids: None,
                    shells: None,
                    vertices: None,
                    value: Some(7),
                },
            );
            fbb1.finish(mapping1, None);
            let buf1 = fbb1.finished_data();
            let material_mapping1 = unsafe { flatbuffers::root_unchecked::<MaterialMapping>(buf1) };

            // Second mapping: array of values
            let mut fbb2 = FlatBufferBuilder::new();
            let theme2 = fbb2.create_string("theme5");
            let solids = fbb2.create_vector(&[1u32]);
            let vertices = fbb2.create_vector(&[8u32, 9]);
            let mapping2 = MaterialMapping::create(
                &mut fbb2,
                &MaterialMappingArgs {
                    theme: Some(theme2),
                    solids: Some(solids),
                    shells: None,
                    vertices: Some(vertices),
                    value: None,
                },
            );
            fbb2.finish(mapping2, None);
            let buf2 = fbb2.finished_data();
            let material_mapping2 = unsafe { flatbuffers::root_unchecked::<MaterialMapping>(buf2) };

            let mappings = [material_mapping1, material_mapping2];

            let decoded = decode_materials(&mappings);

            assert!(decoded.is_some());
            let materials = decoded.unwrap();
            assert_eq!(materials.len(), 2);

            // Check first mapping
            assert!(materials.contains_key("theme4"));
            let material_ref1 = &materials["theme4"];
            assert_eq!(material_ref1.value, Some(7));
            assert!(material_ref1.values.is_none());

            // Check second mapping
            assert!(materials.contains_key("theme5"));
            let material_ref2 = &materials["theme5"];
            assert!(material_ref2.value.is_none());
            assert!(material_ref2.values.is_some());

            if let Some(CjMaterialValues::Nested(nested)) = &material_ref2.values {
                assert_eq!(nested.len(), 1);

                if let CjMaterialValues::Nested(solid_values) = &nested[0] {
                    assert_eq!(solid_values.len(), 1);

                    if let CjMaterialValues::Indices(indices) = &solid_values[0] {
                        assert_eq!(indices.len(), 2);
                        assert_eq!(indices[0], Some(8));
                        assert_eq!(indices[1], Some(9));
                    } else {
                        panic!("Expected Indices for shell values");
                    }
                } else {
                    panic!("Expected Nested for solid values");
                }
            } else {
                panic!("Expected Nested values");
            }
        }

        Ok(())
    }

    #[test]
    fn test_decode_textures() -> Result<()> {
        // Test case 1: MultiPoint-like texture values
        {
            let mut fbb = FlatBufferBuilder::new();
            let theme = fbb.create_string("theme1");

            // Create vertices for MultiPoint
            let vertices = fbb.create_vector(&[0u32, 10, 20, u32::MAX]);
            let strings = fbb.create_vector(&[4u32]); // One string with 4 vertices

            let mapping = TextureMapping::create(
                &mut fbb,
                &TextureMappingArgs {
                    theme: Some(theme),
                    solids: None,
                    shells: None,
                    surfaces: None,
                    strings: Some(strings),
                    vertices: Some(vertices),
                },
            );

            fbb.finish(mapping, None);
            let buf = fbb.finished_data();
            let texture_mapping = unsafe { flatbuffers::root_unchecked::<TextureMapping>(buf) };

            let decoded = decode_textures(&[texture_mapping]);

            assert!(decoded.is_some());
            let textures = decoded.unwrap();
            assert_eq!(textures.len(), 1);
            assert!(textures.contains_key("theme1"));

            let texture_ref = &textures["theme1"];

            match &texture_ref.values {
                CjTextureValues::Indices(indices) => {
                    assert_eq!(indices.len(), 4);
                    assert_eq!(indices[0], Some(0));
                    assert_eq!(indices[1], Some(10));
                    assert_eq!(indices[2], Some(20));
                    assert_eq!(indices[3], None);
                }
                _ => panic!("Expected Indices"),
            }
        }

        // Test case 2: MultiLineString-like texture values
        {
            let mut fbb = FlatBufferBuilder::new();
            let theme = fbb.create_string("theme2");

            // Create vertices for MultiLineString
            let vertices = fbb.create_vector(&[0u32, 10, 20, 1, 11, u32::MAX]);
            let strings = fbb.create_vector(&[3u32, 3u32]); // Two strings with 3 vertices each
            let surfaces = fbb.create_vector(&[2u32]); // One surface with 2 strings
            let mapping = TextureMapping::create(
                &mut fbb,
                &TextureMappingArgs {
                    theme: Some(theme),
                    solids: None,
                    shells: None,
                    surfaces: Some(surfaces),
                    strings: Some(strings),
                    vertices: Some(vertices),
                },
            );

            fbb.finish(mapping, None);
            let buf = fbb.finished_data();
            let texture_mapping = unsafe { flatbuffers::root_unchecked::<TextureMapping>(buf) };

            let decoded = decode_textures(&[texture_mapping]);

            assert!(decoded.is_some());
            let textures = decoded.unwrap();
            assert_eq!(textures.len(), 1);
            assert!(textures.contains_key("theme2"));

            let texture_ref = &textures["theme2"];

            match &texture_ref.values {
                CjTextureValues::Nested(surface_values) => {
                    assert_eq!(surface_values.len(), 1); // One surface

                    match &surface_values[0] {
                        CjTextureValues::Nested(string_values) => {
                            assert_eq!(string_values.len(), 2); // Two strings

                            // First string
                            match &string_values[0] {
                                CjTextureValues::Indices(indices) => {
                                    assert_eq!(indices.len(), 3);
                                    assert_eq!(indices[0], Some(0));
                                    assert_eq!(indices[1], Some(10));
                                    assert_eq!(indices[2], Some(20));
                                }
                                _ => panic!("Expected Indices for first string"),
                            }

                            // Second string
                            match &string_values[1] {
                                CjTextureValues::Indices(indices) => {
                                    assert_eq!(indices.len(), 3);
                                    assert_eq!(indices[0], Some(1));
                                    assert_eq!(indices[1], Some(11));
                                    assert_eq!(indices[2], None);
                                }
                                _ => panic!("Expected Indices for second string"),
                            }
                        }
                        _ => panic!("Expected Nested for string values"),
                    }
                }
                _ => panic!("Expected Nested for surface values"),
            }
        }

        // Test case 3: MultiSurface-like texture values
        {
            let mut fbb = FlatBufferBuilder::new();
            let theme = fbb.create_string("theme3");

            // Create vertices for MultiSurface
            let vertices = fbb.create_vector(&[
                0u32,
                10,
                20,
                30, // First surface, first string
                1,
                11,
                21,
                u32::MAX, // Second surface, first string
                2,
                12,
                u32::MAX,
                32, // Third surface, first string
            ]);

            let strings = fbb.create_vector(&[4u32, 4u32, 4u32]); // Three strings with 4 vertices each
            let surfaces = fbb.create_vector(&[1u32, 1u32, 1u32]); // Three surfaces with 1 string each
            let shells = fbb.create_vector(&[3u32]); // One shell with 3 surfaces

            let mapping = TextureMapping::create(
                &mut fbb,
                &TextureMappingArgs {
                    theme: Some(theme),
                    solids: None,
                    shells: Some(shells),
                    surfaces: Some(surfaces),
                    strings: Some(strings),
                    vertices: Some(vertices),
                },
            );

            fbb.finish(mapping, None);
            let buf = fbb.finished_data();
            let texture_mapping = unsafe { flatbuffers::root_unchecked::<TextureMapping>(buf) };

            let decoded = decode_textures(&[texture_mapping]);

            assert!(decoded.is_some());
            let textures = decoded.unwrap();
            assert_eq!(textures.len(), 1);
            assert!(textures.contains_key("theme3"));

            let texture_ref = &textures["theme3"];

            match &texture_ref.values {
                CjTextureValues::Nested(shell_values) => {
                    assert_eq!(shell_values.len(), 1); // One shell

                    match &shell_values[0] {
                        CjTextureValues::Nested(surface_values) => {
                            assert_eq!(surface_values.len(), 3); // Three surfaces

                            // First surface
                            match &surface_values[0] {
                                CjTextureValues::Nested(string_values) => {
                                    assert_eq!(string_values.len(), 1); // One string

                                    match &string_values[0] {
                                        CjTextureValues::Indices(indices) => {
                                            assert_eq!(indices.len(), 4);
                                            assert_eq!(indices[0], Some(0));
                                            assert_eq!(indices[1], Some(10));
                                            assert_eq!(indices[2], Some(20));
                                            assert_eq!(indices[3], Some(30));
                                        }
                                        _ => panic!("Expected Indices for first surface string"),
                                    }
                                }
                                _ => panic!("Expected Nested for first surface string values"),
                            }
                        }
                        _ => panic!("Expected Nested for surface values"),
                    }
                }
                _ => panic!("Expected Nested for shell values"),
            }
        }

        // Test case 4: Solid-like texture values
        {
            let mut fbb = FlatBufferBuilder::new();
            let theme = fbb.create_string("theme4");

            // Create vertices for Solid
            let vertices = fbb.create_vector(&[
                0u32,
                10,
                20,
                30, // First shell, first surface, first string
                1,
                11,
                21,
                u32::MAX, // First shell, second surface, first string
                2,
                12,
                u32::MAX,
                32, // First shell, third surface, first string
                3,
                13,
                23,
                33, // Second shell, first surface, first string
                4,
                14,
                24,
                u32::MAX, // Second shell, second surface, first string
            ]);

            let strings = fbb.create_vector(&[4u32, 4u32, 4u32, 4u32, 4u32]); // Five strings with 4 vertices each
            let surfaces = fbb.create_vector(&[1u32, 1u32, 1u32, 1u32, 1u32]); // Five surfaces with 1 string each
            let shells = fbb.create_vector(&[3u32, 2u32]); // Two shells with 3 and 2 surfaces
            let solids = fbb.create_vector(&[2u32]); // One solid with 2 shells

            let mapping = TextureMapping::create(
                &mut fbb,
                &TextureMappingArgs {
                    theme: Some(theme),
                    solids: Some(solids),
                    shells: Some(shells),
                    surfaces: Some(surfaces),
                    strings: Some(strings),
                    vertices: Some(vertices),
                },
            );

            fbb.finish(mapping, None);
            let buf = fbb.finished_data();
            let texture_mapping = unsafe { flatbuffers::root_unchecked::<TextureMapping>(buf) };

            let decoded = decode_textures(&[texture_mapping]);

            assert!(decoded.is_some());
            let textures = decoded.unwrap();
            assert_eq!(textures.len(), 1);
            assert!(textures.contains_key("theme4"));

            let texture_ref = &textures["theme4"];

            match &texture_ref.values {
                CjTextureValues::Nested(solid_values) => {
                    assert_eq!(solid_values.len(), 1); // One solid

                    match &solid_values[0] {
                        CjTextureValues::Nested(shell_values) => {
                            assert_eq!(shell_values.len(), 2); // Two shells

                            // First shell
                            match &shell_values[0] {
                                CjTextureValues::Nested(surface_values) => {
                                    assert_eq!(surface_values.len(), 3); // Three surfaces

                                    // Check first surface of first shell
                                    match &surface_values[0] {
                                        CjTextureValues::Nested(string_values) => {
                                            assert_eq!(string_values.len(), 1); // One string

                                            match &string_values[0] {
                                                CjTextureValues::Indices(indices) => {
                                                    assert_eq!(indices.len(), 4);
                                                    assert_eq!(indices[0], Some(0));
                                                    assert_eq!(indices[1], Some(10));
                                                    assert_eq!(indices[2], Some(20));
                                                    assert_eq!(indices[3], Some(30));
                                                }
                                                _ => panic!("Expected Indices for first shell, first surface string"),
                                            }
                                        }
                                        _ => panic!("Expected Nested for first shell, first surface string values"),
                                    }
                                }
                                _ => panic!("Expected Nested for first shell surface values"),
                            }

                            // Second shell
                            match &shell_values[1] {
                                CjTextureValues::Nested(surface_values) => {
                                    assert_eq!(surface_values.len(), 2); // Two surfaces

                                    // Check first surface of second shell
                                    match &surface_values[0] {
                                        CjTextureValues::Nested(string_values) => {
                                            assert_eq!(string_values.len(), 1); // One string

                                            match &string_values[0] {
                                                CjTextureValues::Indices(indices) => {
                                                    assert_eq!(indices.len(), 4);
                                                    assert_eq!(indices[0], Some(3));
                                                    assert_eq!(indices[1], Some(13));
                                                    assert_eq!(indices[2], Some(23));
                                                    assert_eq!(indices[3], Some(33));
                                                }
                                                _ => panic!("Expected Indices for second shell, first surface string"),
                                            }
                                        }
                                        _ => panic!("Expected Nested for second shell, first surface string values"),
                                    }
                                }
                                _ => panic!("Expected Nested for second shell surface values"),
                            }
                        }
                        _ => panic!("Expected Nested for shell values"),
                    }
                }
                _ => panic!("Expected Nested for solid values"),
            }
        }

        // Test case 5: Multiple texture mappings
        {
            // First mapping
            let mut fbb1 = FlatBufferBuilder::new();
            let theme1 = fbb1.create_string("winter");
            let vertices1 = fbb1.create_vector(&[0u32, 10, 20]);
            let strings1 = fbb1.create_vector(&[3u32]);

            let mapping1 = TextureMapping::create(
                &mut fbb1,
                &TextureMappingArgs {
                    theme: Some(theme1),
                    solids: None,
                    shells: None,
                    surfaces: None,
                    strings: Some(strings1),
                    vertices: Some(vertices1),
                },
            );

            fbb1.finish(mapping1, None);
            let buf1 = fbb1.finished_data();
            let texture_mapping1 = unsafe { flatbuffers::root_unchecked::<TextureMapping>(buf1) };

            // Second mapping
            let mut fbb2 = FlatBufferBuilder::new();
            let theme2 = fbb2.create_string("summer");
            let vertices2 = fbb2.create_vector(&[1u32, 11, u32::MAX]);
            let strings2 = fbb2.create_vector(&[3u32]);

            let mapping2 = TextureMapping::create(
                &mut fbb2,
                &TextureMappingArgs {
                    theme: Some(theme2),
                    solids: None,
                    shells: None,
                    surfaces: None,
                    strings: Some(strings2),
                    vertices: Some(vertices2),
                },
            );

            fbb2.finish(mapping2, None);
            let buf2 = fbb2.finished_data();
            let texture_mapping2 = unsafe { flatbuffers::root_unchecked::<TextureMapping>(buf2) };

            let mappings = [texture_mapping1, texture_mapping2];

            let decoded = decode_textures(&mappings);

            assert!(decoded.is_some());
            let textures = decoded.unwrap();
            assert_eq!(textures.len(), 2);

            // Check first mapping
            assert!(textures.contains_key("winter"));
            let texture_ref1 = &textures["winter"];

            match &texture_ref1.values {
                CjTextureValues::Indices(indices) => {
                    assert_eq!(indices.len(), 3);
                    assert_eq!(indices[0], Some(0));
                    assert_eq!(indices[1], Some(10));
                    assert_eq!(indices[2], Some(20));
                }
                _ => panic!("Expected Indices for winter theme"),
            }

            // Check second mapping
            assert!(textures.contains_key("summer"));
            let texture_ref2 = &textures["summer"];

            match &texture_ref2.values {
                CjTextureValues::Indices(indices) => {
                    assert_eq!(indices.len(), 3);
                    assert_eq!(indices[0], Some(1));
                    assert_eq!(indices[1], Some(11));
                    assert_eq!(indices[2], None);
                }
                _ => panic!("Expected Indices for summer theme"),
            }
        }

        Ok(())
    }
}
