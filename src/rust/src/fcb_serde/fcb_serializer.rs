use crate::feature_generated::{
    CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, CityObjectType, Geometry,
    GeometryArgs, GeometryType, SemanticObject, SemanticObjectArgs, SemanticSurfaceType, Vertex,
};
use crate::geometry_encoderdecoder::FcbGeometryEncoderDecoder;
use crate::header_generated::{GeographicalExtent, Vector};

use cjseq::{CityObject as CjCityObject, Geometry as CjGeometry, GeometryType as CjGeometryType};

pub fn to_fcb_city_object_type(co_type: &str) -> CityObjectType {
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

fn to_fcb_geometry_type(geometry_type: &CjGeometryType) -> GeometryType {
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

pub fn to_fcb_city_feature<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &str,
    objects: &[flatbuffers::WIPOffset<CityObject<'a>>],
    vertices: &[Vec<i64>],
) -> flatbuffers::WIPOffset<CityFeature<'a>> {
    let id = Some(fbb.create_string(id));
    let objects = Some(fbb.create_vector(objects));
    let vertices = Some(
        fbb.create_vector(
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
        fbb,
        &CityFeatureArgs {
            id,
            objects,
            vertices,
        },
    )
}

pub fn to_fcb_city_object<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &str,
    co: &CjCityObject,
) -> flatbuffers::WIPOffset<CityObject<'a>> {
    let id = Some(fbb.create_string(id));

    let type_ = to_fcb_city_object_type(&co.thetype);
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
                    .map(|g| to_fcb_geometry(fbb, g))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(fbb.create_vector(&geometries))
    };
    // let attributes = Some(self.fbb.create_vector(co.attributes));
    // let columns = Some(self.fbb.create_vector(co.columns));
    let children = {
        let children_strings: Vec<_> = co
            .children
            .as_ref()
            .map(|c| c.iter().map(|s| fbb.create_string(s)).collect())
            .unwrap_or_default();
        Some(fbb.create_vector(&children_strings))
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
            .map(|p| p.iter().map(|s| fbb.create_string(s)).collect())
            .unwrap_or_default();
        Some(fbb.create_vector(&parents_strings))
    };

    CityObject::create(
        fbb,
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

fn to_fcb_semantic_surface_type(ss_type: &str) -> SemanticSurfaceType {
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

fn to_fcb_geometry<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    geometry: &CjGeometry,
) -> flatbuffers::WIPOffset<Geometry<'a>> {
    let type_ = to_fcb_geometry_type(&geometry.thetype);
    let lod = geometry.lod.as_ref().map(|lod| fbb.create_string(lod));

    let mut encoder_decoder = FcbGeometryEncoderDecoder::new();
    encoder_decoder.encode(&geometry.boundaries, geometry.semantics.as_ref());
    let (solids, shells, surfaces, strings, boundary_indices) = encoder_decoder.boundaries();
    let (semantics_surfaces, semantics_values) = encoder_decoder.semantics();

    let solids = Some(fbb.create_vector(&solids));
    let shells = Some(fbb.create_vector(&shells));
    let surfaces = Some(fbb.create_vector(&surfaces));
    let strings = Some(fbb.create_vector(&strings));
    let boundary_indices = Some(fbb.create_vector(&boundary_indices));

    let semantics_objects = {
        let semantics_objects = semantics_surfaces
            .iter()
            .map(|s| {
                let children = s.children.clone().map(|c| fbb.create_vector(&c.to_vec()));
                let semantics_type = to_fcb_semantic_surface_type(&s.thetype);
                let semantic_object = SemanticObject::create(
                    fbb,
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
        Some(fbb.create_vector(&semantics_objects))
    };

    let semantics_values = Some(fbb.create_vector(semantics_values));

    Geometry::create(
        fbb,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fcb_serde::fcb_deserializer::to_cj_co_type;
    use crate::feature_generated::root_as_city_feature;
    use anyhow::Result;
    use cjseq::CityJSONFeature;
    use flatbuffers::FlatBufferBuilder;

    #[test]
    fn test_to_fcb_city_feature() -> Result<()> {
        let cj_city_feature: CityJSONFeature = serde_json::from_str(
            r#"{"type":"CityJSONFeature","id":"NL.IMBAG.Pand.0503100000005156","CityObjects":{"NL.IMBAG.Pand.0503100000005156-0":{"type":"BuildingPart","attributes":{},"geometry":[{"type":"Solid","lod":"1.2","boundaries":[[[[6,1,0,5,4,3,7,8]],[[9,5,0,10]],[[10,0,1,11]],[[12,3,4,13]],[[13,4,5,9]],[[14,7,3,12]],[[15,8,7,14]],[[16,6,8,15]],[[11,1,6,16]],[[11,16,15,14,12,13,9,10]]]],"semantics":{"surfaces":[{"type":"GroundSurface"},{"type":"RoofSurface"},{"on_footprint_edge":true,"type":"WallSurface"},{"on_footprint_edge":false,"type":"WallSurface"}],"values":[[0,2,2,2,2,2,2,2,2,1]]}},{"type":"Solid","lod":"1.3","boundaries":[[[[3,7,8,6,1,17,0,5,4,18]],[[19,5,0,20]],[[21,22,17,1,23]],[[24,7,3,25]],[[26,8,7,24]],[[20,0,17,22]],[[27,28,22,21]],[[29,4,5,19]],[[30,18,4,29]],[[23,1,6,31]],[[25,3,18,30,32]],[[31,6,8,26]],[[33,34,28,27]],[[32,30,34,33]],[[32,33,27,21,23,31,26,24,25]],[[34,30,29,19,20,22,28]]]],"semantics":{"surfaces":[{"type":"GroundSurface"},{"type":"RoofSurface"},{"on_footprint_edge":true,"type":"WallSurface"},{"on_footprint_edge":false,"type":"WallSurface"}],"values":[[0,2,2,2,2,2,3,2,2,2,2,2,3,3,1,1]]}},{"type":"Solid","lod":"2.2","boundaries":[[[[1,35,17,0,5,4,18,3,7,8,6]],[[36,5,0,37]],[[38,35,1,39]],[[40,7,3,41]],[[42,8,7,40]],[[37,0,17,43]],[[44,45,43,46]],[[47,4,5,36]],[[48,18,4,47]],[[39,1,6,49]],[[41,3,18,48,50]],[[46,43,17,35,38]],[[49,6,8,42]],[[51,52,45,44]],[[53,54,55]],[[54,53,56]],[[50,48,52,51]],[[53,55,38,39,49,42]],[[54,56,44,46,38,55]],[[50,51,44,56,53,42,40,41]],[[52,48,47,36,37,43,45]]]],"semantics":{"surfaces":[{"type":"GroundSurface"},{"type":"RoofSurface"},{"on_footprint_edge":true,"type":"WallSurface"},{"on_footprint_edge":false,"type":"WallSurface"}],"values":[[0,2,2,2,2,2,3,2,2,2,2,2,2,3,3,3,3,1,1,1,1]]}}],"parents":["NL.IMBAG.Pand.0503100000005156"]},"NL.IMBAG.Pand.0503100000005156":{"type":"Building","geographicalExtent":[84734.8046875,446636.5625,0.6919999718666077,84746.9453125,446651.0625,11.119057655334473],"attributes":{"b3_bag_bag_overlap":0.0,"b3_bouwlagen":3,"b3_dak_type":"slanted","b3_h_dak_50p":8.609999656677246,"b3_h_dak_70p":9.239999771118164,"b3_h_dak_max":10.970000267028809,"b3_h_dak_min":3.890000104904175,"b3_h_maaiveld":0.6919999718666077,"b3_kas_warenhuis":false,"b3_mutatie_ahn3_ahn4":false,"b3_nodata_fractie_ahn3":0.002518891589716077,"b3_nodata_fractie_ahn4":0.0,"b3_nodata_radius_ahn3":0.359510600566864,"b3_nodata_radius_ahn4":0.34349295496940613,"b3_opp_buitenmuur":165.03,"b3_opp_dak_plat":51.38,"b3_opp_dak_schuin":63.5,"b3_opp_grond":99.21,"b3_opp_scheidingsmuur":129.53,"b3_puntdichtheid_ahn3":16.353534698486328,"b3_puntdichtheid_ahn4":46.19647216796875,"b3_pw_bron":"AHN4","b3_pw_datum":2020,"b3_pw_selectie_reden":"PREFERRED_AND_LATEST","b3_reconstructie_onvolledig":false,"b3_rmse_lod12":3.2317864894866943,"b3_rmse_lod13":0.642620861530304,"b3_rmse_lod22":0.09925124794244766,"b3_val3dity_lod12":"[]","b3_val3dity_lod13":"[]","b3_val3dity_lod22":"[]","b3_volume_lod12":845.0095825195312,"b3_volume_lod13":657.8263549804688,"b3_volume_lod22":636.9927368164062,"begingeldigheid":"1999-04-28","documentdatum":"1999-04-28","documentnummer":"408040.tif","eindgeldigheid":null,"eindregistratie":null,"geconstateerd":false,"identificatie":"NL.IMBAG.Pand.0503100000005156","oorspronkelijkbouwjaar":2000,"status":"Pand in gebruik","tijdstipeindregistratielv":null,"tijdstipinactief":null,"tijdstipinactieflv":null,"tijdstipnietbaglv":null,"tijdstipregistratie":"2010-10-13T12:29:24Z","tijdstipregistratielv":"2010-10-13T12:30:50Z","voorkomenidentificatie":1},"geometry":[{"type":"MultiSurface","lod":"0","boundaries":[[[0,1,2,3,4,5]]]}],"children":["NL.IMBAG.Pand.0503100000005156-0"]}},"vertices":[[-353581,253246,-44957],[-348730,242291,-44957],[-343550,244604,-44957],[-344288,246257,-44957],[-341437,247537,-44957],[-345635,256798,-44957],[-343558,244600,-44957],[-343662,244854,-44957],[-343926,244734,-44957],[-345635,256798,-36439],[-353581,253246,-36439],[-348730,242291,-36439],[-344288,246257,-36439],[-341437,247537,-36439],[-343662,244854,-36439],[-343926,244734,-36439],[-343558,244600,-36439],[-352596,251020,-44957],[-344083,246349,-44957],[-345635,256798,-41490],[-353581,253246,-41490],[-352596,251020,-35952],[-352596,251020,-41490],[-348730,242291,-35952],[-343662,244854,-35952],[-344288,246257,-35952],[-343926,244734,-35952],[-347233,253386,-35952],[-347233,253386,-41490],[-341437,247537,-41490],[-344083,246349,-41490],[-343558,244600,-35952],[-344083,246349,-35952],[-347089,253741,-35952],[-347089,253741,-41490],[-350613,246543,-44957],[-345635,256798,-41507],[-353581,253246,-41516],[-350613,246543,-34688],[-348730,242291,-36953],[-343662,244854,-37089],[-344288,246257,-37099],[-343926,244734,-36944],[-352596,251020,-41514],[-347233,253386,-37262],[-347233,253386,-41508],[-352596,251020,-37264],[-341437,247537,-41498],[-344083,246349,-41501],[-343558,244600,-37083],[-344083,246349,-37212],[-347089,253741,-37402],[-347089,253741,-41508],[-349425,246738,-34864],[-349425,246738,-34529],[-349862,246897,-34699],[-349238,248437,-35307]]}"#,
        )?;

        // Create FlatBuffer and encode
        let mut fbb = FlatBufferBuilder::new();
        let city_objects_buf: Vec<_> = cj_city_feature
            .city_objects
            .iter()
            .map(|(id, co)| to_fcb_city_object(&mut fbb, id, co))
            .collect();
        let city_feature = to_fcb_city_feature(
            &mut fbb,
            "test_id",
            &city_objects_buf,
            &cj_city_feature.vertices,
        );

        fbb.finish(city_feature, None);
        let buf = fbb.finished_data();

        // Get encoded city object
        let fb_city_feature = root_as_city_feature(buf).unwrap();
        assert_eq!("test_id", fb_city_feature.id());
        assert_eq!(
            cj_city_feature.city_objects.len(),
            fb_city_feature.objects().unwrap().len()
        );

        assert_eq!(
            cj_city_feature.vertices.len(),
            fb_city_feature.vertices().unwrap().len()
        );
        assert_ne!(
            fb_city_feature.vertices().unwrap().get(0).x(),
            i32::default()
        );
        assert_ne!(
            fb_city_feature.vertices().unwrap().get(0).y(),
            i32::default()
        );
        assert_ne!(
            fb_city_feature.vertices().unwrap().get(0).z(),
            i32::default()
        );

        // iterate over city objects and check if the fields are correct
        for (id, cjco) in cj_city_feature.city_objects.iter() {
            let fb_city_object = fb_city_feature
                .objects()
                .unwrap()
                .iter()
                .find(|co| co.id() == id)
                .unwrap();
            assert_eq!(id, fb_city_object.id());
            assert_eq!(cjco.thetype, to_cj_co_type(fb_city_object.type_()));

            //TODO: check attributes later

            let fb_geometry = fb_city_object.geometry().unwrap();
            for fb_geometry in fb_geometry.iter() {
                let cj_geometry = cjco
                    .geometry
                    .as_ref()
                    .unwrap()
                    .iter()
                    .find(|g| g.lod == fb_geometry.lod().map(|lod| lod.to_string()))
                    .unwrap();
                assert_eq!(cj_geometry.thetype, fb_geometry.type_().to_cj());
            }

            for parent in fb_city_object.parents().unwrap().iter() {
                assert!(cjco.parents.as_ref().unwrap().contains(&parent.to_string()));
            }

            for child in fb_city_object.children().unwrap().iter() {
                assert!(cjco.children.as_ref().unwrap().contains(&child.to_string()));
            }

            if let Some(ge) = cjco.geographical_extent.as_ref() {
                // Check min x,y,z
                assert_eq!(
                    ge[0],
                    fb_city_object.geographical_extent().unwrap().min().x()
                );
                assert_eq!(
                    ge[1],
                    fb_city_object.geographical_extent().unwrap().min().y()
                );
                assert_eq!(
                    cjco.geographical_extent.as_ref().unwrap()[2],
                    fb_city_object.geographical_extent().unwrap().min().z()
                );

                // Check max x,y,z
                assert_eq!(
                    cjco.geographical_extent.as_ref().unwrap()[3],
                    fb_city_object.geographical_extent().unwrap().max().x()
                );
                assert_eq!(
                    cjco.geographical_extent.as_ref().unwrap()[4],
                    fb_city_object.geographical_extent().unwrap().max().y()
                );
                assert_eq!(
                    cjco.geographical_extent.as_ref().unwrap()[5],
                    fb_city_object.geographical_extent().unwrap().max().z()
                );
            }
        }

        Ok(())
    }
}
