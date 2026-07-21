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
    Appearance as CjAppearance, BorderColor as CjBorderColor, CityJSON, CityJSONFeature,
    CityObject as CjCityObject, CityObjectType as CjCityObjectType, Geometry as CjGeometry,
    GeometryType as CjGeometryType, PointOfContact as CjPointOfContact,
    ReferenceSystem as CjReferenceSystem, SemanticSurfaceType as CjSemanticSurfaceType,
    TextureFormat as CjTextureFormat, TextureType as CjTextureType, Transform as CjTransform,
    WrapMode as CjWrapMode,
};

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

    // Handle extensions, if present. `extensions` is the CityJSON member
    // itself: a map of extension name to `{url, version}`. Only that reference
    // is written; the schema document it points at is not fetched, so writing
    // a file never makes a network request.
    let extensions = cj
        .extensions
        .as_ref()
        .and_then(|e| e.as_object())
        .map(|extensions| {
            let extensions = extensions
                .iter()
                .map(|(name, ext)| {
                    let url = ext.get("url").and_then(|v| v.as_str()).unwrap_or_default();
                    let version = ext
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    to_extension(fbb, name, url, version)
                })
                .collect::<Vec<_>>();
            fbb.create_vector(&extensions)
        });

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
    // A `referenceSystem` need not have the three-element OGC shape at all —
    // the schema constrains only its prefix — so each accessor is fallible.
    let authority = Some(fbb.create_string(ref_system.authority().unwrap_or_default()));

    let version = ref_system
        .version()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    let code = ref_system
        .code()
        .and_then(|c| c.parse::<i32>().ok())
        .unwrap_or(0);

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
    // `metadata.schema.json` types `address` as a bare object with no named
    // members ("any properties can be used, to accommodate the different ways
    // addresses are structured in different countries", § 5.3), so each of the
    // header's fixed address fields is looked up by name and may be absent.
    // A number spelled as a JSON number rather than a string is kept verbatim.
    let address_member = |key: &str| -> Option<String> {
        let members = &poc.address.as_ref()?.members;
        let value = members.get(key)?;
        match value {
            Value::String(s) => Some(s.clone()),
            Value::Null => None,
            other => Some(other.to_string()),
        }
    };
    // The spec's own examples spell the postcode `postcode`; the schema's
    // CityJSON 1.x examples used `postalCode`. Accept either.
    let address_either = |a: &str, b: &str| address_member(a).or_else(|| address_member(b));

    let poc_address_thoroughfare_number =
        address_member("thoroughfareNumber").map(|v| fbb.create_string(&v));
    let poc_address_thoroughfare_name =
        address_member("thoroughfareName").map(|v| fbb.create_string(&v));
    let poc_address_locality = address_member("locality").map(|v| fbb.create_string(&v));
    let poc_address_postcode =
        address_either("postcode", "postalCode").map(|v| fbb.create_string(&v));
    let poc_address_country = address_member("country").map(|v| fbb.create_string(&v));
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

/// Writes one entry of the CityJSON `extensions` member into the header.
///
/// Only the reference is written — the name, the URL of the extension schema
/// and the version, which is all `extensions` contains. The schema document
/// itself is not fetched: doing so made writing a file depend on the network
/// (and on the extension host still being up), and nothing in this crate reads
/// the embedded schema back.
pub fn to_extension<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    name: &str,
    url: &str,
    version: &str,
) -> flatbuffers::WIPOffset<Extension<'a>> {
    let name = fbb.create_string(name);
    let url = fbb.create_string(url);
    let version = fbb.create_string(version);

    Extension::create(
        fbb,
        &ExtensionArgs {
            name: Some(name),
            url: Some(url),
            version: Some(version),
            ..Default::default()
        },
    )
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

/// The FlatCityBuf tag for a CityJSON `wrapMode`.
///
/// Exhaustive by construction: there is no `_` arm, so a spelling added to
/// `cjseq::WrapMode` -- which mirrors `appearance.schema.json`'s five --
/// stops this crate from compiling until it is mapped here. This replaces a
/// table lookup keyed on a *string*, whose fallback turned a mis-spelling into
/// a valid-looking default: that is how a texture written
/// `"wrapMode": "wrap"` came back as `"None"` and survived until the C++
/// conformance corpus caught it.
pub(crate) fn fb_wrap_mode(w: CjWrapMode) -> WrapMode {
    match w {
        CjWrapMode::None => WrapMode::None,
        CjWrapMode::Wrap => WrapMode::Wrap,
        CjWrapMode::Mirror => WrapMode::Mirror,
        CjWrapMode::Clamp => WrapMode::Clamp,
        CjWrapMode::Border => WrapMode::Border,
    }
}

/// The FlatCityBuf tag for a CityJSON `textureType`. Exhaustive, as above.
pub(crate) fn fb_texture_type(t: CjTextureType) -> TextureType {
    match t {
        CjTextureType::Unknown => TextureType::Unknown,
        CjTextureType::Specific => TextureType::Specific,
        CjTextureType::Typical => TextureType::Typical,
    }
}

/// The FlatCityBuf tag for a CityJSON texture `type`. Exhaustive, as above.
///
/// `type` is optional in `appearance.schema.json` (the `Texture` object has no
/// `required` keyword at all) but mandatory in `header.fbs`, whose default is
/// `PNG`. An absent `type` therefore comes back as `"PNG"`; that lossiness
/// predates this change and is fixed only by changing the schema, which would
/// break the C++ reader.
pub(crate) fn fb_texture_format(f: Option<CjTextureFormat>) -> TextureFormat {
    match f {
        Some(CjTextureFormat::PNG) | None => TextureFormat::PNG,
        Some(CjTextureFormat::JPG) => TextureFormat::JPG,
    }
}

pub(super) fn to_appearance<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    appearance: &CjAppearance,
) -> flatbuffers::WIPOffset<Appearance<'a>> {
    // Handle appearance if present

    // `appearance.materials` and `appearance.textures` are typed by cjseq
    // straight from `appearance.schema.json`, so every member here is read off
    // a field rather than looked up by name in a `serde_json::Value`.
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
                // `image` is optional in CityJSON but mandatory in the `.fbs`,
                // so an absent one is written as the empty string.
                let image = fbb.create_string(t.image.as_deref().unwrap_or_default());
                let border_color = t.border_color.as_ref().map(|c| match c {
                    CjBorderColor::Rgb(c) => fbb.create_vector(c),
                    CjBorderColor::Rgba(c) => fbb.create_vector(c),
                });
                Texture::create(
                    fbb,
                    &TextureArgs {
                        type_: fb_texture_format(t.thetype),
                        image: Some(image),
                        wrap_mode: t.wrap_mode.map(fb_wrap_mode),
                        texture_type: t.texture_type.map(fb_texture_type),
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
            .filter(|g| g.geometry_type() != CjGeometryType::GeometryInstance)
            .collect::<Vec<_>>()
    });
    let geometry_instances = co.geometry.as_ref().map(|gs| {
        gs.iter()
            .filter(|g| g.geometry_type() == CjGeometryType::GeometryInstance)
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
        // An unspecified role is `null` in CityJSON; the header has no way to
        // spell that, so it is written as the empty string.
        let children_roles_strings = co.children_roles.as_ref().map(|c| {
            c.iter()
                .map(|r| fbb.create_string(r.as_deref().unwrap_or_default()))
                .collect::<Vec<_>>()
        });
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

/// Converts a CityJSON City Object type to the FlatBuffers enum.
///
/// An Extension type has no tag of its own: it becomes `ExtensionObject` plus
/// the CityJSON name verbatim (leading `+` included) in `extension_type`.
///
/// The known names are associated `const`s on `CityObjectType`, not enum
/// variants, so this matches on `*co_type` rather than on the reference — a
/// `const` pattern against a `&CityObjectType` is `E0308`, and rustc's fix-it
/// suggestion silently turns the arm into a catch-all binding.
///
/// There is deliberately no `_` arm: listing every known const plus `Extension`
/// *is* exhaustive, so a name added to `KnownCityObjectType` upstream fails to
/// compile here rather than silently becoming a `GenericCityObject`.
pub(super) fn to_co_type(co_type: &CjCityObjectType) -> (CityObjectType, Option<String>) {
    match *co_type {
        CjCityObjectType::Bridge => (CityObjectType::Bridge, None),
        CjCityObjectType::BridgePart => (CityObjectType::BridgePart, None),
        CjCityObjectType::BridgeInstallation => (CityObjectType::BridgeInstallation, None),
        CjCityObjectType::BridgeConstructiveElement => {
            (CityObjectType::BridgeConstructiveElement, None)
        }
        CjCityObjectType::BridgeRoom => (CityObjectType::BridgeRoom, None),
        CjCityObjectType::BridgeFurniture => (CityObjectType::BridgeFurniture, None),
        CjCityObjectType::Building => (CityObjectType::Building, None),
        CjCityObjectType::BuildingPart => (CityObjectType::BuildingPart, None),
        CjCityObjectType::BuildingInstallation => (CityObjectType::BuildingInstallation, None),
        CjCityObjectType::BuildingConstructiveElement => {
            (CityObjectType::BuildingConstructiveElement, None)
        }
        CjCityObjectType::BuildingFurniture => (CityObjectType::BuildingFurniture, None),
        CjCityObjectType::BuildingStorey => (CityObjectType::BuildingStorey, None),
        CjCityObjectType::BuildingRoom => (CityObjectType::BuildingRoom, None),
        CjCityObjectType::BuildingUnit => (CityObjectType::BuildingUnit, None),
        CjCityObjectType::CityFurniture => (CityObjectType::CityFurniture, None),
        CjCityObjectType::CityObjectGroup => (CityObjectType::CityObjectGroup, None),
        CjCityObjectType::GenericCityObject => (CityObjectType::GenericCityObject, None),
        CjCityObjectType::LandUse => (CityObjectType::LandUse, None),
        CjCityObjectType::OtherConstruction => (CityObjectType::OtherConstruction, None),
        CjCityObjectType::PlantCover => (CityObjectType::PlantCover, None),
        CjCityObjectType::SolitaryVegetationObject => {
            (CityObjectType::SolitaryVegetationObject, None)
        }
        CjCityObjectType::TINRelief => (CityObjectType::TINRelief, None),
        CjCityObjectType::Road => (CityObjectType::Road, None),
        CjCityObjectType::Railway => (CityObjectType::Railway, None),
        CjCityObjectType::Waterway => (CityObjectType::Waterway, None),
        CjCityObjectType::TransportSquare => (CityObjectType::TransportSquare, None),
        CjCityObjectType::Tunnel => (CityObjectType::Tunnel, None),
        CjCityObjectType::TunnelPart => (CityObjectType::TunnelPart, None),
        CjCityObjectType::TunnelInstallation => (CityObjectType::TunnelInstallation, None),
        CjCityObjectType::TunnelConstructiveElement => {
            (CityObjectType::TunnelConstructiveElement, None)
        }
        CjCityObjectType::TunnelHollowSpace => (CityObjectType::TunnelHollowSpace, None),
        CjCityObjectType::TunnelFurniture => (CityObjectType::TunnelFurniture, None),
        CjCityObjectType::WaterBody => (CityObjectType::WaterBody, None),
        CjCityObjectType::Extension(ref name) => {
            (CityObjectType::ExtensionObject, Some(name.clone()))
        }
    }
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
        CjGeometryType::GeometryInstance => GeometryType::GeometryInstance,
    }
}

/// A semantic surface type as FlatCityBuf stores it: the enum tag, plus the
/// CityJSON name verbatim when the tag is `ExtraSemanticSurface`, which has no
/// spelling of its own.
pub(super) struct FcbSemanticSurfaceType {
    pub(super) type_: SemanticSurfaceType,
    pub(super) extension_type: Option<String>,
}

impl FcbSemanticSurfaceType {
    fn known(type_: SemanticSurfaceType) -> Self {
        FcbSemanticSurfaceType {
            type_,
            extension_type: None,
        }
    }
}

/// Converts a CityJSON semantic surface type to the FlatBuffers enum.
///
/// As with [`to_co_type`], the match is on the value rather than the reference:
/// the known names are associated `const`s, and a `const` pattern against a
/// reference is `E0308`. Listing every one of them is what makes this
/// exhaustive — adding a name to `KnownSemanticSurfaceType` upstream will fail
/// to compile here rather than silently decay to `ExtraSemanticSurface`.
impl From<&CjSemanticSurfaceType> for FcbSemanticSurfaceType {
    fn from(ss_type: &CjSemanticSurfaceType) -> Self {
        match *ss_type {
            CjSemanticSurfaceType::RoofSurface => Self::known(SemanticSurfaceType::RoofSurface),
            CjSemanticSurfaceType::GroundSurface => Self::known(SemanticSurfaceType::GroundSurface),
            CjSemanticSurfaceType::WallSurface => Self::known(SemanticSurfaceType::WallSurface),
            CjSemanticSurfaceType::ClosureSurface => {
                Self::known(SemanticSurfaceType::ClosureSurface)
            }
            CjSemanticSurfaceType::OuterCeilingSurface => {
                Self::known(SemanticSurfaceType::OuterCeilingSurface)
            }
            CjSemanticSurfaceType::OuterFloorSurface => {
                Self::known(SemanticSurfaceType::OuterFloorSurface)
            }
            CjSemanticSurfaceType::Window => Self::known(SemanticSurfaceType::Window),
            CjSemanticSurfaceType::Door => Self::known(SemanticSurfaceType::Door),
            CjSemanticSurfaceType::InteriorWallSurface => {
                Self::known(SemanticSurfaceType::InteriorWallSurface)
            }
            CjSemanticSurfaceType::CeilingSurface => {
                Self::known(SemanticSurfaceType::CeilingSurface)
            }
            CjSemanticSurfaceType::FloorSurface => Self::known(SemanticSurfaceType::FloorSurface),
            CjSemanticSurfaceType::WaterSurface => Self::known(SemanticSurfaceType::WaterSurface),
            CjSemanticSurfaceType::WaterGroundSurface => {
                Self::known(SemanticSurfaceType::WaterGroundSurface)
            }
            CjSemanticSurfaceType::WaterClosureSurface => {
                Self::known(SemanticSurfaceType::WaterClosureSurface)
            }
            CjSemanticSurfaceType::TrafficArea => Self::known(SemanticSurfaceType::TrafficArea),
            CjSemanticSurfaceType::AuxiliaryTrafficArea => {
                Self::known(SemanticSurfaceType::AuxiliaryTrafficArea)
            }
            CjSemanticSurfaceType::TransportationMarking => {
                Self::known(SemanticSurfaceType::TransportationMarking)
            }
            CjSemanticSurfaceType::TransportationHole => {
                Self::known(SemanticSurfaceType::TransportationHole)
            }
            CjSemanticSurfaceType::Extension(ref name) => FcbSemanticSurfaceType {
                type_: SemanticSurfaceType::ExtraSemanticSurface,
                extension_type: Some(name.clone()),
            },
        }
    }
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
    let type_ = to_geom_type(&geometry.geometry_type());
    let lod = geometry.lod().map(|lod| fbb.create_string(lod));

    let encoded = encode(geometry);
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
                    let children = s.children.as_ref().map(|c| {
                        let c = c.iter().map(|&i| i as u32).collect::<Vec<_>>();
                        fbb.create_vector(&c)
                    });

                    let FcbSemanticSurfaceType {
                        type_,
                        extension_type,
                    } = FcbSemanticSurfaceType::from(&s.thetype);
                    let extension_type = extension_type.map(|s| fbb.create_string(&s));
                    let attributes = if s.other.is_empty() {
                        None
                    } else {
                        let other = Value::Object(s.other.clone().into_iter().collect());
                        semantic_attr_schema.as_ref().map(|schema| {
                            fbb.create_vector(&encode_attributes_with_schema(&other, schema))
                        })
                    };
                    SemanticObject::create(
                        fbb,
                        &SemanticObjectArgs {
                            type_,
                            extension_type,
                            attributes,
                            children,
                            parent: s.parent.map(|p| p as u32),
                        },
                    )
                })
                .collect::<Vec<_>>();

            (
                Some(fbb.create_vector(&semantics_objects)),
                // Absent, not empty, for `"values": null` -- which is valid
                // CityJSON (required-but-nullable) and is not the same thing as
                // an empty array.
                values.map(|values| fbb.create_vector(&values)),
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
                    // Present-but-empty, so that `"values": []` stays distinct
                    // from the absent vector written for `"values": null`.
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
                // `"values": null`: a theme, and no arrays at all.
                GMMaterialMapping::NullValues(theme) => {
                    let theme = Some(fbb.create_string(theme));
                    MaterialMapping::create(
                        fbb,
                        &MaterialMappingArgs {
                            theme,
                            ..Default::default()
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
                // A theme with no `values` member carries no arrays at all, so
                // that it stays distinct from one whose `values` is empty.
                let (solids, shells, surfaces, strings, vertices) = if t.has_values {
                    (
                        Some(fbb.create_vector(&t.solids)),
                        Some(fbb.create_vector(&t.shells)),
                        Some(fbb.create_vector(&t.surfaces)),
                        Some(fbb.create_vector(&t.strings)),
                        Some(fbb.create_vector(&t.vertices)),
                    )
                } else {
                    (None, None, None, None, None)
                };
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
    // `template`, `transformationMatrix` and single-index boundaries are part
    // of the `GeometryInstance` variant's type, so there is nothing to check
    // at runtime beyond having been handed the right variant.
    let CjGeometry::GeometryInstance {
        boundaries,
        template,
        transformation_matrix: m,
    } = geometry
    else {
        panic!(
            "to_geometry_instance was given a {:?}",
            geometry.geometry_type()
        );
        //TODO: don't use panic, instead, return Result type
    };

    let template = *template as u32;
    let indices = boundaries.iter().map(|&i| i as u32).collect::<Vec<_>>();
    let boundaries = Some(fbb.create_vector(&indices));
    let transformation = Some(TransformationMatrix::new(
        m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9], m[10], m[11], m[12], m[13],
        m[14], m[15],
    ));
    GeometryInstance::create(
        fbb,
        &GeometryInstanceArgs {
            template,
            transformation: transformation.as_ref(),
            boundaries,
        },
    )
}

/// `geometry-templates.vertices-templates` is an untyped `Value` in CityJSON;
/// anything that is not a 3-element array of numbers is not a vertex and is
/// skipped rather than guessed at.
pub(super) fn to_templates_vertices<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    vertices: &Value,
) -> flatbuffers::WIPOffset<flatbuffers::Vector<'a, DoubleVertex>> {
    let vertices_vec = vertices
        .as_array()
        .map(|vs| {
            vs.iter()
                .filter_map(|v| {
                    let v = v.as_array()?;
                    let coords: Vec<f64> = v.iter().filter_map(|c| c.as_f64()).collect();
                    let [x, y, z]: [f64; 3] = coords.try_into().ok()?;
                    Some(DoubleVertex::new(x, y, z))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
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
                    .find(|g| g.lod() == fb_geometry.lod())
                    .unwrap();
                assert_eq!(
                    cj_geometry.geometry_type(),
                    fb_geometry
                        .type_()
                        .to_cj()
                        .expect("a written geometry has a known type")
                );
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

    /// `metadata.schema.json` types `pointOfContact.address` as a bare object
    /// with no named members, so BOTH `postcode` (the spec's own examples) and
    /// `postalCode` (CityJSON 1.x's) are schema-valid spellings and the writer
    /// accepts either. The header has one fixed slot for them, so the reader
    /// has to pick one spelling to write back, and it picks `postcode`.
    ///
    /// That means a document spelling it `postalCode` round-trips RENAMED --
    /// a deliberate normalisation, not a loss, and the same shape of choice as
    /// the texture `image` empty-vs-absent one. Pinned here so that changing
    /// either half of it is a test failure rather than a silent behaviour
    /// change for anyone diffing input against output.
    #[test]
    fn either_postcode_spelling_is_accepted_and_both_come_back_as_postcode() -> Result<()> {
        let address_after_round_trip = |address: serde_json::Value| -> Result<cjseq::Address> {
            let cj: CityJSON = serde_json::from_value(serde_json::json!({
                "type": "CityJSON",
                "version": "2.0",
                "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]},
                "CityObjects": {},
                "vertices": [],
                "metadata": {
                    "pointOfContact": {
                        "contactName": "A Person",
                        "emailAddress": "a@example.org",
                        "address": address
                    }
                }
            }))?;

            let mut fbb = FlatBufferBuilder::new();
            let header = to_fcb_header(
                &mut fbb,
                &cj,
                HeaderWriterOptions {
                    write_index: false,
                    feature_count: 0,
                    index_node_size: 16,
                    attribute_indices: None,
                    geographical_extent: None,
                },
                &AttributeSchema::new(),
                None,
                None,
            )?;
            fbb.finish(header, None);
            let buf = fbb.finished_data().to_vec();
            let header = flatbuffers::root::<crate::fb::Header>(&buf).unwrap();
            Ok(crate::reader::deserializer::to_cj_address(&header)
                .expect("the address has a postcode, so it is not empty"))
        };

        for spelling in ["postcode", "postalCode"] {
            let address = address_after_round_trip(serde_json::json!({
                "locality": "Delft",
                spelling: "2628 CN"
            }))?;
            assert_eq!(
                address.members.get("postcode"),
                Some(&serde_json::json!("2628 CN")),
                "`{spelling}` must be accepted and written back as `postcode`"
            );
            assert_eq!(
                address.members.get("postalCode"),
                None,
                "the header has one postcode slot; `postalCode` is not also emitted"
            );
            //-- the neighbouring member is untouched by the normalisation
            assert_eq!(
                address.members.get("locality"),
                Some(&serde_json::json!("Delft"))
            );
        }

        //-- and `postcode` wins when a document carries both, matching the
        //-- `.or_else()` order in `address_either`
        let address = address_after_round_trip(serde_json::json!({
            "postcode": "2628 CN",
            "postalCode": "1234 AB"
        }))?;
        assert_eq!(
            address.members.get("postcode"),
            Some(&serde_json::json!("2628 CN"))
        );

        Ok(())
    }
}
