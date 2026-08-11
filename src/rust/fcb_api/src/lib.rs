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

pub async fn create_app() -> Router {
    let fcb_url = env::var("FCB_URL")
        .unwrap_or_else(|_| "https://flatcitybuf.open3d.city/data/3dbag_all_index.fcb".to_string());

    let base_url = env::var("BASE_URL").unwrap_or_else(|_| "https://api.3dbag.nl".to_string());

    let max_return_features = env::var("MAX_RETURN_FEATURES")
        .unwrap_or_else(|_| "100".to_string())
        .parse::<u32>()
        .unwrap_or(100);

    // Determine if the URL is local or remote
    let is_local = is_local_file(&fcb_url);

    // Load FCB metadata
    let fcb_metadata = load_fcb_metadata(&fcb_url, is_local).await;

    let state = Arc::new(AppState {
        fcb_url: fcb_url.clone(),
        max_return_features,
        base_url,
        fcb_metadata,
        is_local_file: is_local,
    });

    tracing::info!("FCB URL: {}", state.fcb_url);
    tracing::info!("Is local file: {}", state.is_local_file);
    tracing::info!("Max return features: {}", state.max_return_features);
    tracing::info!(
        "Loaded FCB metadata with {} columns",
        state.fcb_metadata.columns.len()
    );

    // Route to appropriate handlers based on URL type
    if is_local {
        Router::new()
            .route("/", get(handlers::landing_page))
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
    }
}

async fn load_fcb_metadata(fcb_url: &str, is_local: bool) -> FcbMetadata {
    use fcb_core::{FcbReader, HttpFcbReader};
    use metadata::ColumnMetadata;
    use std::fs::File;
    use std::io::BufReader;

    let mut metadata = FcbMetadata::new();

    if is_local {
        // Load metadata from local file
        match File::open(fcb_url) {
            Ok(file) => {
                let buf_reader = BufReader::new(file);
                match FcbReader::open(buf_reader) {
                    Ok(reader) => {
                        let header = reader.header();

                        // Get columns information
                        if let Some(columns) = header.columns() {
                            // Get attribute index information
                            let attr_indices = header.attribute_index();
                            let indexed_columns: std::collections::HashSet<u16> =
                                if let Some(indices) = attr_indices {
                                    indices.iter().map(|idx| idx.index()).collect()
                                } else {
                                    std::collections::HashSet::new()
                                };

                            // Build the metadata map
                            for column in columns.iter() {
                                let col_meta = ColumnMetadata {
                                    index: column.index(),
                                    column_type: column.type_(),
                                    is_indexed: indexed_columns.contains(&column.index()),
                                };
                                metadata.columns.insert(column.name().to_string(), col_meta);
                            }
                        }

                        tracing::info!(
                            "Successfully loaded FCB metadata from local file: {}",
                            fcb_url
                        );
                    }
                    Err(e) => {
                        tracing::error!("Failed to open local FCB reader from {}: {}", fcb_url, e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to open local FCB file {}: {}", fcb_url, e);
            }
        }
    } else {
        // Load metadata from HTTP URL
        match HttpFcbReader::open(fcb_url).await {
            Ok(http_reader) => {
                // Get the header information
                let header = http_reader.header();

                // Get columns information
                if let Some(columns) = header.columns() {
                    // Get attribute index information
                    let attr_indices = header.attribute_index();
                    let indexed_columns: std::collections::HashSet<u16> =
                        if let Some(indices) = attr_indices {
                            indices.iter().map(|idx| idx.index()).collect()
                        } else {
                            std::collections::HashSet::new()
                        };

                    // Build the metadata map
                    for column in columns.iter() {
                        let col_meta = ColumnMetadata {
                            index: column.index(),
                            column_type: column.type_(),
                            is_indexed: indexed_columns.contains(&column.index()),
                        };
                        metadata.columns.insert(column.name().to_string(), col_meta);
                    }
                }

                tracing::info!(
                    "Successfully loaded FCB metadata from HTTP URL: {}",
                    fcb_url
                );
            }
            Err(e) => {
                tracing::error!("Failed to load FCB metadata from {}: {}", fcb_url, e);
                // Continue with empty metadata - filters will fail gracefully
            }
        }
    }

    metadata
}
