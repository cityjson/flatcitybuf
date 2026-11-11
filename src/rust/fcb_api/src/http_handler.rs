use crate::error::ApiError;
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
use tracing::{error, info, warn};

use crate::constants::*;
use crate::filter_parser::{parse_filter, ParseError};
use crate::handlers::{collect_features, determine_format, validate_output_crs, BboxQuery, FeatureQuery};
use crate::link::{build_link_header, build_link_json};
use crate::models::*;
use crate::AppState;

pub async fn collection_items(
    Path(collection_id): Path<String>,
    headers: HeaderMap,
    query: Result<AxumQuery<BboxQuery>, axum::extract::rejection::QueryRejection>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, ApiError> {
    let AxumQuery(mut query) = query.map_err(|_| StatusCode::BAD_REQUEST)?;
    info!(
        "Serving items for collection: {} with query: {:?}",
        collection_id, query
    );

    if collection_id != PAND_COLLECTION_ID {
        return Err(StatusCode::NOT_FOUND.into());
    }

    // Determine format from query parameter or Accept header
    let format = determine_format(&query.f, &headers).to_owned();
    query.f = Some(format.clone());
    let (limit, offset) = query.pagination(state.max_return_features)?;

    let bbox = query.spatial_query(&state)?;

    // Parse filter if provided
    let filter_conditions = if let Some(filter_str) = &query.filter {
        match parse_filter(filter_str, &state.fcb_metadata) {
            Ok(conditions) => Some(conditions),
            Err(ParseError::InvalidSyntax(msg)) => {
                warn!("Invalid filter syntax: {}", msg);
                return Err(StatusCode::BAD_REQUEST.into());
            }
            Err(ParseError::UnsupportedOperator(op)) => {
                warn!("Unsupported operator in filter: {}", op);
                return Err(StatusCode::BAD_REQUEST.into());
            }
            Err(ParseError::InvalidValue(val)) => {
                warn!("Invalid value in filter: {}", val);
                return Err(StatusCode::BAD_REQUEST.into());
            }
        }
    } else {
        None
    };

    if let Some(conditions) = &filter_conditions {
        if conditions.is_empty()
            || conditions
                .iter()
                .any(|(name, _, _)| !state.fcb_metadata.is_column_indexed(name))
        {
            return Err(StatusCode::BAD_REQUEST.into());
        }
    }

    let http_reader = match state.http_reader().await {
        Ok(reader) => reader,
        Err(e) => {
            warn!("Failed to open FCB reader: {:?}", e);
            return Err(StatusCode::SERVICE_UNAVAILABLE.into());
        }
    };

    // Store header for later use
    let header = http_reader.header();

    // TODO: think the best way not to open a new reader again
    let feature_reader = match state.http_reader().await {
        Ok(reader) => reader,
        Err(e) => {
            warn!("Failed to open FCB reader: {:?}", e);
            return Err(StatusCode::SERVICE_UNAVAILABLE.into());
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
            return Err(StatusCode::SERVICE_UNAVAILABLE.into());
        }
    };

    // Handle different output formats
    let number_matched = total_count;
    let number_returned = features.len();

    match format.as_str() {
        "cjseq" => {
            // Generate CityJSONSeq format
            let metadata = match deserializer::to_cj_metadata(&header) {
                Ok(meta) => meta,
                Err(e) => {
                    warn!("Failed to generate CityJSON metadata: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR.into());
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
            let cjj = match deserializer::to_cj_metadata(&header) {
                Ok(meta) => meta,
                Err(e) => {
                    warn!("Failed to generate CityJSON metadata: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR.into());
                }
            };

            // Collect all features into one CityJSON object, which merges
            // duplicate vertices and moves the transform to the data.
            let cjj = collect_features(cjj, features)?;

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
            let cjj = match deserializer::to_cj_metadata(&header) {
                Ok(meta) => meta,
                Err(e) => {
                    warn!("Failed to generate CityJSON metadata: {:?}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR.into());
                }
            };

            // Collect all features into one CityJSON object, which merges
            // duplicate vertices and moves the transform to the data.
            let cjj = collect_features(cjj, features)?;

            // Convert to OBJ
            let obj_str = fcb_core::obj::to_obj_string(&cjj).map_err(|e| {
                error!("OBJ conversion error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

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
        _ => {
            let links = build_link_json(
                &state.base_url,
                &collection_id,
                &query,
                limit,
                offset,
                number_matched,
                number_returned,
            );
            let link_header = build_link_header(
                &state.base_url,
                &collection_id,
                &query,
                limit,
                offset,
                number_matched,
                number_returned,
            );
            let response = FeatureCollection {
                r#type: Type::FeatureCollection,
                metadata: state.fcb_metadata.cityjson.clone(),
                features: features
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
                number_matched: Some(number_matched as u64),
                number_returned: Some(number_returned as u64),
                time_stamp: Some(Utc::now().to_rfc3339()),
                links: Some(links),
            };
            Ok(([(header::LINK, link_header)], Json(response)).into_response())
        }
    }
}

pub async fn collection_item_by_id(
    Path((collection_id, item_id)): Path<(String, String)>,
    query: Result<AxumQuery<FeatureQuery>, axum::extract::rejection::QueryRejection>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<FeatureCityJson>, ApiError> {
    let AxumQuery(query) = query.map_err(|_| StatusCode::BAD_REQUEST)?;
    info!(
        "Serving item: {} from collection: {}",
        item_id, collection_id
    );

    if collection_id != PAND_COLLECTION_ID {
        return Err(StatusCode::NOT_FOUND.into());
    }

    validate_output_crs(query.crs.as_deref(), &state)?;
    let http_reader = match state.http_reader().await {
        Ok(reader) => reader,
        Err(e) => {
            warn!("Failed to open FCB reader: {:?}", e);
            return Err(StatusCode::SERVICE_UNAVAILABLE.into());
        }
    };

    let feature = fetch_feature_by_id(http_reader, &item_id)
        .await
        .map_err(ApiError::storage)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(FeatureCityJson {
        metadata: state.fcb_metadata.cityjson.clone(),
        feature: Some(
            serde_json::to_value(feature).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ),
        id: Some(item_id.clone()),
        links: Some(vec![
            Link {
                href: format!(
                    "{}/collections/{collection_id}/items/{item_id}",
                    state.base_url
                ),
                rel: "self".into(),
                r#type: Some(CONTENT_TYPE_JSON.into()),
                title: Some(TITLE_THIS_DOCUMENT.into()),
                ..Default::default()
            },
            Link {
                href: format!("{}/collections/{collection_id}", state.base_url),
                rel: "collection".into(),
                r#type: Some(CONTENT_TYPE_JSON.into()),
                ..Default::default()
            },
            Link {
                href: format!("{}/collections/{collection_id}/items", state.base_url),
                rel: "parent".into(),
                r#type: Some(CONTENT_TYPE_CITYJSON.into()),
                ..Default::default()
            },
        ]),
    }))
}

async fn fetch_features_by_bbox<T: AsyncHttpRangeClient + Send + Sync>(
    reader: HttpFcbReader<T>,
    bbox: &[f64],
    limit: usize,
    offset: usize,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    let (minx, miny, maxx, maxy) = (bbox[0], bbox[1], bbox[2], bbox[3]);

    let mut iter = reader
        .select_query_paged(
            Query::BBox(minx, miny, maxx, maxy),
            Some(limit),
            Some(offset),
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

    let features_count = iter.features_count().unwrap_or(count);

    Ok((features, features_count))
}

async fn fetch_features_limited<T: AsyncHttpRangeClient + Send + Sync>(
    reader: HttpFcbReader<T>,
    limit: usize,
    offset: usize,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    // For full scan, emulate pagination by skipping `offset` features client-side.
    let mut iter = reader.select_all().await?;

    // Skip offset
    let mut skipped: usize = 0;
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

    let features_count = iter.features_count().unwrap_or(count);

    Ok((features, features_count))
}

async fn fetch_feature_by_id<T: AsyncHttpRangeClient + Send + Sync>(
    reader: HttpFcbReader<T>,
    feature_id: &str,
) -> Result<Option<CityJSONFeature>, anyhow::Error> {
    if feature_id.len() > 50 {
        return Ok(None);
    }
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
    limit: usize,
    offset: usize,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    match (bbox, filter_conditions) {
        (Some(_), Some(_)) => unreachable!("combined queries rejected by validation"),
        // Only filter
        (None, Some(conditions)) => {
            info!("Fetching features with filter: {conditions:?}");
            let mut iter = match reader
                .select_attr_query_paged(&conditions, Some(limit), Some(offset))
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

            let features_count = iter.features_count().unwrap_or(count);
            Ok((features, features_count))
        }
        // Only bbox
        (Some(bbox), None) => fetch_features_by_bbox(reader, bbox, limit, offset).await,
        // Neither bbox nor filter
        (None, None) => fetch_features_limited(reader, limit, offset).await,
    }
}
