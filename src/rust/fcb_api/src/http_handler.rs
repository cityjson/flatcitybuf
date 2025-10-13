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
use std::sync::Arc;
use tracing::{info, warn};

use crate::constants::*;
use crate::crs::transform_bbox;
use crate::filter_parser::{parse_filter, ParseError};
use crate::handlers::{determine_format, BboxQuery};
use crate::link::{build_link_header, build_link_json};
use crate::models::*;
use crate::AppState;

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

    // Store header for later use
    let header = http_reader.header();

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
    let number_matched = total_count as i32;
    let number_returned = features.len() as i32;

    match format {
        "cjseq" => {
            // Generate CityJSONSeq format
            let metadata = match deserializer::to_cj_metadata(&header) {
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

            // Build Link header for pagination
            let link_header = build_link_header(
                &state.base_url,
                &collection_id,
                &query,
                limit,
                offset,
                number_matched,
                number_returned,
            );

            Ok((
                [
                    (header::CONTENT_TYPE, "application/city+json-seq"),
                    (header::LINK, link_header.as_str()),
                    (
                        header::CONTENT_DISPOSITION,
                        "inline; filename=\"data.city.jsonl\"",
                    ),
                ],
                output,
            )
                .into_response())
        }
        "cityjson" => {
            // Generate CityJSON format by combining all features
            let mut cjj = match deserializer::to_cj_metadata(&header) {
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

            // Build Link header for pagination
            let link_header = build_link_header(
                &state.base_url,
                &collection_id,
                &query,
                limit,
                offset,
                number_matched,
                number_returned,
            );

            Ok((
                [
                    (header::CONTENT_TYPE, "application/city+json"),
                    (header::LINK, link_header.as_str()),
                    (
                        header::CONTENT_DISPOSITION,
                        "inline; filename=\"data.city.json\"",
                    ),
                ],
                json_str,
            )
                .into_response())
        }
        "obj" => {
            // Generate OBJ format
            let mut cjj = match deserializer::to_cj_metadata(&header) {
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

            // Build Link header for pagination
            let link_header = build_link_header(
                &state.base_url,
                &collection_id,
                &query,
                limit,
                offset,
                number_matched,
                number_returned,
            );

            Ok((
                [
                    (header::CONTENT_TYPE, "text/plain"),
                    (header::LINK, link_header.as_str()),
                    (header::CONTENT_DISPOSITION, "inline; filename=\"data.obj\""),
                ],
                obj_str,
            )
                .into_response())
        }
        "json" | _ => {
            // Default JSON format (FeatureCollection)
            // Generate pagination links using the shared link module
            let collection_links = build_link_json(
                &state.base_url,
                &collection_id,
                &query,
                limit,
                offset,
                number_matched,
                number_returned,
            );

            let feature_collection = FeatureCollection {
                r#type: Type::FeatureCollection,
                features: features
                    .into_iter()
                    .map(|f| FeatureCityJson {
                        feature: Some(serde_json::to_value(f.clone()).unwrap_or_default()),
                        id: Some(f.id.clone()),
                        links: Some(vec![
                            Link {
                                href: format!(
                                    "{}/collections/{}/items/{}",
                                    state.base_url.trim_end_matches('/'),
                                    collection_id,
                                    f.id
                                ),
                                rel: "self".to_string(),
                                r#type: Some("application/city+json".to_string()),
                                title: Some("this document".to_string()),
                                ..Default::default()
                            },
                            Link {
                                href: format!(
                                    "{}/collections/{}",
                                    state.base_url.trim_end_matches('/'),
                                    collection_id
                                ),
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
                links: Some(collection_links),
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
