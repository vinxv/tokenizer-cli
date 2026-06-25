use axum::{
    extract::Request,
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::{handlers, state::AppState};

async fn access_log(req: Request, next: Next) -> impl IntoResponse {
    let method = req.method().clone();
    let uri = req.uri().clone();

    let start = Instant::now();
    let res = next.run(req).await;
    let latency = start.elapsed();

    info!(
        method = %method,
        uri = %uri,
        status = res.status().as_u16(),
        latency_ms = latency.as_millis(),
        "{} {} {} {:?}",
        method,
        uri,
        res.status().as_u16(),
        latency
    );

    res
}

/// Assemble the full Axum router with all routes and middleware.
pub fn build_router(state: AppState, enable_web: bool) -> Router {
    let mut router = Router::new()
        // ── Health ──────────────────────────────────────────────────────────
        .route("/ping", get(ping))
        // ── Token operations ────────────────────────────────────────────────
        .route("/v1/token/count", post(handlers::token::count))
        .route("/v1/token/encode", post(handlers::token::encode))
        .route("/v1/token/decode", post(handlers::token::decode))
        // ── Chat template rendering ─────────────────────────────────────────
        .route("/v1/chat/render", post(handlers::chat::render))
        // ── Model listing ───────────────────────────────────────────────────
        .route("/v1/models", get(handlers::models::list_models))
        // ── Middleware ──────────────────────────────────────────────────────
        .layer(middleware::from_fn(access_log))
        .layer(CorsLayer::permissive())
        // ── State ───────────────────────────────────────────────────────────
        .with_state(state);

    if enable_web {
        router = router.route(
            "/",
            get(|| async { axum::response::Html(include_str!("web_ui.html")) }),
        );
    }

    router
}

async fn ping() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}
