use cjseq::{
    Boundaries as CjBoundaries, Semantics as CjSemantics, SemanticsSurface as CjSemanticsSurface,
    SemanticsValues as CjSemanticsValues,
};

#[derive(Debug, Clone, Default)]
pub(crate) struct GMBoundaries {
    pub(crate) solids: Vec<u32>,   // Number of shells per solid
    pub(crate) shells: Vec<u32>,   // Number of surfaces per shell
    pub(crate) surfaces: Vec<u32>, // Number of rings per surface
    pub(crate) strings: Vec<u32>,  // Number of indices per ring
    pub(crate) indices: Vec<u32>,  // Flattened list of all indices
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
}

/// Encodes the provided CityJSON boundaries and semantics into flattened arrays.
///
/// # Arguments
///
/// * `boundaries` - Reference to the CityJSON boundaries to encode.
/// * `semantics` - Optional reference to the semantics associated with the boundaries.
///
/// # Returns
/// Nothing.
pub(crate) fn encode(
    cj_boundaries: &CjBoundaries,
    semantics: Option<&CjSemantics>,
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

    EncodedGeometry {
        boundaries,
        semantics,
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
        let encoded_boundaries = encode(&boundaries, None);
        assert_eq!(vec![2, 44, 0, 7], encoded_boundaries.boundaries.indices);
        assert_eq!(vec![4], encoded_boundaries.boundaries.strings);
        assert!(encoded_boundaries.boundaries.surfaces.is_empty());
        assert!(encoded_boundaries.boundaries.shells.is_empty());
        assert!(encoded_boundaries.boundaries.solids.is_empty());

        // MultiLineString
        let boundaries = json!([[2, 3, 5], [77, 55, 212]]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let encoded_boundaries = encode(&boundaries, None);

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
        let encoded_boundaries = encode(&boundaries, None);

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
        let encoded_boundaries = encode(&boundaries, None);

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
        let encoded_boundaries = encode(&boundaries, None);
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
