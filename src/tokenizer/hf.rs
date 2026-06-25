/// HuggingFace tokenizer backend.
///
/// Combines `hf-hub` (file download + caching) with the `fastokens` crate.
/// `fastokens` 0.2.x supports BPE tokenizer.json files.
///
/// # Loading flow
/// 1. `hf-hub::api::tokio::ApiBuilder` is configured with an optional bearer
///    token and a custom cache directory.
/// 2. `repo.get("tokenizer.json")` either returns a cached path or streams
///    the file from the Hub and writes it to the local cache.
/// 3. `fastokens::Tokenizer::from_file(path)` parses the JSON and builds
///    the in-memory tokenizer.
/// 4. The resulting tokenizer is stored in an `Arc` for cheap, lock-free
///    sharing across concurrent requests.
use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use fastokens::Tokenizer;
use hf_hub::{api::tokio::ApiBuilder, Repo, RepoType};
use tracing::{debug, info, warn};

use crate::error::{AppError, AppResult};

use super::{TokenizerBackend, TokenizerBackendKind, TokenizerInfo};

// ── Known tokenizer files ─────────────────────────────────────────────────────

/// Files we attempt to download.  `required` = true means failure is fatal.
/// Exported so that other backends (e.g. ModelScope) can reuse the same list.
pub const TOKENIZER_FILES: &[(&str, bool)] = &[
    ("tokenizer.json", true),
    ("tokenizer_config.json", false),
    ("chat_template.jinja", false),
    ("special_tokens_map.json", false),
    ("vocab.json", false),
    ("merges.txt", false),
    ("vocab.txt", false),
];

// ── Pull (download only) ──────────────────────────────────────────────────────

/// Download all known tokenizer files for `model_id` into the cache directory.
///
/// Returns a list of `(filename, local_path)` for every file that was
/// successfully fetched. Skips optional files that the model does not have
/// (404 / 401 are treated as "not present" for optional files).
///
/// # Arguments
/// * `model_id`  — HuggingFace repo id, e.g. `"meta-llama/Meta-Llama-3-8B"`.
/// * `cache_dir` — Directory where downloaded files are stored.
/// * `hf_token`  — Optional bearer token for private / gated models.
pub async fn pull(
    model_id: &str,
    cache_dir: &str,
    hf_token: Option<&str>,
) -> AppResult<Vec<(String, PathBuf)>> {
    let mut builder = ApiBuilder::new();
    if !cache_dir.is_empty() {
        builder = builder.with_cache_dir(cache_dir.into());
    }
    if let Some(token) = hf_token {
        builder = builder.with_token(Some(token.to_string()));
    }

    let api = builder.build().map_err(|e| AppError::LoadError {
        model: model_id.to_string(),
        reason: format!("failed to build HF API client: {e}"),
    })?;

    let repo = api.repo(Repo::new(model_id.to_string(), RepoType::Model));

    let mut results: Vec<(String, PathBuf)> = Vec::new();

    for &(filename, required) in TOKENIZER_FILES {
        debug!("Fetching '{filename}' for '{model_id}'");
        match repo.get(filename).await {
            Ok(path) => {
                info!("Downloaded '{filename}' → {path:?}");
                results.push((filename.to_string(), path));
            }
            Err(e) => {
                let msg = e.to_string();
                let is_missing = msg.contains("401")
                    || msg.contains("404")
                    || msg.contains("Unauthorized")
                    || msg.contains("Not Found")
                    || msg.contains("not found")
                    || msg.contains("does not exist");

                if required {
                    return Err(if is_missing {
                        AppError::ModelNotFound(model_id.to_string())
                    } else {
                        AppError::LoadError {
                            model: model_id.to_string(),
                            reason: format!("failed to fetch {filename}: {e}"),
                        }
                    });
                } else {
                    warn!("Optional file '{filename}' not available for '{model_id}': {e}");
                }
            }
        }
    }

    Ok(results)
}

/// Concrete HuggingFace-backed tokenizer.
///
/// The inner `fastokens::Tokenizer` is shared behind an `Arc`.
pub struct HfTokenizer {
    model_id: String,
    inner: Arc<Tokenizer>,
    vocab_size: usize,
}

impl HfTokenizer {
    /// Load (or restore from local cache) the tokenizer for `model_id`.
    ///
    /// # Arguments
    /// * `model_id`  — HuggingFace repo id, e.g. `"Qwen/Qwen3-32B"`.
    /// * `cache_dir` — Directory where downloaded files are stored.
    ///   If empty the hf-hub default (`~/.cache/huggingface/hub`) is used.
    /// * `hf_token`  — Optional bearer token for private / gated models.
    pub async fn load(model_id: &str, cache_dir: &str, hf_token: Option<&str>) -> AppResult<Self> {
        debug!("Loading HF tokenizer for '{model_id}'");

        // Build the Hub API client
        let mut builder = ApiBuilder::new();

        if !cache_dir.is_empty() {
            builder = builder.with_cache_dir(cache_dir.into());
        }

        if let Some(token) = hf_token {
            builder = builder.with_token(Some(token.to_string()));
        }

        let api = builder.build().map_err(|e| AppError::LoadError {
            model: model_id.to_string(),
            reason: format!("failed to build HF API client: {e}"),
        })?;

        // Resolve the tokenizer.json path (downloads if not cached)
        let repo = api.repo(Repo::new(model_id.to_string(), RepoType::Model));

        let tokenizer_path = repo.get("tokenizer.json").await.map_err(|e| {
            let reason = e.to_string();
            // HuggingFace returns 401 for repos that don't exist (unauthenticated)
            // and 404 for repos that exist but the file is missing (authenticated).
            // Both cases mean "model not usable" from the caller's perspective.
            if reason.contains("401")
                || reason.contains("Unauthorized")
                || reason.contains("404")
                || reason.contains("not found")
                || reason.contains("Not Found")
                || reason.contains("does not exist")
                || reason.contains("404 Not Found")
            {
                AppError::ModelNotFound(model_id.to_string())
            } else {
                AppError::LoadError {
                    model: model_id.to_string(),
                    reason: format!("failed to fetch tokenizer.json from Hub: {e}"),
                }
            }
        })?;

        debug!(
            "tokenizer.json for '{model_id}' resolved to {:?}",
            tokenizer_path
        );

        // Load using fastokens (blocking I/O — offload to threadpool)
        let model_id_owned = model_id.to_string();
        let tokenizer = tokio::task::spawn_blocking(move || {
            Tokenizer::from_file(&tokenizer_path).map_err(|e| AppError::LoadError {
                model: model_id_owned,
                reason: format!("failed to parse tokenizer.json: {e}"),
            })
        })
        .await
        .map_err(|e| AppError::LoadError {
            model: model_id.to_string(),
            reason: format!("join error during tokenizer load: {e}"),
        })??;

        let vocab_size = tokenizer.vocab_size();

        info!("Loaded HF tokenizer for '{model_id}' (vocab_size={vocab_size})");

        Ok(Self {
            model_id: model_id.to_string(),
            inner: Arc::new(tokenizer),
            vocab_size,
        })
    }

    /// Parse a `tokenizer.json` already present at `tokenizer_path` on disk and
    /// build an `HfTokenizer` without touching the network.
    ///
    /// Both `HfTokenizer::load()` (HF path) and the ModelScope backend call this
    /// after obtaining the local file path by their respective means.
    pub fn from_path(model_id: &str, tokenizer_path: &std::path::Path) -> AppResult<Self> {
        let model_id_owned = model_id.to_string();
        let path = tokenizer_path.to_path_buf();

        // Tokenizer::from_file does blocking I/O — run in the threadpool.
        // NOTE: This is a *sync* fn; callers inside async contexts must wrap it
        // themselves (see HfTokenizer::load for the tokio::task::spawn_blocking
        // pattern).  We keep it sync here so ModelScope can call it without
        // nesting spawn_blocking inside spawn_blocking.
        let tokenizer = Tokenizer::from_file(&path).map_err(|e| AppError::LoadError {
            model: model_id_owned.clone(),
            reason: format!("failed to parse tokenizer.json: {e}"),
        })?;

        let vocab_size = tokenizer.vocab_size();
        info!("Parsed tokenizer for '{model_id}' from {path:?} (vocab_size={vocab_size})");

        Ok(Self {
            model_id: model_id_owned,
            inner: Arc::new(tokenizer),
            vocab_size,
        })
    }
}

#[async_trait]
impl TokenizerBackend for HfTokenizer {
    fn info(&self) -> TokenizerInfo {
        TokenizerInfo {
            model_id: self.model_id.clone(),
            vocab_size: self.vocab_size,
            backend: TokenizerBackendKind::HuggingFace,
        }
    }

    async fn encode(&self, text: &str) -> AppResult<Vec<u32>> {
        let inner = self.inner.clone();
        let text = text.to_string();
        let model_id = self.model_id.clone();

        tokio::task::spawn_blocking(move || {
            inner
                .encode_with_special_tokens(&text, true)
                .map_err(|e| AppError::TokenizerError(format!("[{model_id}] encode failed: {e}")))
        })
        .await
        .map_err(|e| AppError::TokenizerError(format!("join error in encode: {e}")))?
    }

    async fn decode(&self, ids: &[u32]) -> AppResult<String> {
        let inner = self.inner.clone();
        let ids = ids.to_vec();
        let model_id = self.model_id.clone();

        tokio::task::spawn_blocking(move || {
            inner
                .decode(&ids, true)
                .map_err(|e| AppError::TokenizerError(format!("[{model_id}] decode failed: {e}")))
        })
        .await
        .map_err(|e| AppError::TokenizerError(format!("join error in decode: {e}")))?
    }

    async fn token_texts(&self, ids: &[u32]) -> AppResult<Vec<String>> {
        let inner = self.inner.clone();
        let ids = ids.to_vec();
        let model_id = self.model_id.clone();

        tokio::task::spawn_blocking(move || {
            let singletons: Vec<[u32; 1]> = ids.into_iter().map(|id| [id]).collect();
            let slices: Vec<&[u32]> = singletons.iter().map(|ids| ids.as_slice()).collect();
            inner.decode_batch(&slices, false).map_err(|e| {
                AppError::TokenizerError(format!("[{model_id}] token decode failed: {e}"))
            })
        })
        .await
        .map_err(|e| AppError::TokenizerError(format!("join error in token decode: {e}")))?
    }
}
