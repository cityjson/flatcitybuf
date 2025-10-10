use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::constants::*;
use crate::models::*;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct BboxQuery {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub bbox: Option<String>,
    #[serde(rename = "bbox-crs")]
    pub bbox_crs: Option<String>,
    pub filter: Option<String>,
    pub f: Option<String>,
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
        },
        "cityjson": {
            "version": CITYJSON_VERSION,
            "transform": {
                "scale": CITYJSON_SCALE,
                "translate": CITYJSON_TRANSLATE
            },
            "extensions": CITYJSON_EXTENSIONS
        }
    });

    Ok(Json(collection))
}
