use crate::fb::header_generated::Vec2;
use cjseq::{
    Appearance as CjAppearance, Boundaries as CjBoundaries, MaterialObject as CjMaterial,
    MaterialReference as CjMaterialReference, MaterialValues, Semantics as CjSemantics,
    SemanticsSurface as CjSemanticsSurface, SemanticsValues as CjSemanticsValues,
    TextureObject as CjTexture, TextureReference as CjTextureReference, TextureValues,
};
use flatbuffers::{FlatBufferBuilder, WIPOffset};
use serde_json::Value;
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
pub(crate) struct MaterialMapping {
    pub(crate) theme: String,
    pub(crate) solids: Vec<u32>,
    pub(crate) shells: Vec<u32>,
    pub(crate) vertices: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TextureMapping {
    pub(crate) theme: String,
    pub(crate) solids: Vec<u32>,
    pub(crate) shells: Vec<u32>,
    pub(crate) surfaces: Vec<u32>,
    pub(crate) strings: Vec<u32>,
    pub(crate) vertices: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GMAppearance {
    pub(crate) materials: Vec<MaterialMapping>,
    pub(crate) textures: Vec<TextureMapping>,
    pub(crate) vertices_texture: Vec<Vec2>,
    pub(crate) default_theme_texture: Option<String>,
    pub(crate) default_theme_material: Option<String>,
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
    pub(crate) appearance: Option<GMAppearance>,
}

/// Encodes the provided CityJSON boundaries and semantics into flattened arrays.
///
/// # Arguments
///
/// * `boundaries` - Reference to the CityJSON boundaries to encode.
/// * `semantics` - Optional reference to the semantics associated with the boundaries.
/// * `appearance` - Optional reference to the appearance data.
///
/// # Returns
/// Nothing.
pub(crate) fn encode(
    cj_boundaries: &CjBoundaries,
    semantics: Option<&CjSemantics>,
    appearance: Option<&CjAppearance>,
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
    let appearance = appearance.map(encode_appearance);

    EncodedGeometry {
        boundaries,
        semantics,
        appearance,
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
pub(crate) fn encode_appearance(appearance: &CjAppearance) -> GMAppearance {
    let mut gm_appearance = GMAppearance::default();

    // Handle materials if present
    if let Some(materials) = &appearance.materials {
        for material in materials {
            let mut mapping = MaterialMapping {
                theme: material.name.clone(),
                solids: Vec::new(),
                shells: Vec::new(),
                vertices: Vec::new(),
            };

            // Add material properties
            if let Some(value) = material.ambient_intensity {
                mapping.vertices.push(value as u32);
            }

            gm_appearance.materials.push(mapping);
        }
    }

    // Handle textures if present
    if let Some(textures) = &appearance.textures {
        for texture in textures {
            let mut mapping = TextureMapping {
                theme: texture.image.clone(),
                solids: Vec::new(),
                shells: Vec::new(),
                surfaces: Vec::new(),
                strings: Vec::new(),
                vertices: Vec::new(),
            };

            // Add texture properties
            if let Some(border_color) = &texture.border_color {
                mapping
                    .vertices
                    .extend(border_color.iter().map(|&x| x as u32));
            }

            gm_appearance.textures.push(mapping);
        }
    }

    // Handle vertices-texture if present
    if let Some(vertices) = &appearance.vertices_texture {
        gm_appearance.vertices_texture = vertices.iter().map(|v| Vec2::new(v[0], v[1])).collect();
    }

    // Handle default themes
    gm_appearance.default_theme_texture = appearance.default_theme_texture.clone();
    gm_appearance.default_theme_material = appearance.default_theme_material.clone();

    gm_appearance
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
    fn test_encode_appearance() -> Result<()> {
        let appearance_json = json!({
            "materials": [
                {
                    "name": "roofandground",
                    "ambientIntensity": 0.2000,
                    "diffuseColor": [0.9000, 0.1000, 0.7500],
                    "emissiveColor": [0.9000, 0.1000, 0.7500],
                    "specularColor": [0.9000, 0.1000, 0.7500],
                    "shininess": 0.2,
                    "transparency": 0.5,
                    "isSmooth": false,
                    "theme": "theme1",
                    "values": [0, 1, 2]
                }
            ],
            "textures": [
                {
                    "type": "PNG",
                    "image": "appearances/myroof.jpg",
                    "wrapMode": "wrap",
                    "textureType": "unknown",
                    "borderColor": [0.0, 0.1, 0.2, 1.0],
                    "theme": "theme2",
                    "values": [0, 1, 2, 3]
                }
            ],
            "vertices-texture": [
                [0.0, 0.5],
                [1.0, 0.0],
                [1.0, 1.0],
                [0.0, 1.0]
            ],
            "default-theme-texture": "theme2",
            "default-theme-material": "theme1"
        });

        let appearance: CjAppearance = serde_json::from_value(appearance_json)?;
        let encoded = encode_appearance(&appearance);

        // Check materials
        assert_eq!(encoded.materials.len(), 1);
        assert_eq!(encoded.materials[0].theme, "theme1");
        assert_eq!(encoded.materials[0].vertices, vec![0, 1, 2]);

        // Check textures
        assert_eq!(encoded.textures.len(), 1);
        assert_eq!(encoded.textures[0].theme, "theme2");
        assert_eq!(encoded.textures[0].vertices, vec![0, 1, 2, 3]);

        // Check vertices-texture
        assert_eq!(encoded.vertices_texture.len(), 4);
        assert_eq!(encoded.vertices_texture[0], Vec2::new(0.0, 0.5));
        assert_eq!(encoded.vertices_texture[1], Vec2::new(1.0, 0.0));
        assert_eq!(encoded.vertices_texture[2], Vec2::new(1.0, 1.0));
        assert_eq!(encoded.vertices_texture[3], Vec2::new(0.0, 1.0));

        // Check default themes
        assert_eq!(encoded.default_theme_texture.unwrap(), "theme2");
        assert_eq!(encoded.default_theme_material.unwrap(), "theme1");

        Ok(())
    }

    #[test]
    fn test_encode_boundaries() -> Result<()> {
        // MultiPoint
        let boundaries = json!([2, 44, 0, 7]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let encoded_boundaries = encode(&boundaries, None, None);
        assert_eq!(vec![2, 44, 0, 7], encoded_boundaries.boundaries.indices);
        assert_eq!(vec![4], encoded_boundaries.boundaries.strings);
        assert!(encoded_boundaries.boundaries.surfaces.is_empty());
        assert!(encoded_boundaries.boundaries.shells.is_empty());
        assert!(encoded_boundaries.boundaries.solids.is_empty());

        // MultiLineString
        let boundaries = json!([[2, 3, 5], [77, 55, 212]]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let encoded_boundaries = encode(&boundaries, None, None);

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
        let encoded_boundaries = encode(&boundaries, None, None);

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
        let encoded_boundaries = encode(&boundaries, None, None);

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
        let encoded_boundaries = encode(&boundaries, None, None);
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
                other: json!({
                    "slope": 33.4,
                }),
            },
            CjSemanticsSurface {
                thetype: "RoofSurface".to_string(),
                parent: None,
                children: None,
                other: json!({
                    "slope": 66.6,
                }),
            },
            CjSemanticsSurface {
                thetype: "OuterCeilingSurface".to_string(),
                parent: Some(0),
                children: None,
                other: json!({
                    "colour": "blue",
                }),
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
                other: json!({}),
            },
            CjSemanticsSurface {
                thetype: "WallSurface".to_string(),
                parent: None,
                children: None,
                other: json!({}),
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
}
