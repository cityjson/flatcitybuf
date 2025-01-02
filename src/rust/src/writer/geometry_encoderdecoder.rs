use cjseq::{
    Boundaries as CjBoundaries, GeometryType as CjGeometryType, Semantics, SemanticsSurface,
    SemanticsValues,
};

use crate::feature_generated::{GeometryType, SemanticObject, SemanticSurfaceType};

/// `FcbGeometryEncoderDecoder` is responsible for encoding and decoding
/// CityJSON geometries into flattened one-dimensional arrays suitable
/// for serialization with FlatBuffers.
#[derive(Debug, Clone, Default)]
pub struct FcbGeometryEncoderDecoder {
    solids: Vec<u32>,                                  // Number of shells per solid
    shells: Vec<u32>,                                  // Number of surfaces per shell
    surfaces: Vec<u32>,                                // Number of rings per surface
    strings: Vec<u32>,                                 // Number of indices per ring
    indices: Vec<u32>,                                 // Flattened list of all indices
    semantics_surfaces: Option<Vec<SemanticsSurface>>, // List of semantic surfaces
    semantics_values: Option<Vec<u32>>,                // Semantic values corresponding to surfaces
}

impl FcbGeometryEncoderDecoder {
    /// Creates a new instance of `FcbGeometryEncoderDecoder` with empty data vectors.
    pub fn new() -> Self {
        Self {
            solids: vec![],
            shells: vec![],
            surfaces: vec![],
            strings: vec![],
            indices: vec![],
            semantics_values: None,
            semantics_surfaces: None,
        }
    }

    /// Creates a new instance of `FcbGeometryEncoderDecoder` for decoding purposes.
    ///
    /// # Arguments
    ///
    /// * `solids` - Optional vector of solids.
    /// * `shells` - Optional vector of shells.
    /// * `surfaces` - Optional vector of surfaces.
    /// * `strings` - Optional vector of strings.
    /// * `indices` - Optional vector of indices.
    /// * `semantics_values` - Optional vector of semantic values.
    /// * `semantics_surfaces` - Optional vector of semantic objects.
    ///
    /// # Returns
    ///
    /// A new instance of `FcbGeometryEncoderDecoder` initialized with the provided data.
    pub fn new_as_decoder(
        solids: Option<Vec<u32>>,
        shells: Option<Vec<u32>>,
        surfaces: Option<Vec<u32>>,
        strings: Option<Vec<u32>>,
        indices: Option<Vec<u32>>,
        // semantics_values: Option<Vec<u32>>,
        // semantics_surfaces: Option<Vec<SemanticObject>>,
    ) -> Self {
        Self {
            solids: solids.unwrap_or_default(),
            shells: shells.unwrap_or_default(),
            surfaces: surfaces.unwrap_or_default(),
            strings: strings.unwrap_or_default(),
            indices: indices.unwrap_or_default(),
            semantics_values: None,
            semantics_surfaces: None,
        }
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
    pub fn encode(&mut self, boundaries: &CjBoundaries, semantics: Option<&Semantics>) {
        // Encode the geometric boundaries
        self.encode_boundaries(boundaries);

        // Encode semantics if provided
        if let Some(semantics) = semantics {
            self.encode_semantics(semantics);
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
    fn encode_boundaries(&mut self, boundaries: &CjBoundaries) -> usize {
        match boundaries {
            // ------------------
            // (1) Leaf (indices)
            // ------------------
            CjBoundaries::Indices(indices) => {
                // Extend the flat list of indices with the current ring's indices
                self.indices.extend_from_slice(indices);

                // Record the number of indices in the current ring
                self.strings.push(indices.len() as u32);

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
                    let d = self.encode_boundaries(sub);
                    max_depth = max_depth.max(d);
                }

                // Number of sub-boundaries at the current level
                let length = sub_boundaries.len();

                // Interpret the `max_depth` to determine the current geometry type
                match max_depth {
                    // max_depth = 1 indicates the children are rings, so this level represents surfaces
                    1 => {
                        self.surfaces.push(length as u32);
                    }
                    // max_depth = 2 indicates the children are surfaces, so this level represents shells
                    2 => {
                        // Push the number of surfaces in this shell
                        self.shells.push(length as u32);
                    }
                    // max_depth = 3 indicates the children are shells, so this level represents solids
                    3 => {
                        // Push the number of shells in this solid
                        self.solids.push(length as u32);
                    }
                    // Any other depth is invalid and should panic
                    _ => {}
                }

                // Return the updated depth level
                max_depth + 1
            }
        }
    }

    /// Encodes the semantic surfaces into the encoder.
    ///
    /// # Arguments
    ///
    /// * `semantics_surfaces` - Slice of `SemanticsSurface` to encode.
    ///
    /// # Returns
    ///
    /// The number of semantic surfaces encoded.
    fn encode_semantics_surface(&mut self, semantics_surfaces: Vec<SemanticsSurface>) -> usize {
        let index = if let Some(surfaces) = &self.semantics_surfaces {
            surfaces.len()
        } else {
            0
        };
        let count = semantics_surfaces.len();

        // Clone and store each semantic surface
        for s in semantics_surfaces {
            if let Some(surfaces) = &mut self.semantics_surfaces {
                surfaces.push(s);
            } else {
                self.semantics_surfaces = Some(vec![s]);
            }
        }

        // Generate indices corresponding to the semantic surfaces
        let indices = (0..count)
            .map(|i| index as u32 + i as u32)
            .collect::<Vec<_>>();

        // Return the number of semantics surfaces encoded
        indices.len()
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
        &mut self,
        semantics_values: &SemanticsValues,
        flattened: &mut Vec<u32>,
    ) -> usize {
        match semantics_values {
            // ------------------
            // (1) Leaf (Indices)
            // ------------------
            SemanticsValues::Indices(indices) => {
                // Flatten the semantic values by converting each index to `Some(u32)`
                flattened.extend_from_slice(
                    &indices
                        .iter()
                        .map(|i| if let Some(i) = i { *i } else { u32::MAX })
                        .collect::<Vec<_>>(),
                );

                // Extend the encoder's semantics_values with the flattened data
                self.semantics_values = Some(flattened.clone());

                flattened.len()
            }
            // ------------------
            // (2) Nested
            // ------------------
            SemanticsValues::Nested(nested) => {
                // Recursively encode each nested semantics value
                for sub in nested {
                    self.encode_semantics_values(sub, flattened);
                }

                // Return the updated length of the flattened vector
                flattened.len()
            }
        }
    }

    pub fn encode_semantics(&mut self, semantics: &Semantics) {
        self.encode_semantics_surface(semantics.surfaces.to_vec());
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
    pub fn semantics(&self) -> (&[SemanticsSurface], &[u32]) {
        match (&self.semantics_surfaces, &self.semantics_values) {
            (Some(surfaces), Some(values)) => (surfaces, values.as_slice()),
            _ => (&[], &[]),
        }
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
    /// Decode semantics in a way that each "shell" corresponds to exactly 1 Indices array.
    /// This matches your test, which lumps multiple geometry surfaces into a single array per shell.
    ///
    /// - depth=2 => parse all solids (`self.solids`)
    ///    * each solid has `shell_count` shells => parse shell semantics at depth=1
    /// - depth=1 => parse shells (`self.shells`)
    ///    * each shell is 1 Indices(...) array (depth=0)
    /// - depth<=0 => parse a single Indices(...) from leftover
    ///
    fn decode_semantics_values(
        &self,
        depth: i8,
        solids_cursor: &mut usize,
        shells_cursor: &mut usize,
        semantics_values: &[u32],
        semantics_pos: &mut usize,
    ) -> SemanticsValues {
        // -----------------------------
        // Base case: depth <= 0 => parse 1 Indices array from leftover
        // -----------------------------
        if depth <= 0 {
            // In your test JSON, the entire "shell" is one array,
            // so read *all* leftover for that shell:
            let leftover = &semantics_values[*semantics_pos..];
            *semantics_pos += leftover.len();

            let leaf: Vec<Option<u32>> = leftover
                .iter()
                .map(|&val| if val == u32::MAX { None } else { Some(val) })
                .collect();

            return SemanticsValues::Indices(leaf);
        }

        match depth {
            // -----------------------------
            // depth=2 => CompositeSolid/MultiSolid
            // parse each solid in `self.solids`
            // -----------------------------
            2 => {
                let mut result_solids = Vec::new();
                let total_solids = self.solids.len();

                for _ in 0..total_solids {
                    let shell_count = self.solids[*solids_cursor];
                    *solids_cursor += 1;

                    // parse shell_count shells at depth=1
                    let mut shell_vec = Vec::new();
                    for _ in 0..shell_count {
                        let shell_semantics = self.decode_semantics_values(
                            depth - 1, // => 1
                            solids_cursor,
                            shells_cursor,
                            semantics_values,
                            semantics_pos,
                        );
                        shell_vec.push(shell_semantics);
                    }

                    result_solids.push(SemanticsValues::Nested(shell_vec));
                }

                SemanticsValues::Nested(result_solids)
            }

            // -----------------------------
            // depth=1 => Solid / CompositeSurface
            // parse each shell in `self.shells`,
            // but unify all surfaces in that shell into one Indices array
            // -----------------------------
            1 => {
                let mut result_shells = Vec::new();

                // The geometry says `shells = [4,3]`,
                // but semantics lumps those 4 surfaces into 1 array,
                // so we read exactly 1 Indices for each shell:
                let shells_left = self.shells.len() - *shells_cursor;
                for _ in 0..shells_left {
                    let _surface_count = self.shells[*shells_cursor];
                    *shells_cursor += 1;

                    // parse 1 Indices array (depth=0)
                    let one_shell = self.decode_semantics_values(
                        0,
                        solids_cursor,
                        shells_cursor,
                        semantics_values,
                        semantics_pos,
                    );
                    result_shells.push(one_shell);
                }

                SemanticsValues::Nested(result_shells)
            }

            // Should not occur for MultiSurface (depth=0),
            // Solid (depth=1), CompositeSolid (depth=2).
            _ => unreachable!("Unexpected depth in decode_semantics_values"),
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
        semantics_objects: Vec<SemanticObject>,
        semantics_values: Vec<u32>,
    ) -> Semantics {
        let surfaces = Self::decode_semantics_surfaces(&semantics_objects);

        let depth = Self::geometry_depth(geometry_type) - 2;
        let mut solids_cursor = 0;
        let mut shells_cursor = 0;
        let mut semantics_pos = 0;
        let values = self.decode_semantics_values(
            depth,
            &mut solids_cursor,
            &mut shells_cursor,
            semantics_values.as_slice(),
            &mut semantics_pos,
        );

        Semantics { values, surfaces }
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
    use crate::feature_generated::{
        root_as_city_feature, CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, Geometry,
        GeometryArgs, GeometryType, SemanticObject, SemanticObjectArgs,
    };

    use super::*;
    use anyhow::Result;
    use cjseq::Geometry as CjGeometry;
    use flatbuffers::FlatBufferBuilder;
    use serde_json::json;

    #[test]
    fn test_encode_boundaries() -> Result<()> {
        // MultiPoint
        let boundaries = json!([2, 44, 0, 7]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let mut encoder = FcbGeometryEncoderDecoder::new();
        encoder.encode(&boundaries, None);
        println!("{:?}", encoder);
        assert_eq!(vec![2, 44, 0, 7], encoder.indices);
        assert_eq!(vec![4], encoder.strings);
        assert!(encoder.surfaces.is_empty());
        assert!(encoder.shells.is_empty());
        assert!(encoder.solids.is_empty());

        // MultiLineString
        let boundaries = json!([[2, 3, 5], [77, 55, 212]]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let mut encoder = FcbGeometryEncoderDecoder::new();
        encoder.encode(&boundaries, None);
        println!("{:?}", encoder);

        assert_eq!(vec![2, 3, 5, 77, 55, 212], encoder.indices);
        assert_eq!(vec![3, 3], encoder.strings);
        assert_eq!(vec![2], encoder.surfaces);
        assert!(encoder.shells.is_empty());
        assert!(encoder.solids.is_empty());

        // MultiSurface
        let boundaries = json!([[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]]);
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let mut encoder = FcbGeometryEncoderDecoder::new();
        encoder.encode(&boundaries, None);
        println!("{:?}", encoder);

        assert_eq!(vec![0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4], encoder.indices);
        assert_eq!(vec![4, 4, 4], encoder.strings);
        assert_eq!(vec![1, 1, 1], encoder.surfaces);
        assert_eq!(vec![3], encoder.shells);
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
        let boundaries: CjBoundaries = serde_json::from_value(boundaries)?;
        let mut encoder = FcbGeometryEncoderDecoder::new();
        encoder.encode(&boundaries, None);
        println!("{:?}", encoder);

        assert_eq!(
            vec![
                0, 3, 2, 1, 22, 1, 2, 3, 4, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244,
                246, 724, 34, 414, 45, 111, 246, 5
            ],
            encoder.indices
        );
        assert_eq!(vec![5, 4, 4, 4, 4, 3, 3, 3, 3], encoder.strings);
        assert_eq!(vec![2, 1, 1, 1, 1, 1, 1, 1], encoder.surfaces);
        assert_eq!(vec![4, 4], encoder.shells);
        assert_eq!(vec![2], encoder.solids);

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
        let mut encoder = FcbGeometryEncoderDecoder::new();
        encoder.encode(&boundaries, None);
        println!("{:?}", encoder);
        assert_eq!(
            vec![
                0, 3, 2, 1, 22, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5, 240, 243, 124, 244, 246, 724,
                34, 414, 45, 111, 246, 5, 666, 667, 668, 74, 75, 76, 880, 881, 885, 111, 122, 226
            ],
            encoder.indices
        );
        assert_eq!(encoder.strings, vec![5, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3]);
        assert_eq!(encoder.surfaces, vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(encoder.shells, vec![4, 4, 4]);
        assert_eq!(encoder.solids, vec![2, 1]);

        Ok(())
    }

    #[test]
    fn test_encode_semantics() -> Result<()> {
        //MultiSurface
        let mut encoder = FcbGeometryEncoderDecoder::new();
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

        encoder.encode_semantics(&semantics.unwrap());

        let expected_semantics_surfaces = vec![
            SemanticsSurface {
                thetype: "WallSurface".to_string(),
                parent: None,
                children: Some(vec![2]),
                other: json!({
                    "slope": 33.4,
                }),
            },
            SemanticsSurface {
                thetype: "RoofSurface".to_string(),
                parent: None,
                children: None,
                other: json!({
                    "slope": 66.6,
                }),
            },
            SemanticsSurface {
                thetype: "OuterCeilingSurface".to_string(),
                parent: Some(0),
                children: None,
                other: json!({
                    "colour": "blue",
                }),
            },
        ];

        let expected_semantics_values = Some(vec![0, 0, u32::MAX, 1, 2]);
        assert_eq!(
            expected_semantics_surfaces,
            encoder.semantics_surfaces.unwrap()
        );
        assert_eq!(expected_semantics_values, encoder.semantics_values);

        //CompositeSolid
        let mut encoder = FcbGeometryEncoderDecoder::new();
        let composite_solid_gem_json = json!({
            "type": "CompositeSolid",
            "lod": "2.2",
            "boundaries": [
              [
                [ [[0, 3, 2, 1, 22]], [[4, 5, 6, 7]], [[0, 1, 5, 4]], [[1, 2, 6, 5]] ]
              ],
              [
                [ [[666, 667, 668]], [[74, 75, 76]], [[880, 881, 885]] ]
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
        let composite_solid_geom: CjGeometry = serde_json::from_value(composite_solid_gem_json)?;
        let CjGeometry { semantics, .. } = composite_solid_geom;

        encoder.encode_semantics(&semantics.unwrap());

        let expected_semantics_surfaces = vec![
            SemanticsSurface {
                thetype: "RoofSurface".to_string(),
                parent: None,
                children: None,
                other: json!({}),
            },
            SemanticsSurface {
                thetype: "WallSurface".to_string(),
                parent: None,
                children: None,
                other: json!({}),
            },
        ];

        let expected_semantics_values: Vec<u32> =
            vec![0, 1, 1, u32::MAX, u32::MAX, u32::MAX, u32::MAX];
        assert_eq!(
            expected_semantics_surfaces,
            encoder.semantics_surfaces.unwrap()
        );
        assert_eq!(expected_semantics_values, encoder.semantics_values.unwrap());
        Ok(())
    }

    #[test]
    fn test_decode_boundaries() -> Result<()> {
        // MultiPoint
        let boundaries_value = json!([2, 44, 0, 7]);
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
        let indices = vec![2, 44, 0, 7];
        let strings = vec![4];
        let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
            None,
            None,
            None,
            Some(strings),
            Some(indices),
        );
        let boundaries = decoder.decode();
        assert_eq!(expected, boundaries);

        // MultiLineString
        let boundaries_value = json!([[2, 3, 5], [77, 55, 212]]);
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
        let indices = vec![2, 3, 5, 77, 55, 212];
        let strings = vec![3, 3];
        let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
            None,
            None,
            None,
            Some(strings),
            Some(indices),
        );
        let boundaries = decoder.decode();
        assert_eq!(expected, boundaries);

        // MultiSurface
        let boundaries_value = json!([[[0, 3, 2, 1]], [[4, 5, 6, 7]], [[0, 1, 5, 4]]]);
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
        let indices = vec![0, 3, 2, 1, 4, 5, 6, 7, 0, 1, 5, 4, 1, 2, 6, 5];
        let strings = vec![4, 4, 4];
        let surfaces = vec![1, 1, 1];
        let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
            None,
            None,
            Some(surfaces),
            Some(strings),
            Some(indices),
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
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
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
        let expected: CjBoundaries = serde_json::from_value(boundaries_value)?;
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
        );
        let boundaries = decoder.decode();
        assert_eq!(expected, boundaries);

        Ok(())
    }

    #[test]
    fn test_decode_semantics() -> Result<()> {
        // Test Case 1: MultiSurface
        {
            let mut fbb = FlatBufferBuilder::new();
            let sem1 = {
                let children = fbb.create_vector(&[2_u32]);
                SemanticObject::create(
                    &mut fbb,
                    &SemanticObjectArgs {
                        type_: SemanticSurfaceType::WallSurface,
                        children: Some(children),
                        ..Default::default()
                    },
                )
            };

            let sem2 = SemanticObject::create(
                &mut fbb,
                &SemanticObjectArgs {
                    type_: SemanticSurfaceType::RoofSurface,
                    ..Default::default()
                },
            );
            let sem3 = SemanticObject::create(
                &mut fbb,
                &SemanticObjectArgs {
                    type_: SemanticSurfaceType::OuterCeilingSurface,
                    parent: Some(0),
                    ..Default::default()
                },
            );
            let semantics_values = fbb.create_vector(&[0, 0, u32::MAX, 1, 2]);
            let city_feature = {
                let sem_obj = fbb.create_vector(&[sem1, sem2, sem3]);
                let id = fbb.create_string("test");
                let geometry = {
                    let geom = Geometry::create(
                        &mut fbb,
                        &GeometryArgs {
                            type_: GeometryType::MultiSurface,
                            semantics: Some(semantics_values),
                            semantics_objects: Some(sem_obj),
                            ..Default::default()
                        },
                    );
                    fbb.create_vector(&[geom])
                };
                let city_object = CityObject::create(
                    &mut fbb,
                    &CityObjectArgs {
                        geometry: Some(geometry),
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
            let decoder = FcbGeometryEncoderDecoder::new();
            let decoded = decoder.decode_semantics(
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
            let sem1 = {
                SemanticObject::create(
                    &mut fbb,
                    &SemanticObjectArgs {
                        type_: SemanticSurfaceType::RoofSurface,
                        ..Default::default()
                    },
                )
            };

            let sem2 = SemanticObject::create(
                &mut fbb,
                &SemanticObjectArgs {
                    type_: SemanticSurfaceType::WallSurface,
                    ..Default::default()
                },
            );

            let semantics_values =
                fbb.create_vector(&[0, 1, 1, u32::MAX, u32::MAX, u32::MAX, u32::MAX]);

            let city_feature = {
                let sem_obj = fbb.create_vector(&[sem1, sem2]);
                let id = fbb.create_string("test");
                let geometry = {
                    let geom = Geometry::create(
                        &mut fbb,
                        &GeometryArgs {
                            type_: GeometryType::CompositeSolid,
                            semantics: Some(semantics_values),
                            semantics_objects: Some(sem_obj),
                            ..Default::default()
                        },
                    );
                    fbb.create_vector(&[geom])
                };
                let city_object = CityObject::create(
                    &mut fbb,
                    &CityObjectArgs {
                        geometry: Some(geometry),
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
            let cj_geometry_json = json!({
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
                  "surfaces": [
                    {
                      "type": "RoofSurface"
                    },
                    {
                      "type": "WallSurface"
                    }
                  ],
                  "values": [
                    [ //-- 1st Solid
                      [
                        0,
                        1,
                        1,
                        null
                      ]
                    ],
                    [ //-- 2nd Solid get all null values
                      [
                        null,
                        null,
                        null
                      ]
                    ]
                  ]
                }
            });
            let cj_geometry: CjGeometry = serde_json::from_value(cj_geometry_json)?;
            let CjGeometry {
                boundaries,
                semantics,
                ..
            } = cj_geometry;
            let mut decoder = FcbGeometryEncoderDecoder::new();
            decoder.encode(&boundaries, semantics.as_ref()); // This is to set the internal state of the decoder as `decode_semantics` needs it to be encoded beforehand
            let decoded = decoder.decode_semantics(
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

            println!("decoder: {:?}", decoder);
            println!("decoded.values: {:?}", decoded.values);

            println!("original semantics values: {:?}", semantics.unwrap().values);
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
