use kosync_server::{create_router, AppState, Database};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_path = std::env::var("KOSYNC_DB_PATH").unwrap_or_else(|_| "kosync.db".into());
    let db = Database::open(&db_path)?;
    let state = AppState { db: Arc::new(db) };

    let app = create_router(state);

    let port = std::env::var("KOSYNC_PORT").unwrap_or_else(|_| "7200".into());
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
