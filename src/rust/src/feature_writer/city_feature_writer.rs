use serde_json::value::Index;

use super::FeatureWriter;
use crate::{
    feature_generated::{
        CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, CityObjectType, Geometry, GeometryArgs, GeometryType, SemanticObject, SemanticSurfaceType, Vertex
    },
    header_generated::GeographicalExtent,
    Column,
};

use cjseq::{Boundaries}


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

    fn create_geometry(
        &mut self,
        geometry_type: &str,
        lod: &str,
        boundaries: Boundaries,
        semantics: &[u32],
        semantics_objects: &[flatbuffers::WIPOffset<SemanticObject<'a>>],
    ) -> flatbuffers::WIPOffset<Geometry<'a>> {
        let type_ = self.create_geometry_type(geometry_type);
        let lod = Some(self.fbb.create_string(lod));

        let encoder_decoder = GeometryEncoderDecoder::new().encode(boundaries);
        let [solids, shells, surfaces, strings, boundary_indices] = encoder_decoder.values();

        let solids = Some(self.fbb.create_vector(solids));
        let shells = Some(self.fbb.create_vector(shells));
        let surfaces = Some(self.fbb.create_vector(surfaces));
        let strings = Some(self.fbb.create_vector(strings));
        let boundary_indices = Some(self.fbb.create_vector(boundary_indices));

        let semantics = Some(self.fbb.create_vector(semantics));
        let semantics_objects = Some(self.fbb.create_vector(semantics_objects));

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
                semantics,
                semantics_objects,
            },
        )
    }
}
pub struct GeometryEncoderDecoder {
    solids: Vec<u32>,
    shells: Vec<u32>,
    surfaces: Vec<u32>,
    strings: Vec<u32>,
    indices: Vec<u32>,
    semantics: Vec<u32>,
}

impl GeometryEncoderDecoder {
    pub fn new() -> Self {
        Self {
            solids: vec![],
            shells: vec![],
            surfaces: vec![],
            strings: vec![],
            indices: vec![],
            semantics: vec![],
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
    pub fn encode(mut self, boundaries: Boundaries) -> Self {
        self.encode_boundaries(&boundaries);
        self
    }


    pub fn values(&self) -> [&[u32]; 5] {
        [
            &self.solids,
            &self.shells,
            &self.surfaces,
            &self.strings,
            &self.indices,
        ]
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

                            let ring_indices = self.indices[index_cursor..index_cursor + ring_size as usize]
                                .iter()
                                .map(|x| *x as usize)
                                .collect::<Vec<_>>();
                            index_cursor += ring_size as usize;

                            let ring_indices = ring_indices.into_iter()
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
        }
        else if !self.shells.is_empty() {
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
                        let ring_indices = self.indices[index_cursor..index_cursor + ring_size as usize]
                            .iter()
                            .map(|x| *x as usize)
                            .collect::<Vec<_>>();
                        index_cursor += ring_size as usize;

                        ring_vec.push(Boundaries::Indices(ring_indices.into_iter().map(|x| x as u32).collect()));
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
        }
        else if !self.surfaces.is_empty() {
            let mut surface_vec = Vec::new();
            for &rings_count in &self.surfaces {
                let mut ring_vec = Vec::new();
                for _ in 0..rings_count {
                    let ring_size = self.strings[ring_cursor] as usize;
                    ring_cursor += 1;
                    let ring_indices = self.indices[index_cursor..index_cursor + ring_size as usize]
                        .iter()
                        .map(|x| *x as usize)
                        .collect::<Vec<_>>();
                    index_cursor += ring_size as usize;

                    ring_vec.push(Boundaries::Indices(ring_indices.into_iter().map(|x| x as u32).collect()));
                }
                surface_vec.push(Boundaries::Nested(ring_vec));
            }
            if surface_vec.len() == 1 {
                surface_vec.into_iter().next().unwrap()
            } else {
                Boundaries::Nested(surface_vec)
            }
        }
        else if !self.strings.is_empty() {
            let mut ring_vec = Vec::new();
            for &ring_size in &self.strings {
                let ring_indices = self.indices[index_cursor..index_cursor + ring_size as usize]
                    .iter()
                    .map(|x| *x as usize)
                    .collect::<Vec<_>>();
                index_cursor += ring_size as usize;
                ring_vec.push(Boundaries::Indices(ring_indices.into_iter().map(|x| x as u32).collect()));
            }
            if ring_vec.len() == 1 {
                ring_vec.into_iter().next().unwrap()
            } else {
                Boundaries::Nested(ring_vec)
            }
        }
        else {
            Boundaries::Indices(self.indices.into_iter().map(|x| x as u32).collect())
        }
    }
}
