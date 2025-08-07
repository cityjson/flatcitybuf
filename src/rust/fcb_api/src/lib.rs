pub mod constants;
pub mod filter_parser;
pub mod handlers;
pub mod metadata;
pub mod models;

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
}

pub async fn create_app() -> Router {
    let fcb_url = env::var("FCB_URL").unwrap_or_else(|_| {
        "https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb".to_string()
    });

    let base_url = env::var("BASE_URL").unwrap_or_else(|_| "https://api.3dbag.nl".to_string());

    let max_return_features = env::var("MAX_RETURN_FEATURES")
        .unwrap_or_else(|_| "100".to_string())
        .parse::<u32>()
        .unwrap_or(100);

    // Load FCB metadata
    let fcb_metadata = load_fcb_metadata(&fcb_url).await;

    let state = Arc::new(AppState {
        fcb_url,
        max_return_features,
        base_url,
        fcb_metadata,
    });

    tracing::info!("FCB URL: {}", state.fcb_url);
    tracing::info!("Max return features: {}", state.max_return_features);
    tracing::info!(
        "Loaded FCB metadata with {} columns",
        state.fcb_metadata.columns.len()
    );

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
            get(handlers::collection_items),
        )
        .route(
            "/collections/:collection_id/items/:item_id",
            get(handlers::collection_item_by_id),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn load_fcb_metadata(fcb_url: &str) -> FcbMetadata {
    use fcb_core::HttpFcbReader;
    use metadata::ColumnMetadata;

    let mut metadata = FcbMetadata::new();

    // Try to load metadata from the FCB file
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

            tracing::info!("Successfully loaded FCB metadata from {}", fcb_url);
        }
        Err(e) => {
            tracing::error!("Failed to load FCB metadata from {}: {}", fcb_url, e);
            // Continue with empty metadata - filters will fail gracefully
        }
    }

    metadata
}
