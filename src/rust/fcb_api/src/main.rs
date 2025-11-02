use fcb_api::create_app;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing with better formatting for production
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fcb_api=info,fcb_core=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();

    let app = create_app().await;

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a valid u16");

    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    const ASCII_ART: &str = r#"
    ███████╗██╗      █████╗ ████████╗ ██████╗██╗████████╗██╗   ██╗██████╗ ██╗   ██╗███████╗     █████╗ ██████╗ ██╗
    ██╔════╝██║     ██╔══██╗╚══██╔══╝██╔════╝██║╚══██╔══╝╚██╗ ██╔╝██╔══██╗██║   ██║██╔════╝    ██╔══██╗██╔══██╗██║
    █████╗  ██║     ███████║   ██║   ██║     ██║   ██║    ╚████╔╝ ██████╔╝██║   ██║█████╗      ███████║██████╔╝██║
    ██╔══╝  ██║     ██╔══██║   ██║   ██║     ██║   ██║     ╚██╔╝  ██╔══██╗██║   ██║██╔══╝      ██╔══██║██╔═══╝ ██║
    ██║     ███████╗██║  ██║   ██║   ╚██████╗██║   ██║      ██║   ██████╔╝╚██████╔╝██║         ██║  ██║██║     ██║
    ╚═╝     ╚══════╝╚═╝  ╚═╝   ╚═╝    ╚═════╝╚═╝   ╚═╝      ╚═╝   ╚═════╝  ╚═════╝ ╚═╝         ╚═╝  ╚═╝╚═╝     ╚═╝
"#;

    println!("{}", ASCII_ART);
    tracing::info!("FCB API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
