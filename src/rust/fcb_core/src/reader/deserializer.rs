use std::{collections::HashMap, mem::size_of};

use crate::{
    error::Error,
    fb::*,
    geom_decoder::{
        decode_materials, decode_points, decode_rings, decode_semantics, decode_shells,
        decode_solids, decode_surfaces, decode_textures,
    },
};
use byteorder::{ByteOrder, LittleEndian};
use cjseq::{
    Address as CjAddress, Appearance as CjAppearance, BorderColor as CjBorderColor, CityJSON,
    CityJSONFeature, CityObject as CjCityObject, CityObjectType as CjCityObjectType,
    Color as CjColor, Geometry as CjGeometry, GeometryCommon as CjGeometryCommon,
    GeometryTemplates as CjGeometryTemplates, MaterialObject as CjMaterialObject,
    Metadata as CjMetadata, PointOfContact as CjPointOfContact,
    ReferenceSystem as CjReferenceSystem, Semantics as CjSemantics,
    TextureFormat as CjTextureFormat, TextureObject as CjTextureObject,
    TextureType as CjTextureType, Transform as CjTransform, WrapMode as CjWrapMode,
};
use serde_json::{json, Value};

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
        let mut extensions_map = serde_json::Map::new();
        for extension in extensions_vec.iter() {
            if let Some(name) = extension.name() {
                extensions_map.insert(
                    name.to_string(),
                    json!({
                        "url": extension.url().unwrap_or_default(),
                        "version": extension.version().unwrap_or_default(),
                    }),
                );
            }
        }

        if !extensions_map.is_empty() {
            cj.extensions = Some(Value::Object(extensions_map));
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
        other: HashMap::new(),
    });

    // Decode Geometry Templates if present
    if let (Some(fb_templates), Some(fb_vertices)) =
        (header.templates(), header.templates_vertices())
    {
        let templates = fb_templates
            .iter()
            .map(|g| decode_geometry(g, semantic_attr_schema)) // Use local decode_geometry
            .collect::<Result<Vec<_>, _>>()?;

        let vertices_templates = Value::Array(
            fb_vertices
                .iter()
                .map(|v| json!([v.x(), v.y(), v.z()]))
                .collect(),
        );

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
        organization: None,
        address: to_cj_address(header),
        other: HashMap::new(),
    })
}

/// Rebuilds the free-form `address` object from the header's fixed fields.
/// Members the writer had nothing to write are simply absent, which is what
/// the schema wants — it names no required member at all.
pub(crate) fn to_cj_address(header: &Header) -> Option<CjAddress> {
    let mut members: HashMap<String, Value> = HashMap::new();
    let mut insert = |key: &str, value: Option<&str>| {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            members.insert(key.to_string(), Value::String(value.to_string()));
        }
    };
    insert(
        "thoroughfareNumber",
        header.poc_address_thoroughfare_number(),
    );
    insert("thoroughfareName", header.poc_address_thoroughfare_name());
    insert("locality", header.poc_address_locality());
    insert("postcode", header.poc_address_postcode());
    insert("country", header.poc_address_country());

    if members.is_empty() {
        None
    } else {
        Some(CjAddress { members })
    }
}

/// The CityJSON spelling of a FlatBuffers City Object type.
///
/// `ExtensionObject` carries its CityJSON name in `extension_type`, which the
/// spec requires to start with `+`.
pub(crate) fn to_cj_co_type(
    co_type: CityObjectType,
    extension_type: Option<&str>,
) -> CjCityObjectType {
    match co_type {
        CityObjectType::Bridge => CjCityObjectType::Bridge,
        CityObjectType::BridgePart => CjCityObjectType::BridgePart,
        CityObjectType::BridgeInstallation => CjCityObjectType::BridgeInstallation,
        CityObjectType::BridgeConstructiveElement => CjCityObjectType::BridgeConstructiveElement,
        CityObjectType::BridgeRoom => CjCityObjectType::BridgeRoom,
        CityObjectType::BridgeFurniture => CjCityObjectType::BridgeFurniture,
        CityObjectType::Building => CjCityObjectType::Building,
        CityObjectType::BuildingPart => CjCityObjectType::BuildingPart,
        CityObjectType::BuildingInstallation => CjCityObjectType::BuildingInstallation,
        CityObjectType::BuildingConstructiveElement => {
            CjCityObjectType::BuildingConstructiveElement
        }
        CityObjectType::BuildingFurniture => CjCityObjectType::BuildingFurniture,
        CityObjectType::BuildingStorey => CjCityObjectType::BuildingStorey,
        CityObjectType::BuildingRoom => CjCityObjectType::BuildingRoom,
        CityObjectType::BuildingUnit => CjCityObjectType::BuildingUnit,
        CityObjectType::CityFurniture => CjCityObjectType::CityFurniture,
        CityObjectType::CityObjectGroup => CjCityObjectType::CityObjectGroup,
        CityObjectType::GenericCityObject => CjCityObjectType::GenericCityObject,
        CityObjectType::LandUse => CjCityObjectType::LandUse,
        CityObjectType::OtherConstruction => CjCityObjectType::OtherConstruction,
        CityObjectType::PlantCover => CjCityObjectType::PlantCover,
        CityObjectType::SolitaryVegetationObject => CjCityObjectType::SolitaryVegetationObject,
        CityObjectType::TINRelief => CjCityObjectType::TINRelief,
        CityObjectType::Road => CjCityObjectType::Road,
        CityObjectType::Railway => CjCityObjectType::Railway,
        CityObjectType::Waterway => CjCityObjectType::Waterway,
        CityObjectType::TransportSquare => CjCityObjectType::TransportSquare,
        CityObjectType::Tunnel => CjCityObjectType::Tunnel,
        CityObjectType::TunnelPart => CjCityObjectType::TunnelPart,
        CityObjectType::TunnelInstallation => CjCityObjectType::TunnelInstallation,
        CityObjectType::TunnelConstructiveElement => CjCityObjectType::TunnelConstructiveElement,
        CityObjectType::TunnelHollowSpace => CjCityObjectType::TunnelHollowSpace,
        CityObjectType::TunnelFurniture => CjCityObjectType::TunnelFurniture,
        CityObjectType::WaterBody => CjCityObjectType::WaterBody,
        // ExtensionObject, and any tag a newer writer may add.
        _ => {
            CjCityObjectType::Extension(extension_type.unwrap_or("+UnknownCityObject").to_string())
        }
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

            // These are emitted by the writer, so the reader must handle
            // them: panicking here made any file containing such an attribute
            // unreadable by the implementation that wrote it.
            ColumnType::Byte => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Number(serde_json::Number::from(bytes[offset] as i8)),
                );
                offset += size_of::<u8>();
            }
            ColumnType::UByte => {
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Number(serde_json::Number::from(bytes[offset])),
                );
                offset += size_of::<u8>();
            }
            ColumnType::Binary => {
                let len = LittleEndian::read_u32(&bytes[offset..offset + size_of::<u32>()]);
                offset += size_of::<u32>();
                let raw = &bytes[offset..offset + len as usize];
                map.insert(
                    column.name().to_string(),
                    serde_json::Value::Array(
                        raw.iter()
                            .map(|b| serde_json::Value::Number(serde_json::Number::from(*b)))
                            .collect(),
                    ),
                );
                offset += len as usize;
            }
            // An unknown ColumnType has no known width, so the remainder of
            // the blob cannot be parsed. Stop rather than guess.
            _ => {
                return serde_json::Value::Object(map);
            }
        }
    }

    serde_json::Value::Object(map)
}

/// A material colour: `appearance.schema.json` fixes `diffuseColor`,
/// `emissiveColor` and `specularColor` at exactly three numbers, which
/// `cjseq::Color` spells as `[f64; 3]`. A stored vector of any other length is
/// not a colour and is dropped rather than padded or truncated.
fn to_color(color: Option<flatbuffers::Vector<'_, f64>>) -> Option<CjColor> {
    let values: Vec<f64> = color?.iter().collect();
    values.try_into().ok()
}

/// A texture's `borderColor`, which the schema allows to be three *or* four
/// numbers (`"minItems": 3, "maxItems": 4`) -- the one CityJSON colour that is
/// not fixed at three.
fn to_border_color(color: Option<flatbuffers::Vector<'_, f64>>) -> Option<CjBorderColor> {
    let values: Vec<f64> = color?.iter().collect();
    match values.len() {
        3 => values.try_into().ok().map(CjBorderColor::Rgb),
        4 => values.try_into().ok().map(CjBorderColor::Rgba),
        _ => None,
    }
}

/// The CityJSON `wrapMode` for a FlatCityBuf tag.
///
/// The inverse of `serializer::fb_wrap_mode`. A FlatBuffers enumeration is
/// generated as a newtype over `u8`, not as a Rust `enum`, so this direction
/// cannot be made exhaustive by the compiler the way its mirror is. The `_`
/// arm therefore *errors*: it does not fall back to a default, which is how a
/// texture written `"wrapMode": "wrap"` used to come back as `"None"`. An
/// unrecognised tag means the file was written by a newer writer or is
/// corrupt, and the caller is told so.
fn cj_wrap_mode(tag: WrapMode) -> Result<CjWrapMode, Error> {
    Ok(match tag {
        WrapMode::None => CjWrapMode::None,
        WrapMode::Wrap => CjWrapMode::Wrap,
        WrapMode::Mirror => CjWrapMode::Mirror,
        WrapMode::Clamp => CjWrapMode::Clamp,
        WrapMode::Border => CjWrapMode::Border,
        _ => return Err(Error::UnknownEnumTag("wrapMode", format!("{tag:?}"))),
    })
}

/// The CityJSON `textureType` for a FlatCityBuf tag. Errors on an unknown tag,
/// as above.
fn cj_texture_type(tag: TextureType) -> Result<CjTextureType, Error> {
    Ok(match tag {
        TextureType::Unknown => CjTextureType::Unknown,
        TextureType::Specific => CjTextureType::Specific,
        TextureType::Typical => CjTextureType::Typical,
        _ => return Err(Error::UnknownEnumTag("textureType", format!("{tag:?}"))),
    })
}

/// The CityJSON texture `type` for a FlatCityBuf tag. Errors on an unknown
/// tag, as above.
fn cj_texture_format(tag: TextureFormat) -> Result<CjTextureFormat, Error> {
    Ok(match tag {
        TextureFormat::PNG => CjTextureFormat::PNG,
        TextureFormat::JPG => CjTextureFormat::JPG,
        _ => return Err(Error::UnknownEnumTag("type", format!("{tag:?}"))),
    })
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

                // An unspecified role is `null` in CityJSON, and the writer
                // spells that as the empty string.
                let children_roles = co.children_roles().map(|c| {
                    c.iter()
                        .map(|s| (!s.is_empty()).then(|| s.to_string()))
                        .collect()
                });

                let mut cjco = CjCityObject::new(to_cj_co_type(co.type_(), co.extension_type()));
                cjco.geographical_extent = geographical_extent;
                cjco.attributes = attributes;
                cjco.geometry = final_geometries;
                cjco.children = co
                    .children()
                    .map(|c| c.iter().map(|s| s.to_string()).collect());
                cjco.children_roles = children_roles;
                cjco.parents = co
                    .parents()
                    .map(|p| p.iter().map(|s| s.to_string()).collect());
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

        // Decode materials. `name` is the schema's one required member; every
        // other one is optional, and an absent one stays absent rather than
        // reappearing as `null`.
        if let Some(materials) = appearance.materials() {
            let cj_materials = materials
                .iter()
                .map(|m| CjMaterialObject {
                    name: m.name().to_string(),
                    ambient_intensity: m.ambient_intensity(),
                    diffuse_color: to_color(m.diffuse_color()),
                    emissive_color: to_color(m.emissive_color()),
                    specular_color: to_color(m.specular_color()),
                    shininess: m.shininess(),
                    transparency: m.transparency(),
                    is_smooth: m.is_smooth(),
                })
                .collect();

            cj_appearance.materials = Some(cj_materials);
        }

        // Decode textures
        if let Some(textures) = appearance.textures() {
            let cj_textures = textures
                .iter()
                .map(|t| {
                    Ok(CjTextureObject {
                        thetype: Some(cj_texture_format(t.type_())?),
                        // `image` is mandatory in the `.fbs` but optional in
                        // CityJSON, so the empty string the writer stores for
                        // an absent one decodes back to absent.
                        image: Some(t.image())
                            .filter(|i| !i.is_empty())
                            .map(str::to_string),
                        wrap_mode: t.wrap_mode().map(cj_wrap_mode).transpose()?,
                        texture_type: t.texture_type().map(cj_texture_type).transpose()?,
                        border_color: to_border_color(t.border_color()),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;

            cj_appearance.textures = Some(cj_textures);
        }

        // Decode vertices_texture
        if let Some(vertices_texture) = appearance.vertices_texture() {
            cj_appearance.vertices_texture = Some(
                vertices_texture
                    .iter()
                    .map(|v| vec![v.u(), v.v()])
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
    let geometry_type = g.type_();

    // The surfaces decide whether there is a `semantics` member at all; the
    // values vector being absent is `"values": null`, which is a member with a
    // null value rather than no member.
    let semantics: Option<CjSemantics> = g.semantics_objects().map(|semantics_objects| {
        let semantics_objects = semantics_objects.iter().collect::<Vec<_>>();
        let semantics_values = g.semantics().map(|v| v.iter().collect::<Vec<_>>());
        decode_semantics(
            &solids,
            &shells,
            geometry_type,
            semantics_objects,
            semantics_values,
            semantic_attr_schema,
        )
    });

    // Decode material mappings if present
    let material = if let Some(material_mappings) = g.material() {
        decode_materials(geometry_type, &material_mappings.iter().collect::<Vec<_>>())
    } else {
        None
    };

    // Decode texture mappings if present
    let texture = if let Some(texture_mappings) = g.texture() {
        decode_textures(geometry_type, &texture_mappings.iter().collect::<Vec<_>>())
    } else {
        None
    };

    let common = CjGeometryCommon {
        semantics,
        material,
        texture,
    };
    let lod = g.lod().map(|v| v.to_string());

    // The geometry type selects the depth of `boundaries`; nothing looks at
    // which of the count arrays happen to be populated.
    Ok(match geometry_type {
        GeometryType::MultiPoint => CjGeometry::MultiPoint {
            lod,
            boundaries: decode_points(&indices),
            common,
        },
        GeometryType::MultiLineString => CjGeometry::MultiLineString {
            lod,
            boundaries: decode_rings(&strings, &indices),
            common,
        },
        GeometryType::MultiSurface => CjGeometry::MultiSurface {
            lod,
            boundaries: decode_surfaces(&surfaces, &strings, &indices),
            common,
        },
        GeometryType::CompositeSurface => CjGeometry::CompositeSurface {
            lod,
            boundaries: decode_surfaces(&surfaces, &strings, &indices),
            common,
        },
        GeometryType::MultiSolid => CjGeometry::MultiSolid {
            lod,
            boundaries: decode_solids(&solids, &shells, &surfaces, &strings, &indices),
            common,
        },
        GeometryType::CompositeSolid => CjGeometry::CompositeSolid {
            lod,
            boundaries: decode_solids(&solids, &shells, &surfaces, &strings, &indices),
            common,
        },
        // `Solid`, and any tag a newer writer may add: a Solid is the shape
        // the writer falls back to as well, so the two agree.
        _ => CjGeometry::Solid {
            lod,
            boundaries: decode_shells(&shells, &surfaces, &strings, &indices),
            common,
        },
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
            vec![fb_boundaries.get(0) as usize]
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

    // A GeometryInstance has no lod, semantics, material or texture: they live
    // on the template it refers to, which is why the variant does not have
    // fields for them.
    Ok(CjGeometry::GeometryInstance {
        boundaries,
        template: template_index as usize,
        transformation_matrix: transformation_matrix_array,
    })
}

pub(crate) fn to_cj_vertices(vertices: Vec<&Vertex>) -> Vec<Vec<i64>> {
    vertices
        .iter()
        .map(|v| vec![v.x() as i64, v.y() as i64, v.z() as i64])
        .collect()
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

        // Verify the decoded geometry. The variant carries `template`,
        // `transformationMatrix` and single-index boundaries by construction,
        // so an irrefutable-let would be a weaker assertion than this match.
        let CjGeometry::GeometryInstance {
            boundaries,
            template,
            transformation_matrix: matrix,
        } = cj_geometry
        else {
            panic!("expected a GeometryInstance");
        };
        assert_eq!(template, 5);
        assert_eq!(boundaries, vec![42]);
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
