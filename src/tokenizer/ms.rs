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
/// # BPE tokenizer requirement
/// `fastokens` 0.2.x loads BPE `tokenizer.json` files. Repositories that only
/// ship `vocab.txt`, or use non-BPE tokenizer models, are not supported by this
/// backend.
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

// ── Public API ────────────────────────────────────────────────────────────────

/// Download all known tokenizer files for `model_id` from ModelScope into the
/// cache directory.
///
/// Unlike the HF backend, **every** file is treated as optional during
/// download; completeness is verified at the end:
/// - If `tokenizer.json` was downloaded → ready for `HfTokenizer::from_path`.
/// - If `tokenizer.json` is missing → `AppError::ModelNotFound`.
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

    // Completeness check: fastokens requires tokenizer.json.
    let has_tokenizer_json = results.iter().any(|(n, _)| n == "tokenizer.json");
    if !has_tokenizer_json {
        return Err(AppError::ModelNotFound(model_id.to_string()));
    }

    Ok(results)
}

/// Load the tokenizer for `model_id` from ModelScope.
///
/// 1. Downloads all available tokenizer files via `pull()`.
/// 2. If `tokenizer.json` is present, passes it to `HfTokenizer::from_path`.
pub async fn load(
    model_id: &str,
    cache_dir: &str,
    ms_token: Option<&str>,
) -> AppResult<HfTokenizer> {
    debug!("Loading ModelScope tokenizer for '{model_id}'");

    let (namespace, name) = split_model_id(model_id)?;
    let local_tokenizer_json = cache_path(cache_dir, namespace, name, "tokenizer.json");

    let files = if local_tokenizer_json.exists() {
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

    // Resolve tokenizer.json path from either the existing cache or the
    // freshly downloaded file set.
    let model_id_owned = model_id.to_string();

    let tokenizer_path = if let Some((_, p)) = files.iter().find(|(n, _)| n == "tokenizer.json") {
        p.clone()
    } else {
        return Err(AppError::ModelNotFound(model_id_owned));
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
