use axum::{routing::get, Router};

async fn health() -> &'static str {
    "healthy"
}

pub fn app_router() -> Router {
    Router::new().route("/health", get(health))
}