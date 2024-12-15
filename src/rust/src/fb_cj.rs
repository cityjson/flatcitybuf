use crate::{error::CityJSONError, header_generated::*};
use cjseq::{
    Address, CityJSON, Metadata as CjMetadata, PointOfContact,
    ReferenceSystem as CjReferenceSystem, Transform as CjTransform,
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

    cj.version = header
        .reference_system()
        .ok_or(CityJSONError::MissingField("version"))?
        .version()
        .to_string();
    cj.thetype = String::from("CityJSON");
    println!("version: {}", cj.version);

    let reference_system: ReferenceSystem<'_> = header
        .reference_system()
        .ok_or(CityJSONError::MissingField("reference_system"))?;
    let geographical_extent = match header.geographical_extent() {
        Some(extent) => [
            extent.min().x(),
            extent.min().y(),
            extent.min().z(),
            extent.max().x(),
            extent.max().y(),
            extent.max().z(),
        ],
        None => Default::default(),
    };
    let metadata = CjMetadata {
        geographical_extent: Some(geographical_extent),
        identifier: Some(header.identifier().unwrap_or_default().to_string()),
        point_of_contact: Some(PointOfContact {
            contact_name: header
                .poc_contact_name()
                .ok_or(CityJSONError::MissingField("contact_name"))?
                .to_string(),
            contact_type: header.poc_contact_type().map(|ct| ct.to_string()),
            role: header.poc_role().map(|r| r.to_string()),
            phone: header.poc_phone().map(|p| p.to_string()),
            email_address: header
                .poc_email()
                .ok_or(CityJSONError::MissingField("email_address"))?
                .to_string(),
            website: header.poc_website().map(|w| w.to_string()),
            address: Some(Address {
                thoroughfare_number: header
                    .poc_address_thoroughfare_number()
                    .unwrap_or_default()
                    .parse::<i64>()
                    .unwrap_or_default(),
                thoroughfare_name: header
                    .poc_address_thoroughfare_name()
                    .unwrap_or_default()
                    .to_string(),
                locality: header
                    .poc_address_locality()
                    .unwrap_or_default()
                    .to_string(),
                postal_code: header
                    .poc_address_postcode()
                    .unwrap_or_default()
                    .to_string(),
                country: header.poc_address_country().unwrap_or_default().to_string(),
            }),
        }),
        reference_date: Some(header.reference_date().unwrap_or_default().to_string()),
        reference_system: Some(CjReferenceSystem::new(
            None,
            reference_system.authority().unwrap_or_default().to_string(),
            reference_system.version().to_string(),
            reference_system.code().to_string(),
        )),
        title: Some(header.title().unwrap_or_default().to_string()),
    };
    println!("metadata: {:?}", metadata);

    cj.metadata = Some(metadata);

    Ok(cj)
}
