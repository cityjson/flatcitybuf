use super::FeatureWriter;
use crate::{
    feature_generated::{
        CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, CityObjectType, Geometry,
        GeometryType, SemanticObject, SemanticSurfaceType, Vertex,
    },
    header_generated::GeographicalExtent,
    Column,
};

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
        boundaries: &[u32],
        semantics: &[u32],
        semantics_objects: &[flatbuffers::WIPOffset<SemanticObject<'a>>],
    ) -> flatbuffers::WIPOffset<Geometry<'a>> {
        let type_ = self.create_geometry_type(geometry_type);
        let lod = Some(self.fbb.create_string(lod));

        let encoder_decoder = GeometryEncoderDecoder::encode(boundaries);
        let (solids, shells, surfaces, strings) = encoder_decoder.decode();

        // let solids = Some(self.fbb.create_vector(solids));
        // let shells = Some(self.fbb.create_vector(shells));
        // let surfaces = Some(self.fbb.create_vector(surfaces));
        // let strings = Some(self.fbb.create_vector(strings));

        let boundaries = Some(self.fbb.create_vector(boundaries));
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
                boundaries,
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
    pub fn encode(&mut self, boundaries: &[u32]) -> Self {
        todo!("encode boundaries")
    }

    pub fn values(&self) -> [&Vec<u32>; 6] {
        [
            self.solids,
            self.shells,
            self.surfaces,
            self.strings,
            self.indices,
            self.semantics,
        ]
    }

    pub fn decode(self) -> Vec<u32> {
        todo!("decode boundaries")
    }
}
