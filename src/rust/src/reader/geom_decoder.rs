use cjseq::{
    Boundaries as CjBoundaries, GeometryType as CjGeometryType, Semantics, SemanticsSurface,
    SemanticsValues,
};

use crate::feature_generated::{GeometryType, SemanticObject, SemanticSurfaceType};

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
pub fn decode(
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
pub fn decode_semantics_surfaces(semantics_objects: &[SemanticObject]) -> Vec<SemanticsSurface> {
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
pub fn decode_semantics(
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

#[cfg(test)]
mod tests {
    use crate::{
        feature_generated::{
            root_as_city_feature, CityFeature, CityFeatureArgs, CityObject, CityObjectArgs,
            GeometryType,
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
}
