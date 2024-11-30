use crate::header_generated::*;
use cjseq::{
    Address, CityJSON, GeographicalExtent as CjGeographicalExtent, Metadata as CjMetadata,
    PointOfContact, ReferenceSystem as CjReferenceSystem, Transform as CjTransform,
};
pub fn header_to_cityjson(header: Header) -> Result<CityJSON, Box<dyn std::error::Error>> {
    let mut cj = CityJSON::new();

    if let Some(transform) = header.transform() {
        let scale = transform.scale();
        let translate = transform.translate();
        cj.transform = CjTransform {
            scale: vec![scale.x(), scale.y(), scale.z()],
            translate: vec![translate.x(), translate.y(), translate.z()],
        };
    }

    cj.version = header.reference_system().unwrap().version().to_string();
    cj.thetype = String::from("CityJSON");
    println!("version: {}", cj.version);

    let reference_system: ReferenceSystem<'_> = header.reference_system().unwrap();
    let geographical_extent: CjGeographicalExtent = match header.geographical_extent() {
        Some(extent) => [
            extent.min().x(),
            extent.min().y(),
            extent.min().z(),
            extent.max().x(),
            extent.max().y(),
            extent.max().z(),
        ],
        None => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    };
    let metadata = CjMetadata {
        geographical_extent,
        identifier: header.identifier().unwrap_or_default().to_string(),
        point_of_contact: PointOfContact {
            contact_name: header.poc_contact_name().unwrap().to_string(),
            contact_type: header.poc_contact_type().unwrap().to_string(),
            role: Some(header.poc_role().unwrap().to_string()),
            phone: Some(header.poc_phone().unwrap().to_string()),
            email_address: Some(header.poc_email().unwrap().to_string()),
            website: Some(header.poc_website().unwrap().to_string()),
            address: Some(Address {
                thoroughfare_number: header
                    .poc_address_thoroughfare_number()
                    .unwrap()
                    .parse::<i64>()
                    .unwrap(),
                thoroughfare_name: header.poc_address_thoroughfare_name().unwrap().to_string(),
                locality: header.poc_address_locality().unwrap().to_string(),
                postal_code: header.poc_address_postcode().unwrap().to_string(),
                country: header.poc_address_country().unwrap().to_string(),
            }),
        },
        reference_date: header.reference_date().unwrap().to_string(),
        reference_system: CjReferenceSystem::new(
            reference_system.authority().unwrap().to_string(),
            reference_system.code().to_string(),
            reference_system.version().to_string(),
        )
        .to_url(None),
        title: header.title().unwrap().to_string(),
    };
    println!("metadata: {:?}", metadata);

    cj.metadata = Some(metadata);

    Ok(cj)
}
