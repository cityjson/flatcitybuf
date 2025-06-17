use std::{collections::HashMap, mem::size_of};

use crate::{
    error::Error,
    fb::*,
    geom_decoder::{decode, decode_materials, decode_semantics, decode_textures},
};
use byteorder::{ByteOrder, LittleEndian};
use cjseq::{
    Address as CjAddress, Appearance as CjAppearance, Boundaries as CjBoundaries, CityJSON,
    CityJSONFeature, CityObject as CjCityObject, Extension as CjExtension,
    ExtensionFile as CjExtensionFile, Geometry as CjGeometry,
    GeometryTemplates as CjGeometryTemplates, GeometryType as CjGeometryType,
    MaterialObject as CjMaterial, Metadata as CjMetadata, PointOfContact as CjPointOfContact,
    ReferenceSystem as CjReferenceSystem, Semantics as CjSemantics, TextFormat as CjTextFormat,
    TextType as CjTextType, TextureObject as CjTexture, Transform as CjTransform,
    WrapMode as CjWrapMode,
};

use super::meta::{Column as MetaColumn, ColumnType as MetaColumnType, Meta};

pub fn to_cj_metadata(header: &Header) -> Result<CityJSON, Error> {
    let mut cj = CityJSON::new();
    let semantic_attr_schema = header.semantic_columns();
    if let Some(transform) = header.transform() {
        let (scale, translate) = (transform.scale(), transform.translate());
        cj.transform = CjTransform {
            scale: vec![scale.x(), scale.y(), scale.z()],
            translate: vec![translate.x(), translate.y(), translate.z()],
        };
    }

    // Extract extensions if present
    if let Some(extensions_vec) = header.extensions() {
        let mut extensions_map = HashMap::new();
        for extension in extensions_vec.iter() {
            if let Some(name) = extension.name() {
                let url = extension.url().unwrap_or_default().to_string();
                let version = extension.version().unwrap_or_default().to_string();

                // Create a CityJSONExtension with metadata
                let cj_extension = CjExtension { url, version };

                extensions_map.insert(name.to_string(), cj_extension);
            }
        }

        if !extensions_map.is_empty() {
            cj.extensions = Some(extensions_map);
        }
    }

    let reference_system = header.reference_system().map(|rs| {
        CjReferenceSystem::new(
            None,
            rs.authority().unwrap_or_default().to_string(),
            rs.version().to_string(),
            rs.code().to_string(),
        )
    });
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

    let point_of_contact = match header.poc_contact_name() {
        Some(_) => Some(to_cj_point_of_contact(header)?),
        None => None,
    };

    cj.metadata = Some(CjMetadata {
        geographical_extent: Some(geographical_extent),
        identifier: header.identifier().map(|i| i.to_string()),
        point_of_contact,
        reference_date: header.reference_date().map(|r| r.to_string()),
        reference_system,
        title: header.title().map(|t| t.to_string()),
    });

    // Decode Geometry Templates if present
    if let (Some(fb_templates), Some(fb_vertices)) =
        (header.templates(), header.templates_vertices())
    {
        let templates = fb_templates
            .iter()
            .map(|g| decode_geometry(g, semantic_attr_schema)) // Use local decode_geometry
            .collect::<Result<Vec<_>, _>>()?;

        let vertices_templates = fb_vertices
            .iter()
            .map(|v| [v.x(), v.y(), v.z()])
            .collect::<Vec<_>>();

        cj.geometry_templates = Some(CjGeometryTemplates {
            templates,
            vertices_templates,
        });
    }

    Ok(cj)
}

pub(crate) fn to_meta(header: Header) -> Result<Meta, Error> {
    let columns = header.columns().map(|c| {
        c.iter()
            .map(|c| {
                let i = c.index();
                MetaColumn {
                    index: i,
                    name: c.name().to_string(),
                    _type: match c.type_() {
                        ColumnType::Int => MetaColumnType::Int,
                        ColumnType::UInt => MetaColumnType::UInt,
                        ColumnType::Bool => MetaColumnType::Bool,
                        ColumnType::Float => MetaColumnType::Float,
                        ColumnType::Double => MetaColumnType::Double,
                        ColumnType::String => MetaColumnType::String,
                        ColumnType::DateTime => MetaColumnType::DateTime,
                        ColumnType::Json => MetaColumnType::Json,
                        ColumnType::Binary => MetaColumnType::Binary,
                        ColumnType::Short => MetaColumnType::Short,
                        ColumnType::UShort => MetaColumnType::UShort,
                        ColumnType::Long => MetaColumnType::Long,
                        ColumnType::ULong => MetaColumnType::ULong,
                        _ => unreachable!(),
                    },
                    title: c.title().map(|t| t.to_string()),
                    description: c.description().map(|d| d.to_string()),
                    precision: Some(c.precision()),
                    scale: Some(c.scale()),
                    nullable: Some(c.nullable()),
                    unique: Some(c.unique()),
                    primary_key: Some(c.primary_key()),
                    metadata: c.metadata().map(|m| m.to_string()),
                    attr_index: Some(
                        header
                            .attribute_index()
                            .map(|attr_indices| attr_indices.iter().any(|i| i.index() == c.index()))
                            .unwrap_or(false),
                    ),
                }
            })
            .collect::<Vec<_>>()
    });
    if columns.is_none() {
        return Err(Error::MissingRequiredField("columns".to_string()));
    }
    Ok(Meta {
        columns: columns.unwrap(),
        feature_count: header.features_count(),
    })
}

pub(crate) fn to_cj_point_of_contact(header: &Header) -> Result<CjPointOfContact, Error> {
    Ok(CjPointOfContact {
        contact_name: header
            .poc_contact_name()
            .ok_or(Error::MissingRequiredField("contact_name".to_string()))?
            .to_string(),
        contact_type: header.poc_contact_type().map(|ct| ct.to_string()),
        role: header.poc_role().map(|r| r.to_string()),
        phone: header.poc_phone().map(|p| p.to_string()),
        email_address: header
            .poc_email()
            .ok_or(Error::MissingRequiredField("email_address".to_string()))?
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

pub(crate) fn to_cj_co_type(co_type: CityObjectType, extension_type: Option<&str>) -> String {
    // If this is an extension type and extension_type is available, use it
    if co_type == CityObjectType::ExtensionObject {
        if let Some(extension_type) = extension_type {
            return extension_type.to_string();
        }
    }
    // Otherwise use the standard mapping
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
        CityObjectType::ExtensionObject => "Unknown".to_string(), // Fallback if extension_type is None
        _ => "Unknown".to_string(),
    }
}

pub fn decode_attributes(
    columns: &flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Column<'_>>>,
    attributes: flatbuffers::Vector<'_, u8>,
) -> serde_json::Value {
    if attributes.is_empty() {
        return serde_json::Value::Object(serde_json::Map::new());
    }

    let mut map = serde_json::Map::new();
    let bytes = attributes.bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let col_index = LittleEndian::read_u16(&bytes[offset..offset + size_of::<u16>()]) as u16;
        offset += size_of::<u16>();
        if col_index >= columns.len() as u16 {
            panic!("column index out of range"); //TODO: handle this as an error
        }
        let column = columns.iter().find(|c| c.index() == col_index);
        if column.is_none() {
            panic!("column not found"); //TODO: handle this as an error
        }
        let column = column.unwrap();
        match column.type_() {
            ColumnType::Int => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Number(serde_json::Number::from(LittleEndian::read_i32(
                        &bytes[offset..offset + size_of::<i32>()],
                    ))),
                );
                offset += size_of::<i32>();
            }
            ColumnType::UInt => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Number(serde_json::Number::from(LittleEndian::read_u32(
                        &bytes[offset..offset + size_of::<u32>()],
                    ))),
                );
                offset += size_of::<u32>();
            }
            ColumnType::Bool => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Bool(bytes[offset] != 0),
                );
                offset += size_of::<u8>();
            }
            ColumnType::Short => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Number(serde_json::Number::from(LittleEndian::read_i16(
                        &bytes[offset..offset + size_of::<i16>()],
                    ))),
                );
                offset += size_of::<i16>();
            }
            ColumnType::UShort => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Number(serde_json::Number::from(LittleEndian::read_u16(
                        &bytes[offset..offset + size_of::<u16>()],
                    ))),
                );
                offset += size_of::<u16>();
            }
            ColumnType::Long => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Number(serde_json::Number::from(LittleEndian::read_i64(
                        &bytes[offset..offset + size_of::<i64>()],
                    ))),
                );
                offset += size_of::<i64>();
            }
            ColumnType::ULong => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Number(serde_json::Number::from(LittleEndian::read_u64(
                        &bytes[offset..offset + size_of::<u64>()],
                    ))),
                );
                offset += size_of::<u64>();
            }
            ColumnType::Float => {
                let f = LittleEndian::read_f32(&bytes[offset..offset + size_of::<f32>()]);
                if let Some(num) = serde_json::Number::from_f64(f as f64) {
                    map.insert(column.name().to_string(), serde_json::Value::Number(num));
                }
                offset += size_of::<f32>();
            }
            ColumnType::Double => {
                let f = LittleEndian::read_f64(&bytes[offset..offset + size_of::<f64>()]);
                if let Some(num) = serde_json::Number::from_f64(f) {
                    map.insert(column.name().to_string(), serde_json::Value::Number(num));
                }
                offset += size_of::<f64>();
            }
            ColumnType::String => {
                let len = LittleEndian::read_u32(&bytes[offset..offset + size_of::<u32>()]);
                offset += size_of::<u32>();
                let s = String::from_utf8(bytes[offset..offset + len as usize].to_vec())
                    .unwrap_or_default();
                map.insert(column.name().to_string(), serde_json::Value::String(s));
                offset += len as usize;
            }
            ColumnType::DateTime => {
                let len = LittleEndian::read_u32(&bytes[offset..offset + size_of::<u32>()]);
                offset += size_of::<u32>();
                let s = String::from_utf8(bytes[offset..offset + len as usize].to_vec())
                    .unwrap_or_default();
                map.insert(column.name().to_string(), serde_json::Value::String(s));
                offset += len as usize;
            }
            ColumnType::Json => {
                let len = LittleEndian::read_u32(&bytes[offset..offset + size_of::<u32>()]);
                offset += size_of::<u32>();
                let s = String::from_utf8(bytes[offset..offset + len as usize].to_vec())
                    .unwrap_or_default();
                map.insert(column.name().to_string(), serde_json::from_str(&s).unwrap());
                offset += len as usize;
            }

            // TODO: handle other column types
            _ => unreachable!(),
        }
    }

    serde_json::Value::Object(map)
}

pub fn to_cj_feature(
    feature: CityFeature,
    root_attr_schema: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Column<'_>>>>,
    semantic_attr_schema: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Column<'_>>>>,
) -> Result<CityJSONFeature, Error> {
    // Ensure function returns Result
    let mut cj = CityJSONFeature::new();
    cj.id = feature.id().to_string();

    if let Some(objects) = feature.objects() {
        let city_objects_result: Result<HashMap<String, CjCityObject>, Error> = objects
            .iter()
            .map(|co| {
                let geographical_extent = co.geographical_extent().map(|extent| {
                    [
                        extent.min().x(),
                        extent.min().y(),
                        extent.min().z(),
                        extent.max().x(),
                        extent.max().y(),
                        extent.max().z(),
                    ]
                });

                let mut all_geometries: Vec<cjseq::Geometry> = Vec::new();

                // Process standard geometries
                if let Some(standard_geometries) = co.geometry() {
                    let decoded_standard = standard_geometries
                        .iter()
                        .map(|g| decode_geometry(g, semantic_attr_schema)) // Returns Result<CjGeometry, Error>
                        .collect::<Result<Vec<_>, _>>()?; // Collect Results, propagate error
                    all_geometries.extend(decoded_standard);
                }

                // Process geometry instances
                if let Some(instances) = co.geometry_instances() {
                    let decoded_instances = instances
                        .iter()
                        .map(|inst| decode_geometry_instance(&inst)) // Use reference, returns Result<CjGeometry, Error>
                        .collect::<Result<Vec<_>, _>>()?; // Collect Results, propagate error
                    all_geometries.extend(decoded_instances);
                }

                let final_geometries = if all_geometries.is_empty() {
                    None
                } else {
                    Some(all_geometries)
                };

                let attributes = if root_attr_schema.is_none() && co.columns().is_none() {
                    None
                } else {
                    co.attributes().map(|a| {
                        decode_attributes(&co.columns().unwrap_or(root_attr_schema.unwrap()), a)
                    })
                };

                let children_roles = co
                    .children_roles()
                    .map(|c| c.iter().map(|s| s.to_string()).collect());

                let cjco = CjCityObject::new(
                    to_cj_co_type(co.type_(), co.extension_type()),
                    geographical_extent,
                    attributes,
                    final_geometries, // Use the combined list
                    co.children()
                        .map(|c| c.iter().map(|s| s.to_string()).collect()),
                    children_roles,
                    co.parents()
                        .map(|p| p.iter().map(|s| s.to_string()).collect()),
                    None, // Assuming appearance is handled elsewhere or not needed here
                );
                Ok((co.id().to_string(), cjco)) // Return Result for map operation
            })
            .collect(); // Collect Results from map

        let city_objects = city_objects_result?;
        cj.city_objects = city_objects;
    }

    cj.vertices = feature
        .vertices()
        .map_or(Vec::new(), |v| to_cj_vertices(v.iter().collect()));

    // Decode appearance if present
    if let Some(appearance) = feature.appearance() {
        let mut cj_appearance = CjAppearance {
            materials: None,
            textures: None,
            vertices_texture: None,
            default_theme_texture: None,
            default_theme_material: None,
        };

        // Decode materials
        if let Some(materials) = appearance.materials() {
            let cj_materials = materials
                .iter()
                .map(|m| {
                    // Helper function to convert color vectors
                    let convert_color = |color_opt: Option<flatbuffers::Vector<'_, f64>>| {
                        color_opt.map(|c| {
                            let color_vec: Vec<f64> = c.iter().collect();
                            assert!(color_vec.len() == 3, "color must be a vector of 3 elements");
                            [color_vec[0], color_vec[1], color_vec[2]]
                        })
                    };

                    CjMaterial {
                        name: m.name().to_string(),
                        ambient_intensity: m.ambient_intensity(),
                        diffuse_color: convert_color(m.diffuse_color()),
                        emissive_color: convert_color(m.emissive_color()),
                        specular_color: convert_color(m.specular_color()),
                        shininess: m.shininess(),
                        transparency: m.transparency(),
                        is_smooth: m.is_smooth(),
                    }
                })
                .collect();

            cj_appearance.materials = Some(cj_materials);
        }

        // Decode textures
        if let Some(textures) = appearance.textures() {
            let cj_textures = textures
                .iter()
                .map(|t| {
                    CjTexture {
                        image: t.image().to_string(),
                        texture_format: match t.type_() {
                            TextureFormat::PNG => CjTextFormat::Png,
                            TextureFormat::JPG => CjTextFormat::Jpg,
                            _ => CjTextFormat::Png, // Default to PNG
                        },
                        wrap_mode: t.wrap_mode().map(|w| match w {
                            WrapMode::None => CjWrapMode::None,
                            WrapMode::Wrap => CjWrapMode::Wrap,
                            WrapMode::Mirror => CjWrapMode::Mirror,
                            WrapMode::Clamp => CjWrapMode::Clamp,
                            WrapMode::Border => CjWrapMode::Border,
                            _ => CjWrapMode::None, // Default to None
                        }),
                        texture_type: t.texture_type().map(|t| match t {
                            TextureType::Unknown => CjTextType::Unknown,
                            TextureType::Specific => CjTextType::Specific,
                            TextureType::Typical => CjTextType::Typical,
                            _ => CjTextType::Unknown, // Default to Unknown
                        }),
                        border_color: t.border_color().map(|c| {
                            let color_vec: Vec<f64> = c.iter().collect();
                            assert!(color_vec.len() == 4, "color must be a vector of 4 elements");
                            [color_vec[0], color_vec[1], color_vec[2], color_vec[3]]
                        }),
                    }
                })
                .collect::<Vec<_>>();

            cj_appearance.textures = Some(cj_textures);
        }

        // Decode vertices_texture
        if let Some(vertices_texture) = appearance.vertices_texture() {
            cj_appearance.vertices_texture = Some(
                vertices_texture
                    .iter()
                    .map(|v| [v.u(), v.v()])
                    .collect::<Vec<_>>(),
            );
        }

        // Decode default themes
        if let Some(default_theme_texture) = appearance.default_theme_texture() {
            cj_appearance.default_theme_texture = Some(default_theme_texture.to_string());
        }

        if let Some(default_theme_material) = appearance.default_theme_material() {
            cj_appearance.default_theme_material = Some(default_theme_material.to_string());
        }

        cj.appearance = Some(cj_appearance);
    }

    Ok(cj) // Return Result
}

pub(crate) fn decode_geometry(
    g: Geometry,
    semantic_attr_schema: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Column<'_>>>>,
) -> Result<CjGeometry, Error> {
    let solids = g
        .solids()
        .map(|v| v.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let shells = g
        .shells()
        .map(|v| v.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let surfaces = g
        .surfaces()
        .map(|v| v.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let strings = g
        .strings()
        .map(|v| v.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let indices = g
        .boundaries()
        .map(|v| v.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let boundaries = decode(&solids, &shells, &surfaces, &strings, &indices);
    let semantics: Option<CjSemantics> = if let (Some(semantics_objects), Some(semantics)) =
        (g.semantics_objects(), g.semantics())
    {
        let semantics_objects = semantics_objects.iter().collect::<Vec<_>>();
        let semantics = semantics.iter().collect::<Vec<_>>();
        Some(decode_semantics(
            &solids,
            &shells,
            g.type_(),
            semantics_objects,
            semantics,
            semantic_attr_schema,
        ))
    } else {
        None
    };

    // Decode material mappings if present
    let material = if let Some(material_mappings) = g.material() {
        decode_materials(&material_mappings.iter().collect::<Vec<_>>())
    } else {
        None
    };

    // Decode texture mappings if present
    let texture = if let Some(texture_mappings) = g.texture() {
        decode_textures(&texture_mappings.iter().collect::<Vec<_>>())
    } else {
        None
    };

    Ok(CjGeometry {
        thetype: g.type_().to_cj(),
        lod: g.lod().map(|v| v.to_string()),
        boundaries,
        semantics,
        material,
        texture,
        template: None,
        transformation_matrix: None,
    })
}

/// Decodes a FlatBuffers GeometryInstance into a CityJSON Geometry struct.
///
/// # Arguments
///
/// * `instance` - A reference to the FlatBuffers GeometryInstance object.
///
/// # Returns
///
/// A Result containing the CityJSON Geometry struct representing the instance,
/// or an Error if decoding fails (e.g., missing required fields).
pub(crate) fn decode_geometry_instance(instance: &GeometryInstance) -> Result<CjGeometry, Error> {
    let template_index = instance.template();

    let boundaries = match instance.boundaries() {
        Some(fb_boundaries) => {
            if fb_boundaries.len() != 1 {
                return Err(Error::InvalidAttributeValue {
                    msg: format!("geometryinstance boundaries should contain exactly one vertex index, found {}", fb_boundaries.len())
                });
            }
            let reference_vertex_index = fb_boundaries.get(0);
            CjBoundaries::Indices(vec![reference_vertex_index])
        }
        None => {
            return Err(Error::MissingRequiredField(
                "geometryinstance boundaries".to_string(),
            ));
        }
    };

    let fb_matrix = instance.transformation().ok_or_else(|| {
        Error::MissingRequiredField("geometryinstance transformation field".to_string())
    })?;

    // Convert FlatBuffers TransformationMatrix struct to a [f64; 16] array
    let transformation_matrix_array = [
        fb_matrix.m00(),
        fb_matrix.m01(),
        fb_matrix.m02(),
        fb_matrix.m03(),
        fb_matrix.m10(),
        fb_matrix.m11(),
        fb_matrix.m12(),
        fb_matrix.m13(),
        fb_matrix.m20(),
        fb_matrix.m21(),
        fb_matrix.m22(),
        fb_matrix.m23(),
        fb_matrix.m30(),
        fb_matrix.m31(),
        fb_matrix.m32(),
        fb_matrix.m33(),
    ];

    Ok(CjGeometry {
        thetype: CjGeometryType::GeometryInstance,
        lod: None, // LOD is not typically associated directly with the instance itself
        boundaries,
        semantics: None,
        material: None,
        texture: None,
        template: Some(template_index as usize),
        transformation_matrix: Some(transformation_matrix_array),
    })
}

pub(crate) fn to_cj_vertices(vertices: Vec<&Vertex>) -> Vec<Vec<i64>> {
    vertices
        .iter()
        .map(|v| vec![v.x() as i64, v.y() as i64, v.z() as i64])
        .collect()
}

/// Convert a FlatBuffer Extension to a CityJSON ExtensionFile
///
/// # Arguments
///
/// * `extension` - The FlatBuffer Extension object to convert
///
/// # Returns
///
/// A Result containing the converted CityJSON ExtensionFile
pub(crate) fn to_cj_extension_file(extension: &Extension) -> Result<CjExtensionFile, Error> {
    let name = extension.name().unwrap_or_default().to_string();
    let description = extension.description().unwrap_or_default().to_string();
    let url = extension.url().unwrap_or_default().to_string();
    let version = extension.version().unwrap_or_default().to_string();
    let version_city_json = extension.version_cityjson().unwrap_or_default().to_string();

    // Parse the stringified JSON components
    let extra_attributes = if let Some(attr_str) = extension.extra_attributes() {
        if attr_str.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(attr_str).unwrap_or(serde_json::Value::Null)
        }
    } else {
        serde_json::Value::Null
    };

    let extra_city_objects = if let Some(objs_str) = extension.extra_city_objects() {
        if objs_str.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(objs_str).unwrap_or(serde_json::Value::Null)
        }
    } else {
        serde_json::Value::Null
    };

    let extra_root_properties = if let Some(props_str) = extension.extra_root_properties() {
        if props_str.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(props_str).unwrap_or(serde_json::Value::Null)
        }
    } else {
        serde_json::Value::Null
    };

    let extra_semantic_surfaces = if let Some(surfaces_str) = extension.extra_semantic_surfaces() {
        if surfaces_str.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(surfaces_str).unwrap_or(serde_json::Value::Null)
        }
    } else {
        serde_json::Value::Null
    };

    Ok(CjExtensionFile {
        thetype: "CityJSONExtension".to_string(),
        name,
        description,
        url,
        version,
        version_city_json,
        extra_attributes,
        extra_city_objects,
        extra_root_properties,
        extra_semantic_surfaces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    use flatbuffers::FlatBufferBuilder;
    #[test]
    fn test_decode_geometry_instance() -> Result<()> {
        let mut fbb = FlatBufferBuilder::new();

        // Create test transformation matrix
        let transformation = TransformationMatrix::new(
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 10.0, 10.0, 10.0,
            1.0, // Translation part (10,10,10)
        );

        // Create boundary with a single vertex index
        let boundaries_vec = vec![42u32]; // Reference vertex index 42
        let boundaries = fbb.create_vector(&boundaries_vec);

        // Create a GeometryInstance
        let geometry_instance = GeometryInstance::create(
            &mut fbb,
            &crate::fb::GeometryInstanceArgs {
                template: 5, // Template index
                transformation: Some(&transformation),
                boundaries: Some(boundaries),
            },
        );

        fbb.finish(geometry_instance, None);
        let buf = fbb.finished_data();

        // Get a reference to the created GeometryInstance
        let geometry_instance = flatbuffers::root::<GeometryInstance>(buf).unwrap();

        // Decode the instance
        let cj_geometry = decode_geometry_instance(&geometry_instance)?;

        // Verify the decoded geometry
        assert_eq!(cj_geometry.thetype, CjGeometryType::GeometryInstance);
        assert_eq!(cj_geometry.template, Some(5));

        // Check boundaries
        match &cj_geometry.boundaries {
            CjBoundaries::Indices(indices) => {
                assert_eq!(indices.len(), 1);
                assert_eq!(indices[0], 42);
            }
            _ => panic!("Expected Indices boundaries"),
        }

        // Check transformation matrix
        assert!(cj_geometry.transformation_matrix.is_some());
        let matrix = cj_geometry.transformation_matrix.unwrap();
        assert_eq!(matrix[0], 1.0); // First element
        assert_eq!(matrix[12], 10.0); // Translation X
        assert_eq!(matrix[13], 10.0); // Translation Y
        assert_eq!(matrix[14], 10.0); // Translation Z

        Ok(())
    }

    #[test]
    fn test_decode_geometry_instance_missing_boundaries() -> Result<()> {
        let mut fbb = FlatBufferBuilder::new();

        // Create test transformation matrix
        let transformation = TransformationMatrix::new(
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        );

        // Create a GeometryInstance WITHOUT boundaries
        let geometry_instance = GeometryInstance::create(
            &mut fbb,
            &crate::fb::GeometryInstanceArgs {
                template: 5,
                transformation: Some(&transformation),
                boundaries: None, // Missing boundaries
            },
        );

        fbb.finish(geometry_instance, None);
        let buf = fbb.finished_data();
        let geometry_instance = flatbuffers::root::<GeometryInstance>(buf).unwrap();

        // Decode and assert error
        let result = decode_geometry_instance(&geometry_instance);
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::MissingRequiredField(field) => {
                assert!(field.contains("geometryinstance boundaries"));
            }
            _ => panic!("Expected MissingRequiredField error"),
        }

        Ok(())
    }

    #[test]
    fn test_decode_geometry_instance_missing_transformation() -> Result<()> {
        let mut fbb = FlatBufferBuilder::new();

        // Create boundary with a single vertex index
        let boundaries_vec = vec![42u32];
        let boundaries = fbb.create_vector(&boundaries_vec);

        // Create a GeometryInstance WITHOUT transformation
        let geometry_instance = GeometryInstance::create(
            &mut fbb,
            &crate::fb::GeometryInstanceArgs {
                template: 5,
                transformation: None, // Missing transformation
                boundaries: Some(boundaries),
            },
        );

        fbb.finish(geometry_instance, None);
        let buf = fbb.finished_data();
        let geometry_instance = flatbuffers::root::<GeometryInstance>(buf).unwrap();

        // Decode and assert error
        let result = decode_geometry_instance(&geometry_instance);
        assert!(result.is_err());
        match result.err().unwrap() {
            Error::MissingRequiredField(field) => {
                assert!(field.contains("geometryinstance transformation field"));
            }
            _ => panic!("Expected MissingRequiredField error"),
        }

        Ok(())
    }

    #[test]
    fn test_decode_geometry_instance_invalid_boundaries() -> Result<()> {
        let mut fbb = FlatBufferBuilder::new();

        // Create test transformation matrix
        let transformation = TransformationMatrix::new(
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        );

        // --- Test Case 1: Zero boundaries ---
        let boundaries_vec_zero: Vec<u32> = vec![];
        let boundaries_zero = fbb.create_vector(&boundaries_vec_zero);
        let geometry_instance_zero = GeometryInstance::create(
            &mut fbb,
            &crate::fb::GeometryInstanceArgs {
                template: 5,
                transformation: Some(&transformation),
                boundaries: Some(boundaries_zero),
            },
        );
        fbb.finish(geometry_instance_zero, None);
        let buf_zero = fbb.finished_data();
        let instance_zero = flatbuffers::root::<GeometryInstance>(buf_zero).unwrap();

        let result_zero = decode_geometry_instance(&instance_zero);
        assert!(result_zero.is_err());
        match result_zero.err().unwrap() {
            Error::InvalidAttributeValue { msg } => {
                assert!(msg.contains("should contain exactly one vertex index, found 0"));
            }
            _ => panic!("Expected InvalidAttributeValue error for zero boundaries"),
        }

        // --- Test Case 2: Multiple boundaries ---
        fbb.reset(); // Reset builder for the next case
        let boundaries_vec_multi = vec![42u32, 43u32]; // Two indices
        let boundaries_multi = fbb.create_vector(&boundaries_vec_multi);
        // Recreate transformation as it's part of the buffer
        let transformation_multi = TransformationMatrix::new(
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        );
        let geometry_instance_multi = GeometryInstance::create(
            &mut fbb,
            &crate::fb::GeometryInstanceArgs {
                template: 5,
                transformation: Some(&transformation_multi),
                boundaries: Some(boundaries_multi),
            },
        );
        fbb.finish(geometry_instance_multi, None);
        let buf_multi = fbb.finished_data();
        let instance_multi = flatbuffers::root::<GeometryInstance>(buf_multi).unwrap();

        let result_multi = decode_geometry_instance(&instance_multi);
        assert!(result_multi.is_err());
        match result_multi.err().unwrap() {
            Error::InvalidAttributeValue { msg } => {
                assert!(msg.contains("should contain exactly one vertex index, found 2"));
            }
            _ => panic!("Expected InvalidAttributeValue error for multiple boundaries"),
        }

        Ok(())
    }
}
