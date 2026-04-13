/// Shared application state injected into every axum handler via `State<AppState>`.
use std::sync::Arc;

use crate::{config::Config, error::AppResult, tokenizer::registry::Registry};

/// Top-level app state — cheap to clone (all heap-allocated behind Arc).
#[derive(Clone)]
pub struct AppState {
    pub registry: Registry,
    pub config: Arc<Config>,
}

impl AppState {
    /// Build the initial app state. Heavy one-time initialisation goes here.
    pub async fn new(cfg: &Config) -> AppResult<Self> {
        let registry = Registry::new(cfg);
        Ok(Self {
            registry,
            config: Arc::new(cfg.clone()),
        })
    }
}
