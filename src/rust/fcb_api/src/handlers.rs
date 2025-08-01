use axum::{
    extract::{Path, Query as AxumQuery, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use cjseq::CityJSONFeature;
use fcb_core::packed_rtree::Query;
use fcb_core::{FixedStringKey, HttpFcbReader, KeyType, Operator};
use http_range_client::AsyncHttpRangeClient;
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

use crate::constants::*;
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
    bbox_crs: Option<String>,
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
    AxumQuery(query): AxumQuery<BboxQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<FeatureCollection>, StatusCode> {
    info!(
        "Serving items for collection: {} with query: {:?}",
        collection_id, query
    );

    if collection_id != PAND_COLLECTION_ID {
        return Err(StatusCode::NOT_FOUND);
    }

    let limit = query
        .limit
        .unwrap_or(DEFAULT_LIMIT)
        .min(state.max_return_features as i32);

    let bbox = query.bbox.as_ref().map(|bbox_str| {
        let parts: Result<Vec<f64>, _> = bbox_str
            .split(',')
            .map(|s| s.trim().parse::<f64>())
            .collect();

        match parts {
            Ok(coords) => {
                if coords.len() == 4 {
                    Ok(coords)
                } else if coords.len() == 6 {
                    Ok(vec![coords[0], coords[1], coords[3], coords[4]])
                } else {
                    Err(StatusCode::BAD_REQUEST)
                }
            }
            Err(_) => Err(StatusCode::BAD_REQUEST),
        }
    });

    let bbox = match bbox {
        Some(Ok(bbox)) => Some(bbox),
        Some(Err(e)) => {
            warn!("Failed to parse bbox: {:?}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
        None => None,
    };
    let http_reader = match HttpFcbReader::open(&state.fcb_url).await {
        Ok(reader) => reader,
        Err(e) => {
            warn!("Failed to open FCB reader: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Apply bbox filtering if provided

    let res = if let Some(bbox) = &bbox {
        fetch_features_by_bbox(http_reader, bbox, limit).await
    } else {
        fetch_features_limited(http_reader, limit as u32).await
    };

    let (features, total_count) = match res {
        Ok(features) => features,
        Err(e) => {
            warn!("Failed to fetch features: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

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
                        href: format!("/collections/{}", collection_id),
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
            href: format!("/collections/{}/items", collection_id),
            rel: "self".to_string(),
            r#type: Some("application/json".to_string()),
            title: Some("this document".to_string()),
            ..Default::default()
        }]),
    };

    Ok(Json(feature_collection))
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
                href: format!("/collections/{}/items/{}", collection_id, item_id),
                rel: "self".to_string(),
                r#type: Some("application/json".to_string()),
                title: Some("this document".to_string()),
                ..Default::default()
            },
            Link {
                href: format!("/collections/{}", collection_id),
                rel: "collection".to_string(),
                r#type: Some("application/json".to_string()),
                ..Default::default()
            },
            Link {
                href: format!("/collections/{}/items", collection_id),
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
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    let (minx, miny, maxx, maxy) = (bbox[0], bbox[1], bbox[2], bbox[3]);

    let mut iter = reader
        .select_query(Query::BBox(minx, miny, maxx, maxy))
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
) -> Result<(Vec<CityJSONFeature>, usize), anyhow::Error> {
    let mut iter = reader.select_all().await?;

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

    let mut iter = reader.select_attr_query(&query).await?;

    if let Some(feature_result) = iter.next().await? {
        return Ok(Some(feature_result.cj_feature()?));
    }

    Ok(None)
}
