use crate::error::ApiError;
use anyhow::anyhow;
use axum::{
    extract::{Path, Query as AxumQuery, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use chrono::Utc;
use cjseq::CityJSONFeature;
use fcb_core::packed_rtree::Query;
use fcb_core::{FcbReader, FixedStringKey, KeyType, Operator};
use std::error::Error as StdError;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::constants::*;
use crate::filter_parser::{parse_filter, ParseError};
use crate::handlers::{
    collect_features, determine_format, validate_output_crs, BboxQuery, FeatureQuery,
};
use crate::link::{build_feature_links, build_link_header, build_link_json};
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

    // Local reads are synchronous: run them off the async workers.
    let path = state.fcb_url.clone();
    let has_rtree = state.fcb_metadata.has_rtree;
    let read = tokio::task::spawn_blocking(move || {
        fetch_features_with_filter(
            open_reader(&path)?,
            bbox.as_ref(),
            filter_conditions,
            limit,
            offset,
            has_rtree,
        )
    });
    let (features, total_count) = match read.await {
        Ok(Ok(features)) => features,
        Ok(Err(e)) => return Err(ApiError::storage(format!("{e:#}"))),
        Err(e) => return Err(ApiError::storage(e)),
    };

    // Handle different output formats
    let number_matched = total_count;
    let number_returned = features.len();

    match format.as_str() {
        "cjseq" => {
            // Generate CityJSONSeq format
            let metadata = state.fcb_metadata.cityjson_header()?;

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
            let cjj = state.fcb_metadata.cityjson_header()?;

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
            let cjj = state.fcb_metadata.cityjson_header()?;

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
    if !state.fcb_metadata.has_item_index() {
        return Err(StatusCode::NOT_FOUND.into());
    }
    let path = state.fcb_url.clone();
    let id = item_id.clone();
    let feature =
        tokio::task::spawn_blocking(move || fetch_feature_by_id(open_reader(&path)?, &id))
            .await
            .map_err(ApiError::storage)?
            .map_err(|e| ApiError::storage(format!("{e:#}")))?
            .ok_or(StatusCode::NOT_FOUND)?;
    let links = build_feature_links(
        &state.base_url,
        &collection_id,
        &item_id,
        query.crs.as_deref(),
    );
    let id = feature.id.clone();
    Ok(Json(FeatureCityJson {
        metadata: state.fcb_metadata.cityjson.clone(),
        feature: Some(
            serde_json::to_value(feature).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        ),
        id: Some(id),
        links: Some(links),
    }))
}

fn open_reader(path: &str) -> Result<FcbReader<BufReader<File>>, anyhow::Error> {
    Ok(FcbReader::open(BufReader::new(File::open(path)?))?)
}

fn fetch_features_by_bbox(
    reader: FcbReader<BufReader<File>>,
    bbox: &[f64],
    limit: usize,
    offset: usize,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    let (minx, miny, maxx, maxy) = (bbox[0], bbox[1], bbox[2], bbox[3]);

    let mut iter = reader
        .select_query(
            Query::BBox(minx, miny, maxx, maxy),
            Some(limit),
            Some(offset),
        )
        .map_err(|e| {
            error!("Failed to select query: {:?}", e);
            error!("Query error details: {:#}", e);
            if let Some(source) = e.source() {
                error!("Error source: {:?}", source);
            }
            anyhow!("Failed to select query: {:?}", e)
        })?;

    let mut features = Vec::new();
    let mut count = 0;

    while count < limit {
        match iter.next()? {
            Some(feature_iter) => {
                features.push(feature_iter.cur_cj_feature()?);
                count += 1;
            }
            None => break,
        }
    }

    let features_count = iter.features_count().unwrap_or(count);

    Ok((features, features_count))
}

fn fetch_features_limited(
    reader: FcbReader<BufReader<File>>,
    limit: usize,
    offset: usize,
    has_rtree: bool,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    // The R-tree leaves locate the page. Without them the only way to a
    // page is through every feature before it.
    let mut iter = if has_rtree {
        reader.select_all_paged(Some(limit), Some(offset))?
    } else {
        let mut iter = reader.select_all()?;
        let mut skipped: usize = 0;
        while skipped < offset && iter.next()?.is_some() {
            skipped += 1;
        }
        iter
    };

    let mut features = Vec::new();
    while features.len() < limit {
        match iter.next()? {
            Some(feature_iter) => features.push(feature_iter.cur_cj_feature()?),
            None => break,
        }
    }

    let features_count = iter.features_count().unwrap_or(features.len());

    Ok((features, features_count))
}

fn fetch_feature_by_id(
    reader: FcbReader<BufReader<File>>,
    feature_id: &str,
) -> Result<Option<CityJSONFeature>, anyhow::Error> {
    if feature_id.len() > 50 {
        return Ok(None);
    }
    let query: Vec<(String, Operator, KeyType)> = vec![(
        ITEM_ID_COLUMN.to_string(),
        Operator::Eq,
        KeyType::StringKey50(FixedStringKey::from_str(feature_id)),
    )];

    let mut iter = reader.select_attr_query(query)?;

    if let Some(feature_iter) = iter.next()? {
        return Ok(Some(feature_iter.cur_cj_feature()?));
    }

    Ok(None)
}

fn fetch_features_with_filter(
    reader: FcbReader<BufReader<File>>,
    bbox: Option<&Vec<f64>>,
    filter_conditions: Option<Vec<(String, Operator, KeyType)>>,
    limit: usize,
    offset: usize,
    has_rtree: bool,
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    match (bbox, filter_conditions) {
        (Some(_), Some(_)) => unreachable!("combined queries rejected by validation"),
        // Only filter
        (None, Some(conditions)) => {
            info!("Fetching features with filter: {conditions:?}");
            // Clone conditions for error logging before moving
            let conditions_log = conditions.clone();
            let mut iter = match reader.select_attr_query(conditions) {
                Ok(iter) => iter,
                Err(e) => {
                    error!(
                        "Failed to execute attribute query with conditions {:?}: {:?}",
                        conditions_log, e
                    );
                    error!("Attribute query error details: {:#}", e);
                    if let Some(source) = e.source() {
                        error!("Error source: {:?}", source);
                    }
                    return Err(e.into());
                }
            };

            // Skip offset
            let mut skipped = 0;
            while skipped < offset {
                match iter.next()? {
                    Some(_) => skipped += 1,
                    None => break,
                }
            }

            let mut features = Vec::new();
            let mut count = 0;

            while count < limit {
                match iter.next()? {
                    Some(feature_iter) => {
                        features.push(feature_iter.cur_cj_feature()?);
                        count += 1;
                    }
                    None => break,
                }
            }

            let features_count = iter.features_count().unwrap_or(count);
            Ok((features, features_count))
        }
        // Only bbox
        (Some(bbox), None) => fetch_features_by_bbox(reader, bbox, limit, offset),
        // Neither bbox nor filter
        (None, None) => fetch_features_limited(reader, limit, offset, has_rtree),
    }
}
