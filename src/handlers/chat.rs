use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    chat_template::{render_chat_template, ChatTemplateSource},
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ChatRenderRequest {
    /// HuggingFace / ModelScope model id or OpenAI alias.
    pub model: String,
    /// OpenAI Chat Completions-style messages.
    pub messages: Vec<Value>,
    /// Whether to append the assistant generation marker when the template
    /// supports it. Defaults to true for Chat Completions prompts.
    #[serde(default)]
    pub add_generation_prompt: Option<bool>,
    /// Extra request fields are passed through to the Jinja context.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct ChatRenderResponse {
    pub model: String,
    pub text: String,
    pub count: usize,
    pub template_source: ChatTemplateSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_name: Option<String>,
}

/// `POST /v1/chat/render`
pub async fn render(
    State(state): State<AppState>,
    Json(req): Json<ChatRenderRequest>,
) -> AppResult<Json<ChatRenderResponse>> {
    validate_messages(&req.messages)?;

    let resolved = state
        .registry
        .chat_template_for_model(&req.model)
        .await?
        .ok_or_else(|| {
            AppError::InvalidInput(format!(
                "no chat template found for '{}': cache tokenizer_config.json with chat_template or a chat_template.jinja file",
                req.model
            ))
        })?;

    let text = render_chat_template(
        &resolved.template,
        req.messages,
        req.add_generation_prompt.unwrap_or(true),
        resolved.defaults,
        req.extra,
    )?;

    let tok = state.registry.get_or_load(&req.model).await?;
    let count = tok.count(&text).await?;

    Ok(Json(ChatRenderResponse {
        model: req.model,
        text,
        count,
        template_source: resolved.source,
        template_name: resolved.source_name,
    }))
}

fn validate_messages(messages: &[Value]) -> AppResult<()> {
    if messages.is_empty() {
        return Err(AppError::InvalidInput(
            "messages must contain at least one item".to_string(),
        ));
    }

    for (idx, message) in messages.iter().enumerate() {
        if !message.is_object() {
            return Err(AppError::InvalidInput(format!(
                "messages[{idx}] must be an object"
            )));
        }
    }

    Ok(())
}
