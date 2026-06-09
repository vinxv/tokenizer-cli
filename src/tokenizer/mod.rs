use async_trait::async_trait;

use crate::error::AppResult;

/// Canonical information about a loaded tokenizer.
#[derive(Debug, Clone)]
pub struct TokenizerInfo {
    /// Canonical model identifier (e.g. `"meta-llama/Meta-Llama-3-8B"`)
    pub model_id: String,
    /// Vocabulary size
    pub vocab_size: usize,
    /// Short description of the backend being used
    pub backend: TokenizerBackendKind,
}

/// Discriminates which backend is handling a given model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenizerBackendKind {
    /// HuggingFace / ModelScope BPE tokenizer loaded through `fastokens`
    HuggingFace,
    /// OpenAI `tiktoken` (cl100k_base, o200k_base, …)
    Tiktoken,
}

impl std::fmt::Display for TokenizerBackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizerBackendKind::HuggingFace => write!(f, "huggingface"),
            TokenizerBackendKind::Tiktoken => write!(f, "tiktoken"),
        }
    }
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

    /// Counts tokens in `text`. Default implementation wraps `encode`.
    async fn count(&self, text: &str) -> AppResult<usize> {
        Ok(self.encode(text).await?.len())
    }
}

// ----- Sub-modules -----------------------------------------------------------

pub mod hf;
pub mod ms;
pub mod openai;
pub mod registry;
