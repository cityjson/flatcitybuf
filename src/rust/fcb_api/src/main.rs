mod handlers;
mod models;

use axum::{routing::get, Router};
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub fcb_url: String,
    pub max_return_features: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let fcb_url = env::var("FCB_URL").unwrap_or_else(|_| {
        "https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb".to_string()
    });

    let max_return_features = env::var("MAX_RETURN_FEATURES")
        .unwrap_or_else(|_| "100".to_string())
        .parse::<u32>()
        .unwrap_or(100);

    let state = Arc::new(AppState {
        fcb_url,
        max_return_features,
    });

    tracing::info!("FCB URL: {}", state.fcb_url);
    tracing::info!("Max return features: {}", state.max_return_features);

    let app = Router::new()
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
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("FCB API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
