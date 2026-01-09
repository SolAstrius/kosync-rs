pub mod db;
pub mod error;
pub mod handlers;
pub mod models;

use axum::{
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub use db::Database;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Legacy KOSync API (v1)
        .route("/users/create", post(handlers::create_user))
        .route("/users/auth", get(handlers::auth_user))
        .route("/syncs/progress", put(handlers::update_progress))
        .route("/syncs/progress/{document}", get(handlers::get_progress))
        // Extended API (v2) - annotations
        .route("/syncs/annotations/{document}", get(handlers::get_annotations))
        .route("/syncs/annotations/{document}", put(handlers::update_annotations))
        // Health check
        .route("/healthcheck", get(handlers::healthcheck))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
