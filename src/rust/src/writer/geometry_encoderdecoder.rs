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

    /// Encodes semantic surfaces and values from a CityJSON Semantics object.
    ///
    /// # Arguments
    ///
    /// * `semantics` - Reference to the CityJSON Semantics object containing surfaces and values
    pub fn encode_semantics(&mut self, semantics: &Semantics) {
        self.encode_semantics_surface(semantics.surfaces.to_vec());
        let mut values = Vec::new();
        self.encode_semantics_values(&semantics.values, &mut values);
    }

    /// Returns the encoded boundary arrays as a tuple.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// * solids - Number of shells per solid
    /// * shells - Number of surfaces per shell  
    /// * surfaces - Number of rings per surface
    /// * strings - Number of indices per ring
    /// * indices - Flattened list of vertex indices
    pub fn boundaries(&self) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
        (
            self.solids.clone(),
            self.shells.clone(),
            self.surfaces.clone(),
            self.strings.clone(),
            self.indices.clone(),
        )
    }

    /// Returns the encoded semantic surfaces and values.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// * surfaces - Slice of semantic surface definitions
    /// * values - Slice of semantic value indices
    pub fn semantics(&self) -> (&[SemanticsSurface], &[u32]) {
        match (&self.semantics_surfaces, &self.semantics_values) {
            (Some(surfaces), Some(values)) => (surfaces, values.as_slice()),
            _ => (&[], &[]),
        }
    }

    /// Decodes the flattened arrays back into a nested CityJSON boundaries structure.
    ///
    /// Uses cursor indices to track position in each array while rebuilding the
    /// hierarchical structure of solids, shells, surfaces and rings.
    ///
    /// # Returns
    ///
    /// The reconstructed CityJSON boundaries structure
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

    /// Converts FlatBuffers semantic surface objects into CityJSON semantic surfaces.
    ///
    /// # Arguments
    ///
    /// * `semantics_objects` - Slice of FlatBuffers semantic surface objects
    ///
    /// # Returns
    ///
    /// Vector of CityJSON semantic surface definitions
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
                    let subvals = Self::decode_semantics_(
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
                    let subvals = Self::decode_semantics_(
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
        &self,
        geometry_type: GeometryType,
        semantics_objects: Vec<SemanticObject>,
        semantics_values: Vec<u32>,
    ) -> Semantics {
        let surfaces = Self::decode_semantics_surfaces(&semantics_objects);

        let mut part_lists = PartLists {
            solids: &self.solids,
            shells: &self.shells,
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

        // otherwise, top-level call to decode_semantics_(d, None, None)
        let result = Self::decode_semantics_(d, None, None, &mut part_lists, &semantics_values);

        Semantics {
            surfaces,
            values: result,
        }
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
