use axum::{
    extract::{Path, Query as AxumQuery, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use chrono::Utc;
use cjseq::CityJSONFeature;
use fcb_core::packed_rtree::Query;
use fcb_core::{deserializer, FixedStringKey, HttpFcbReader, KeyType, Operator};
use http_range_client::AsyncHttpRangeClient;
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

use crate::constants::*;
use crate::crs::{transform_bbox, DUTCH_CRS};
use crate::filter_parser::{parse_filter, ParseError};
use crate::models::*;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CollectionQuery {
    pub f: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BboxQuery {
    limit: Option<i32>,
    offset: Option<i32>,
    bbox: Option<String>,
    crs: Option<String>,
    #[serde(rename = "bbox-crs")]
    bbox_crs: Option<String>,
    filter: Option<String>,
    f: Option<String>,
}

/// Determine output format from query parameter or Accept header
/// Priority: query parameter 'f' > Accept header > default 'json'
fn determine_format<'a>(query_format: &'a Option<String>, headers: &HeaderMap) -> &'a str {
    // First priority: query parameter
    if let Some(f) = query_format {
        return f.as_str();
    }

    // Second priority: Accept header
    if let Some(accept) = headers.get(header::ACCEPT) {
        if let Ok(accept_str) = accept.to_str() {
            // Parse Accept header and match against supported formats
            for media_type in accept_str.split(',') {
                let media_type = media_type.split(';').next().unwrap_or("").trim();
                match media_type {
                    "application/city+json-seq" => return "cjseq",
                    "application/city+json" => return "cityjson",
                    "text/plain" | "model/obj" => return "obj",
                    "application/json" => return "json",
                    _ => continue,
                }
            }
        }
    }

    // Default format
    "json"
}

pub async fn landing_page(
    State(state): State<Arc<AppState>>,
) -> Result<Json<LandingPage>, StatusCode> {
    info!("Serving landing page");

    let landing_page = LandingPage {
        title: Some(API_TITLE.to_string()),
        description: Some(API_DESCRIPTION.to_string()),
        links: vec![
            Link {
                href: format!("{}/", state.base_url),
                rel: REL_SELF.to_string(),
                r#type: Some(CONTENT_TYPE_JSON.to_string()),
                title: Some(TITLE_THIS_DOCUMENT.to_string()),
                ..Default::default()
            },
            Link {
                href: format!("{}/api", state.base_url),
                rel: REL_SERVICE_DESC.to_string(),
                r#type: Some(CONTENT_TYPE_OPENAPI.to_string()),
                title: Some(TITLE_API_DEFINITION.to_string()),
                ..Default::default()
            },
            Link {
                href: format!("{}/api.html", state.base_url),
                rel: REL_SERVICE_DOC.to_string(),
                r#type: Some(CONTENT_TYPE_HTML.to_string()),
                title: Some(TITLE_API_DOCUMENTATION.to_string()),
                ..Default::default()
            },
            Link {
                href: format!("{}/conformance", state.base_url),
                rel: REL_CONFORMANCE.to_string(),
                r#type: Some(CONTENT_TYPE_JSON.to_string()),
                title: Some(TITLE_CONFORMANCE.to_string()),
                ..Default::default()
            },
            Link {
                href: format!("{}/collections", state.base_url),
                rel: REL_DATA.to_string(),
                r#type: Some(CONTENT_TYPE_JSON.to_string()),
                title: Some(TITLE_COLLECTIONS.to_string()),
                ..Default::default()
            },
        ],
    };

    Ok(Json(landing_page))
}

pub async fn conformance() -> Result<Json<ConfClasses>, StatusCode> {
    info!("Serving conformance declaration");

    let conformance = ConfClasses {
        conforms_to: vec![CITYJSON_SPEC.to_string()],
    };

    Ok(Json(conformance))
}

pub async fn collections(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Serving collections");

    let response = serde_json::json!({
        "collections": [
            {
                "crs": [
                    STORAGE_CRS
                ],
                "description": PAND_COLLECTION_DESCRIPTION,
                "extent": {
                    "spatial": {
                        "bbox": [
                            [DEFAULT_BBOX[0] as i64, DEFAULT_BBOX[1] as i64, DEFAULT_BBOX[2] as i64, DEFAULT_BBOX[3] as i64]
                        ],
                        "crs": STORAGE_CRS
                    }
                },
                "id": PAND_COLLECTION_ID,
                "itemType": ITEM_TYPE_FEATURE,
                "links": [
                    {
                        "href": format!("{}/collections/{}", state.base_url, PAND_COLLECTION_ID),
                        "rel": REL_SELF,
                        "title": TITLE_THIS_DOCUMENT,
                        "type": CONTENT_TYPE_JSON
                    },
                    {
                        "href": format!("{}/collections/{}/items", state.base_url, PAND_COLLECTION_ID),
                        "rel": REL_ITEMS,
                        "title": TITLE_PAND_ITEMS,
                        "type": CONTENT_TYPE_GEOJSON
                    },
                    {
                        "href": LICENSE_URL,
                        "rel": REL_LICENSE,
                        "title": LICENSE_TITLE,
                        "type": CONTENT_TYPE_HTML
                    },
                    {
                        "href": LICENSE_RDF_URL,
                        "rel": REL_LICENSE,
                        "title": LICENSE_TITLE,
                        "type": CONTENT_TYPE_RDF_XML
                    }
                ],
                "storageCrs": STORAGE_CRS,
                "title": PAND_COLLECTION_TITLE,
                "version": {
                    "api": API_VERSION,
                    "collection": COLLECTION_VERSION
                }
            }
        ],
        "crs": [
            STORAGE_CRS
        ],
        "links": [
            {
                "href": format!("{}/collections", state.base_url),
                "rel": REL_SELF,
                "title": TITLE_THIS_DOCUMENT,
                "type": CONTENT_TYPE_JSON
            }
        ]
    });

    Ok(Json(response))
}

pub async fn collection_by_id(
    Path(collection_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Serving collection: {}", collection_id);

    if collection_id != PAND_COLLECTION_ID {
        return Err(StatusCode::NOT_FOUND);
    }

    let collection = serde_json::json!({
        "crs": [STORAGE_CRS],
        "description": PAND_COLLECTION_DESCRIPTION,
        "extent": {
            "spatial": {
                "bbox": [DEFAULT_BBOX],
                "crs": STORAGE_CRS
            }
        },
        "id": PAND_COLLECTION_ID,
        "itemType": ITEM_TYPE_FEATURE,
        "links": [
            {
                "href": format!("{}/collections/{}", state.base_url, PAND_COLLECTION_ID),
                "rel": REL_SELF,
                "title": TITLE_THIS_DOCUMENT,
                "type": CONTENT_TYPE_JSON
            },
            {
                "href": format!("{}/collections/{}/items", state.base_url, PAND_COLLECTION_ID),
                "rel": REL_ITEMS,
                "title": TITLE_PAND_ITEMS,
                "type": CONTENT_TYPE_GEOJSON
            },
            {
                "href": LICENSE_URL,
                "rel": REL_LICENSE,
                "title": LICENSE_TITLE,
                "type": CONTENT_TYPE_HTML
            },
            {
                "href": LICENSE_RDF_URL,
                "rel": REL_LICENSE,
                "title": LICENSE_TITLE,
                "type": CONTENT_TYPE_RDF_XML
            }
        ],
        "storageCrs": STORAGE_CRS,
        "title": PAND_COLLECTION_TITLE,
        "version": {
            "api": API_VERSION,
            "collection": COLLECTION_VERSION
        }
    });

    Ok(Json(collection))
}

pub async fn collection_items(
    Path(collection_id): Path<String>,
    headers: HeaderMap,
    AxumQuery(query): AxumQuery<BboxQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, StatusCode> {
    info!(
        "Serving items for collection: {} with query: {:?}",
        collection_id, query
    );

    if collection_id != PAND_COLLECTION_ID {
        return Err(StatusCode::NOT_FOUND);
    }

    // Determine format from query parameter or Accept header
    let format = determine_format(&query.f, &headers);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_LIMIT)
        .min(state.max_return_features as i32);
    let offset = query.offset.unwrap_or(0).max(0);

    // Parse and transform bbox if provided
    let bbox = if let Some(bbox_str) = &query.bbox {
        let parts: Result<Vec<f64>, _> = bbox_str
            .split(',')
            .map(|s| s.trim().parse::<f64>())
            .collect();

        let parsed_bbox = match parts {
            Ok(coords) => {
                if coords.len() == 4 {
                    Ok(coords)
                } else if coords.len() == 6 {
                    // Extract 2D bbox from 3D bbox (ignore z coordinates)
                    Ok(vec![coords[0], coords[1], coords[3], coords[4]])
                } else {
                    Err(StatusCode::BAD_REQUEST)
                }
            }
            Err(_) => {
                warn!("Failed to parse bbox coordinates");
                Err(StatusCode::BAD_REQUEST)
            }
        };

        // Apply coordinate transformation if bbox-crs is provided
        match parsed_bbox {
            Ok(coords) => {
                if let Some(bbox_crs) = &query.bbox_crs {
                    // Transform from bbox_crs to Dutch CRS
                    match transform_bbox(&coords, bbox_crs, DUTCH_CRS) {
                        Ok(transformed) => {
                            info!(
                                "Transformed bbox from {} to {}: {:?} -> {:?}",
                                bbox_crs, DUTCH_CRS, coords, transformed
                            );
                            Some(transformed)
                        }
                        Err(e) => {
                            warn!(
                                "Failed to transform bbox from {} to {}: {}",
                                bbox_crs, DUTCH_CRS, e
                            );
                            return Err(StatusCode::BAD_REQUEST);
                        }
                    }
                } else {
                    // No transformation needed, bbox is already in Dutch CRS
                    Some(coords)
                }
            }
            Err(e) => {
                warn!("Failed to parse bbox: {:?}", e);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    } else {
        None
    };

    // Parse filter if provided
    let filter_conditions = if let Some(filter_str) = &query.filter {
        match parse_filter(filter_str, &state.fcb_metadata) {
            Ok(conditions) => Some(conditions),
            Err(ParseError::InvalidSyntax(msg)) => {
                warn!("Invalid filter syntax: {}", msg);
                return Err(StatusCode::BAD_REQUEST);
            }
            Err(ParseError::UnsupportedOperator(op)) => {
                warn!("Unsupported operator in filter: {}", op);
                return Err(StatusCode::BAD_REQUEST);
            }
            Err(ParseError::InvalidValue(val)) => {
                warn!("Invalid value in filter: {}", val);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    } else {
        None
    };

    let http_reader = match HttpFcbReader::open(&state.fcb_url).await {
        Ok(reader) => reader,
        Err(e) => {
            warn!("Failed to open FCB reader: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Store header buffer for later use
    let header_buf = http_reader.header();

    // TODO: think the best way not to open a new reader again
    let feature_reader = match HttpFcbReader::open(&state.fcb_url).await {
        Ok(reader) => reader,
        Err(e) => {
            warn!("Failed to open FCB reader: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Apply filtering (bbox and/or attribute filters)
    let res = fetch_features_with_filter(
        feature_reader,
        bbox.as_ref(),
        filter_conditions,
        limit,
        offset,
    )
    .await;

    let (features, total_count) = match res {
        Ok(features) => features,
        Err(e) => {
            warn!("Failed to fetch features: {:?}", e);
            // Check if this is an attribute-related error that should return 400
            let error_msg = e.to_string();
            if error_msg.contains("AttributeIndexNotFound")
                || error_msg.contains("NoColumnsInHeader")
                || error_msg.contains("QueryExecutionError")
                || error_msg.contains("Failed to execute streaming query")
            {
                return Err(StatusCode::BAD_REQUEST);
            }
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Handle different output formats
    match format {
        "cjseq" => {
            // Generate CityJSONSeq format
            let metadata = match deserializer::to_cj_metadata(&header_buf) {
                Ok(meta) => meta,
                Err(e) => {
                    warn!("Failed to generate CityJSON metadata: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };

            let mut output = String::new();
            // First line: metadata
            output.push_str(&serde_json::to_string(&metadata).unwrap_or_default());
            output.push('\n');

            // Following lines: individual features
            for feature in features {
                output.push_str(&serde_json::to_string(&feature).unwrap_or_default());
                output.push('\n');
            }

            Ok((
                [(header::CONTENT_TYPE, "application/city+json-seq")],
                output,
            )
                .into_response())
        }
        "cityjson" => {
            // Generate CityJSON format by combining all features
            let mut cjj = match deserializer::to_cj_metadata(&header_buf) {
                Ok(meta) => meta,
                Err(e) => {
                    warn!("Failed to generate CityJSON metadata: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };

            // Add all features to the CityJSON object
            for mut feature in features {
                cjj.add_cjfeature(&mut feature);
            }

            // Remove duplicate vertices and update transform
            cjj.remove_duplicate_vertices();
            cjj.update_transform();

            let json_str = serde_json::to_string(&cjj).unwrap_or_default();
            Ok(([(header::CONTENT_TYPE, "application/city+json")], json_str).into_response())
        }
        "obj" => {
            // Generate OBJ format
            let mut cjj = match deserializer::to_cj_metadata(&header_buf) {
                Ok(meta) => meta,
                Err(e) => {
                    warn!("Failed to generate CityJSON metadata: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };

            // Add all features to the CityJSON object
            for mut feature in features {
                cjj.add_cjfeature(&mut feature);
            }

            // Remove duplicate vertices and update transform
            cjj.remove_duplicate_vertices();
            cjj.update_transform();

            // Convert to OBJ
            let obj_str = cjseq::conv::obj::to_obj_string(&cjj);
            Ok(([(header::CONTENT_TYPE, "text/plain")], obj_str).into_response())
        }
        "json" | _ => {
            // Default JSON format (FeatureCollection)
            let number_matched = total_count as i32;
            let number_returned = features.len() as i32;

            let feature_collection = FeatureCollection {
                r#type: Type::FeatureCollection,
                features: features
                    .into_iter()
                    .map(|f| FeatureCityJson {
                        feature: Some(serde_json::to_value(f.clone()).unwrap_or_default()),
                        id: Some(f.id.clone()),
                        links: Some(vec![
                            Link {
                                href: format!("/collections/{}/items/{}", collection_id, f.id),
                                rel: "self".to_string(),
                                r#type: Some("application/city+json".to_string()),
                                title: Some("this document".to_string()),
                                ..Default::default()
                            },
                            Link {
                                href: format!("/collections/{collection_id}"),
                                rel: "collection".to_string(),
                                r#type: Some("application/json".to_string()),
                                title: Some("Collection".to_string()),
                                ..Default::default()
                            },
                        ]),
                        ..Default::default()
                    })
                    .collect(),
                number_matched: Some(number_matched),
                number_returned: Some(number_returned),
                time_stamp: Some(Utc::now().to_rfc3339()),
                links: Some(vec![Link {
                    href: format!("/collections/{collection_id}/items"),
                    rel: "self".to_string(),
                    r#type: Some("application/json".to_string()),
                    title: Some("this document".to_string()),
                    ..Default::default()
                }]),
            };

            Ok(Json(feature_collection).into_response())
        }
    }
}

pub async fn collection_item_by_id(
    Path((collection_id, item_id)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<FeatureCityJson>, StatusCode> {
    info!(
        "Serving item: {} from collection: {}",
        item_id, collection_id
    );

    if collection_id != PAND_COLLECTION_ID {
        return Err(StatusCode::NOT_FOUND);
    }

    let http_reader = match HttpFcbReader::open(&state.fcb_url).await {
        Ok(reader) => reader,
        Err(e) => {
            warn!("Failed to open FCB reader: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let feature = fetch_feature_by_id(http_reader, &item_id).await;

    let feature = feature.map(|f| FeatureCityJson {
        feature: f.map(|f| serde_json::to_value(f).unwrap_or_default()),
        id: Some(item_id.clone()),
        links: Some(vec![
            Link {
                href: format!("/collections/{collection_id}/items/{item_id}"),
                rel: "self".to_string(),
                r#type: Some("application/json".to_string()),
                title: Some("this document".to_string()),
                ..Default::default()
            },
            Link {
                href: format!("/collections/{collection_id}"),
                rel: "collection".to_string(),
                r#type: Some("application/json".to_string()),
                ..Default::default()
            },
            Link {
                href: format!("/collections/{collection_id}/items"),
                rel: "parent".to_string(),
                r#type: Some("application/city+json".to_string()),
                ..Default::default()
            },
        ]),
    });

    match feature {
        Ok(feature) => Ok(Json(feature)),
        Err(e) => {
            warn!("Failed to fetch feature by ID: {:?}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn fetch_features_by_bbox<T: AsyncHttpRangeClient + Send + Sync>(
    reader: HttpFcbReader<T>,
    bbox: &Vec<f64>,
    limit: i32,
    offset: i32,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    let (minx, miny, maxx, maxy) = (bbox[0], bbox[1], bbox[2], bbox[3]);

    let mut iter = reader
        .select_query_paged(
            Query::BBox(minx, miny, maxx, maxy),
            Some(limit as usize),
            Some(offset as usize),
        )
        .await?;

    let mut features = Vec::new();
    let mut count = 0;

    while count < limit {
        match iter.next().await? {
            Some(feature_result) => {
                features.push(feature_result.cj_feature()?);
                count += 1;
            }
            None => break,
        }
    }

    let features_count = iter.features_count().unwrap_or(count as usize);

    Ok((features, features_count))
}

async fn fetch_features_limited<T: AsyncHttpRangeClient + Send + Sync>(
    reader: HttpFcbReader<T>,
    limit: u32,
    offset: u32,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    // For full scan, emulate pagination by skipping `offset` features client-side.
    let mut iter = reader.select_all().await?;

    // Skip offset
    let mut skipped: u32 = 0;
    while skipped < offset {
        match iter.next().await? {
            Some(_) => skipped += 1,
            None => break,
        }
    }

    let mut features = Vec::new();
    let mut count = 0;

    while count < limit {
        match iter.next().await? {
            Some(feature_result) => {
                features.push(feature_result.cj_feature()?);
                count += 1;
            }
            None => break,
        }
    }

    let features_count = iter.features_count().unwrap_or(count as usize);

    Ok((features, features_count))
}

async fn fetch_feature_by_id<T: AsyncHttpRangeClient + Send + Sync>(
    reader: HttpFcbReader<T>,
    feature_id: &str,
) -> Result<Option<CityJSONFeature>, anyhow::Error> {
    let query: Vec<(String, Operator, KeyType)> = vec![(
        "identificatie".to_string(),
        Operator::Eq,
        KeyType::StringKey50(FixedStringKey::from_str(feature_id)),
    )];

    let mut iter = reader
        .select_attr_query_paged(&query, Some(1), Some(0))
        .await?;

    if let Some(feature_result) = iter.next().await? {
        return Ok(Some(feature_result.cj_feature()?));
    }

    Ok(None)
}

async fn fetch_features_with_filter<T: AsyncHttpRangeClient + Send + Sync>(
    reader: HttpFcbReader<T>,
    bbox: Option<&Vec<f64>>,
    filter_conditions: Option<Vec<(String, Operator, KeyType)>>,
    limit: i32,
    offset: i32,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    match (bbox, filter_conditions) {
        // Both bbox and filter
        (Some(bbox), Some(_)) => {
            // For now, we'll prioritize bbox over attribute filtering
            // In a full implementation, we'd need to apply both filters
            fetch_features_by_bbox(reader, bbox, limit, offset).await
        }
        // Only filter
        (None, Some(conditions)) => {
            println!("Fetching features with filter: {conditions:?}");
            let mut iter = match reader
                .select_attr_query_paged(&conditions, Some(limit as usize), Some(offset as usize))
                .await
            {
                Ok(iter) => iter,
                Err(e) => {
                    warn!("Failed to execute attribute query: {:?}", e);
                    return Err(e.into());
                }
            };

            let mut features = Vec::new();
            let mut count = 0;

            while count < limit {
                match iter.next().await? {
                    Some(feature_result) => {
                        features.push(feature_result.cj_feature()?);
                        count += 1;
                    }
                    None => break,
                }
            }

            let features_count = iter.features_count().unwrap_or(count as usize);
            Ok((features, features_count))
        }
        // Only bbox
        (Some(bbox), None) => fetch_features_by_bbox(reader, bbox, limit, offset).await,
        // Neither bbox nor filter
        (None, None) => fetch_features_limited(reader, limit as u32, offset as u32).await,
    }
}
