use std::any::Any;

use super::FeatureWriter;
use crate::feature_writer::geometry_encoderdecoder::FcbGeometryEncoderDecoder;
use crate::{
    feature_generated::{
        CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, CityObjectType, Geometry,
        GeometryArgs, GeometryType, SemanticObject, SemanticObjectArgs, SemanticSurfaceType,
        Vertex,
    },
    header_generated::{GeographicalExtent, Vector},
    Column,
};
use cjseq::{
    Boundaries, CityObject as CjCityObject, Geometry as CjGeometry, GeometryType as CjGeometryType,
    Semantics,
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

    fn create_geometry_type(&self, geometry_type: &CjGeometryType) -> GeometryType {
        match geometry_type {
            CjGeometryType::MultiPoint => GeometryType::MultiPoint,
            CjGeometryType::MultiLineString => GeometryType::MultiLineString,
            CjGeometryType::MultiSurface => GeometryType::MultiSurface,
            CjGeometryType::CompositeSurface => GeometryType::CompositeSurface,
            CjGeometryType::Solid => GeometryType::Solid,
            CjGeometryType::MultiSolid => GeometryType::MultiSolid,
            CjGeometryType::CompositeSolid => GeometryType::CompositeSolid,
            _ => GeometryType::Solid,
        }
    }

    pub fn create_city_feature(
        &mut self,
        id: &str,
        objects: &[flatbuffers::WIPOffset<CityObject<'a>>],
        vertices: &Vec<Vec<i64>>,
    ) -> flatbuffers::WIPOffset<CityFeature<'a>> {
        let id = Some(self.fbb.create_string(id));
        let objects = Some(self.fbb.create_vector(objects));
        let vertices = Some(
            self.fbb.create_vector(
                &vertices
                    .iter()
                    .map(|v| {
                        Vertex::new(
                            v[0].try_into().unwrap(),
                            v[1].try_into().unwrap(),
                            v[2].try_into().unwrap(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
        );
        CityFeature::create(
            &mut self.fbb,
            &CityFeatureArgs {
                id,
                objects,
                vertices,
            },
        )
    }

    pub fn create_city_object(
        &mut self,
        id: &str,
        co: &CjCityObject,
    ) -> flatbuffers::WIPOffset<CityObject<'a>> {
        let id = Some(self.fbb.create_string(id));

        let type_ = self.create_city_object_type(&co.thetype);
        let geographical_extent = co.geographical_extent.as_ref().map(|ge| {
            let min = Vector::new(ge[0], ge[1], ge[2]);
            let max = Vector::new(ge[3], ge[4], ge[5]);
            GeographicalExtent::new(&min, &max)
        });
        let geometries = {
            let geometries = co
                .geometry
                .as_ref()
                .map(|geometries| {
                    geometries
                        .iter()
                        .map(|g| self.create_geometry(g))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(self.fbb.create_vector(&geometries))
        };
        // let attributes = Some(self.fbb.create_vector(co.attributes));
        // let columns = Some(self.fbb.create_vector(co.columns));
        let children = {
            let children_strings: Vec<_> = co
                .children
                .as_ref()
                .map(|c| c.iter().map(|s| self.fbb.create_string(s)).collect())
                .unwrap_or_default();
            Some(self.fbb.create_vector(&children_strings))
        };

        // let children_roles = {
        //     let children_roles_strings: Vec<_> = co
        //         .childre
        //         .iter()
        //         .map(|s| self.fbb.create_string(s))
        //         .collect();
        //     Some(self.fbb.create_vector(&children_roles_strings))
        // };
        let children_roles = None; // TODO: implement this later

        let parents = {
            let parents_strings: Vec<_> = co
                .parents
                .as_ref()
                .map(|p| p.iter().map(|s| self.fbb.create_string(s)).collect())
                .unwrap_or_default();
            Some(self.fbb.create_vector(&parents_strings))
        };

        CityObject::create(
            &mut self.fbb,
            &CityObjectArgs {
                id,
                type_,
                geographical_extent: geographical_extent.as_ref(),
                geometry: geometries,
                attributes: None,
                columns: None,
                children,
                children_roles,
                parents,
            },
        )
    }

    fn semantic_surface_type(ss_type: &str) -> SemanticSurfaceType {
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

    fn create_geometry(&mut self, geometry: &CjGeometry) -> flatbuffers::WIPOffset<Geometry<'a>> {
        let type_ = self.create_geometry_type(&geometry.thetype);
        let lod = geometry.lod.as_ref().map(|lod| self.fbb.create_string(lod));

        let encoder_decoder = FcbGeometryEncoderDecoder::new()
            .encode(&geometry.boundaries, geometry.semantics.as_ref());
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
                    let semantics_type = Self::semantic_surface_type(&s.thetype);
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
