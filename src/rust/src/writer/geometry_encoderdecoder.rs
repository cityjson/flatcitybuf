use cjseq::{
    Boundaries as CjBoundaries, GeometryType as CjGeometryType, Semantics, SemanticsSurface,
    SemanticsValues,
};

use crate::feature_generated::{GeometryType, SemanticObject, SemanticSurfaceType};

#[derive(Debug, Clone, Default)]
pub struct FcbGeometryEncoderDecoder {
    solids: Vec<u32>,
    shells: Vec<u32>,
    surfaces: Vec<u32>,
    strings: Vec<u32>,
    indices: Vec<u32>,

    semantics_surfaces: Vec<SemanticsSurface>,
    semantics_values: Vec<Option<u32>>,
}

impl FcbGeometryEncoderDecoder {
    pub fn new() -> Self {
        Self {
            solids: vec![],
            shells: vec![],
            surfaces: vec![],
            strings: vec![],
            indices: vec![],
            semantics_values: vec![],
            semantics_surfaces: vec![],
        }
    }

    pub fn new_as_decoder(
        solids: Option<Vec<u32>>,
        shells: Option<Vec<u32>>,
        surfaces: Option<Vec<u32>>,
        strings: Option<Vec<u32>>,
        indices: Option<Vec<u32>>,
        semantics_values: Option<Vec<u32>>,
        semantics_surfaces: Option<Vec<SemanticObject>>,
    ) -> Self {
        let semantics_values = semantics_values.map(|values| {
            values
                .into_iter()
                .map(|v| (v != u32::MAX).then_some(v))
                .collect()
        });

        let semantics_surfaces =
            semantics_surfaces.map(|surfaces| Self::decode_semantics_surfaces(&surfaces));

        Self {
            solids: solids.unwrap_or_default(),
            shells: shells.unwrap_or_default(),
            surfaces: surfaces.unwrap_or_default(),
            strings: strings.unwrap_or_default(),
            indices: indices.unwrap_or_default(),
            semantics_values: semantics_values.unwrap_or_default(),
            semantics_surfaces: semantics_surfaces.unwrap_or_default(),
        }
    }
    pub fn encode(mut self, boundaries: &CjBoundaries, semantics: Option<&Semantics>) -> Self {
        self.encode_boundaries(boundaries);
        if let Some(semantics) = semantics {
            self.encode_semantics(semantics);
        }
        self
    }

    fn encode_boundaries(&mut self, boundaries: &CjBoundaries) -> usize {
        match boundaries {
            CjBoundaries::Indices(indices) => {
                let start_len = self.indices.len();
                self.indices.extend_from_slice(indices);
                let ring_size = self.indices.len() - start_len;
                self.strings.push(ring_size as u32);
                0 // Return 0 for direct indices (MultiPoint)
            }
            CjBoundaries::Nested(boundaries) => {
                let mut max_depth = 0;

                // First pass to determine the depth
                for sub in boundaries.iter() {
                    let d = self.encode_boundaries(sub);
                    max_depth = max_depth.max(d);
                }

                // For MultiSurface (depth 1), we need to push 1 for each surface
                if max_depth == 1 {
                    for _ in 0..boundaries.len() {
                        self.surfaces.push(1);
                    }
                } else {
                    match max_depth {
                        0 => (), // MultiPoint or LineString
                        2 => {
                            // For shells, count the number of surfaces
                            self.shells.push(boundaries.len() as u32);
                        }
                        3 => {
                            // For solids, count the number of shells
                            self.solids.push(boundaries.len() as u32);
                        }
                        _ => unreachable!("Invalid geometry nesting depth"),
                    }
                }
                max_depth + 1
            }
        }
    }

    fn encode_semantics_surface(&mut self, semantics_surfaces: &[SemanticsSurface]) -> usize {
        let index = self.semantics_surfaces.len();
        let count = semantics_surfaces.len();
        for s in semantics_surfaces {
            self.semantics_surfaces.push(s.clone());
        }
        let indices = (0..count)
            .map(|i| index as u32 + i as u32)
            .collect::<Vec<_>>();
        indices.len()
    }

    fn encode_semantics_values(
        &mut self,
        semantics_values: &SemanticsValues,
        flattened: &mut Vec<Option<u32>>,
    ) -> usize {
        match semantics_values {
            SemanticsValues::Indices(indices) => {
                flattened.extend_from_slice(&indices.iter().map(|x| Some(*x)).collect::<Vec<_>>());
                self.semantics_values
                    .extend_from_slice(&indices.iter().map(|x| Some(*x)).collect::<Vec<_>>());
                flattened.len()
            }
            SemanticsValues::Nested(nested) => {
                for sub in nested {
                    self.encode_semantics_values(sub, flattened);
                }
                flattened.len()
            }
        }
    }

    pub fn encode_semantics(&mut self, semantics: &Semantics) {
        self.encode_semantics_surface(&semantics.surfaces);
        let mut values = Vec::new();
        self.encode_semantics_values(&semantics.values, &mut values);
    }

    pub fn boundaries(&self) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
        (
            self.solids.clone(),
            self.shells.clone(),
            self.surfaces.clone(),
            self.strings.clone(),
            self.indices.clone(),
        )
    }

    pub fn semantics(&self) -> (&[SemanticsSurface], &[Option<u32>]) {
        (&self.semantics_surfaces, &self.semantics_values)
    }

    pub fn decode(&self) -> CjBoundaries {
        let mut shell_cursor = 0;
        let mut surface_cursor = 0;
        let mut ring_cursor = 0;
        let mut index_cursor = 0;

        if !self.solids.is_empty() {
            let mut solids_vec = Vec::new();
            for &shell_count in &self.solids {
                let mut shell_vec = Vec::new();
                for _ in 0..shell_count {
                    let surfaces_in_shell = self.shells[shell_cursor] as usize;
                    shell_cursor += 1;

                    let mut surface_vec = Vec::new();
                    for _ in 0..surfaces_in_shell {
                        let rings_in_surface = self.surfaces[surface_cursor] as usize;
                        surface_cursor += 1;

                        let mut ring_vec = Vec::new();
                        for _ in 0..rings_in_surface {
                            let ring_size = self.strings[ring_cursor] as usize;
                            ring_cursor += 1;

                            let ring_indices = self.indices[index_cursor..index_cursor + ring_size]
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
        } else if !self.shells.is_empty() {
            let mut shell_vec = Vec::new();
            for &surface_count in &self.shells {
                let mut surface_vec = Vec::new();
                for _ in 0..surface_count {
                    let rings_in_surface = self.surfaces[surface_cursor] as usize;
                    surface_cursor += 1;

                    let mut ring_vec = Vec::new();
                    for _ in 0..rings_in_surface {
                        let ring_size = self.strings[ring_cursor] as usize;
                        ring_cursor += 1;
                        let ring_indices = self.indices[index_cursor..index_cursor + ring_size]
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
        } else if !self.surfaces.is_empty() {
            let mut surface_vec = Vec::new();
            for &rings_count in &self.surfaces {
                let mut ring_vec = Vec::new();
                for _ in 0..rings_count {
                    let ring_size = self.strings[ring_cursor] as usize;
                    ring_cursor += 1;
                    let ring_indices = self.indices[index_cursor..index_cursor + ring_size]
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
        } else if !self.strings.is_empty() {
            let mut ring_vec = Vec::new();
            for &ring_size in &self.strings {
                let ring_indices = self.indices[index_cursor..index_cursor + ring_size as usize]
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
            CjBoundaries::Indices(self.indices.clone())
        }
    }

    pub fn decode_semantics_surfaces(
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
                other: serde_json::Value::Null,
                // TODO: Think how to handle `other`
            }
        });
        surfaces.collect()
    }

    fn decode_semantics_values(
        &self,
        depth: i8,
        solids_cursor: &mut usize,
        shells_cursor: &mut usize,
        surface_cursor: &mut usize,
        semantics_values: &[u32],
        semantics_pos: &mut usize,
    ) -> SemanticsValues {
        if depth <= 0 {
            let mut leaf = Vec::with_capacity(semantics_values.len());
            while *semantics_pos < semantics_values.len() {
                let val = semantics_values[*semantics_pos];
                *semantics_pos += 1;
                if val == u32::MAX {
                    leaf.push(None);
                } else {
                    leaf.push(Some(val));
                }
            }
            return SemanticsValues::Indices(
                leaf.iter()
                    .map(|x| match x {
                        Some(v) => *v,
                        None => 0, //TODO: Fix this, this should be null
                    })
                    .collect(),
            );
        }

        match depth {
            3 => {
                let mut results = Vec::new();
                for &shell_count in &self.solids[*solids_cursor..] {
                    *solids_cursor += 1;
                    let mut items = Vec::new();
                    for _ in 0..shell_count {
                        let subvals = self.decode_semantics_values(
                            depth - 1,
                            solids_cursor,
                            shells_cursor,
                            surface_cursor,
                            semantics_values,
                            semantics_pos,
                        );
                        items.push(subvals);
                    }
                    results.push(SemanticsValues::Nested(items));
                    if *solids_cursor >= self.solids.len() {
                        break;
                    }
                }
                if results.len() == 1 {
                    results.into_iter().next().unwrap()
                } else {
                    SemanticsValues::Nested(results)
                }
            }
            2 => {
                let mut results = Vec::new();
                for &surface_count in &self.shells[*shells_cursor..] {
                    *shells_cursor += 1;
                    let mut items = Vec::new();
                    for _ in 0..surface_count {
                        let subvals = self.decode_semantics_values(
                            depth - 1,
                            solids_cursor,
                            shells_cursor,
                            surface_cursor,
                            semantics_values,
                            semantics_pos,
                        );
                        items.push(subvals);
                    }
                    results.push(SemanticsValues::Nested(items));

                    if *shells_cursor >= self.shells.len() {
                        break;
                    }
                }
                if results.len() == 1 {
                    results.into_iter().next().unwrap()
                } else {
                    SemanticsValues::Nested(results)
                }
            }
            1 => {
                let mut results = Vec::new();
                for &rings_count in &self.surfaces[*surface_cursor..] {
                    *surface_cursor += 1;
                    let mut items = Vec::new();
                    for _ in 0..rings_count {
                        // each sub-item is depth-1 => 0 => leaf array
                        let subvals = self.decode_semantics_values(
                            depth - 1,
                            solids_cursor,
                            shells_cursor,
                            surface_cursor,
                            semantics_values,
                            semantics_pos,
                        );
                        items.push(subvals);
                    }
                    results.push(SemanticsValues::Nested(items));

                    if *surface_cursor >= self.surfaces.len() {
                        break;
                    }
                }
                if results.len() == 1 {
                    results.into_iter().next().unwrap()
                } else {
                    SemanticsValues::Nested(results)
                }
            }
            _ => {
                unreachable!("Unexpected depth in decode_semantics_values_recursive()");
            }
        }
    }

    fn geometry_depth(geometry_type: GeometryType) -> i8 {
        match geometry_type {
            GeometryType::MultiPoint => 0,
            GeometryType::MultiLineString => 1,
            GeometryType::MultiSurface | GeometryType::CompositeSurface => 2,
            GeometryType::Solid => 3,
            GeometryType::MultiSolid | GeometryType::CompositeSolid => 4,
            _ => 3,
        }
    }
    pub fn decode_semantics(
        &self,
        geometry_type: GeometryType,
        semantics_objects: &[SemanticObject],
        semantics_values: &[u32],
    ) -> Semantics {
        let surfaces = Self::decode_semantics_surfaces(semantics_objects);

        let depth = Self::geometry_depth(geometry_type) - 2;
        let mut solids_cursor = 0;
        let mut shells_cursor = 0;
        let mut surface_cursor = 0;
        let mut semantics_pos = 0;
        let values = self.decode_semantics_values(
            depth,
            &mut solids_cursor,
            &mut shells_cursor,
            &mut surface_cursor,
            semantics_values,
            &mut semantics_pos,
        );

        Semantics { values, surfaces }
    }
}

impl GeometryType {
    pub fn to_string(&self) -> &'static str {
        match *self {
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

    pub fn to_cj(&self) -> CjGeometryType {
        match *self {
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
    use super::*;
    use crate::feature_generated::SemanticObject;
    use anyhow::Result;
    use cjseq::*;
    use serde_json::json;

    #[test]
    fn test_encode() -> Result<()> {
        // MultiPoint
        let boundaries = json!([2, 44, 0, 7]);
        let boundaries: NestedArray = serde_json::from_value(boundaries)?;
        let encoder = FcbGeometryEncoderDecoder::new().encode(&boundaries, None);
        println!("{:?}", encoder);
        assert_eq!(encoder.indices, vec![2, 44, 0, 7]);
        assert_eq!(encoder.strings, vec![4]);
        assert!(encoder.surfaces.is_empty());
        assert!(encoder.shells.is_empty());
        assert!(encoder.solids.is_empty());

        // MultiLineString
        let boundaries = json!([[2, 3, 5], [77, 55, 212]]);
        let boundaries: NestedArray = serde_json::from_value(boundaries)?;
        let encoder = FcbGeometryEncoderDecoder::new().encode(&boundaries, None);
        assert_eq!(encoder.indices, vec![2, 3, 5, 77, 55, 212]);
        assert_eq!(encoder.strings, vec![3, 3]);
        assert!(encoder.surfaces.is_empty());
        assert!(encoder.shells.is_empty());
        assert!(encoder.solids.is_empty());

        // MultiSurface
        let boundaries = json!([[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]]);
        let boundaries: NestedArray = serde_json::from_value(boundaries)?;
        let encoder = FcbGeometryEncoderDecoder::new().encode(&boundaries, None);
        assert_eq!(encoder.indices, vec![0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4]);
        assert_eq!(encoder.strings, vec![4, 4, 4]);
        assert_eq!(encoder.surfaces, vec![1, 1, 1]);
        assert!(encoder.shells.is_empty());
        assert!(encoder.solids.is_empty());

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
        let boundaries: NestedArray = serde_json::from_value(boundaries)?;
        let encoder = FcbGeometryEncoderDecoder::new().encode(&boundaries, None);
        assert_eq!(
            encoder.indices,
            vec![
                0, 3, 2, 1, 22, 1, 2, 3, 4, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244,
                246, 724, 34, 414, 45, 111, 246, 5
            ]
        );
        assert_eq!(encoder.strings, vec![5, 4, 4, 4, 4, 3, 3, 3, 3]);
        assert_eq!(encoder.surfaces, vec![2, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(encoder.shells, vec![4, 4]);
        assert_eq!(encoder.solids, vec![2]);

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
        let boundaries: NestedArray = serde_json::from_value(boundaries)?;
        let encoder = FcbGeometryEncoderDecoder::new().encode(&boundaries, None);
        assert_eq!(
            encoder.indices,
            vec![
                0, 3, 2, 1, 22, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244, 246, 724,
                34, 414, 45, 111, 246, 5, 666, 667, 668, 74, 75, 76, 880, 881, 885, 111, 122, 226
            ]
        );
        assert_eq!(encoder.strings, vec![5, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3]);
        assert_eq!(encoder.surfaces, vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(encoder.shells, vec![4, 4, 4]);
        assert_eq!(encoder.solids, vec![2, 1]);

        Ok(())
    }

    // #[test]
    // fn test_encode_boundaries_nested() -> Result<()> {
    //     let mut encoder = FcbGeometryEncoderDecoder::new();
    //     let nested = CjBoundaries::Nested(vec![
    //         CjBoundaries::Indices(vec![1, 2]),
    //         CjBoundaries::Indices(vec![3, 4]),
    //     ]);

    //     encoder.encode_boundaries(&nested);

    //     assert_eq!(encoder.indices, vec![1, 2, 3, 4]);
    //     assert_eq!(encoder.strings, vec![2, 2]);
    //     assert_eq!(encoder.surfaces, vec![2]);
    //     Ok(())
    // }

    // #[test]
    // fn test_encode_semantics() -> Result<()> {
    //     let mut encoder = FcbGeometryEncoderDecoder::new();
    //     let surfaces = vec![SemanticsSurface {
    //         thetype: "RoofSurface".to_string(),
    //         parent: None,
    //         children: None,
    //         other: serde_json::Value::Null,
    //     }];
    //     let values = SemanticsValues::Indices(vec![0, 1]);
    //     let semantics = Semantics { surfaces, values };

    //     encoder.encode_semantics(&semantics);

    //     assert_eq!(encoder.semantics_surfaces.len(), 1);
    //     assert_eq!(encoder.semantics_surfaces[0].thetype, "RoofSurface");
    //     assert_eq!(encoder.semantics_values, vec![Some(0), Some(1)]);
    //     Ok(())
    // }

    #[test]
    fn test_decode() -> Result<()> {
        // MultiPoint
        let boundaries_value = json!([2, 44, 0, 7]);
        let expected: NestedArray = serde_json::from_value(boundaries_value)?;
        let indices = vec![2, 44, 0, 7];
        let strings = vec![4];
        let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
            None,
            None,
            None,
            Some(strings),
            Some(indices),
            None,
            None,
        );
        let boundaries = decoder.decode();
        assert_eq!(expected, boundaries);

        // MultiLineString
        let boundaries_value = json!([[2, 3, 5], [77, 55, 212]]);
        let expected: NestedArray = serde_json::from_value(boundaries_value)?;
        let indices = vec![2, 3, 5, 77, 55, 212];
        let strings = vec![3, 3];
        let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
            None,
            None,
            None,
            Some(strings),
            Some(indices),
            None,
            None,
        );
        let boundaries = decoder.decode();
        assert_eq!(expected, boundaries);

        // MultiSurface
        let boundaries_value = json!([[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]]);
        let expected: NestedArray = serde_json::from_value(boundaries_value)?;
        let indices = vec![0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5];
        let strings = vec![4, 4, 4];
        let surfaces = vec![1, 1, 1];
        let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
            None,
            None,
            Some(surfaces),
            Some(strings),
            Some(indices),
            None,
            None,
        );
        let boundaries = decoder.decode();
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
        let expected: NestedArray = serde_json::from_value(boundaries_value)?;
        let indices = vec![
            0, 3, 2, 1, 22, 1, 2, 3, 4, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244,
            246, 724, 34, 414, 45, 111, 246, 5,
        ];
        let strings = vec![5, 4, 4, 4, 4, 3, 3, 3, 3];
        let surfaces = vec![2, 1, 1, 1, 1, 1, 1, 1];
        let shells = vec![4, 4];
        let solids = vec![2];
        let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
            Some(solids),
            Some(shells),
            Some(surfaces),
            Some(strings),
            Some(indices),
            None,
            None,
        );
        let boundaries = decoder.decode();
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
        let expected: NestedArray = serde_json::from_value(boundaries_value)?;
        let indices = vec![
            0, 3, 2, 1, 22, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244, 246, 724, 34,
            414, 45, 111, 246, 5, 666, 667, 668, 74, 75, 76, 880, 881, 885, 111, 122, 226,
        ];
        let strings = vec![5, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3];
        let surfaces = vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
        let shells = vec![4, 4, 4, 4];
        let solids = vec![2, 1];
        let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
            Some(solids),
            Some(shells),
            Some(surfaces),
            Some(strings),
            Some(indices),
            None,
            None,
        );
        let boundaries = decoder.decode();
        assert_eq!(expected, boundaries);

        Ok(())
    }
}
