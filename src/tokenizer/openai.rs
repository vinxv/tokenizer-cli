/// OpenAI / tiktoken tokenizer backend.
///
/// Wraps `tiktoken-rs` to support OpenAI's built-in encodings:
/// `cl100k_base` (GPT-4 / GPT-3.5-turbo) and `o200k_base` (GPT-4o).
///
/// # TODO (Phase 2)
/// - Map common model aliases (`gpt-4`, `gpt-4o`, `gpt-3.5-turbo`, …) to their
///   corresponding encoding name.
/// - Implement `TokenizerBackend` using `tiktoken_rs::get_bpe_from_model`.
use async_trait::async_trait;

use crate::error::{AppError, AppResult};

use super::{TokenizerBackend, TokenizerBackendKind, TokenizerInfo};

/// A model alias → tiktoken encoding name mapping.
/// E.g. `"gpt-4o"` → `"o200k_base"`.
pub fn encoding_for_model(model_id: &str) -> Option<&'static str> {
    match model_id {
        "gpt-4o" | "gpt-4o-mini" => Some("o200k_base"),
        "gpt-4" | "gpt-4-turbo" | "gpt-3.5-turbo" => Some("cl100k_base"),
        "text-davinci-003" | "text-davinci-002" => Some("p50k_base"),
        // Raw encoding names are accepted as-is
        "o200k_base" => Some("o200k_base"),
        "cl100k_base" => Some("cl100k_base"),
        "p50k_base" => Some("p50k_base"),
        "r50k_base" => Some("r50k_base"),
        _ => None,
    }
}

/// Concrete OpenAI-tiktoken-backed tokenizer.
pub struct OpenAiTokenizer {
    model_id: String,
    encoding_name: &'static str,
    // TODO: replace with real tiktoken_rs::CoreBPE
    _inner: (),
}

impl OpenAiTokenizer {
    /// Load the tiktoken BPE for `model_id`.
    /// Returns `None` if the model is not a recognised OpenAI model.
    pub fn load(model_id: &str) -> AppResult<Self> {
        let encoding_name = encoding_for_model(model_id).ok_or_else(|| {
            AppError::ModelNotFound(format!("'{}' is not a known OpenAI model", model_id))
        })?;

        // TODO: let inner = tiktoken_rs::get_bpe_from_model(encoding_name)?;
        Ok(Self {
            model_id: model_id.to_string(),
            encoding_name,
            _inner: (),
        })
    }
}

#[async_trait]
impl TokenizerBackend for OpenAiTokenizer {
    fn info(&self) -> TokenizerInfo {
        TokenizerInfo {
            model_id: self.model_id.clone(),
            // o200k_base: 200 019,  cl100k_base: 100 277
            vocab_size: if self.encoding_name == "o200k_base" {
                200_019
            } else {
                100_277
            },
            backend: TokenizerBackendKind::Tiktoken,
        }
    }

    async fn encode(&self, _text: &str) -> AppResult<Vec<u32>> {
        // TODO: self._inner.encode_with_special_tokens(text)
        Err(AppError::TokenizerError(
            "OpenAiTokenizer::encode not implemented".to_string(),
        ))
    }

    async fn decode(&self, _ids: &[u32]) -> AppResult<String> {
        // TODO: self._inner.decode(ids.iter().map(|&i| i as usize).collect())
        Err(AppError::TokenizerError(
            "OpenAiTokenizer::decode not implemented".to_string(),
        ))
    }
}
