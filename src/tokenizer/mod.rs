use async_trait::async_trait;

use crate::error::AppResult;

/// Canonical information about a loaded tokenizer.
#[derive(Debug, Clone)]
pub struct TokenizerInfo {
    /// Canonical model identifier (e.g. `"meta-llama/Meta-Llama-3-8B"`)
    pub model_id: String,
    /// Vocabulary size
    pub vocab_size: usize,
}

/// The core trait every tokenizer backend must implement.
///
/// All methods take `&self` — implementations must be internally thread-safe
/// (e.g. wrap mutable state in `Arc<RwLock<…>>`).
#[async_trait]
pub trait TokenizerBackend: Send + Sync {
    /// Returns metadata about the tokenizer.
    fn info(&self) -> TokenizerInfo;

    /// Encodes `text` into a sequence of token ids.
    async fn encode(&self, text: &str) -> AppResult<Vec<u32>>;

    /// Decodes a sequence of token ids back to a string.
    async fn decode(&self, ids: &[u32]) -> AppResult<String>;

    /// Decodes each token id independently for token-list inspection.
    async fn token_texts(&self, ids: &[u32]) -> AppResult<Vec<String>> {
        let mut tokens = Vec::with_capacity(ids.len());
        for id in ids {
            tokens.push(self.decode(&[*id]).await?);
        }
        Ok(tokens)
    }

    /// Counts tokens in `text`. Default implementation wraps `encode`.
    async fn count(&self, text: &str) -> AppResult<usize> {
        Ok(self.encode(text).await?.len())
    }
}

// ----- Sub-modules -----------------------------------------------------------

pub mod hf;
pub mod ms;
pub mod registry;
