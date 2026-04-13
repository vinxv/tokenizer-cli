/// Handlers for `/v1/token/*` endpoints.
///
/// All three operations (count / encode / decode) share the same request
/// schema shape – only the response differs.
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{error::AppResult, state::AppState};

// ── Request / Response types ──────────────────────────────────────────────────

/// Input for `/v1/token/count` and `/v1/token/encode`.
#[derive(Debug, Deserialize)]
pub struct TextRequest {
    /// HuggingFace model id or OpenAI model alias, e.g. `"gpt-4o"`.
    pub model: String,
    /// The text to tokenize.
    pub text: String,
}

/// Input for `/v1/token/decode`.
#[derive(Debug, Deserialize)]
pub struct DecodeRequest {
    /// HuggingFace model id or OpenAI model alias.
    pub model: String,
    /// Token ids to decode.
    pub ids: Vec<u32>,
}

/// Response for `/v1/token/count`.
#[derive(Debug, Serialize)]
pub struct CountResponse {
    pub model: String,
    pub count: usize,
}

/// Response for `/v1/token/encode`.
#[derive(Debug, Serialize)]
pub struct EncodeResponse {
    pub model: String,
    pub ids: Vec<u32>,
}

/// Response for `/v1/token/decode`.
#[derive(Debug, Serialize)]
pub struct DecodeResponse {
    pub model: String,
    pub text: String,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /v1/token/count`
pub async fn count(
    State(state): State<AppState>,
    Json(req): Json<TextRequest>,
) -> AppResult<Json<CountResponse>> {
    let tok = state.registry.get_or_load(&req.model).await?;
    let count = tok.count(&req.text).await?;
    Ok(Json(CountResponse {
        model: req.model,
        count,
    }))
}

/// `POST /v1/token/encode`
pub async fn encode(
    State(state): State<AppState>,
    Json(req): Json<TextRequest>,
) -> AppResult<Json<EncodeResponse>> {
    let tok = state.registry.get_or_load(&req.model).await?;
    let ids = tok.encode(&req.text).await?;
    Ok(Json(EncodeResponse {
        model: req.model,
        ids,
    }))
}

/// `POST /v1/token/decode`
pub async fn decode(
    State(state): State<AppState>,
    Json(req): Json<DecodeRequest>,
) -> AppResult<Json<DecodeResponse>> {
    let tok = state.registry.get_or_load(&req.model).await?;
    let text = tok.decode(&req.ids).await?;
    Ok(Json(DecodeResponse {
        model: req.model,
        text,
    }))
}
