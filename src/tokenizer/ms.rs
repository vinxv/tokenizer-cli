/// ModelScope tokenizer backend.
///
/// Downloads tokenizer files from <https://www.modelscope.cn> via the
/// ModelScope REST API, then reuses `HfTokenizer::from_path()` to parse
/// the resulting `tokenizer.json`.  No separate `TokenizerBackend` impl is
/// needed: the returned `HfTokenizer` is fully functional.
///
/// # Download URL pattern (from the official Python SDK)
/// ```text
/// https://www.modelscope.cn/api/v1/models/{namespace}/{name}/repo
///   ?Revision=master&FilePath={filename}
/// ```
///
/// # Vocab-only fallback
/// Older BERT-style models on ModelScope ship only `vocab.txt` (no
/// `tokenizer.json`).  When that happens `pull()` still succeeds as long as
/// `vocab.txt` is present, and `load()` synthesises a `tokenizer.json` from
/// the vocab file using the `tokenizers` crate's WordPiece builder.
///
/// # Authentication
/// ModelScope uses a session cookie (`m_session_id`) rather than a Bearer
/// header.  The token stored in `Config::ms_token` is passed as that cookie.
/// Public models are accessible without any token.
use std::{
    fs,
    path::{Path, PathBuf},
};

use reqwest::header;
use tracing::{debug, info, warn};

use crate::error::{AppError, AppResult};

use super::hf::{HfTokenizer, TOKENIZER_FILES};

// ── Constants ─────────────────────────────────────────────────────────────────

const MS_ENDPOINT: &str = "https://www.modelscope.cn";
const MS_REVISION: &str = "master";

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Split `model_id` ("namespace/name") into its two parts.
fn split_model_id(model_id: &str) -> AppResult<(&str, &str)> {
    let mut parts = model_id.splitn(2, '/');
    match (parts.next(), parts.next()) {
        (Some(ns), Some(name)) if !ns.is_empty() && !name.is_empty() => Ok((ns, name)),
        _ => Err(AppError::LoadError {
            model: model_id.to_string(),
            reason: format!("invalid ModelScope model id '{model_id}': expected 'namespace/name'"),
        }),
    }
}

/// Compute the local cache path for a single file, mirroring hf-hub's layout.
///
/// ```text
/// {cache_dir}/ms--{namespace}--{name}/snapshots/master/{filename}
/// ```
fn cache_path(cache_dir: &str, namespace: &str, name: &str, filename: &str) -> PathBuf {
    Path::new(cache_dir)
        .join(format!("ms--{namespace}--{name}"))
        .join("snapshots")
        .join(MS_REVISION)
        .join(filename)
}

/// Build the download URL for a single file.
fn file_url(namespace: &str, name: &str, filename: &str) -> String {
    format!(
        "{MS_ENDPOINT}/api/v1/models/{namespace}/{name}/repo\
         ?Revision={MS_REVISION}&FilePath={filename}"
    )
}

/// Synthesise a `tokenizer.json` from `vocab.txt` when a ModelScope repo does
/// not provide one (common for older BERT-style models).
///
/// Reads `do_lower_case` from `tokenizer_config.json` if present (defaults `true`).
/// The generated file is written next to `vocab.txt` so future runs hit the
/// cache and skip the synthesis step.
fn build_bert_tokenizer_json(
    files: &[(String, PathBuf)],
    vocab_path: &Path,
    model_id: &str,
) -> AppResult<PathBuf> {
    use tokenizers::{
        decoders::wordpiece::WordPiece as WordPieceDecoder, models::wordpiece::WordPiece,
        normalizers::bert::BertNormalizer, pre_tokenizers::bert::BertPreTokenizer, Tokenizer,
    };

    // Respect `do_lower_case` from tokenizer_config.json if available.
    let do_lower_case = files
        .iter()
        .find(|(n, _)| n == "tokenizer_config.json")
        .and_then(|(_, p)| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["do_lower_case"].as_bool())
        .unwrap_or(true);

    let vocab_str = vocab_path.to_str().ok_or_else(|| AppError::LoadError {
        model: model_id.to_string(),
        reason: "vocab.txt path contains non-UTF-8 characters".to_string(),
    })?;

    let wordpiece = WordPiece::from_file(vocab_str)
        .unk_token("[UNK]".into())
        .build()
        .map_err(|e| AppError::LoadError {
            model: model_id.to_string(),
            reason: format!("failed to build WordPiece model from vocab.txt: {e}"),
        })?;

    let mut tokenizer = Tokenizer::new(wordpiece);
    tokenizer.with_normalizer(Some(BertNormalizer::new(
        true, // clean_text
        true, // handle_chinese_chars
        None, // strip_accents (None = deduce from lowercase)
        do_lower_case,
    )));
    tokenizer.with_pre_tokenizer(Some(BertPreTokenizer));
    tokenizer.with_decoder(Some(WordPieceDecoder::default()));

    let out_path = vocab_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("tokenizer.json");

    tokenizer
        .save(&out_path, true)
        .map_err(|e| AppError::LoadError {
            model: model_id.to_string(),
            reason: format!("failed to write synthesised tokenizer.json: {e}"),
        })?;

    info!(
        "Synthesised tokenizer.json from vocab.txt for '{model_id}' → {out_path:?} \
         (do_lower_case={do_lower_case})"
    );
    Ok(out_path)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Download all known tokenizer files for `model_id` from ModelScope into the
/// cache directory.
///
/// Unlike the HF backend, **every** file is treated as optional during
/// download; completeness is verified at the end:
/// - If `tokenizer.json` was downloaded → ready for `HfTokenizer::from_path`.
/// - If only `vocab.txt` was downloaded → `load()` will synthesise a
///   `tokenizer.json` from it.
/// - If neither was downloaded → `AppError::ModelNotFound`.
///
/// Returns a list of `(filename, local_path)` for every file fetched.
pub async fn pull(
    model_id: &str,
    cache_dir: &str,
    ms_token: Option<&str>,
) -> AppResult<Vec<(String, PathBuf)>> {
    let (namespace, name) = split_model_id(model_id)?;

    // Build a reusable reqwest client with optional auth cookie.
    let mut headers = header::HeaderMap::new();
    if let Some(token) = ms_token {
        let cookie_val = format!("m_session_id={token}");
        headers.insert(
            header::COOKIE,
            header::HeaderValue::from_str(&cookie_val).map_err(|_| AppError::LoadError {
                model: model_id.to_string(),
                reason: "invalid ms_token: cannot set cookie header".to_string(),
            })?,
        );
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .default_headers(headers)
        .build()
        .map_err(|e| AppError::LoadError {
            model: model_id.to_string(),
            reason: format!("failed to build HTTP client: {e}"),
        })?;

    let mut results: Vec<(String, PathBuf)> = Vec::new();

    for &(filename, _) in TOKENIZER_FILES {
        // On ModelScope every file is optional during download — we check
        // completeness after all attempts.
        let url = file_url(namespace, name, filename);
        let local = cache_path(cache_dir, namespace, name, filename);

        // Serve from cache when already present.
        if local.exists() {
            debug!("ModelScope cache hit: '{filename}' for '{model_id}'");
            results.push((filename.to_string(), local));
            continue;
        }

        debug!("ModelScope fetching '{filename}' for '{model_id}' from {url}");
        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("ModelScope request error for '{filename}' ({model_id}): {e}");
                continue;
            }
        };

        let status = resp.status();

        // 404 / 401 / 403 = file not in this repo (skip quietly)
        if status == reqwest::StatusCode::NOT_FOUND
            || status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            debug!("ModelScope: '{filename}' not available for '{model_id}' (HTTP {status})");
            continue;
        }

        if !status.is_success() {
            warn!("ModelScope: unexpected HTTP {status} fetching '{filename}' for '{model_id}'");
            continue;
        }

        // Ensure parent directories exist.
        if let Some(parent) = local.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::LoadError {
                model: model_id.to_string(),
                reason: format!("failed to create cache dir {parent:?}: {e}"),
            })?;
        }

        let bytes = resp.bytes().await.map_err(|e| AppError::LoadError {
            model: model_id.to_string(),
            reason: format!("failed to read response body for '{filename}': {e}"),
        })?;

        fs::write(&local, &bytes).map_err(|e| AppError::LoadError {
            model: model_id.to_string(),
            reason: format!("failed to write {local:?}: {e}"),
        })?;

        info!("ModelScope downloaded '{filename}' → {local:?}");
        results.push((filename.to_string(), local));
    }

    // Completeness check: need tokenizer.json OR vocab.txt to proceed.
    let has_tokenizer_json = results.iter().any(|(n, _)| n == "tokenizer.json");
    let has_vocab_txt = results.iter().any(|(n, _)| n == "vocab.txt");
    if !has_tokenizer_json && !has_vocab_txt {
        return Err(AppError::ModelNotFound(model_id.to_string()));
    }

    Ok(results)
}

/// Load the tokenizer for `model_id` from ModelScope.
///
/// 1. Downloads all available tokenizer files via `pull()`.
/// 2. If `tokenizer.json` is present, passes it to `HfTokenizer::from_path`.
/// 3. If only `vocab.txt` is present, synthesises a BERT WordPiece
///    `tokenizer.json` (cached), then loads that.
pub async fn load(
    model_id: &str,
    cache_dir: &str,
    ms_token: Option<&str>,
) -> AppResult<HfTokenizer> {
    debug!("Loading ModelScope tokenizer for '{model_id}'");

    let (namespace, name) = split_model_id(model_id)?;
    let local_tokenizer_json = cache_path(cache_dir, namespace, name, "tokenizer.json");
    let local_vocab_txt = cache_path(cache_dir, namespace, name, "vocab.txt");

    let files = if local_tokenizer_json.exists() || local_vocab_txt.exists() {
        debug!("ModelScope tokenizer data already cached for '{model_id}', skipping pull");
        let mut results = Vec::new();
        for &(filename, _) in TOKENIZER_FILES {
            let p = cache_path(cache_dir, namespace, name, filename);
            if p.exists() {
                results.push((filename.to_string(), p));
            }
        }
        results
    } else {
        pull(model_id, cache_dir, ms_token).await?
    };

    // Resolve tokenizer.json path — either downloaded directly or synthesised
    // from vocab.txt in a spawn_blocking task.
    let model_id_owned = model_id.to_string();

    let tokenizer_path = if let Some((_, p)) = files.iter().find(|(n, _)| n == "tokenizer.json") {
        p.clone()
    } else {
        // Vocab-only model: build tokenizer.json from vocab.txt.
        let vocab_path = files
            .iter()
            .find(|(n, _)| n == "vocab.txt")
            .map(|(_, p)| p.clone())
            .ok_or_else(|| AppError::ModelNotFound(model_id_owned.clone()))?;

        let files_clone = files.clone();
        let mid = model_id_owned.clone();
        tokio::task::spawn_blocking(move || {
            build_bert_tokenizer_json(&files_clone, &vocab_path, &mid)
        })
        .await
        .map_err(|e| AppError::LoadError {
            model: model_id_owned.clone(),
            reason: format!("join error while synthesising tokenizer.json: {e}"),
        })??
    };

    // Parse tokenizer.json (blocking I/O) via the shared HfTokenizer path.
    let tok = tokio::task::spawn_blocking(move || {
        HfTokenizer::from_path(&model_id_owned, &tokenizer_path)
    })
    .await
    .map_err(|e| AppError::LoadError {
        model: model_id.to_string(),
        reason: format!("join error during ModelScope tokenizer load: {e}"),
    })??;

    info!("Loaded ModelScope tokenizer for '{model_id}'");
    Ok(tok)
}
