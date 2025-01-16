use crate::attribute::{encode_attributes_with_schema, AttributeSchema, AttributeSchemaMethods};
use crate::feature_generated::{
    CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, CityObjectType, Geometry,
    GeometryArgs, GeometryType, SemanticObject, SemanticObjectArgs, SemanticSurfaceType, Vertex,
};
use crate::geom_encoder::encode;
use crate::header_generated::{
    GeographicalExtent, Header, HeaderArgs, ReferenceSystem, ReferenceSystemArgs, Transform, Vector,
};
use crate::{Column, ColumnArgs, NodeItem};

use cjseq::{
    CityJSON, CityJSONFeature, CityObject as CjCityObject, Geometry as CjGeometry,
    GeometryType as CjGeometryType, PointOfContact as CjPointOfContact,
    ReferenceSystem as CjReferenceSystem, Transform as CjTransform,
};
use flatbuffers::FlatBufferBuilder;
use serde_json::Value;

use super::geom_encoder::{GMBoundaries, GMSemantics};
use super::header_writer::HeaderWriterOptions;

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
pub fn to_fcb_header<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    cj: &CityJSON,
    header_options: HeaderWriterOptions,
    attr_schema: &AttributeSchema,
) -> flatbuffers::WIPOffset<Header<'a>> {
    let version = Some(fbb.create_string(&cj.version));
    let transform = to_transform(&cj.transform);
    let features_count: u64 = header_options.feature_count;
    let columns = Some(to_columns(fbb, attr_schema));
    let index_node_size = header_options.index_node_size;

    if let Some(meta) = cj.metadata.as_ref() {
        let reference_system = meta
            .reference_system
            .as_ref()
            .map(|ref_sys| to_reference_system(fbb, ref_sys));
        let geographical_extent = meta
            .geographical_extent
            .as_ref()
            .map(to_geographical_extent);
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
        Header::create(
            fbb,
            &HeaderArgs {
                transform: Some(transform).as_ref(),
                columns,
                features_count,
                index_node_size,
                geographical_extent: geographical_extent.as_ref(),
                reference_system,
                identifier,
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
            },
        )
    } else {
        Header::create(
            fbb,
            &HeaderArgs {
                transform: Some(transform).as_ref(),
                columns,
                features_count,
                index_node_size,
                version,
                ..Default::default()
            },
        )
    }
}

/// Converts CityJSON geographical extent to FlatBuffers format
///
/// # Arguments
///
/// * `geographical_extent` - Array of 6 values [minx, miny, minz, maxx, maxy, maxz]
pub(crate) fn to_geographical_extent(geographical_extent: &[f64; 6]) -> GeographicalExtent {
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
pub(crate) fn to_transform(transform: &CjTransform) -> Transform {
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
pub(crate) fn to_reference_system<'a>(
    fbb: &mut FlatBufferBuilder<'a>,
    ref_system: &CjReferenceSystem,
) -> flatbuffers::WIPOffset<ReferenceSystem<'a>> {
    let authority = Some(fbb.create_string(&ref_system.authority));

    let version = ref_system.version.parse::<i32>().unwrap_or_else(|e| {
        println!("failed to parse version: {}", e);
        0
    });
    let code = ref_system.code.parse::<i32>().unwrap_or_else(|e| {
        println!("failed to parse code: {}", e);
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
pub fn to_fcb_city_feature<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &str,
    city_feature: &CityJSONFeature,
    attr_schema: &AttributeSchema,
) -> (flatbuffers::WIPOffset<CityFeature<'a>>, NodeItem) {
    let id = Some(fbb.create_string(id));
    let city_objects: Vec<_> = city_feature
        .city_objects
        .iter()
        .map(|(id, co)| to_city_object(fbb, id, co, attr_schema))
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
    let min_x = city_feature.vertices.iter().map(|v| v[0]).min().unwrap();
    let min_y = city_feature.vertices.iter().map(|v| v[1]).min().unwrap();
    let max_x = city_feature.vertices.iter().map(|v| v[0]).max().unwrap();
    let max_y = city_feature.vertices.iter().map(|v| v[1]).max().unwrap();
    let bbox = NodeItem::new(min_x, min_y, max_x, max_y);
    (
        CityFeature::create(
            fbb,
            &CityFeatureArgs {
                id,
                objects,
                vertices,
            },
        ),
        bbox,
    )
}

/// Converts CityJSON city object to FlatBuffers format
///
/// # Arguments
///
/// * `fbb` - FlatBuffers builder instance
/// * `id` - Object identifier
/// * `co` - CityJSON city object
pub(crate) fn to_city_object<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    id: &str,
    co: &CjCityObject,
    attr_schema: &AttributeSchema,
) -> flatbuffers::WIPOffset<CityObject<'a>> {
    let id = Some(fbb.create_string(id));

    let type_ = to_co_type(&co.thetype);
    let geographical_extent = co.geographical_extent.as_ref().map(to_geographical_extent);
    let geometries = {
        let geometries = co
            .geometry
            .as_ref()
            .map(|gs| gs.iter().map(|g| to_geometry(fbb, g)).collect::<Vec<_>>());
        geometries.map(|geometries| fbb.create_vector(&geometries))
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
            geographical_extent: geographical_extent.as_ref(),
            geometry: geometries,
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
pub(crate) fn to_co_type(co_type: &str) -> CityObjectType {
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

/// Converts CityJSON geometry type to FlatBuffers enum
///
/// # Arguments
///
/// * `geometry_type` - CityJSON geometry type
pub(crate) fn to_geom_type(geometry_type: &CjGeometryType) -> GeometryType {
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
pub(crate) fn to_semantic_surface_type(ss_type: &str) -> SemanticSurfaceType {
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

/// Converts CityJSON geometry to FlatBuffers format
///
/// # Arguments
///
/// * `fbb` - FlatBuffers builder instance
/// * `geometry` - CityJSON geometry object
pub fn to_geometry<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    geometry: &CjGeometry,
) -> flatbuffers::WIPOffset<Geometry<'a>> {
    let type_ = to_geom_type(&geometry.thetype);
    let lod = geometry.lod.as_ref().map(|lod| fbb.create_string(lod));

    let encoded = encode(&geometry.boundaries, geometry.semantics.as_ref());
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
                    SemanticObject::create(
                        fbb,
                        &SemanticObjectArgs {
                            type_: to_semantic_surface_type(&s.thetype),
                            attributes: None,
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

pub fn to_columns<'a>(
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

pub fn to_fcb_attribute<'a>(
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

        let (city_feature, feat_node) =
            to_fcb_city_feature(&mut fbb, "test_id", &cj_city_feature, &attr_schema);

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
