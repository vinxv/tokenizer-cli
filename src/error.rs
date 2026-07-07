use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Unified error type for the tokenizer service.
#[derive(Debug, Error)]
pub enum AppError {
    /// The requested model / tokenizer ID was not found and could not be loaded.
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// The tokenizer failed to load (bad vocab file, network error, etc.).
    #[error("Failed to load tokenizer for '{model}': {reason}")]
    LoadError { model: String, reason: String },

    /// Encode / decode operation failed.
    #[error("Tokenizer encode/decode error: {0}")]
    TokenizerError(String),

    /// Input validation failure (e.g. empty text, bad token ids).
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::ModelNotFound(_) => {
                (StatusCode::NOT_FOUND, "MODEL_NOT_FOUND", self.to_string())
            }
            AppError::LoadError { .. } => (StatusCode::BAD_GATEWAY, "LOAD_ERROR", self.to_string()),
            AppError::InvalidInput(_) => {
                (StatusCode::BAD_REQUEST, "INVALID_INPUT", self.to_string())
            }
            AppError::TokenizerError(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "TOKENIZER_ERROR",
                self.to_string(),
            ),
        };

        let body = Json(json!({
            "error": {
                "code": code,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
