use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Json,
};
use cjseq::{CityJSON, CityJSONFeature};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

use crate::constants::*;
use crate::models::*;
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BboxQuery {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub bbox: Option<String>,
    #[serde(rename = "bbox-crs")]
    pub bbox_crs: Option<String>,
    pub filter: Option<String>,
    pub f: Option<String>,
    pub crs: Option<String>,
}

/// Collects `features` into one CityJSON document under `header`, merging
/// duplicate vertices and fitting the transform to the data. A feature that
/// does not fit the document is a server error.
pub(crate) fn collect_features(
    header: CityJSON,
    features: Vec<CityJSONFeature>,
) -> Result<CityJSON, StatusCode> {
    cjseq::cjseq_to_cj(header, features).map_err(|e| {
        warn!("Failed to collect features into one CityJSON object: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

impl BboxQuery {
    pub fn pagination(&self, maximum: u32) -> Result<(usize, usize), StatusCode> {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT as u64);
        if limit == 0 || (self.bbox.is_some() && self.filter.is_some()) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if self
            .f
            .as_deref()
            .is_some_and(|f| !["json", "cityjson", "cjseq", "obj"].contains(&f))
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok((
            usize::try_from(limit.min(maximum as u64)).map_err(|_| StatusCode::BAD_REQUEST)?,
            usize::try_from(self.offset.unwrap_or(DEFAULT_OFFSET))
                .map_err(|_| StatusCode::BAD_REQUEST)?,
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureQuery {
    pub crs: Option<String>,
}

pub fn validate_output_crs(value: Option<&str>, state: &AppState) -> Result<(), StatusCode> {
    if let Some(value) = value {
        if crate::crs::SupportedCrs::parse(value)?.uri() != state.fcb_metadata.storage_crs {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}
impl BboxQuery {
    pub fn spatial_query(&self, state: &AppState) -> Result<Option<Vec<f64>>, StatusCode> {
        use crate::crs::{transform_bbox, SupportedCrs};
        validate_output_crs(self.crs.as_deref(), state)?;
        let target = SupportedCrs::parse(&state.fcb_metadata.storage_crs)?;
        let source = self
            .bbox_crs
            .as_deref()
            .map(SupportedCrs::parse)
            .transpose()?
            .unwrap_or(target);
        let Some(value) = &self.bbox else {
            return Ok(None);
        };
        let coordinates = value
            .split(',')
            .map(|v| v.trim().parse::<f64>().map_err(|_| StatusCode::BAD_REQUEST))
            .collect::<Result<Vec<_>, _>>()?;
        if ![4, 6].contains(&coordinates.len()) || coordinates.iter().any(|v| !v.is_finite()) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let dimensions = coordinates.len() / 2;
        if (0..dimensions).any(|i| coordinates[i] > coordinates[i + dimensions]) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let mut bbox = vec![
            coordinates[0],
            coordinates[1],
            coordinates[dimensions],
            coordinates[dimensions + 1],
        ];
        if source == SupportedCrs::Wgs84 {
            bbox.swap(0, 1);
            bbox.swap(2, 3);
        }
        transform_bbox(&bbox, source.horizontal(), target.horizontal())
            .map(Some)
            .map_err(|_| StatusCode::BAD_REQUEST)
    }
}

/// Determine output format from query parameter or Accept header
/// Priority: query parameter 'f' > Accept header > default 'json'
pub fn determine_format<'a>(query_format: &'a Option<String>, headers: &HeaderMap) -> &'a str {
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

    Ok(Json(
        serde_json::json!({"collections":[collection_value(&state)],"crs":[storage_crs(&state)],"links":[{"href":format!("{}/collections",state.base_url),"rel":"self","type":"application/json"}]}),
    ))
}

pub async fn collection_by_id(
    Path(collection_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Serving collection: {}", collection_id);

    if collection_id != PAND_COLLECTION_ID {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(collection_value(&state)))
}

pub fn storage_crs(state: &AppState) -> &str {
    &state.fcb_metadata.storage_crs
}

fn collection_value(state: &AppState) -> serde_json::Value {
    let metadata = &state.fcb_metadata.cityjson;
    let mut cityjson =
        serde_json::json!({"version":metadata["version"],"transform":metadata["transform"]});
    if let Some(extensions) = metadata.get("extensions") {
        cityjson["extensions"] = extensions.clone();
    }
    let mut version = serde_json::json!({"api":API_VERSION});
    if let Some(release) = &state.config.collection_version {
        version["collection"] = release.clone().into();
    }
    let mut collection = serde_json::json!({
        "id":PAND_COLLECTION_ID,"title":PAND_COLLECTION_TITLE,"description":PAND_COLLECTION_DESCRIPTION,
        "itemType":ITEM_TYPE_FEATURE,"crs":[storage_crs(state)],"storageCrs":storage_crs(state),
        "cityjson":cityjson,"version":version,
        "links":[
            {"href":format!("{}/collections/pand",state.base_url),"rel":"self","type":"application/json"},
            {"href":format!("{}/collections/pand/items",state.base_url),"rel":"items","type":"application/json"},
            {"href":LICENSE_URL,"rel":REL_LICENSE,"type":CONTENT_TYPE_HTML,"title":LICENSE_TITLE},
            {"href":LICENSE_RDF_URL,"rel":REL_LICENSE,"type":CONTENT_TYPE_RDF_XML,"title":LICENSE_TITLE}
        ]
    });
    if let Some(e) = &state.fcb_metadata.extent {
        collection["extent"] = serde_json::json!({"spatial":{"bbox":[[e[0],e[1],e[3],e[4]]],"crs":storage_crs(state)}});
    }
    collection
}
