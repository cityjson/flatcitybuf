use std::collections::HashMap;

use crate::{
    feature_generated::{CityFeature, CityObjectType, Geometry, Vertex},
    geometry_encoderdecoder::FcbGeometryEncoderDecoder,
    header_generated::*,
};
use anyhow::{Context, Result};
use cjseq::{
    Address as CjAddress, CityJSON, CityJSONFeature, CityObject as CjCityObject,
    Geometry as CjGeometry, Metadata as CjMetadata, PointOfContact as CjPointOfContact,
    ReferenceSystem as CjReferenceSystem, Semantics as CjSemantics, Transform as CjTransform,
};

pub fn to_cj_metadata(header: &Header) -> Result<CityJSON> {
    let mut cj = CityJSON::new();

    if let Some(transform) = header.transform() {
        let (scale, translate) = (transform.scale(), transform.translate());
        cj.transform = CjTransform {
            scale: vec![scale.x(), scale.y(), scale.z()],
            translate: vec![translate.x(), translate.y(), translate.z()],
        };
    }

    let reference_system = header
        .reference_system()
        .context("missing reference_system")?;
    cj.version = header.version().to_string();
    cj.thetype = String::from("CityJSON");

    let geographical_extent = header
        .geographical_extent()
        .map(|extent| {
            [
                extent.min().x(),
                extent.min().y(),
                extent.min().z(),
                extent.max().x(),
                extent.max().y(),
                extent.max().z(),
            ]
        })
        .unwrap_or_default();

    cj.metadata = Some(CjMetadata {
        geographical_extent: Some(geographical_extent),
        identifier: header.identifier().map(|i| i.to_string()),
        point_of_contact: Some(to_cj_point_of_contact(header)?),
        reference_date: header.reference_date().map(|r| r.to_string()),
        reference_system: Some(CjReferenceSystem::new(
            None,
            reference_system.authority().unwrap_or_default().to_string(),
            reference_system.version().to_string(),
            reference_system.code().to_string(),
        )),
        title: header.title().map(|t| t.to_string()),
    });

    Ok(cj)
}

pub(crate) fn to_cj_point_of_contact(header: &Header) -> Result<CjPointOfContact> {
    Ok(CjPointOfContact {
        contact_name: header
            .poc_contact_name()
            .context("missing contact_name")?
            .to_string(),
        contact_type: header.poc_contact_type().map(|ct| ct.to_string()),
        role: header.poc_role().map(|r| r.to_string()),
        phone: header.poc_phone().map(|p| p.to_string()),
        email_address: header
            .poc_email()
            .context("missing email_address")?
            .to_string(),
        website: header.poc_website().map(|w| w.to_string()),
        address: to_cj_address(header),
    })
}

pub(crate) fn to_cj_address(header: &Header) -> Option<CjAddress> {
    let thoroughfare_number = header
        .poc_address_thoroughfare_number()
        .and_then(|n| n.parse::<i64>().ok())?;
    let thoroughfare_name = header.poc_address_thoroughfare_name()?;
    let locality = header.poc_address_locality()?;
    let postal_code = header.poc_address_postcode()?;
    let country = header.poc_address_country()?;

    Some(CjAddress {
        thoroughfare_number,
        thoroughfare_name: thoroughfare_name.to_string(),
        locality: locality.to_string(),
        postal_code: postal_code.to_string(),
        country: country.to_string(),
    })
}

pub(crate) fn to_cj_co_type(co_type: CityObjectType) -> String {
    match co_type {
        CityObjectType::Bridge => "Bridge".to_string(),
        CityObjectType::BridgePart => "BridgePart".to_string(),
        CityObjectType::BridgeInstallation => "BridgeInstallation".to_string(),
        CityObjectType::BridgeConstructiveElement => "BridgeConstructiveElement".to_string(),
        CityObjectType::BridgeRoom => "BridgeRoom".to_string(),
        CityObjectType::BridgeFurniture => "BridgeFurniture".to_string(),
        CityObjectType::Building => "Building".to_string(),
        CityObjectType::BuildingPart => "BuildingPart".to_string(),
        CityObjectType::BuildingInstallation => "BuildingInstallation".to_string(),
        CityObjectType::BuildingConstructiveElement => "BuildingConstructiveElement".to_string(),
        CityObjectType::BuildingFurniture => "BuildingFurniture".to_string(),
        CityObjectType::BuildingStorey => "BuildingStorey".to_string(),
        CityObjectType::BuildingRoom => "BuildingRoom".to_string(),
        CityObjectType::BuildingUnit => "BuildingUnit".to_string(),
        CityObjectType::CityFurniture => "CityFurniture".to_string(),
        CityObjectType::CityObjectGroup => "CityObjectGroup".to_string(),
        CityObjectType::GenericCityObject => "GenericCityObject".to_string(),
        CityObjectType::LandUse => "LandUse".to_string(),
        CityObjectType::OtherConstruction => "OtherConstruction".to_string(),
        CityObjectType::PlantCover => "PlantCover".to_string(),
        CityObjectType::SolitaryVegetationObject => "SolitaryVegetationObject".to_string(),
        CityObjectType::TINRelief => "TINRelief".to_string(),
        CityObjectType::Road => "Road".to_string(),
        CityObjectType::Railway => "Railway".to_string(),
        CityObjectType::Waterway => "Waterway".to_string(),
        CityObjectType::TransportSquare => "TransportSquare".to_string(),
        CityObjectType::Tunnel => "Tunnel".to_string(),
        CityObjectType::TunnelPart => "TunnelPart".to_string(),
        CityObjectType::TunnelInstallation => "TunnelInstallation".to_string(),
        CityObjectType::TunnelConstructiveElement => "TunnelConstructiveElement".to_string(),
        CityObjectType::TunnelHollowSpace => "TunnelHollowSpace".to_string(),
        CityObjectType::TunnelFurniture => "TunnelFurniture".to_string(),
        CityObjectType::WaterBody => "WaterBody".to_string(),
        _ => "Unknown".to_string(),
    }
}

pub fn to_cj_feature(feature: CityFeature) -> Result<CityJSONFeature> {
    let mut cj = CityJSONFeature::new();
    cj.id = feature.id().to_string();

    if let Some(objects) = feature.objects() {
        let city_objects: HashMap<String, CjCityObject> = objects
            .iter()
            .map(|co| {
                let geographical_extent = co.geographical_extent().map(|extent| {
                    vec![
                        extent.min().x(),
                        extent.min().y(),
                        extent.min().z(),
                        extent.max().x(),
                        extent.max().y(),
                        extent.max().z(),
                    ]
                });
                let geometries = co.geometry().map(|gs| {
                    gs.iter()
                        .map(|g| decode_geometry(g).unwrap())
                        .collect::<Vec<_>>()
                });

                let cjco = CjCityObject::new(
                    to_cj_co_type(co.type_()).to_string(),
                    geographical_extent,
                    None,
                    geometries,
                    co.children()
                        .map(|c| c.iter().map(|s| s.to_string()).collect()),
                    co.parents()
                        .map(|p| p.iter().map(|s| s.to_string()).collect()),
                    None,
                );
                (co.id().to_string(), cjco)
            })
            .collect::<HashMap<String, CjCityObject>>();
        cj.city_objects = city_objects;
    }

    cj.vertices = feature
        .vertices()
        .map_or(Vec::new(), |v| to_cj_vertices(v.iter().collect()));

    Ok(cj)
}

pub(crate) fn decode_geometry(g: Geometry) -> Result<CjGeometry> {
    let decoder = FcbGeometryEncoderDecoder::new_as_decoder(
        g.solids().map(|v| v.iter().collect()),
        g.shells().map(|v| v.iter().collect()),
        g.surfaces().map(|v| v.iter().collect()),
        g.strings().map(|v| v.iter().collect()),
        g.boundaries().map(|v| v.iter().collect()),
    );

    let boundaries = decoder.decode();
    let semantics: Option<CjSemantics> = if let (Some(semantics_objects), Some(semantics)) =
        (g.semantics_objects(), g.semantics())
    {
        let semantics_objects = semantics_objects.iter().collect::<Vec<_>>();
        let semantics = semantics.iter().collect::<Vec<_>>();
        Some(decoder.decode_semantics(g.type_(), semantics_objects, semantics))
    } else {
        None
    };

    Ok(CjGeometry {
        thetype: g.type_().to_cj(),
        lod: g.lod().map(|v| v.to_string()),
        boundaries,
        semantics,
        material: None,
        texture: None,
        template: None,
        transformation_matrix: None,
    })
}

pub(crate) fn to_cj_vertices(vertices: Vec<&Vertex>) -> Vec<Vec<i64>> {
    vertices
        .iter()
        .map(|v| vec![v.x() as i64, v.y() as i64, v.z() as i64])
        .collect()
}
