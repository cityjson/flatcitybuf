use cjseq::{
    Boundaries as CjBoundaries, MaterialReference as CjMaterialReference,
    MaterialValues as CjMaterialValues, Semantics as CjSemantics,
    SemanticsSurface as CjSemanticsSurface, SemanticsValues as CjSemanticsValues,
    TextureReference as CjTextureReference, TextureValues as CjTextureValues,
};
use std::collections::HashMap;

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
    pub(crate) solids: Vec<u32>, // length of this vector is the same as the number of solids, and the value is the length of the shells vector.
    pub(crate) shells: Vec<u32>, // length of this vector is the same as the number of shells, and the value is the length of the surfaces vector.
    pub(crate) vertices: Vec<u32>, // length of this vector is the same as the number of vertices, and the value is the material index.
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
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TextureMapping {
    pub(crate) theme: String,
    pub(crate) solids: Vec<u32>,   // Number of shells per solid
    pub(crate) shells: Vec<u32>,   // Number of surfaces per shell
    pub(crate) surfaces: Vec<u32>, // Number of rings per surface
    pub(crate) strings: Vec<u32>,  // Number of indices per ring
    pub(crate) vertices: Vec<u32>, // Flattened list of all indices
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GMSemantics {
    pub(crate) surfaces: Vec<CjSemanticsSurface>, // List of semantic surfaces
    pub(crate) values: Vec<u32>,                  // Semantic values corresponding to surfaces
}

#[derive(Debug, Clone, Default)]
#[doc(hidden)]
pub(crate) struct EncodedGeometry {
    pub(crate) boundaries: GMBoundaries,
    pub(crate) semantics: Option<GMSemantics>,
    pub(crate) textures: Option<Vec<TextureMapping>>,
    pub(crate) materials: Option<Vec<MaterialMapping>>,
}

/// Encodes the provided CityJSON boundaries and semantics into flattened arrays.
///
/// # Arguments
///
/// * `boundaries` - Reference to the CityJSON boundaries to encode.
/// * `semantics` - Optional reference to the semantics associated with the boundaries.
/// * `textures` - Optional reference to the textures associated with the boundaries.
/// * `materials` - Optional reference to the materials associated with the boundaries.
///
/// # Returns
/// Nothing.
pub(crate) fn encode(
    cj_boundaries: &CjBoundaries,
    semantics: Option<&CjSemantics>,
    textures: Option<&HashMap<String, CjTextureReference>>,
    materials: Option<&HashMap<String, CjMaterialReference>>,
) -> EncodedGeometry {
    let mut boundaries = GMBoundaries {
        solids: vec![],
        shells: vec![],
        surfaces: vec![],
        strings: vec![],
        indices: vec![],
    };
    // Encode the geometric boundaries
    let _ = encode_boundaries(cj_boundaries, &mut boundaries);

    // Encode semantics if provided
    let semantics = semantics.map(encode_semantics);

    // Encode appearance if provided
    let textures = textures.map(encode_texture);
    let materials = materials.map(encode_material);
    EncodedGeometry {
        boundaries,
        semantics,
        materials,
        textures,
    }
}

/// Encodes the CityJSON appearance data into our internal representation
///
/// # Arguments
///
/// * `appearance` - Reference to the CityJSON appearance object
///
/// # Returns
/// A GMAppearance containing the encoded appearance data
pub(crate) fn encode_material(
    materials: &HashMap<String, CjMaterialReference>,
) -> Vec<MaterialMapping> {
    let mut material_mappings = Vec::new();
    for (theme, material) in materials {
        if let Some(value) = material.value {
            // Handle single material value
            let mapping = MaterialMapping::Value(MaterialValue {
                theme: theme.clone(),
                value: value as u32,
            });
            material_mappings.push(mapping);
        } else if let Some(values) = &material.values {
            // Handle material values array
            let mut material_values = MaterialValues {
                theme: theme.clone(),
                solids: Vec::new(),
                shells: Vec::new(),
                vertices: Vec::new(),
            };

            // Process the material values based on their structure
            match values {
                // For MultiSurface/CompositeSurface: values is array of indices
                CjMaterialValues::Indices(indices) => {
                    // Convert indices to u32, replacing None with u32::MAX
                    material_values
                        .vertices
                        .extend(indices.iter().map(|i| i.map_or(u32::MAX, |v| v as u32)));
                }
                // For Solid/MultiSolid/CompositeSolid: values is nested array
                CjMaterialValues::Nested(nested) => {
                    // Check if this is a CompositeSolid (nested contains Nested) or a Solid (nested contains Indices)
                    let is_composite_solid = nested
                        .iter()
                        .any(|v| matches!(v, CjMaterialValues::Nested(_)));

                    if is_composite_solid {
                        // CompositeSolid case (test case 5)
                        // For each solid in the nested array
                        for solid in nested {
                            if let CjMaterialValues::Nested(shells_array) = solid {
                                // Push the number of shells in this solid
                                material_values.solids.push(shells_array.len() as u32);

                                // Process each shell
                                for shell in shells_array {
                                    if let CjMaterialValues::Indices(indices) = shell {
                                        material_values.shells.push(indices.len() as u32);
                                        material_values.vertices.extend(
                                            indices
                                                .iter()
                                                .map(|i| i.map_or(u32::MAX, |v| v as u32)),
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        // Solid case (test case 3)
                        // This is a single solid with multiple shells
                        material_values.solids.push(nested.len() as u32);

                        // Process each shell
                        for shell in nested {
                            if let CjMaterialValues::Indices(indices) = shell {
                                material_values.shells.push(indices.len() as u32);
                                material_values.vertices.extend(
                                    indices.iter().map(|i| i.map_or(u32::MAX, |v| v as u32)),
                                );
                            }
                        }
                    }
                }
            }

            material_mappings.push(MaterialMapping::Values(material_values));
        }
    }
    material_mappings
}

pub(crate) fn encode_texture(
    texture_map: &HashMap<String, CjTextureReference>,
) -> Vec<TextureMapping> {
    let mut texture_mappings = Vec::new();

    for (theme, texture) in texture_map {
        let mut texture_mapping = TextureMapping {
            theme: theme.clone(),
            solids: Vec::new(),
            shells: Vec::new(),
            surfaces: Vec::new(),
            strings: Vec::new(),
            vertices: Vec::new(),
        };

        // Process the texture values based on their structure
        let _ = encode_texture_values(&texture.values, &mut texture_mapping);

        texture_mappings.push(texture_mapping);
    }

    texture_mappings
}

/// Recursively encodes the texture values into flattened arrays.
///
/// # Arguments
///
/// * `values` - Reference to the texture values to encode.
/// * `mapping` - Mutable reference to the TextureMapping struct to populate.
///
/// # Returns
///
/// The maximum depth encountered during encoding.
fn encode_texture_values(values: &CjTextureValues, mapping: &mut TextureMapping) -> usize {
    match values {
        // Leaf node (indices)
        CjTextureValues::Indices(indices) => {
            // Record the number of indices in this ring
            mapping.strings.push(indices.len() as u32);

            // Convert indices to u32, replacing None with u32::MAX
            mapping
                .vertices
                .extend(indices.iter().map(|i| i.map_or(u32::MAX, |v| v as u32)));

            // Return the current depth level (1 for rings)
            1 // ring-level
        }
        // Nested structure
        CjTextureValues::Nested(nested) => {
            let mut max_depth = 0;

            // Recursively encode each nested value and track the maximum depth
            for sub in nested {
                let d = encode_texture_values(sub, mapping);
                max_depth = max_depth.max(d);
            }

            // Number of nested values at the current level
            let length = nested.len();

            // Interpret the `max_depth` to determine the current geometry type
            match max_depth {
                // max_depth = 1 indicates the children are rings, so this level represents surfaces
                1 => {
                    mapping.surfaces.push(length as u32);
                }
                // max_depth = 2 indicates the children are surfaces, so this level represents shells
                2 => {
                    mapping.shells.push(length as u32);
                }
                // max_depth = 3 indicates the children are shells, so this level represents solids
                3 => {
                    mapping.solids.push(length as u32);
                }
                // Any other depth is invalid and should be ignored
                _ => {}
            }

            // Return the updated depth level
            max_depth + 1
        }
    }
}

/// Recursively encodes the CityJSON boundaries into flattened arrays.
///
/// # Arguments
///
/// * `boundaries` - Reference to the CityJSON boundaries to encode.
///
/// # Returns
///
/// The maximum depth encountered during encoding.
///
/// # Panics
///
/// Panics if the `max_depth` is not 1, 2, or 3, indicating an invalid geometry nesting depth.
fn encode_boundaries(boundaries: &CjBoundaries, wip_boundaries: &mut GMBoundaries) -> usize {
    match boundaries {
        // ------------------
        // (1) Leaf (indices)
        // ------------------
        CjBoundaries::Indices(indices) => {
            // Extend the flat list of indices with the current ring's indices
            wip_boundaries.indices.extend_from_slice(indices);

            // Record the number of indices in the current ring
            wip_boundaries.strings.push(indices.len() as u32);

            // Return the current depth level (1 for rings)
            1 // ring-level
        }
        // ------------------
        // (2) Nested
        // ------------------
        CjBoundaries::Nested(sub_boundaries) => {
            let mut max_depth = 0;

            // Recursively encode each sub-boundary and track the maximum depth
            for sub in sub_boundaries {
                let d = encode_boundaries(sub, wip_boundaries);
                max_depth = max_depth.max(d);
            }

            // Number of sub-boundaries at the current level
            let length = sub_boundaries.len();

            // Interpret the `max_depth` to determine the current geometry type
            match max_depth {
                // max_depth = 1 indicates the children are rings, so this level represents surfaces
                1 => {
                    wip_boundaries.surfaces.push(length as u32);
                }
                // max_depth = 2 indicates the children are surfaces, so this level represents shells
                2 => {
                    // Push the number of surfaces in this shell
                    wip_boundaries.shells.push(length as u32);
                }
                // max_depth = 3 indicates the children are shells, so this level represents solids
                3 => {
                    // Push the number of shells in this solid
                    wip_boundaries.solids.push(length as u32);
                }
                // Any other depth is invalid and should panic
                _ => {}
            }

            // Return the updated depth level
            max_depth + 1
        }
    }
}

/// Encodes the semantic values into the encoder.
///
/// # Arguments
///
/// * `semantics_values` - Reference to the `SemanticsValues` to encode.
/// * `flattened` - Mutable reference to a vector where flattened semantics will be stored.
///
/// # Returns
///
/// The number of semantic values encoded.
fn encode_semantics_values(
    semantics_values: &CjSemanticsValues,
    flattened: &mut Vec<u32>,
) -> usize {
    match semantics_values {
        // ------------------
        // (1) Leaf (Indices)
        // ------------------
        CjSemanticsValues::Indices(indices) => {
            // Flatten the semantic values by converting each index to `Some(u32)`
            flattened.extend_from_slice(
                &indices
                    .iter()
                    .map(|i| if let Some(i) = i { *i } else { u32::MAX })
                    .collect::<Vec<_>>(),
            );

            flattened.len()
        }
        // ------------------
        // (2) Nested
        // ------------------
        CjSemanticsValues::Nested(nested) => {
            // Recursively encode each nested semantics value
            for sub in nested {
                encode_semantics_values(sub, flattened);
            }

            // Return the updated length of the flattened vector
            flattened.len()
        }
    }
}

/// Encodes semantic surfaces and values from a CityJSON Semantics object.
///
/// # Arguments
///
/// * `semantics` - Reference to the CityJSON Semantics object containing surfaces and values
pub(crate) fn encode_semantics(semantics: &CjSemantics) -> GMSemantics {
    let mut values = Vec::new();
    let _ = encode_semantics_values(&semantics.values, &mut values);

    GMSemantics {
        surfaces: semantics.surfaces.to_vec(),
        values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use cjseq::Geometry as CjGeometry;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn test_encode_boundaries() -> Result<()> {
        // MultiPoint
        let boundaries = json!([2, 44, 0, 7]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let encoded_boundaries = encode(&boundaries, None, None, None);
        assert_eq!(vec![2, 44, 0, 7], encoded_boundaries.boundaries.indices);
        assert_eq!(vec![4], encoded_boundaries.boundaries.strings);
        assert!(encoded_boundaries.boundaries.surfaces.is_empty());
        assert!(encoded_boundaries.boundaries.shells.is_empty());
        assert!(encoded_boundaries.boundaries.solids.is_empty());

        // MultiLineString
        let boundaries = json!([[2, 3, 5], [77, 55, 212]]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let encoded_boundaries = encode(&boundaries, None, None, None);

        assert_eq!(
            vec![2, 3, 5, 77, 55, 212],
            encoded_boundaries.boundaries.indices
        );
        assert_eq!(vec![3, 3], encoded_boundaries.boundaries.strings);
        assert_eq!(vec![2], encoded_boundaries.boundaries.surfaces);
        assert!(encoded_boundaries.boundaries.shells.is_empty());
        assert!(encoded_boundaries.boundaries.solids.is_empty());

        // MultiSurface
        let boundaries = json!([[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let encoded_boundaries = encode(&boundaries, None, None, None);

        assert_eq!(
            vec![0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4],
            encoded_boundaries.boundaries.indices
        );
        assert_eq!(vec![4, 4, 4], encoded_boundaries.boundaries.strings);
        assert_eq!(vec![1, 1, 1], encoded_boundaries.boundaries.surfaces);
        assert_eq!(vec![3], encoded_boundaries.boundaries.shells);
        assert!(encoded_boundaries.boundaries.solids.is_empty());

        // Solid
        let boundaries = json!([
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
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let encoded_boundaries = encode(&boundaries, None, None, None);

        assert_eq!(
            vec![
                0, 3, 2, 1, 22, 1, 2, 3, 4, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244,
                246, 724, 34, 414, 45, 111, 246, 5
            ],
            encoded_boundaries.boundaries.indices
        );
        assert_eq!(
            vec![5, 4, 4, 4, 4, 3, 3, 3, 3],
            encoded_boundaries.boundaries.strings
        );
        assert_eq!(
            vec![2, 1, 1, 1, 1, 1, 1, 1],
            encoded_boundaries.boundaries.surfaces
        );
        assert_eq!(vec![4, 4], encoded_boundaries.boundaries.shells);
        assert_eq!(vec![2], encoded_boundaries.boundaries.solids);

        // CompositeSolid
        let boundaries = json!([
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
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let encoded_boundaries = encode(&boundaries, None, None, None);
        assert_eq!(
            vec![
                0, 3, 2, 1, 22, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244, 246, 724,
                34, 414, 45, 111, 246, 5, 666, 667, 668, 74, 75, 76, 880, 881, 885, 111, 122, 226
            ],
            encoded_boundaries.boundaries.indices
        );
        assert_eq!(
            encoded_boundaries.boundaries.strings,
            vec![5, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3]
        );
        assert_eq!(
            encoded_boundaries.boundaries.surfaces,
            vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]
        );
        assert_eq!(encoded_boundaries.boundaries.shells, vec![4, 4, 4]);
        assert_eq!(encoded_boundaries.boundaries.solids, vec![2, 1]);

        Ok(())
    }

    #[test]
    fn test_encode_semantics() -> Result<()> {
        //MultiSurface
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
        let CjGeometry { semantics, .. } = multi_sufaces_geom;

        let encoded_semantics = encode_semantics(&semantics.unwrap());

        let expected_semantics_surfaces = vec![
            CjSemanticsSurface {
                thetype: "WallSurface".to_string(),
                parent: None,
                children: Some(vec![2]),
                other: Some(json!({
                    "slope": 33.4,
                })),
            },
            CjSemanticsSurface {
                thetype: "RoofSurface".to_string(),
                parent: None,
                children: None,
                other: Some(json!({
                    "slope": 66.6,
                })),
            },
            CjSemanticsSurface {
                thetype: "OuterCeilingSurface".to_string(),
                parent: Some(0),
                children: None,
                other: Some(json!({
                    "colour": "blue",
                })),
            },
        ];

        let expected_semantics_values = vec![0, 0, u32::MAX, 1, 2];
        assert_eq!(expected_semantics_surfaces, encoded_semantics.surfaces);
        assert_eq!(
            expected_semantics_values,
            encoded_semantics.values.as_slice().to_vec()
        );

        //CompositeSolid
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
          ]],
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
        let composite_solid_geom: CjGeometry = serde_json::from_value(composite_solid_gem_json)?;
        let CjGeometry { semantics, .. } = composite_solid_geom;

        let encoded_semantics = encode_semantics(&semantics.unwrap());

        let expected_semantics_surfaces = vec![
            CjSemanticsSurface {
                thetype: "RoofSurface".to_string(),
                parent: None,
                children: None,
                other: Some(json!({})),
            },
            CjSemanticsSurface {
                thetype: "WallSurface".to_string(),
                parent: None,
                children: None,
                other: Some(json!({})),
            },
        ];

        let expected_semantics_values: Vec<u32> =
            vec![0, 1, 1, u32::MAX, u32::MAX, u32::MAX, u32::MAX];
        assert_eq!(expected_semantics_surfaces, encoded_semantics.surfaces);
        assert_eq!(
            expected_semantics_values,
            encoded_semantics.values.as_slice().to_vec()
        );
        Ok(())
    }

    #[test]
    fn test_encode_material() -> Result<()> {
        // Test case 1: Single material value
        let mut materials = HashMap::new();
        materials.insert(
            "theme1".to_string(),
            CjMaterialReference {
                value: Some(5),
                values: None,
            },
        );

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
        let mut materials = HashMap::new();
        let multi_surface_values = CjMaterialValues::Indices(vec![Some(0), Some(1), None, Some(2)]);
        materials.insert(
            "theme2".to_string(),
            CjMaterialReference {
                value: None,
                values: Some(multi_surface_values),
            },
        );

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 1);
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.theme, "theme2");
                assert_eq!(values.vertices, vec![0, 1, u32::MAX, 2]);
                assert!(values.shells.is_empty());
                assert!(values.solids.is_empty());
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        // Test case 3: Solid material values
        let mut materials = HashMap::new();
        let solid_values = CjMaterialValues::Nested(vec![
            CjMaterialValues::Indices(vec![Some(0), Some(1), None]),
            CjMaterialValues::Indices(vec![Some(2), Some(3), Some(4)]),
        ]);
        materials.insert(
            "theme3".to_string(),
            CjMaterialReference {
                value: None,
                values: Some(solid_values),
            },
        );

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 1);
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.theme, "theme3");
                assert_eq!(values.solids, vec![2]); // 1 solid with 2 shells
                assert_eq!(values.shells, vec![3, 3]); // Each shell has 3 surfaces
                assert_eq!(values.vertices, vec![0, 1, u32::MAX, 2, 3, 4]);
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        // Test case 4: Multiple themes
        let mut materials = HashMap::new();
        materials.insert(
            "theme4".to_string(),
            CjMaterialReference {
                value: Some(7),
                values: None,
            },
        );
        materials.insert(
            "theme5".to_string(),
            CjMaterialReference {
                value: None,
                values: Some(CjMaterialValues::Indices(vec![Some(8), Some(9)])),
            },
        );

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 2);

        // Find and verify each mapping by theme name instead of relying on order
        let theme4_mapping = encoded
            .iter()
            .find(|m| match m {
                MaterialMapping::Value(v) => v.theme == "theme4",
                MaterialMapping::Values(v) => v.theme == "theme4",
            })
            .expect("Should have theme4 mapping");

        let theme5_mapping = encoded
            .iter()
            .find(|m| match m {
                MaterialMapping::Value(v) => v.theme == "theme5",
                MaterialMapping::Values(v) => v.theme == "theme5",
            })
            .expect("Should have theme5 mapping");

        // Verify theme4 mapping
        match theme4_mapping {
            MaterialMapping::Value(value) => {
                assert_eq!(value.theme, "theme4");
                assert_eq!(value.value, 7);
            }
            _ => panic!("Expected MaterialMapping::Value for theme4"),
        }

        // Verify theme5 mapping
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
        let mut materials = HashMap::new();
        let composite_solid_values = CjMaterialValues::Nested(vec![
            CjMaterialValues::Nested(vec![
                CjMaterialValues::Indices(vec![Some(0), Some(1), None]),
                CjMaterialValues::Indices(vec![Some(2), None, None]),
            ]),
            CjMaterialValues::Nested(vec![CjMaterialValues::Indices(vec![
                Some(3),
                Some(4),
                None,
            ])]),
        ]);
        materials.insert(
            "theme6".to_string(),
            CjMaterialReference {
                value: None,
                values: Some(composite_solid_values),
            },
        );

        let encoded = encode_material(&materials);
        assert_eq!(encoded.len(), 1);
        match &encoded[0] {
            MaterialMapping::Values(values) => {
                assert_eq!(values.theme, "theme6");
                assert_eq!(values.solids, vec![2, 1]); // Two solids, the first solid has 2 shells, the second solid has 1 shell
                assert_eq!(values.shells, vec![3, 3, 3]); // Each shell has 3 surfaces
                assert_eq!(
                    values.vertices,
                    vec![0, 1, u32::MAX, 2, u32::MAX, u32::MAX, 3, 4, u32::MAX]
                );
            }
            _ => panic!("Expected MaterialMapping::Values"),
        }

        Ok(())
    }

    #[test]
    fn test_encode_texture() -> Result<()> {
        // Create a theme for testing
        let theme = "test-theme".to_string();

        // MultiPoint-like texture values
        let texture_values = json!([0, 10, 20, null]);
        let texture_values: CjTextureValues = serde_json::from_value(texture_values)?;

        let mut textures = HashMap::new();
        textures.insert(
            theme.clone(),
            CjTextureReference {
                values: texture_values,
            },
        );

        let encoded = encode_texture(&textures);
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].theme, theme);
        assert_eq!(encoded[0].vertices, vec![0, 10, 20, u32::MAX]);
        assert_eq!(encoded[0].strings, vec![4]);
        assert!(encoded[0].surfaces.is_empty());
        assert!(encoded[0].shells.is_empty());
        assert!(encoded[0].solids.is_empty());

        // MultiLineString-like texture values
        let texture_values = json!([[0, 10, 20], [1, 11, null]]);
        let texture_values: CjTextureValues = serde_json::from_value(texture_values)?;

        let mut textures = HashMap::new();
        textures.insert(
            theme.clone(),
            CjTextureReference {
                values: texture_values,
            },
        );

        let encoded = encode_texture(&textures);
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].theme, theme);
        assert_eq!(encoded[0].vertices, vec![0, 10, 20, 1, 11, u32::MAX]);
        assert_eq!(encoded[0].strings, vec![3, 3]);
        assert_eq!(encoded[0].surfaces, vec![2]);
        assert!(encoded[0].shells.is_empty());
        assert!(encoded[0].solids.is_empty());

        // MultiSurface-like texture values
        let texture_values = json!([[[0, 10, 20, 30]], [[1, 11, 21, null]], [[2, 12, null, 32]]]);
        let texture_values: CjTextureValues = serde_json::from_value(texture_values)?;

        let mut textures = HashMap::new();
        textures.insert(
            theme.clone(),
            CjTextureReference {
                values: texture_values,
            },
        );

        let encoded = encode_texture(&textures);
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].theme, theme);
        assert_eq!(
            encoded[0].vertices,
            vec![0, 10, 20, 30, 1, 11, 21, u32::MAX, 2, 12, u32::MAX, 32]
        );
        assert_eq!(encoded[0].strings, vec![4, 4, 4]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![3]);
        assert!(encoded[0].solids.is_empty());

        // Solid-like texture values
        let texture_values = json!([
            [[[0, 10, 20, 30]], [[1, 11, 21, null]], [[2, 12, null, 32]]],
            [[[3, 13, 23, 33]], [[4, 14, 24, null]]]
        ]);
        let texture_values: CjTextureValues = serde_json::from_value(texture_values)?;

        let mut textures = HashMap::new();
        textures.insert(
            theme.clone(),
            CjTextureReference {
                values: texture_values,
            },
        );

        let encoded = encode_texture(&textures);
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].theme, theme);
        assert_eq!(
            encoded[0].vertices,
            vec![
                0,
                10,
                20,
                30,
                1,
                11,
                21,
                u32::MAX,
                2,
                12,
                u32::MAX,
                32,
                3,
                13,
                23,
                33,
                4,
                14,
                24,
                u32::MAX
            ]
        );
        assert_eq!(encoded[0].strings, vec![4, 4, 4, 4, 4]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![3, 2]);
        assert_eq!(encoded[0].solids, vec![2]);

        // CompositeSolid-like texture values
        let texture_values = json!([
            [
                [[[0, 10, 20]], [[1, 11, null]]],
                [[[2, 12, 22]], [[3, null, 23]]]
            ],
            [[[[4, 14, 24]], [[5, 15, 25]]]]
        ]);
        let texture_values: CjTextureValues = serde_json::from_value(texture_values)?;

        let mut textures = HashMap::new();
        textures.insert(
            theme.clone(),
            CjTextureReference {
                values: texture_values,
            },
        );

        let encoded = encode_texture(&textures);
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].theme, theme);
        assert_eq!(
            encoded[0].vertices,
            vec![
                0,
                10,
                20,
                1,
                11,
                u32::MAX,
                2,
                12,
                22,
                3,
                u32::MAX,
                23,
                4,
                14,
                24,
                5,
                15,
                25
            ]
        );
        assert_eq!(encoded[0].strings, vec![3, 3, 3, 3, 3, 3]);
        assert_eq!(encoded[0].surfaces, vec![1, 1, 1, 1, 1, 1]);
        assert_eq!(encoded[0].shells, vec![2, 2, 2]);
        assert_eq!(encoded[0].solids, vec![2, 1]);

        // Multiple themes
        let texture_values1 = json!([0, 10, 20]);
        let texture_values1: CjTextureValues = serde_json::from_value(texture_values1)?;

        let texture_values2 = json!([1, 11, null]);
        let texture_values2: CjTextureValues = serde_json::from_value(texture_values2)?;

        let mut textures = HashMap::new();
        textures.insert(
            "winter".to_string(),
            CjTextureReference {
                values: texture_values1,
            },
        );
        textures.insert(
            "summer".to_string(),
            CjTextureReference {
                values: texture_values2,
            },
        );

        let encoded = encode_texture(&textures);
        assert_eq!(encoded.len(), 2);

        // Find and verify each mapping by theme name instead of relying on order
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

        assert_eq!(summer_mapping.vertices, vec![1, 11, u32::MAX]);
        assert_eq!(summer_mapping.strings, vec![3]);

        Ok(())
    }
}
