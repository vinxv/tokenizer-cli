/// Handler for `GET /v1/models` — lists all tokenizers currently loaded in the registry.
use axum::{extract::State, Json};
use serde::Serialize;

use crate::{chat_template::ChatTemplateSource, error::AppResult, state::AppState};

#[derive(Debug, Serialize)]
pub struct ModelEntry {
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vocab_size: Option<usize>,
    pub backend: String,
    pub status: String,
    pub has_chat_template: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_source: Option<ChatTemplateSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub models: Vec<ModelEntry>,
}

/// `GET /v1/models`
pub async fn list_models(State(state): State<AppState>) -> AppResult<Json<ModelsResponse>> {
    let models = state
        .registry
        .list_models()
        .into_iter()
        .map(|info| ModelEntry {
            model_id: info.model_id,
            vocab_size: info.vocab_size,
            backend: info.backend,
            status: info.status,
            has_chat_template: info.has_chat_template,
            chat_template_source: info.chat_template_source,
            chat_template_name: info.chat_template_name,
        })
        .collect();

    Ok(Json(ModelsResponse { models }))
}
