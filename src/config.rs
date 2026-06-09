use serde::Deserialize;

/// Selects which model-hub backend is used for downloading tokenizers.
///
/// Set via `model_source:` in `config.yaml` or the `TOKSVC_MODEL_SOURCE`
/// environment variable.  Accepted values (case-insensitive): `huggingface`,
/// `modelscope`.
#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelSource {
    /// Download from <https://huggingface.co> (default).
    #[default]
    HuggingFace,
    /// Download from <https://modelscope.cn>.
    ModelScope,
}

/// Application-level configuration.
/// Values can be set via `config.yaml`, environment variables (prefixed `TOKSVC_`),
/// or defaults below.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Host to bind the HTTP server on. Default: `0.0.0.0`
    #[serde(default = "default_host")]
    pub host: String,

    /// Port to bind the HTTP server on. Default: `8080`
    #[serde(default = "default_port")]
    pub port: u16,

    /// Local directory used as cache for downloaded tokenizer files
    /// (shared by both HuggingFace and ModelScope backends).
    /// Default: `~/.cache/tokenizer-svc`
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,

    /// Optional HuggingFace API token (for gated / private models).
    #[serde(default)]
    pub hf_token: Option<String>,

    /// Whether to allow downloading tokenizers on-demand during a request.
    /// If false the model must be pre-cached or pre-loaded at startup.
    #[serde(default = "default_true")]
    pub allow_online_download: bool,

    /// Which model hub to use for downloading tokenizers.
    /// Default: `HuggingFace`.  Set to `modelscope` to use ModelScope.
    #[serde(default)]
    pub model_source: ModelSource,

    /// Optional ModelScope API token (for gated / private models on ModelScope).
    #[serde(default)]
    pub ms_token: Option<String>,
}

impl Config {
    /// Load configuration from file + environment variables.
    pub fn load() -> anyhow::Result<Self> {
        let cfg = config::Config::builder()
            // Optional config file
            .add_source(
                config::File::with_name("config")
                    .required(false)
                    .format(config::FileFormat::Yaml),
            )
            // Environment variable overrides.
            //
            // `prefix_separator("_")` — strip the `TOKSVC_` prefix using a
            //   single underscore as the delimiter.
            // `separator("__")` — use double underscore for nested keys only,
            //   so single underscores in field names (e.g. `model_source`,
            //   `cache_dir`, `allow_online_download`) are preserved.
            //
            // Example env vars:
            //   TOKSVC_MODEL_SOURCE=modelscope
            //   TOKSVC_ALLOW_ONLINE_DOWNLOAD=true
            //   TOKSVC_CACHE_DIR=/custom/cache
            //   TOKSVC_HF_TOKEN=hf_xxx
            //   TOKSVC_MS_TOKEN=xxx
            .add_source(
                config::Environment::with_prefix("TOKSVC")
                    .prefix_separator("_")
                    .separator("__"),
            )
            .build()?
            .try_deserialize()?;
        Ok(cfg)
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_cache_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    format!("{home}/.cache/tokenizer-svc")
}
fn default_true() -> bool {
    true
}
