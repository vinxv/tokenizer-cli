/// Tokenizer registry — central cache mapping model IDs to loaded backends.
///
/// # Concurrency
/// The registry uses a two-level locking strategy:
/// 1. **`DashMap`** — fast, concurrent reads after a model is loaded.
/// 2. **`Mutex<HashMap<model_id, Arc<OnceCell>>>`** — ensures only **one**
///    concurrent load task is ever spawned per model id. All other callers that
///    race to load the same model wait on the same `OnceCell` future.
///
/// This prevents duplicate concurrent downloads of the same model (which can
/// cause file-locking races inside `hf-hub`).
use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;
use tokio::sync::{Mutex, OnceCell};
use tracing::{info, warn};

use crate::{
    chat_template::{find_cached_template, ChatTemplateSource, ResolvedChatTemplate},
    config::{Config, ModelSource},
    error::{AppError, AppResult},
};

use super::{
    hf::{self, HfTokenizer},
    ms, TokenizerBackend, TokenizerBackendKind,
};

/// Shared, lazily-initialised slot for one model's load task.
type LoadSlot = Arc<OnceCell<Arc<dyn TokenizerBackend>>>;
type ChatTemplateCacheEntry = (ChatTemplateSource, Option<String>);

pub struct RegistryModelInfo {
    pub model_id: String,
    pub backend: String,
    pub vocab_size: Option<usize>,
    pub status: String,
    pub source: String,
    pub has_chat_template: bool,
    pub chat_template_source: Option<ChatTemplateSource>,
    pub chat_template_name: Option<String>,
}

/// Thread-safe, shared tokenizer registry.
#[derive(Clone)]
pub struct Registry {
    /// Fully-loaded tokenizers (fast read path).
    loaded: Arc<DashMap<String, Arc<dyn TokenizerBackend>>>,

    /// Pre-scanned models found in the cache directory (fast read path).
    scanned: Arc<DashMap<String, (TokenizerBackendKind, String)>>,

    /// Inflight / pending load slots — prevents duplicate concurrent downloads.
    inflight: Arc<Mutex<HashMap<String, LoadSlot>>>,

    /// Cached chat-template availability discovered from local model files.
    chat_templates: Arc<DashMap<String, ChatTemplateCacheEntry>>,

    cache_dir: String,
    hf_token: Option<String>,
    allow_online: bool,
    model_source: ModelSource,
    ms_token: Option<String>,
}

impl Registry {
    /// Create an empty registry from application config.
    pub fn new(cfg: &Config) -> Self {
        let registry = Self {
            loaded: Arc::new(DashMap::new()),
            scanned: Arc::new(DashMap::new()),
            inflight: Arc::new(Mutex::new(HashMap::new())),
            chat_templates: Arc::new(DashMap::new()),
            cache_dir: cfg.cache_dir.clone(),
            hf_token: cfg.hf_token.clone(),
            allow_online: cfg.allow_online_download,
            model_source: cfg.model_source.clone(),
            ms_token: cfg.ms_token.clone(),
        };
        registry.scan_cache_dir();
        registry
    }

    /// Scans the local cache directory to discover Models without loading them
    fn scan_cache_dir(&self) {
        let path = std::path::Path::new(&self.cache_dir);
        if !path.exists() {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if let Some(rest) = name.strip_prefix("models--") {
                            let model_id = rest.replace("--", "/");
                            self.scanned.insert(
                                model_id.clone(),
                                (TokenizerBackendKind::HuggingFace, "huggingface".to_string()),
                            );
                            self.refresh_cached_chat_template(&model_id);
                            tracing::info!("Scanned model from cache: '{model_id}'");
                        } else if let Some(rest) = name.strip_prefix("ms--") {
                            let model_id = if let Some((org, repo)) = rest.split_once("--") {
                                format!("{org}/{repo}")
                            } else {
                                rest.replace("--", "/")
                            };
                            self.scanned.insert(
                                model_id.clone(),
                                (TokenizerBackendKind::HuggingFace, "modelscope".to_string()),
                            );
                            self.refresh_cached_chat_template(&model_id);
                            tracing::info!("Scanned ModelScope model from cache: '{model_id}'");
                        }
                    }
                }
            }
        }
    }

    /// Returns metadata for every known tokenizer (either scanned or fully loaded).
    pub fn list_models(&self) -> Vec<RegistryModelInfo> {
        let mut results: HashMap<String, RegistryModelInfo> = HashMap::new();

        // Add scanned ones first
        for entry in self.scanned.iter() {
            let (backend, source) = entry.value();
            results.insert(
                entry.key().clone(),
                RegistryModelInfo {
                    model_id: entry.key().clone(),
                    backend: backend.to_string(),
                    vocab_size: None,
                    status: "scanned".to_string(),
                    source: source.clone(),
                    has_chat_template: false,
                    chat_template_source: None,
                    chat_template_name: None,
                },
            );
        }

        // Override with fully loaded ones
        for entry in self.loaded.iter() {
            let info = entry.value().info();
            let source = self
                .scanned
                .get(info.model_id.as_str())
                .map(|s| s.value().1.clone())
                .unwrap_or_else(|| match self.model_source {
                    ModelSource::HuggingFace => "huggingface".to_string(),
                    ModelSource::ModelScope => "modelscope".to_string(),
                });

            results.insert(
                info.model_id.clone(),
                RegistryModelInfo {
                    model_id: info.model_id.clone(),
                    backend: info.backend.to_string(),
                    vocab_size: Some(info.vocab_size),
                    status: "loaded".to_string(),
                    source,
                    has_chat_template: false,
                    chat_template_source: None,
                    chat_template_name: None,
                },
            );
        }

        let mut list: Vec<_> = results.into_values().collect();
        for info in &mut list {
            if let Some(template) = self.chat_templates.get(&info.model_id) {
                let (source, name) = template.value().clone();
                info.has_chat_template = true;
                info.chat_template_source = Some(source);
                info.chat_template_name = name;
            }
        }
        list.sort_by(|a, b| a.model_id.cmp(&b.model_id));
        list
    }

    /// Resolve a model's chat template from cached or downloadable
    /// `tokenizer_config.json`.
    pub async fn chat_template_for_model(
        &self,
        model_id: &str,
    ) -> AppResult<Option<ResolvedChatTemplate>> {
        if let Some(template) = find_cached_template(&self.cache_dir, model_id)? {
            self.chat_templates.insert(
                model_id.to_string(),
                (template.source, template.source_name.clone()),
            );
            return Ok(Some(template));
        }

        if !self.allow_online {
            return Ok(None);
        }

        let model_source = self.source_for_model(model_id);
        match model_source {
            ModelSource::HuggingFace => {
                hf::pull(model_id, &self.cache_dir, self.hf_token.as_deref()).await?
            }
            ModelSource::ModelScope => {
                ms::pull(model_id, &self.cache_dir, self.ms_token.as_deref()).await?
            }
        };

        let template = find_cached_template(&self.cache_dir, model_id)?;
        if let Some(template) = &template {
            self.chat_templates.insert(
                model_id.to_string(),
                (template.source, template.source_name.clone()),
            );
        }
        Ok(template)
    }

    /// Retrieve a tokenizer by model id, loading it if necessary.
    ///
    /// Guarantees that at most **one** network download is performed per model
    /// id, even when many concurrent requests arrive simultaneously.
    pub async fn get_or_load(&self, model_id: &str) -> AppResult<Arc<dyn TokenizerBackend>> {
        // ── 1. Fast path: already fully loaded ──────────────────────────────
        if let Some(entry) = self.loaded.get(model_id) {
            return Ok(entry.clone());
        }

        if !self.allow_online && !self.scanned.contains_key(model_id) {
            warn!("Model '{model_id}' not cached and online download is disabled");
            return Err(AppError::ModelNotFound(model_id.to_string()));
        }

        // ── 2. Slow path: get (or create) the inflight slot ─────────────────
        let slot: LoadSlot = {
            let mut map = self.inflight.lock().await;
            map.entry(model_id.to_string())
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone()
        };

        // ── 3. Execute load exactly once per slot ────────────────────────────
        let cache_dir = self.cache_dir.clone();
        let hf_token = self.hf_token.clone();
        let ms_token = self.ms_token.clone();

        let model_source = self.source_for_model(model_id);

        // Capture a label for the log line *before* the closure moves model_source.
        let source_label = format!("{model_source:?}");
        let model_id_owned = model_id.to_string();

        slot.get_or_try_init(|| async move {
            let tok: Arc<dyn TokenizerBackend> = match model_source {
                ModelSource::HuggingFace => {
                    let t =
                        HfTokenizer::load(&model_id_owned, &cache_dir, hf_token.as_deref()).await?;
                    Arc::new(t)
                }
                ModelSource::ModelScope => {
                    let t = ms::load(&model_id_owned, &cache_dir, ms_token.as_deref()).await?;
                    Arc::new(t)
                }
            };
            Ok::<_, AppError>(tok)
        })
        .await?;

        // ── 4. Promote into the fast-path cache and return ───────────────────
        let arc = slot
            .get()
            .expect("OnceCell must be set after get_or_try_init")
            .clone();
        self.loaded.insert(model_id.to_string(), arc.clone());
        self.refresh_cached_chat_template(model_id);
        info!("Cached tokenizer for '{model_id}' (source={source_label})");

        Ok(arc)
    }

    fn source_for_model(&self, model_id: &str) -> ModelSource {
        let cached_source = self.scanned.get(model_id).map(|s| s.value().1.clone());
        match cached_source.as_deref() {
            Some("modelscope") => ModelSource::ModelScope,
            Some("huggingface") => ModelSource::HuggingFace,
            _ => self.model_source.clone(),
        }
    }

    fn refresh_cached_chat_template(&self, model_id: &str) {
        match find_cached_template(&self.cache_dir, model_id) {
            Ok(Some(template)) => {
                self.chat_templates.insert(
                    model_id.to_string(),
                    (template.source, template.source_name),
                );
            }
            Ok(None) => {
                self.chat_templates.remove(model_id);
            }
            Err(err) => warn!("Failed to inspect chat template for '{model_id}': {err}"),
        }
    }
}
