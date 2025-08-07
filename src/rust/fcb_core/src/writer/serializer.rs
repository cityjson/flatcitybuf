use crate::attribute::{encode_attributes_with_schema, AttributeSchema, AttributeSchemaMethods};
use crate::fb::{
    Appearance, AppearanceArgs, CityFeature, CityFeatureArgs, CityObject, CityObjectArgs,
    CityObjectType, Geometry, GeometryArgs, GeometryType, Material, MaterialArgs, SemanticObject,
    SemanticObjectArgs, SemanticSurfaceType, Texture, TextureArgs, TextureType, Vec2, Vertex,
    WrapMode,
};
use crate::fb::{Column, ColumnArgs};
use crate::fb::{
    GeographicalExtent, Header, HeaderArgs, ReferenceSystem, ReferenceSystemArgs, Transform, Vector,
};
use crate::geom_encoder::encode;
use crate::{
    AttributeIndex, DoubleVertex, Extension, ExtensionArgs, GeometryInstance, GeometryInstanceArgs,
    MaterialMapping, MaterialMappingArgs, TextureFormat, TextureMapping, TextureMappingArgs,
    TransformationMatrix,
};
use cjseq::{
    Appearance as CjAppearance, Boundaries as CjBoundaries, CityJSON, CityJSONFeature,
    CityObject as CjCityObject, Geometry as CjGeometry, GeometryType as CjGeometryType,
    PointOfContact as CjPointOfContact, ReferenceSystem as CjReferenceSystem,
    TextFormat as CjTextFormat, TextType as CjTextType, Transform as CjTransform,
    WrapMode as CjWrapMode,
};

use cjseq::ExtensionFile as CjExtensionFile;

use crate::packed_rtree::NodeItem;
use flatbuffers::FlatBufferBuilder;
use serde_json::Value;

use super::geom_encoder::{GMBoundaries, GMSemantics, MaterialMapping as GMMaterialMapping};
use super::header_writer::HeaderWriterOptions;
use crate::error::Result;

#[derive(Debug, Clone)]
pub(super) struct AttributeIndexInfo {
    pub index: u16,
    pub length: u32,
    pub branching_factor: u16,
    pub num_unique_items: u32,
}
/// -----------------------------------
/// Serializer for Header
/// -----------------------------------
/// Converts a CityJSON header into FlatBuffers format
///
/// # Arguments
///
/// * `fbb` - FlatBuffers builder instance
/// * `cj` - CityJSON data containing header information
/// * `header_metadata` - Additional metadata for the header
pub(super) fn to_fcb_header<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    cj: &CityJSON,
    header_options: HeaderWriterOptions,
    attr_schema: &AttributeSchema,
    semantic_attr_schema: Option<&AttributeSchema>,
    attribute_indices_info: Option<&[AttributeIndexInfo]>,
) -> Result<flatbuffers::WIPOffset<Header<'a>>> {
    let version = Some(fbb.create_string(&cj.version));
    let transform = to_transform(&cj.transform);
    let features_count: u64 = header_options.feature_count;
    let columns = Some(to_columns(fbb, attr_schema));
    let semantic_columns = semantic_attr_schema.map(|schema| to_columns(fbb, schema));
    let index_node_size = header_options.index_node_size;
    let attribute_index = {
        if let Some(attribute_indices_info) = attribute_indices_info {
            let attribute_indices_info_vec = attribute_indices_info
                .iter()
                .map(|info| {
                    AttributeIndex::new(
                        info.index,
                        info.length,
                        info.branching_factor,
                        info.num_unique_items,
                    )
                })
                .collect::<Vec<_>>();
            Some(fbb.create_vector(&attribute_indices_info_vec))
        } else {
            None
        }
    };

    // Handle extensions, if present
    let extensions = if let Some(extensions) = &cj.extensions {
        let mut extension_files = Vec::new();
        for (name, ext) in extensions.iter() {
            let extension_file = ext.fetch_extension_file(name.clone())?;
            extension_files.push(extension_file);
        }

        let extensions = extension_files
            .iter()
            .map(|ext| to_extension(fbb, ext))
            .collect::<Result<Vec<_>>>()?;
        Some(fbb.create_vector(&extensions))
    } else {
        None
    };

    // Use the geographical_extent from the HeaderWriterOptions if provided
    let geographical_extent_from_options = header_options
        .geographical_extent
        .as_ref()
        .map(to_geographical_extent);

    let appearance = cj.appearance.as_ref().map(|app| to_appearance(fbb, app));

    let (templates, templates_vertices) = match &cj.geometry_templates {
        Some(gm) => {
            let templates_vertices = to_templates_vertices(fbb, &gm.vertices_templates);

            let gm_vec = gm
                .templates
                .iter()
                .map(|g| to_geometry(fbb, g, semantic_attr_schema))
                .collect::<Vec<_>>();
            (Some(fbb.create_vector(&gm_vec)), Some(templates_vertices))
        }
        None => (None, None),
    };

    if let Some(meta) = cj.metadata.as_ref() {
        let reference_system = meta
            .reference_system
            .as_ref()
            .map(|ref_sys| to_reference_system(fbb, ref_sys));
        // Use the geographical_extent from the HeaderWriterOptions if provided, otherwise use the one from the metadata
        let geographical_extent = geographical_extent_from_options.or_else(|| {
            meta.geographical_extent
                .as_ref()
                .map(to_geographical_extent)
        });
        let identifier = meta.identifier.as_ref().map(|i| fbb.create_string(i));
        let reference_date = meta.reference_date.as_ref().map(|r| fbb.create_string(r));
        let title = meta.title.as_ref().map(|t| fbb.create_string(t));
        let poc_fields = meta
            .point_of_contact
            .as_ref()
            .map(|poc| to_point_of_contact(fbb, poc));
        let (
            poc_contact_name,
            poc_contact_type,
            poc_role,
            poc_phone,
            poc_email,
            poc_website,
            poc_address_thoroughfare_number,
            poc_address_thoroughfare_name,
            poc_address_locality,
            poc_address_postcode,
            poc_address_country,
        ) = poc_fields.map_or(
            (
                None, None, None, None, None, None, None, None, None, None, None,
            ),
            |poc| {
                (
                    poc.poc_contact_name,
                    poc.poc_contact_type,
                    poc.poc_role,
                    poc.poc_phone,
                    poc.poc_email,
                    poc.poc_website,
                    poc.poc_address_thoroughfare_number,
                    poc.poc_address_thoroughfare_name,
                    poc.poc_address_locality,
                    poc.poc_address_postcode,
                    poc.poc_address_country,
                )
            },
        );

        Ok(Header::create(
            fbb,
            &HeaderArgs {
                transform: Some(transform).as_ref(),
                columns,
                semantic_columns,
                features_count,
                index_node_size,
                geographical_extent: geographical_extent.as_ref(),
                reference_system,
                identifier,
                attribute_index,
                reference_date,
                title,
                poc_contact_name,
                poc_contact_type,
                poc_role,
                poc_phone,
                poc_email,
                poc_website,
                poc_address_thoroughfare_number,
                poc_address_thoroughfare_name,
                poc_address_locality,
                poc_address_postcode,
                poc_address_country,
                attributes: None,
                version,
                appearance,
                templates,
                templates_vertices,
                extensions,
            },
        ))
    } else {
        Ok(Header::create(
            fbb,
            &HeaderArgs {
                transform: Some(transform).as_ref(),
                columns,
                semantic_columns,
                features_count,
                index_node_size,
                geographical_extent: geographical_extent_from_options.as_ref(),
                version,
                attribute_index,
                extensions,
                ..Default::default()
            },
        ))
    }
}

/// Converts CityJSON geographical extent to FlatBuffers format
///
/// # Arguments
///
/// * `geographical_extent` - Array of 6 values [minx, miny, minz, maxx, maxy, maxz]
pub(super) fn to_geographical_extent(geographical_extent: &[f64; 6]) -> GeographicalExtent {
    let min = Vector::new(
        geographical_extent[0],
        geographical_extent[1],
        geographical_extent[2],
    );
    let max = Vector::new(
        geographical_extent[3],
        geographical_extent[4],
        geographical_extent[5],
    );
    GeographicalExtent::new(&min, &max)
}

/// Converts CityJSON transform to FlatBuffers format
///
/// # Arguments
///
/// * `transform` - CityJSON transform containing scale and translate values
pub(super) fn to_transform(transform: &CjTransform) -> Transform {
    let scale = Vector::new(transform.scale[0], transform.scale[1], transform.scale[2]);
    let translate = Vector::new(
        transform.translate[0],
        transform.translate[1],
        transform.translate[2],
    );
    Transform::new(&scale, &translate)
}

/// Converts CityJSON reference system to FlatBuffers format
///
/// # Arguments
///
/// * `fbb` - FlatBuffers builder instance
/// * `metadata` - CityJSON metadata containing reference system information
pub(super) fn to_reference_system<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    ref_system: &CjReferenceSystem,
) -> flatbuffers::WIPOffset<ReferenceSystem<'a>> {
    let authority = Some(fbb.create_string(&ref_system.authority));

    let version = ref_system.version.parse::<i32>().unwrap_or_else(|e| {
        println!("failed to parse version: {e}");
        0
    });
    let code = ref_system.code.parse::<i32>().unwrap_or_else(|e| {
        println!("failed to parse code: {e}");
        0
    });

    let code_string = None; // TODO: implement code_string

    ReferenceSystem::create(
        fbb,
        &ReferenceSystemArgs {
            authority,
            version,
            code,
            code_string,
        },
    )
}

/// Internal struct used only as a return type for `to_point_of_contact`
#[doc(hidden)]
struct FcbPointOfContact<'a> {
    poc_contact_name: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_contact_type: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_role: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_phone: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_email: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_website: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_address_thoroughfare_number: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_address_thoroughfare_name: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_address_locality: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_address_postcode: Option<flatbuffers::WIPOffset<&'a str>>,
    poc_address_country: Option<flatbuffers::WIPOffset<&'a str>>,
}

fn to_point_of_contact<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    poc: &CjPointOfContact,
) -> FcbPointOfContact<'a> {
    let poc_contact_name = Some(fbb.create_string(&poc.contact_name));

    let poc_contact_type = poc.contact_type.as_ref().map(|ct| fbb.create_string(ct));
    let poc_role = poc.role.as_ref().map(|r| fbb.create_string(r));
    let poc_phone = poc.phone.as_ref().map(|p| fbb.create_string(p));
    let poc_email = Some(fbb.create_string(&poc.email_address));
    let poc_website = poc.website.as_ref().map(|w| fbb.create_string(w));
    let poc_address_thoroughfare_number = poc
        .address
        .as_ref()
        .map(|a| fbb.create_string(&a.thoroughfare_number.to_string()));
    let poc_address_thoroughfare_name = poc
        .address
        .as_ref()
        .map(|a| fbb.create_string(&a.thoroughfare_name));
    let poc_address_locality = poc.address.as_ref().map(|a| fbb.create_string(&a.locality));
    let poc_address_postcode = poc
        .address
        .as_ref()
        .map(|a| fbb.create_string(&a.postal_code));
    let poc_address_country = poc.address.as_ref().map(|a| fbb.create_string(&a.country));
    FcbPointOfContact {
        poc_contact_name,
        poc_contact_type,
        poc_role,
        poc_phone,
        poc_email,
        poc_website,
        poc_address_thoroughfare_number,
        poc_address_thoroughfare_name,
        poc_address_locality,
        poc_address_postcode,
        poc_address_country,
    }
}

/// Converts the ExtensionSchema to a FlatBuffers Extension table
///
/// # Arguments
///
/// * `fbb` - FlatBuffers builder instance
/// * `extension` - Extension file
///
/// # Returns
///
/// * `flatbuffers::WIPOffset<Extension<'a>>` - Extension table
pub fn to_extension<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    extension: &CjExtensionFile,
) -> Result<flatbuffers::WIPOffset<Extension<'a>>> {
    let name = fbb.create_string(&extension.name);
    let description = fbb.create_string(&extension.description);
    let url = fbb.create_string(&extension.url);
    let version = fbb.create_string(&extension.version);
    let version_cityjson = fbb.create_string(&extension.version_city_json);

    // Stringified JSON for extension components
    let extra_attributes = serde_json::to_string(&extension.extra_attributes)?;
    let extra_attributes = fbb.create_string(&extra_attributes);

    let extra_city_objects = serde_json::to_string(&extension.extra_city_objects)?;
    let extra_city_objects = fbb.create_string(&extra_city_objects);

    let extra_root_properties = serde_json::to_string(&extension.extra_root_properties)?;
    let extra_root_properties = fbb.create_string(&extra_root_properties);

    let extra_semantic_surfaces = serde_json::to_string(&extension.extra_semantic_surfaces)?;
    let extra_semantic_surfaces = fbb.create_string(&extra_semantic_surfaces);

    Ok(Extension::create(
        fbb,
        &ExtensionArgs {
            name: Some(name),
            description: Some(description),
            url: Some(url),
            version: Some(version),
            version_cityjson: Some(version_cityjson),
            extra_attributes: Some(extra_attributes),
            extra_city_objects: Some(extra_city_objects),
            extra_root_properties: Some(extra_root_properties),
            extra_semantic_surfaces: Some(extra_semantic_surfaces),
        },
    ))
}

/// -----------------------------------
/// Serializer for CityJSONFeature
/// -----------------------------------
/// Creates a CityFeature in FlatBuffers format
///
/// # Arguments
///
/// * `fbb` - FlatBuffers builder instance
/// * `id` - Feature identifier
/// * `objects` - Vector of city objects
/// * `vertices` - Vector of vertex coordinates
pub(super) fn to_fcb_city_feature<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &str,
    city_feature: &CityJSONFeature,
    attr_schema: &AttributeSchema,
    semantic_attr_schema: Option<&AttributeSchema>,
) -> (flatbuffers::WIPOffset<CityFeature<'a>>, NodeItem) {
    let id = Some(fbb.create_string(id));
    let city_objects: Vec<_> = city_feature
        .city_objects
        .iter()
        .map(|(id, co)| to_city_object(fbb, id, co, attr_schema, semantic_attr_schema))
        .collect();
    let objects = Some(fbb.create_vector(&city_objects));
    let vertices = Some(
        fbb.create_vector(
            &city_feature
                .vertices
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

    // Handle appearance if present
    let appearance = city_feature
        .appearance
        .as_ref()
        .map(|app| to_appearance(fbb, app));
    let min_x = city_feature
        .vertices
        .iter()
        .map(|v| v[0])
        .min()
        .unwrap_or(0) as f64;
    let min_y = city_feature
        .vertices
        .iter()
        .map(|v| v[1])
        .min()
        .unwrap_or(0) as f64;
    let max_x = city_feature
        .vertices
        .iter()
        .map(|v| v[0])
        .max()
        .unwrap_or(0) as f64;
    let max_y = city_feature
        .vertices
        .iter()
        .map(|v| v[1])
        .max()
        .unwrap_or(0) as f64;

    let bbox = NodeItem::bounds(min_x, min_y, max_x, max_y);
    (
        CityFeature::create(
            fbb,
            &CityFeatureArgs {
                id,
                objects,
                vertices,
                appearance,
            },
        ),
        bbox,
    )
}

pub(super) fn to_appearance<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    appearance: &CjAppearance,
) -> flatbuffers::WIPOffset<Appearance<'a>> {
    // Handle appearance if present

    let materials = appearance.materials.as_ref().map(|materials| {
        let material_offsets: Vec<_> = materials
            .iter()
            .map(|m| {
                let name = fbb.create_string(&m.name);
                let diffuse_color = m.diffuse_color.map(|c| fbb.create_vector(&c));
                let emissive_color = m.emissive_color.map(|c| fbb.create_vector(&c));
                let specular_color = m.specular_color.map(|c| fbb.create_vector(&c));
                Material::create(
                    fbb,
                    &MaterialArgs {
                        name: Some(name),
                        ambient_intensity: m.ambient_intensity,
                        diffuse_color,
                        emissive_color,
                        specular_color,
                        shininess: m.shininess,
                        transparency: m.transparency,
                        is_smooth: m.is_smooth,
                    },
                )
            })
            .collect();
        fbb.create_vector(&material_offsets)
    });

    let textures = appearance.textures.as_ref().map(|textures| {
        let texture_offsets: Vec<_> = textures
            .iter()
            .map(|t| {
                let image = fbb.create_string(&t.image);
                let border_color = t.border_color.map(|c| fbb.create_vector(&c));
                let texture_format = match t.texture_format {
                    CjTextFormat::Png => TextureFormat::PNG,
                    CjTextFormat::Jpg => TextureFormat::JPG,
                };
                let wrap_mode = t.wrap_mode.as_ref().map(|w| match w {
                    CjWrapMode::None => WrapMode::None,
                    CjWrapMode::Wrap => WrapMode::Wrap,
                    CjWrapMode::Mirror => WrapMode::Mirror,
                    CjWrapMode::Clamp => WrapMode::Clamp,
                    CjWrapMode::Border => WrapMode::Border,
                });
                let texture_type = t.texture_type.as_ref().map(|t| match t {
                    CjTextType::Unknown => TextureType::Unknown,
                    CjTextType::Specific => TextureType::Specific,
                    CjTextType::Typical => TextureType::Typical,
                });
                Texture::create(
                    fbb,
                    &TextureArgs {
                        type_: texture_format,
                        image: Some(image),
                        wrap_mode,
                        texture_type,
                        border_color,
                    },
                )
            })
            .collect();
        fbb.create_vector(&texture_offsets)
    });

    let vertices_texture = appearance.vertices_texture.as_ref().map(|vertices| {
        fbb.create_vector(
            &vertices
                .iter()
                .map(|v| Vec2::new(v[0], v[1]))
                .collect::<Vec<_>>(),
        )
    });

    let default_theme_texture = appearance
        .default_theme_texture
        .as_ref()
        .map(|t| fbb.create_string(t));
    let default_theme_material = appearance
        .default_theme_material
        .as_ref()
        .map(|m| fbb.create_string(m));

    Appearance::create(
        fbb,
        &AppearanceArgs {
            materials,
            textures,
            vertices_texture,
            default_theme_texture,
            default_theme_material,
        },
    )
}

/// Converts CityJSON object to FlatBuffers
///
/// # Arguments
///
/// * `fbb` - FlatBuffers builder instance
/// * `id` - City object ID
/// * `co` - CityJSON object
/// * `attr_schema` - Attribute schema
pub(super) fn to_city_object<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &str,
    co: &CjCityObject,
    attr_schema: &AttributeSchema,
    semantic_attr_schema: Option<&AttributeSchema>,
) -> flatbuffers::WIPOffset<CityObject<'a>> {
    let id = Some(fbb.create_string(id));

    let (type_, extension_type) = to_co_type(&co.thetype);
    let extension_type = extension_type.as_ref().map(|et| fbb.create_string(et));
    let geographical_extent = co.geographical_extent.as_ref().map(to_geographical_extent);
    let geometry_without_instances = co.geometry.as_ref().map(|gs| {
        gs.iter()
            .filter(|g| g.thetype != CjGeometryType::GeometryInstance)
            .collect::<Vec<_>>()
    });
    let geometry_instances = co.geometry.as_ref().map(|gs| {
        gs.iter()
            .filter(|g| g.thetype == CjGeometryType::GeometryInstance)
            .collect::<Vec<_>>()
    });
    let geometries = {
        let geometries = geometry_without_instances.map(|gs| {
            gs.iter()
                .map(|g| to_geometry(fbb, g, semantic_attr_schema))
                .collect::<Vec<_>>()
        });
        geometries.map(|geometries| fbb.create_vector(&geometries))
    };

    let geometry_instances = {
        let geometry_instances = geometry_instances.map(|gs| {
            gs.iter()
                .map(|g| to_geometry_instance(fbb, g))
                .collect::<Vec<_>>()
        });
        geometry_instances.map(|geometry_instances| fbb.create_vector(&geometry_instances))
    };

    let attributes_and_columns = co
        .attributes
        .as_ref()
        .map(|attr| {
            if !attr.is_object() {
                return (None, None);
            }
            let (attr_vec, own_schema) = to_fcb_attribute(fbb, attr, attr_schema);
            let columns = own_schema.map(|schema| to_columns(fbb, &schema));
            (Some(attr_vec), columns)
        })
        .unwrap_or((None, None));

    let (attributes, columns) = attributes_and_columns;

    let children = {
        let children = co
            .children
            .as_ref()
            .map(|c| c.iter().map(|s| fbb.create_string(s)).collect::<Vec<_>>());
        children.map(|c| fbb.create_vector(&c))
    };

    let children_roles = {
        let children_roles_strings = co
            .children_roles
            .as_ref()
            .map(|c| c.iter().map(|r| fbb.create_string(r)).collect::<Vec<_>>());
        children_roles_strings.map(|c| fbb.create_vector(&c))
    };

    let parents = {
        let parents = co
            .parents
            .as_ref()
            .map(|p| p.iter().map(|s| fbb.create_string(s)).collect::<Vec<_>>());
        parents.map(|p| fbb.create_vector(&p))
    };

    CityObject::create(
        fbb,
        &CityObjectArgs {
            id,
            type_,
            extension_type,
            geographical_extent: geographical_extent.as_ref(),
            geometry: geometries,
            geometry_instances,
            attributes,
            columns,
            children,
            children_roles,
            parents,
        },
    )
}

/// Converts CityJSON object type to FlatBuffers enum
///
/// # Arguments
///
/// * `co_type` - String representation of CityJSON object type
pub(super) fn to_co_type(co_type: &str) -> (CityObjectType, Option<String>) {
    // If it starts with a '+', it's an extension type
    let extension_type = if co_type.starts_with('+') {
        Some(co_type.to_string())
    } else {
        None
    };

    let obj_type = if extension_type.is_some() {
        // If an extension type, use ExtensionObject
        CityObjectType::ExtensionObject
    } else {
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
            "GenericCityObject" => CityObjectType::GenericCityObject,
            _ => CityObjectType::GenericCityObject,
        }
    };

    (obj_type, extension_type)
}

/// Converts CityJSON geometry type to FlatBuffers enum
///
/// # Arguments
///
/// * `geometry_type` - CityJSON geometry type
pub(super) fn to_geom_type(geometry_type: &CjGeometryType) -> GeometryType {
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

/// Converts CityJSON semantic surface type to FlatBuffers enum
///
/// # Arguments
///
/// * `ss_type` - String representation of semantic surface type
pub(super) fn to_semantic_surface_type(ss_type: &str) -> (SemanticSurfaceType, Option<String>) {
    // Handle extension types (starting with +)
    if ss_type.starts_with('+') {
        return (
            SemanticSurfaceType::ExtraSemanticSurface,
            Some(ss_type.to_string()),
        );
    }

    // Handle standard surface types
    let surface_type = match ss_type {
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
        _ => SemanticSurfaceType::ExtraSemanticSurface,
    };

    // Standard types don't have extension_type
    (surface_type, None)
}

/// Converts CityJSON geometry to FlatBuffers format
///
/// # Arguments
///
/// * `fbb` - FlatBuffers builder instance
/// * `geometry` - CityJSON geometry object
pub(crate) fn to_geometry<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    geometry: &CjGeometry,
    semantic_attr_schema: Option<&AttributeSchema>,
) -> flatbuffers::WIPOffset<Geometry<'a>> {
    let type_ = to_geom_type(&geometry.thetype);
    let lod = geometry.lod.as_ref().map(|lod| fbb.create_string(lod));

    let encoded = encode(
        &geometry.boundaries,
        geometry.semantics.as_ref(),
        geometry.texture.as_ref(),
        geometry.material.as_ref(),
    );
    let GMBoundaries {
        solids,
        shells,
        surfaces,
        strings,
        indices,
    } = encoded.boundaries;
    let semantics = encoded
        .semantics
        .map(|GMSemantics { surfaces, values }| (surfaces, values));

    let solids = Some(fbb.create_vector(&solids));
    let shells = Some(fbb.create_vector(&shells));
    let surfaces = Some(fbb.create_vector(&surfaces));
    let strings = Some(fbb.create_vector(&strings));
    let boundary_indices = Some(fbb.create_vector(&indices));

    let (semantics_objects, semantics_values) =
        semantics.map_or((None, None), |(surface, values)| {
            let semantics_objects = surface
                .iter()
                .map(|s| {
                    let children = s.children.as_ref().map(|c| fbb.create_vector(c));

                    let (type_, extension_type) = to_semantic_surface_type(&s.thetype);
                    let extension_type = extension_type.map(|s| fbb.create_string(&s));
                    let attributes = if let Some(other) = &s.other {
                        semantic_attr_schema.as_ref().map(|schema| {
                            fbb.create_vector(&encode_attributes_with_schema(other, schema))
                        })
                    } else {
                        None
                    };
                    SemanticObject::create(
                        fbb,
                        &SemanticObjectArgs {
                            type_,
                            extension_type,
                            attributes,
                            children,
                            parent: s.parent,
                        },
                    )
                })
                .collect::<Vec<_>>();

            (
                Some(fbb.create_vector(&semantics_objects)),
                Some(fbb.create_vector(&values)),
            )
        });

    let material_mappings = encoded.materials.map(|m| {
        let mappings = m
            .iter()
            .map(|m| match m {
                GMMaterialMapping::Value(v) => {
                    let theme = Some(fbb.create_string(&v.theme));
                    let value = Some(v.value);
                    MaterialMapping::create(
                        fbb,
                        &MaterialMappingArgs {
                            theme,
                            solids: None,
                            shells: None,
                            vertices: None,
                            value,
                        },
                    )
                }
                GMMaterialMapping::Values(v) => {
                    let theme = Some(fbb.create_string(&v.theme));
                    let solids = Some(fbb.create_vector(&v.solids));
                    let shells = Some(fbb.create_vector(&v.shells));
                    let vertices = Some(fbb.create_vector(&v.vertices));
                    let value = None;
                    MaterialMapping::create(
                        fbb,
                        &MaterialMappingArgs {
                            theme,
                            solids,
                            shells,
                            vertices,
                            value,
                        },
                    )
                }
            })
            .collect::<Vec<_>>();
        fbb.create_vector(&mappings)
    });

    let texture_mappings = encoded.textures.map(|t| {
        let mappings = t
            .iter()
            .map(|t| {
                let theme = Some(fbb.create_string(&t.theme));
                let solids = Some(fbb.create_vector(&t.solids));
                let shells = Some(fbb.create_vector(&t.shells));
                let surfaces = Some(fbb.create_vector(&t.surfaces));
                let strings = Some(fbb.create_vector(&t.strings));
                let vertices = Some(fbb.create_vector(&t.vertices));
                TextureMapping::create(
                    fbb,
                    &TextureMappingArgs {
                        theme,
                        solids,
                        shells,
                        surfaces,
                        strings,
                        vertices,
                    },
                )
            })
            .collect::<Vec<_>>();
        fbb.create_vector(&mappings)
    });

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
            material: material_mappings,
            texture: texture_mappings,
        },
    )
}

pub(super) fn to_geometry_instance<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    geometry: &CjGeometry,
) -> flatbuffers::WIPOffset<GeometryInstance<'a>> {
    if geometry.template.is_none() || geometry.transformation_matrix.is_none() {
        panic!("Geometry instance must have a template and transformation matrix.");
    }
    if let CjBoundaries::Nested(_) = &geometry.boundaries {
        panic!("Nested boundaries are not valid for geometry instances. "); //TODO: don't use panic, instead, return Result type
    }

    let template = geometry.template.unwrap_or(0) as u32;
    let boundaries = match &geometry.boundaries {
        CjBoundaries::Indices(indices) => Some(fbb.create_vector(indices)), //This expect the given CityJSON has only one vertex index.
        CjBoundaries::Nested(_) => {
            panic!("Nested boundaries are not valid for geometry instances. "); //TODO: don't use panic, instead, return Result type
        }
    };
    let transformation = {
        let m = geometry.transformation_matrix.unwrap();
        Some(TransformationMatrix::new(
            m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9], m[10], m[11], m[12], m[13],
            m[14], m[15],
        ))
    };
    GeometryInstance::create(
        fbb,
        &GeometryInstanceArgs {
            template,
            transformation: transformation.as_ref(),
            boundaries,
        },
    )
}

pub(super) fn to_templates_vertices<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    vertices: &[[f64; 3]],
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, DoubleVertex>> {
    let vertices_vec = vertices
        .iter()
        .map(|v| DoubleVertex::new(v[0], v[1], v[2]))
        .collect::<Vec<_>>();
    fbb.create_vector(&vertices_vec)
}

pub(crate) fn to_columns<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    attr_schema: &AttributeSchema,
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<Column<'a>>>> {
    let mut sorted_schema: Vec<_> = attr_schema.iter().collect();
    sorted_schema.sort_by_key(|(_, (index, _))| *index);
    let columns_vec = sorted_schema
        .iter()
        .map(|(name, (index, column_type))| {
            let name = fbb.create_string(name);
            Column::create(
                fbb,
                &ColumnArgs {
                    name: Some(name),
                    index: *index,
                    type_: *column_type,
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    fbb.create_vector(&columns_vec)
}

pub(super) fn to_fcb_attribute<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    attr: &Value,
    schema: &AttributeSchema,
) -> (
    flatbuffers::WIPOffset<flatbuffers::Vector<'a, u8>>,
    Option<AttributeSchema>,
) {
    let mut is_own_schema = false;
    for (key, _) in attr.as_object().unwrap().iter() {
        if !schema.contains_key(key) {
            is_own_schema = true;
        }
    }
    if is_own_schema {
        let mut own_schema = AttributeSchema::new();
        own_schema.add_attributes(attr);
        let encoded = encode_attributes_with_schema(attr, &own_schema);
        (fbb.create_vector(&encoded), Some(own_schema))
    } else {
        let encoded = encode_attributes_with_schema(attr, schema);
        (fbb.create_vector(&encoded), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{deserializer::to_cj_co_type, feature_generated::root_as_city_feature};

    use anyhow::Result;
    use cjseq::CityJSONFeature;
    use flatbuffers::FlatBufferBuilder;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_to_fcb_city_feature() -> Result<()> {
        let cj_city_feature: CityJSONFeature = CityJSONFeature::from_str(
            r#"{"type":"CityJSONFeature","id":"NL.IMBAG.Pand.0503100000005156","CityObjects":{"NL.IMBAG.Pand.0503100000005156-0":{"type":"BuildingPart","attributes":{},"geometry":[{"type":"Solid","lod":"1.2","boundaries":[[[[6,1,0,5,4,3,7,8]],[[9,5,0,10]],[[10,0,1,11]],[[12,3,4,13]],[[13,4,5,9]],[[14,7,3,12]],[[15,8,7,14]],[[16,6,8,15]],[[11,1,6,16]],[[11,16,15,14,12,13,9,10]]]],"semantics":{"surfaces":[{"type":"GroundSurface"},{"type":"RoofSurface"},{"on_footprint_edge":true,"type":"WallSurface"},{"on_footprint_edge":false,"type":"WallSurface"}],"values":[[0,2,2,2,2,2,2,2,2,1]]}},{"type":"Solid","lod":"1.3","boundaries":[[[[3,7,8,6,1,17,0,5,4,18]],[[19,5,0,20]],[[21,22,17,1,23]],[[24,7,3,25]],[[26,8,7,24]],[[20,0,17,43]],[[44,45,43,46]],[[47,4,5,36]],[[48,18,4,47]],[[39,1,6,49]],[[41,3,18,48,50]],[[46,43,17,35,38]],[[49,6,8,42]],[[51,52,45,44]],[[53,54,55]],[[54,53,56]],[[50,48,52,51]],[[53,55,38,39,49,42]],[[54,56,44,46,38,55]],[[50,51,44,56,53,42,40,41]],[[52,48,47,36,37,43,45]]]],"semantics":{"surfaces":[{"type":"GroundSurface"},{"type":"RoofSurface"},{"on_footprint_edge":true,"type":"WallSurface"},{"on_footprint_edge":false,"type":"WallSurface"}],"values":[[0,2,2,2,2,2,3,2,2,2,2,2,3,3,1,1]]}},{"type":"Solid","lod":"2.2","boundaries":[[[[1,35,17,0,5,4,18,3,7,8,6]],[[36,5,0,37]],[[38,35,1,39]],[[40,7,3,41]],[[42,8,7,40]],[[37,0,17,43]],[[44,45,43,46]],[[47,4,5,36]],[[48,18,4,47]],[[39,1,6,49]],[[41,3,18,48,50]],[[46,43,17,35,38]],[[49,6,8,42]],[[51,52,45,44]],[[53,54,55]],[[54,53,56]],[[50,48,52,51]],[[53,55,38,39,49,42]],[[54,56,44,46,38,55]],[[50,51,44,56,53,42,40,41]],[[52,48,47,36,37,43,45]]]],"semantics":{"surfaces":[{"type":"GroundSurface"},{"type":"RoofSurface"},{"on_footprint_edge":true,"type":"WallSurface"},{"on_footprint_edge":false,"type":"WallSurface"}],"values":[[0,2,2,2,2,2,3,2,2,2,2,2,2,3,3,3,3,1,1,1,1]]}}],"parents":["NL.IMBAG.Pand.0503100000005156"]},"NL.IMBAG.Pand.0503100000005156":{"type":"Building","geographicalExtent":[84734.8046875,446636.5625,0.6919999718666077,84746.9453125,446651.0625,11.119057655334473],"attributes":{"b3_bag_bag_overlap":0.0,"b3_bouwlagen":3,"b3_dak_type":"slanted","b3_h_dak_50p":8.609999656677246,"b3_h_dak_70p":9.239999771118164,"b3_h_dak_max":10.970000267028809,"b3_h_dak_min":3.890000104904175,"b3_h_maaiveld":0.6919999718666077,"b3_kas_warenhuis":false,"b3_mutatie_ahn3_ahn4":false,"b3_nodata_fractie_ahn3":0.002518891589716077,"b3_nodata_fractie_ahn4":0.0,"b3_nodata_radius_ahn3":0.359510600566864,"b3_nodata_radius_ahn4":0.34349295496940613,"b3_opp_buitenmuur":165.03,"b3_opp_dak_plat":51.38,"b3_opp_dak_schuin":63.5,"b3_opp_grond":99.21,"b3_opp_scheidingsmuur":129.53,"b3_puntdichtheid_ahn3":16.353534698486328,"b3_puntdichtheid_ahn4":46.19647216796875,"b3_pw_bron":"AHN4","b3_pw_datum":2020,"b3_pw_selectie_reden":"PREFERRED_AND_LATEST","b3_reconstructie_onvolledig":false,"b3_rmse_lod12":3.2317864894866943,"b3_rmse_lod13":0.642620861530304,"b3_rmse_lod22":0.09925124794244766,"b3_val3dity_lod12":"[]","b3_val3dity_lod13":"[]","b3_val3dity_lod22":"[]","b3_volume_lod12":845.0095825195312,"b3_volume_lod13":657.8263549804688,"b3_volume_lod22":636.9927368164062,"begingeldigheid":"1999-04-28","documentdatum":"1999-04-28","documentnummer":"408040.tif","eindgeldigheid":null,"eindregistratie":null,"geconstateerd":false,"identificatie":"NL.IMBAG.Pand.0503100000005156","oorspronkelijkbouwjaar":2000,"status":"Pand in gebruik","tijdstipeindregistratielv":null,"tijdstipinactief":null,"tijdstipinactieflv":null,"tijdstipnietbaglv":null,"tijdstipregistratie":"2010-10-13T12:29:24Z","tijdstipregistratielv":"2010-10-13T12:30:50Z","voorkomenidentificatie":1},"geometry":[{"type":"MultiSurface","lod":"0","boundaries":[[[0,1,2,3,4,5]]]}],"children":["NL.IMBAG.Pand.0503100000005156-0"]}},"vertices":[[-353581,253246,-44957],[-348730,242291,-44957],[-343550,244604,-44957],[-344288,246257,-44957],[-341437,247537,-44957],[-345635,256798,-44957],[-343558,244600,-44957],[-343662,244854,-44957],[-343926,244734,-44957],[-345635,256798,-36439],[-353581,253246,-36439],[-348730,242291,-36439],[-344288,246257,-36439],[-341437,247537,-36439],[-343662,244854,-36439],[-343926,244734,-36439],[-343558,244600,-36439],[-352596,251020,-44957],[-344083,246349,-44957],[-345635,256798,-41490],[-353581,253246,-41490],[-352596,251020,-35952],[-352596,251020,-41490],[-348730,242291,-35952],[-343662,244854,-35952],[-344288,246257,-35952],[-343926,244734,-35952],[-347233,253386,-35952],[-347233,253386,-41490],[-341437,247537,-41490],[-344083,246349,-41490],[-343558,244600,-35952],[-344083,246349,-35952],[-347089,253741,-35952],[-347089,253741,-41490],[-350613,246543,-44957],[-345635,256798,-41507],[-353581,253246,-41516],[-350613,246543,-34688],[-348730,242291,-36953],[-343662,244854,-37089],[-344288,246257,-37099],[-343926,244734,-36944],[-352596,251020,-41514],[-347233,253386,-37262],[-347233,253386,-41508],[-352596,251020,-37264],[-341437,247537,-41498],[-344083,246349,-41501],[-343558,244600,-37083],[-344083,246349,-37212],[-347089,253741,-37402],[-347089,253741,-41508],[-349425,246738,-34864],[-349425,246738,-34529],[-349862,246897,-34699],[-349238,248437,-35307]]}"#,
        )?;

        let mut attr_schema = AttributeSchema::new();
        for (_, co) in cj_city_feature.city_objects.iter() {
            if let Some(attr) = &co.attributes {
                attr_schema.add_attributes(attr);
            }
        }

        // Create FlatBuffer and encode
        let mut fbb = FlatBufferBuilder::new();

        let (city_feature, _) =
            to_fcb_city_feature(&mut fbb, "test_id", &cj_city_feature, &attr_schema, None);

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
        assert_eq!(
            cj_city_feature.vertices[0][0],
            fb_city_feature.vertices().unwrap().get(0).x() as i64,
        );
        assert_eq!(
            cj_city_feature.vertices[0][1],
            fb_city_feature.vertices().unwrap().get(0).y() as i64,
        );
        assert_eq!(
            cj_city_feature.vertices[0][2],
            fb_city_feature.vertices().unwrap().get(0).z() as i64,
        );

        assert_eq!(
            cj_city_feature.vertices[1][0],
            fb_city_feature.vertices().unwrap().get(1).x() as i64,
        );
        assert_eq!(
            cj_city_feature.vertices[1][1],
            fb_city_feature.vertices().unwrap().get(1).y() as i64,
        );
        assert_eq!(
            cj_city_feature.vertices[1][2],
            fb_city_feature.vertices().unwrap().get(1).z() as i64,
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
            assert_eq!(cjco.thetype, to_cj_co_type(fb_city_object.type_(), None));

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

            if let Some(parents) = cjco.parents.as_ref() {
                for parent in fb_city_object.parents().unwrap().iter() {
                    assert!(parents.contains(&parent.to_string()));
                }
            }

            if let Some(children) = cjco.children.as_ref() {
                for child in fb_city_object.children().unwrap().iter() {
                    assert!(children.contains(&child.to_string()));
                }
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
