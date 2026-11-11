//! An OGC API - Features server over a single FlatCityBuf dataset.
//!
//! [`create_app`] builds the [`axum`] `Router`; the dataset is named by the
//! `FCB_URL` environment variable and may be a local path or an HTTP(S) URL,
//! in which case [`fcb_core`]'s range-request reader fetches only the bytes a
//! request needs. Bounding-box and attribute filters are pushed down into the
//! R-tree and B+tree indices rather than scanning.
//!
//! This crate is not published to crates.io; it is built and run from the
//! repository (see the `Dockerfile` in `src/rust`).

pub mod constants;
mod crs;
mod error;
mod filter_parser;
mod fs_handler;
mod handlers;
mod http_handler;
mod link;
mod metadata;
mod models;

use axum::{routing::get, Router};
use metadata::FcbMetadata;
use std::env;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub fcb_url: String,
    pub max_return_features: u32,
    pub base_url: String,
    pub fcb_metadata: FcbMetadata,
    pub is_local_file: bool,
    pub config: AppConfig,
}

/// Check if the URL is a local file path or remote URL
fn is_local_file(url: &str) -> bool {
    // Check if it starts with common URL schemes
    if url.starts_with("http://") || url.starts_with("https://") {
        return false;
    }

    // Check if it starts with common file schemes
    if url.starts_with("file://") {
        return true;
    }

    // Otherwise, treat it as a local path
    true
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub fcb_url: String,
    pub base_url: String,
    pub max_return_features: u32,
    pub probe_timeout_seconds: u64,
    pub collection_version: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let fcb_url = env::var("FCB_URL").unwrap_or_else(|_| {
            "https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb".to_string()
        });

        let base_url = env::var("BASE_URL").unwrap_or_else(|_| "https://api.3dbag.nl".to_string());

        let max_return_features = env::var("MAX_RETURN_FEATURES")
            .unwrap_or_else(|_| "100".to_string())
            .parse::<u32>()?;

        Ok(Self {
            fcb_url,
            base_url,
            max_return_features,
            probe_timeout_seconds: env::var("DATASET_PROBE_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "10".into())
                .parse()?,
            collection_version: env::var("COLLECTION_VERSION")
                .ok()
                .filter(|v| !v.trim().is_empty()),
        })
    }
}

pub async fn create_app() -> anyhow::Result<Router> {
    create_app_with_config(AppConfig::from_env()?).await
}

pub async fn create_app_with_config(config: AppConfig) -> anyhow::Result<Router> {
    anyhow::ensure!(
        config.max_return_features > 0 && config.max_return_features <= i32::MAX as u32,
        "invalid MAX_RETURN_FEATURES"
    );
    anyhow::ensure!(
        config.probe_timeout_seconds > 0,
        "invalid DATASET_PROBE_TIMEOUT_SECONDS"
    );
    let base = reqwest::Url::parse(&config.base_url)?;
    anyhow::ensure!(
        matches!(base.scheme(), "http" | "https")
            && base.host_str().is_some()
            && base.query().is_none()
            && base.fragment().is_none(),
        "invalid BASE_URL"
    );
    let fcb_url = config
        .fcb_url
        .strip_prefix("file://")
        .unwrap_or(&config.fcb_url)
        .to_string();
    let base_url = config.base_url.trim().trim_end_matches('/').to_string();
    let max_return_features = config.max_return_features;
    // Determine if the URL is local or remote
    let is_local = is_local_file(&fcb_url);

    // Load FCB metadata
    let fcb_metadata = load_fcb_metadata(&config).await?;

    let state = Arc::new(AppState {
        fcb_url: fcb_url.clone(),
        max_return_features,
        base_url,
        fcb_metadata,
        is_local_file: is_local,
        config,
    });

    tracing::info!("FCB URL: {}", state.fcb_url);
    tracing::info!("Is local file: {}", state.is_local_file);
    tracing::info!("Max return features: {}", state.max_return_features);
    tracing::info!(
        "Loaded FCB metadata with {} columns",
        state.fcb_metadata.columns.len()
    );

    // Route to appropriate handlers based on URL type
    Ok(if is_local {
        Router::new()
            .route("/", get(handlers::landing_page))
            .route("/readyz", get(readiness))
            .route("/conformance", get(handlers::conformance))
            .route("/collections", get(handlers::collections))
            .route(
                "/collections/:collection_id",
                get(handlers::collection_by_id),
            )
            .route(
                "/collections/:collection_id/items",
                get(fs_handler::collection_items),
            )
            .route(
                "/collections/:collection_id/items/:item_id",
                get(fs_handler::collection_item_by_id),
            )
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .with_state(state)
    } else {
        Router::new()
            .route("/", get(handlers::landing_page))
            .route("/readyz", get(readiness))
            .route("/conformance", get(handlers::conformance))
            .route("/collections", get(handlers::collections))
            .route(
                "/collections/:collection_id",
                get(handlers::collection_by_id),
            )
            .route(
                "/collections/:collection_id/items",
                get(http_handler::collection_items),
            )
            .route(
                "/collections/:collection_id/items/:item_id",
                get(http_handler::collection_item_by_id),
            )
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .with_state(state)
    })
}

async fn load_fcb_metadata(config: &AppConfig) -> anyhow::Result<FcbMetadata> {
    use fcb_core::{FcbReader, HttpFcbReader};
    if is_local_file(&config.fcb_url) {
        let path = config
            .fcb_url
            .strip_prefix("file://")
            .unwrap_or(&config.fcb_url);
        let reader = FcbReader::open(std::io::BufReader::new(std::fs::File::open(path)?))?;
        FcbMetadata::from_header(reader.header())
    } else {
        let reader = tokio::time::timeout(
            std::time::Duration::from_secs(config.probe_timeout_seconds),
            HttpFcbReader::open(&config.fcb_url),
        )
        .await??;
        FcbMetadata::from_header(reader.header())
    }
}
async fn readiness(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> (axum::http::StatusCode, axum::Json<serde_json::Value>) {
    match load_fcb_metadata(&state.config).await {
        Ok(_) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::json!({"status":"ready"})),
        ),
        Err(e) => {
            tracing::warn!(error = %e, "dataset readiness failed");
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({"status":"unavailable"})),
            )
        }
    }
}
