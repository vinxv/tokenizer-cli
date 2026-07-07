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
        router = router
            .route(
                "/",
                get(|| async { axum::response::Html(include_str!("web_ui.html")) }),
            )
            // ── Embedded fonts ──────────────────────────────────────────────
            // Bundled into the binary via include_bytes! so the dashboard is
            // fully self-contained — no requests to Google Fonts or any CDN,
            // which matters for air-gapped / internal deployments.
            .route(
                "/fonts/ibm-plex-sans-300.woff2",
                get(|| async {
                    font_response(include_bytes!("fonts/IBMPlexSans-300.woff2").as_slice())
                }),
            )
            .route(
                "/fonts/ibm-plex-sans-400.woff2",
                get(|| async {
                    font_response(include_bytes!("fonts/IBMPlexSans-400.woff2").as_slice())
                }),
            )
            .route(
                "/fonts/ibm-plex-sans-500.woff2",
                get(|| async {
                    font_response(include_bytes!("fonts/IBMPlexSans-500.woff2").as_slice())
                }),
            )
            .route(
                "/fonts/ibm-plex-mono-400.woff2",
                get(|| async {
                    font_response(include_bytes!("fonts/IBMPlexMono-400.woff2").as_slice())
                }),
            )
            .route(
                "/fonts/ibm-plex-mono-500.woff2",
                get(|| async {
                    font_response(include_bytes!("fonts/IBMPlexMono-500.woff2").as_slice())
                }),
            );
    }

    router
}

/// Return an embedded woff2 font with the correct content-type.
fn font_response(data: &'static [u8]) -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("font/woff2"),
        )],
        data,
    )
}

async fn ping() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}
