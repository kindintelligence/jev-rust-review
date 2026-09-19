use crate::auth::require_admin;
use axum::middleware::from_fn;
use axum::routing::get;
use axum::Router;

async fn list_users() -> &'static str {
    "alice,bob"
}

async fn health() -> &'static str {
    "ok"
}

pub fn app() -> Router {
    Router::new()
        .route("/admin/users", get(list_users))
        .route_layer(from_fn(require_admin))
        .route("/health", get(health))
}
