//! `tokenizer-cli` — unified entry point for the tokenizer service.
//!
//! ```text
//! tokenizer-cli serve                  # start the HTTP API server
//! tokenizer-cli ping
//! tokenizer-cli models
//! tokenizer-cli pull   <model> [--source modelscope]
//! tokenizer-cli count  <model> <text>
//! tokenizer-cli encode <model> <text>
//! tokenizer-cli decode <model> <id>...
//! ```
//!
//! Pass `--json` to any sub-command to emit JSON output that mirrors the
//! HTTP API response body exactly.

use std::{net::SocketAddr, time::Duration};

use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use tokenizer_svc::{
    config::{Config, ModelSource},
    routes::build_router,
    state::AppState,
    tokenizer::{hf, ms},
};
use tracing::{info, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(
    name = "tokenizer-cli",
    about = "Command-line interface for the tokenizer service",
    version
)]
struct Cli {
    /// Emit JSON output matching the HTTP API response shape.
    #[arg(long, global = true)]
    json: bool,

    /// Set log level (error/warn/info/debug/trace). Defaults to 'error'.
    #[arg(long, global = true, default_value = "info")]
    log_level: Level,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the HTTP API server (equivalent to the former `tokenizer-svc` binary).
    ///
    /// Host / port / cache-dir are read from the config file or environment
    /// variables; see `config.rs` for details.
    Serve {
        /// Enable the embedded Web Dashboard on `/`.
        #[arg(long)]
        enable_web: bool,
    },

    /// Check service readiness (mirrors GET /ping).
    Ping,

    /// List all tokenizers currently loaded in the registry (mirrors GET /v1/models).
    Models,

    /// Download tokenizer files for MODEL into the local cache.
    ///
    /// Downloads tokenizer.json (required) plus any optional companion files
    /// (tokenizer_config.json, special_tokens_map.json, vocab.json, merges.txt,
    /// vocab.txt) that the model repository provides.
    ///
    /// Source defaults to `huggingface`; set `--source modelscope` to pull
    /// from <https://www.modelscope.cn> instead.
    Pull {
        /// Model id, e.g. `bert-base-uncased` (HF) or
        /// `iic/nlp_corom_sentence-embedding_chinese-base` (ModelScope).
        model: String,

        /// Model hub to download from (`huggingface` or `modelscope`).
        /// Defaults to `huggingface`.
        #[arg(long, default_value = "huggingface")]
        source: String,

        /// HuggingFace API token (overrides HF_TOKEN env / config file).
        /// Only used when --source huggingface.
        #[arg(long)]
        token: Option<String>,

        /// ModelScope SDK token (overrides TOKSVC_MS_TOKEN env / config file).
        /// Only used when --source modelscope.
        #[arg(long)]
        ms_token: Option<String>,
    },

    /// Count tokens in TEXT using MODEL (mirrors POST /v1/token/count).
    Count {
        /// Model id or OpenAI alias, e.g. `gpt-4o` or `bert-base-uncased`.
        model: String,
        /// Text to count tokens in.
        text: String,
    },

    /// Encode TEXT into token ids using MODEL (mirrors POST /v1/token/encode).
    Encode {
        /// Model id or OpenAI alias.
        model: String,
        /// Text to encode.
        text: String,
    },

    /// Decode token ids into text using MODEL (mirrors POST /v1/token/decode).
    Decode {
        /// Model id or OpenAI alias.
        model: String,
        /// One or more token ids to decode.
        #[arg(required = true)]
        ids: Vec<u32>,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialise tracing at the requested level (default: error — keeps CLI
    // output clean unless the user explicitly bumps it).
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| cli.log_level.to_string()),
        ))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let cfg = Config::load()?;

    // `serve` and `pull` are handled before building a full AppState
    // (no registry warming needed for either).
    match cli.command {
        Command::Serve { enable_web } => return cmd_serve(cfg, enable_web).await,

        Command::Pull {
            model,
            source,
            token,
            ms_token,
        } => {
            let cache_dir = cfg.hf_cache_dir.clone();

            // Resolve the requested source (flag > env > default HF).
            let model_source = match source.to_lowercase().as_str() {
                "modelscope" | "ms" => ModelSource::ModelScope,
                _ => ModelSource::HuggingFace,
            };

            // Token priority: CLI flag > env var > config file.
            let hf_token = {
                let env = std::env::var("HF_TOKEN").ok();
                token
                    .as_deref()
                    .or(env.as_deref())
                    .or(cfg.hf_token.as_deref())
                    .map(str::to_string)
            };
            let ms_token_resolved = {
                let env = std::env::var("TOKSVC_MS_TOKEN").ok();
                ms_token
                    .as_deref()
                    .or(env.as_deref())
                    .or(cfg.ms_token.as_deref())
                    .map(str::to_string)
            };

            return cmd_pull(
                model,
                cache_dir,
                model_source,
                hf_token.as_deref(),
                ms_token_resolved.as_deref(),
                cli.json,
            )
            .await;
        }

        _ => {}
    }

    let state = AppState::new(&cfg).await?;

    match cli.command {
        Command::Serve { .. } | Command::Pull { .. } => unreachable!(),
        Command::Ping => cmd_ping(cli.json),
        Command::Models => cmd_models(&state, cli.json),
        Command::Count { model, text } => cmd_count(&state, model, text, cli.json).await?,
        Command::Encode { model, text } => cmd_encode(&state, model, text, cli.json).await?,
        Command::Decode { model, ids } => cmd_decode(&state, model, ids, cli.json).await?,
    }

    Ok(())
}

// ── Sub-command implementations ───────────────────────────────────────────────

/// Start the HTTP API server.
async fn cmd_serve(cfg: Config, enable_web: bool) -> anyhow::Result<()> {
    info!("Starting tokenizer-svc on {}:{}", cfg.host, cfg.port);
    let app_state = AppState::new(&cfg).await?;
    let app = build_router(app_state, enable_web);
    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Download tokenizer files with a progress spinner.
///
/// Routes to `hf::pull` or `ms::pull` depending on `source`.
async fn cmd_pull(
    model: String,
    cache_dir: String,
    source: ModelSource,
    hf_token: Option<&str>,
    ms_token: Option<&str>,
    as_json: bool,
) -> anyhow::Result<()> {
    let source_label = match source {
        ModelSource::HuggingFace => "HuggingFace Hub",
        ModelSource::ModelScope => "ModelScope",
    };

    // Indeterminate spinner — neither backend exposes byte-level progress.
    let spinner = if !as_json {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message(format!("Pulling '{model}' from {source_label}…"));
        pb.enable_steady_tick(Duration::from_millis(80));
        Some(pb)
    } else {
        None
    };

    let result = match source {
        ModelSource::HuggingFace => hf::pull(&model, &cache_dir, hf_token).await,
        ModelSource::ModelScope => ms::pull(&model, &cache_dir, ms_token).await,
    };

    if let Some(pb) = &spinner {
        pb.finish_and_clear();
    }

    match result {
        Ok(files) => {
            if as_json {
                let entries: Vec<_> = files
                    .iter()
                    .map(|(name, path)| {
                        json!({
                            "file": name,
                            "local_path": path.display().to_string(),
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    json!({
                        "model": model,
                        "source": source_label,
                        "status": "ok",
                        "files": entries,
                    })
                );
            } else {
                println!("✔ Downloaded {model}  [{source_label}]");
                println!("  cache_dir : {cache_dir}");
                println!("  files ({}):", files.len());
                for (name, path) in &files {
                    println!("    {name:<35} → {}", path.display());
                }
            }
            Ok(())
        }
        Err(e) => {
            if as_json {
                println!(
                    "{}",
                    json!({
                        "model": model,
                        "source": source_label,
                        "status": "error",
                        "error": e.to_string(),
                    })
                );
                Ok(()) // exit 0 in JSON mode so consumers can always parse the body
            } else {
                Err(e.into())
            }
        }
    }
}

/// Mirrors `GET /ping` → `{"status":"ok"}`
fn cmd_ping(as_json: bool) {
    if as_json {
        println!("{}", json!({ "status": "ok" }));
    } else {
        println!("status: ok");
    }
}

/// Mirrors `GET /v1/models`
fn cmd_models(state: &AppState, as_json: bool) {
    let models: Vec<_> = state
        .registry
        .list_models()
        .into_iter()
        .map(|info| {
            json!({
                "model_id": info.model_id,
                "vocab_size": info.vocab_size,
                "backend": info.backend.to_string(),
            })
        })
        .collect();

    if as_json {
        println!("{}", json!({ "models": models }));
    } else if models.is_empty() {
        println!("No tokenizers loaded.");
    } else {
        for m in &models {
            println!(
                "{:<50} vocab={:<8} backend={}",
                m["model_id"].as_str().unwrap_or(""),
                m["vocab_size"],
                m["backend"].as_str().unwrap_or(""),
            );
        }
    }
}

/// Mirrors `POST /v1/token/count`
async fn cmd_count(
    state: &AppState,
    model: String,
    text: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let tok = state.registry.get_or_load(&model).await?;
    let count = tok.count(&text).await?;

    if as_json {
        println!("{}", json!({ "model": model, "count": count }));
    } else {
        println!("model: {model}");
        println!("count: {count}");
    }
    Ok(())
}

/// Mirrors `POST /v1/token/encode`
async fn cmd_encode(
    state: &AppState,
    model: String,
    text: String,
    as_json: bool,
) -> anyhow::Result<()> {
    let tok = state.registry.get_or_load(&model).await?;
    let ids = tok.encode(&text).await?;

    if as_json {
        println!("{}", json!({ "model": model, "ids": ids }));
    } else {
        let ids_str: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
        println!("model: {model}");
        println!("ids:   [{}]", ids_str.join(", "));
    }
    Ok(())
}

/// Mirrors `POST /v1/token/decode`
async fn cmd_decode(
    state: &AppState,
    model: String,
    ids: Vec<u32>,
    as_json: bool,
) -> anyhow::Result<()> {
    let tok = state.registry.get_or_load(&model).await?;
    let text = tok.decode(&ids).await?;

    if as_json {
        println!("{}", json!({ "model": model, "text": text }));
    } else {
        println!("model: {model}");
        println!("text:  {text}");
    }
    Ok(())
}
