use serde_json::value::Index;

use super::FeatureWriter;
use crate::{
    feature_generated::{
        CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, CityObjectType, Geometry,
        GeometryArgs, GeometryType, SemanticObject, SemanticObjectArgs, SemanticSurfaceType,
        Vertex,
    },
    header_generated::GeographicalExtent,
    Column,
};

use cjseq::{Boundaries, Semantics, SemanticsSurface, SemanticsValues};

impl<'a> FeatureWriter<'a> {
    fn create_city_object_type(&self, co_type: &str) -> CityObjectType {
        match co_type {
            "Bridge" => CityObjectType::Bridge,
            "BridgePart" => CityObjectType::BridgePart,
            "BridgeInstallation" => CityObjectType::BridgeInstallation,
            "BridgeConstructiveElement" => CityObjectType::BridgeConstructiveElement,
            "BridgeRoom" => CityObjectType::BridgeRoom,
            "BridgeFurniture" => CityObjectType::BridgeFurniture,

            "Building" => CityObjectType::Building,
            "BuildingPart" => CityObjectType::BuildingPart,
            "BuildingInstallation" => CityObjectType::BuildingInstallation,
            "BuildingConstructiveElement" => CityObjectType::BuildingConstructiveElement,
            "BuildingFurniture" => CityObjectType::BuildingFurniture,
            "BuildingStorey" => CityObjectType::BuildingStorey,
            "BuildingRoom" => CityObjectType::BuildingRoom,
            "BuildingUnit" => CityObjectType::BuildingUnit,

            "CityFurniture" => CityObjectType::CityFurniture,
            "CityObjectGroup" => CityObjectType::CityObjectGroup,
            "GenericCityObject" => CityObjectType::GenericCityObject,
            "LandUse" => CityObjectType::LandUse,
            "OtherConstruction" => CityObjectType::OtherConstruction,
            "PlantCover" => CityObjectType::PlantCover,
            "SolitaryVegetationObject" => CityObjectType::SolitaryVegetationObject,
            "TINRelief" => CityObjectType::TINRelief,

            "Road" => CityObjectType::Road,
            "Railway" => CityObjectType::Railway,
            "Waterway" => CityObjectType::Waterway,
            "TransportSquare" => CityObjectType::TransportSquare,

            "Tunnel" => CityObjectType::Tunnel,
            "TunnelPart" => CityObjectType::TunnelPart,
            "TunnelInstallation" => CityObjectType::TunnelInstallation,
            "TunnelConstructiveElement" => CityObjectType::TunnelConstructiveElement,
            "TunnelHollowSpace" => CityObjectType::TunnelHollowSpace,
            "TunnelFurniture" => CityObjectType::TunnelFurniture,

            "WaterBody" => CityObjectType::WaterBody,
            _ => CityObjectType::GenericCityObject,
        }
    }

    fn create_semantic_surface_type(&self, semantic_surface_type: &str) -> SemanticSurfaceType {
        match semantic_surface_type {
            "RoofSurface" => SemanticSurfaceType::RoofSurface,
            "GroundSurface" => SemanticSurfaceType::GroundSurface,
            "WallSurface" => SemanticSurfaceType::WallSurface,
            "ClosureSurface" => SemanticSurfaceType::ClosureSurface,
            "OuterCeilingSurface" => SemanticSurfaceType::OuterCeilingSurface,
            "OuterFloorSurface" => SemanticSurfaceType::OuterFloorSurface,
            "Window" => SemanticSurfaceType::Window,
            "Door" => SemanticSurfaceType::Door,
            "InteriorWallSurface" => SemanticSurfaceType::InteriorWallSurface,
            "CeilingSurface" => SemanticSurfaceType::CeilingSurface,
            "FloorSurface" => SemanticSurfaceType::FloorSurface,

            "WaterSurface" => SemanticSurfaceType::WaterSurface,
            "WaterGroundSurface" => SemanticSurfaceType::WaterGroundSurface,
            "WaterClosureSurface" => SemanticSurfaceType::WaterClosureSurface,

            "TrafficArea" => SemanticSurfaceType::TrafficArea,
            "AuxiliaryTrafficArea" => SemanticSurfaceType::AuxiliaryTrafficArea,
            "TransportationMarking" => SemanticSurfaceType::TransportationMarking,
            "TransportationHole" => SemanticSurfaceType::TransportationHole,

            _ => SemanticSurfaceType::RoofSurface,
        }
    }

    fn create_geometry_type(&self, geometry_type: &str) -> GeometryType {
        match geometry_type {
            "MultiPoint" => GeometryType::MultiPoint,
            "MultiLineString" => GeometryType::MultiLineString,
            "MultiSurface" => GeometryType::MultiSurface,
            "CompositeSurface" => GeometryType::CompositeSurface,
            "Solid" => GeometryType::Solid,
            "MultiSolid" => GeometryType::MultiSolid,
            "CompositeSolid" => GeometryType::CompositeSolid,
            _ => GeometryType::Solid,
        }
    }

    fn create_city_feature(
        &mut self,
        id: &str,
        objects: &[flatbuffers::WIPOffset<CityObject<'a>>],
        vertices: &[Vertex],
    ) -> flatbuffers::WIPOffset<CityFeature<'a>> {
        let id = Some(self.fbb.create_string(id));
        let objects = Some(self.fbb.create_vector(objects));
        let vertices = Some(self.fbb.create_vector(vertices));
        CityFeature::create(
            &mut self.fbb,
            &CityFeatureArgs {
                id,
                objects,
                vertices,
            },
        )
    }

    fn create_city_object(
        &mut self,
        co_type: &str,
        id: &str,
        geographical_extent: &GeographicalExtent,
        geometry: &[flatbuffers::WIPOffset<Geometry<'a>>],
        attributes: &[u8],
        columns: &[flatbuffers::WIPOffset<Column<'a>>],
        children: &[&str],
        children_roles: &[&str],
        parents: &[&str],
    ) -> flatbuffers::WIPOffset<CityObject<'a>> {
        let id = Some(self.fbb.create_string(id));
        let type_ = self.create_city_object_type(co_type);
        let geographical_extent = Some(geographical_extent);
        let geometry = Some(self.fbb.create_vector(geometry));
        let attributes = Some(self.fbb.create_vector(attributes));
        let columns = Some(self.fbb.create_vector(columns));
        let children = {
            let children_strings: Vec<_> =
                children.iter().map(|s| self.fbb.create_string(s)).collect();
            Some(self.fbb.create_vector(&children_strings))
        };

        let children_roles = {
            let children_roles_strings: Vec<_> = children_roles
                .iter()
                .map(|s| self.fbb.create_string(s))
                .collect();
            Some(self.fbb.create_vector(&children_roles_strings))
        };

        let parents = {
            let parents_strings: Vec<_> =
                parents.iter().map(|s| self.fbb.create_string(s)).collect();
            Some(self.fbb.create_vector(&parents_strings))
        };

        CityObject::create(
            &mut self.fbb,
            &CityObjectArgs {
                id,
                type_,
                geographical_extent,
                geometry,
                attributes,
                columns,
                children,
                children_roles,
                parents,
            },
        )
    }

    fn semantic_surface_type(&self, ss_type: &str) -> SemanticSurfaceType {
        match ss_type {
            "RoofSurface" => SemanticSurfaceType::RoofSurface,
            "GroundSurface" => SemanticSurfaceType::GroundSurface,
            "WallSurface" => SemanticSurfaceType::WallSurface,
            "ClosureSurface" => SemanticSurfaceType::ClosureSurface,
            "OuterCeilingSurface" => SemanticSurfaceType::OuterCeilingSurface,
            "OuterFloorSurface" => SemanticSurfaceType::OuterFloorSurface,
            "Window" => SemanticSurfaceType::Window,
            "Door" => SemanticSurfaceType::Door,
            "InteriorWallSurface" => SemanticSurfaceType::InteriorWallSurface,
            "CeilingSurface" => SemanticSurfaceType::CeilingSurface,
            "FloorSurface" => SemanticSurfaceType::FloorSurface,

            "WaterSurface" => SemanticSurfaceType::WaterSurface,
            "WaterGroundSurface" => SemanticSurfaceType::WaterGroundSurface,
            "WaterClosureSurface" => SemanticSurfaceType::WaterClosureSurface,

            "TrafficArea" => SemanticSurfaceType::TrafficArea,
            "AuxiliaryTrafficArea" => SemanticSurfaceType::AuxiliaryTrafficArea,
            "TransportationMarking" => SemanticSurfaceType::TransportationMarking,
            "TransportationHole" => SemanticSurfaceType::TransportationHole,
            _ => unreachable!(),
        }
    }

    fn create_geometry(
        &mut self,
        geometry_type: &str,
        lod: &str,
        boundaries: &Boundaries,
        semantics: Option<&Semantics>,
    ) -> flatbuffers::WIPOffset<Geometry<'a>> {
        let type_ = self.create_geometry_type(geometry_type);
        let lod = Some(self.fbb.create_string(lod));

        let encoder_decoder = FcbGeometryEncoderDecoder::new().encode(boundaries, semantics);
        let (solids, shells, surfaces, strings, boundary_indices) = encoder_decoder.boundaries();
        let (semantics_surfaces, semantics_values) = encoder_decoder.semantics();

        let solids = Some(self.fbb.create_vector(solids));
        let shells = Some(self.fbb.create_vector(shells));
        let surfaces = Some(self.fbb.create_vector(surfaces));
        let strings = Some(self.fbb.create_vector(strings));
        let boundary_indices = Some(self.fbb.create_vector(boundary_indices));

        let semantics_objects = {
            let semantics_objects = semantics_surfaces
                .iter()
                .map(|s| {
                    let children = s.children.clone().map(|c| {
                        self.fbb
                            .create_vector(&c.iter().map(|x| *x as u32).collect::<Vec<_>>())
                    });
                    let semantics_type = self.semantic_surface_type(&s.thetype);
                    let semantic_object = SemanticObject::create(
                        &mut self.fbb,
                        &SemanticObjectArgs {
                            type_: semantics_type,
                            attributes: None,
                            children,
                            parent: s.parent,
                        },
                    );
                    semantic_object
                })
                .collect::<Vec<_>>();
            Some(self.fbb.create_vector(&semantics_objects))
        };

        let semantics_values = Some(
            self.fbb.create_vector(
                &semantics_values
                    .iter()
                    .map(|v| match v {
                        Some(v) => *v as u32,
                        None => u32::MAX,
                    })
                    .collect::<Vec<_>>(),
            ),
        );

        Geometry::create(
            &mut self.fbb,
            &GeometryArgs {
                type_,
                lod,
                solids,
                shells,
                surfaces,
                strings,
                boundaries: boundary_indices,
                semantics: semantics_values,
                semantics_objects,
            },
        )
    }
}

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

    fn encode_boundaries(&mut self, boundaries: &Boundaries) -> usize {
        match boundaries {
            Boundaries::Indices(indices) => {
                self.indices.extend_from_slice(&indices);
                self.strings.push(self.indices.len() as u32);
                1
            }
            Boundaries::Nested(boundaries) => {
                let mut max_depth = 0;
                for sub in boundaries {
                    let d = self.encode_boundaries(sub);
                    if d > max_depth {
                        max_depth = d;
                    }
                }

                let count = boundaries.len() as u32;
                match max_depth {
                    1 => self.surfaces.push(count),
                    2 => self.shells.push(count),
                    3 => self.solids.push(count),
                    _ => unreachable!(),
                }
                max_depth + 1
            }
        }
    }
    pub fn encode(mut self, boundaries: &Boundaries, semantics: Option<&Semantics>) -> Self {
        self.encode_boundaries(&boundaries);
        if let Some(semantics) = semantics {
            self.encode_semantics(semantics);
        }
        self
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

    pub fn boundaries(&self) -> (&[u32], &[u32], &[u32], &[u32], &[u32]) {
        (
            &self.solids,
            &self.shells,
            &self.surfaces,
            &self.strings,
            &self.indices,
        )
    }

    pub fn semantics(&self) -> (&[SemanticsSurface], &[Option<u32>]) {
        (&self.semantics_surfaces, &self.semantics_values)
    }

    pub fn decode(self) -> Boundaries {
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

                            let ring_indices = self.indices
                                [index_cursor..index_cursor + ring_size as usize]
                                .iter()
                                .map(|x| *x as usize)
                                .collect::<Vec<_>>();
                            index_cursor += ring_size as usize;

                            let ring_indices = ring_indices
                                .into_iter()
                                .map(|x| x as u32)
                                .collect::<Vec<_>>();
                            ring_vec.push(Boundaries::Indices(ring_indices));
                        }

                        surface_vec.push(Boundaries::Nested(ring_vec));
                    }

                    shell_vec.push(Boundaries::Nested(surface_vec));
                }

                solids_vec.push(Boundaries::Nested(shell_vec));
            }

            if solids_vec.len() == 1 {
                solids_vec.into_iter().next().unwrap()
            } else {
                Boundaries::Nested(solids_vec)
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
                        let ring_indices = self.indices
                            [index_cursor..index_cursor + ring_size as usize]
                            .iter()
                            .map(|x| *x as usize)
                            .collect::<Vec<_>>();
                        index_cursor += ring_size as usize;

                        ring_vec.push(Boundaries::Indices(
                            ring_indices.into_iter().map(|x| x as u32).collect(),
                        ));
                    }
                    surface_vec.push(Boundaries::Nested(ring_vec));
                }
                shell_vec.push(Boundaries::Nested(surface_vec));
            }
            if shell_vec.len() == 1 {
                shell_vec.into_iter().next().unwrap()
            } else {
                Boundaries::Nested(shell_vec)
            }
        } else if !self.surfaces.is_empty() {
            let mut surface_vec = Vec::new();
            for &rings_count in &self.surfaces {
                let mut ring_vec = Vec::new();
                for _ in 0..rings_count {
                    let ring_size = self.strings[ring_cursor] as usize;
                    ring_cursor += 1;
                    let ring_indices = self.indices
                        [index_cursor..index_cursor + ring_size as usize]
                        .iter()
                        .map(|x| *x as usize)
                        .collect::<Vec<_>>();
                    index_cursor += ring_size as usize;

                    ring_vec.push(Boundaries::Indices(
                        ring_indices.into_iter().map(|x| x as u32).collect(),
                    ));
                }
                surface_vec.push(Boundaries::Nested(ring_vec));
            }
            if surface_vec.len() == 1 {
                surface_vec.into_iter().next().unwrap()
            } else {
                Boundaries::Nested(surface_vec)
            }
        } else if !self.strings.is_empty() {
            let mut ring_vec = Vec::new();
            for &ring_size in &self.strings {
                let ring_indices = self.indices[index_cursor..index_cursor + ring_size as usize]
                    .iter()
                    .map(|x| *x as usize)
                    .collect::<Vec<_>>();
                index_cursor += ring_size as usize;
                ring_vec.push(Boundaries::Indices(
                    ring_indices.into_iter().map(|x| x as u32).collect(),
                ));
            }
            if ring_vec.len() == 1 {
                ring_vec.into_iter().next().unwrap()
            } else {
                Boundaries::Nested(ring_vec)
            }
        } else {
            Boundaries::Indices(self.indices.into_iter().map(|x| x as u32).collect())
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
