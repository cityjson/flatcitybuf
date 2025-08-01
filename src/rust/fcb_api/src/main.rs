use fcb_api::create_app;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let app = create_app().await;

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("FCB API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
