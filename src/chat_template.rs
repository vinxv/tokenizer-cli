use std::{
    fs,
    path::{Path, PathBuf},
};

use minijinja::{
    value::from_args, Environment, Error as JinjaError, ErrorKind as JinjaErrorKind, State,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatTemplateSource {
    JinjaFile,
    TokenizerConfig,
}

#[derive(Debug, Clone)]
pub struct ResolvedChatTemplate {
    pub template: String,
    pub source: ChatTemplateSource,
    pub source_name: Option<String>,
    pub defaults: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct TokenizerConfig {
    chat_template: Option<Value>,
    bos_token: Option<Value>,
    eos_token: Option<Value>,
}

pub fn find_cached_template(
    cache_dir: &str,
    model_id: &str,
) -> AppResult<Option<ResolvedChatTemplate>> {
    for path in tokenizer_config_candidates(cache_dir, model_id) {
        if let Some(template) = parse_tokenizer_config(&path)? {
            return Ok(Some(template));
        }
    }

    if let Some(path) = jinja_template_candidates(cache_dir, model_id)
        .into_iter()
        .next()
    {
        return load_jinja_template(&path).map(Some);
    }

    Ok(None)
}

fn load_jinja_template(path: &Path) -> AppResult<ResolvedChatTemplate> {
    let template = fs::read_to_string(path).map_err(|e| {
        AppError::InvalidInput(format!(
            "failed to read jinja template '{}': {e}",
            path.display()
        ))
    })?;

    Ok(ResolvedChatTemplate {
        template,
        source: ChatTemplateSource::JinjaFile,
        source_name: file_name(path),
        defaults: Map::new(),
    })
}

fn parse_tokenizer_config(path: &Path) -> AppResult<Option<ResolvedChatTemplate>> {
    let raw = fs::read_to_string(path).map_err(|e| AppError::LoadError {
        model: path.display().to_string(),
        reason: format!("failed to read tokenizer_config.json: {e}"),
    })?;

    let cfg: TokenizerConfig = serde_json::from_str(&raw).map_err(|e| AppError::LoadError {
        model: path.display().to_string(),
        reason: format!("failed to parse tokenizer_config.json: {e}"),
    })?;

    let Some(template_value) = cfg.chat_template else {
        return Ok(None);
    };

    let template = extract_chat_template_value(&template_value)?;
    let mut defaults = Map::new();

    if let Some(token) = extract_special_token(cfg.bos_token.as_ref()) {
        defaults.insert("bos_token".to_string(), Value::String(token));
    }
    if let Some(token) = extract_special_token(cfg.eos_token.as_ref()) {
        defaults.insert("eos_token".to_string(), Value::String(token));
    }

    Ok(Some(ResolvedChatTemplate {
        template,
        source: ChatTemplateSource::TokenizerConfig,
        source_name: file_name(path),
        defaults,
    }))
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

pub fn render_chat_template(
    template: &str,
    messages: Vec<Value>,
    add_generation_prompt: bool,
    defaults: Map<String, Value>,
    extra: Map<String, Value>,
) -> AppResult<String> {
    let mut context = defaults;
    for (key, value) in extra {
        context.insert(key, value);
    }
    if let Some(Value::Object(kwargs)) = context.get("chat_template_kwargs").cloned() {
        for (key, value) in kwargs {
            context.insert(key, value);
        }
    }
    context.insert("messages".to_string(), Value::Array(messages));
    context.insert(
        "add_generation_prompt".to_string(),
        Value::Bool(add_generation_prompt),
    );

    let mut env = Environment::new();
    env.add_function("raise_exception", raise_exception);
    env.set_unknown_method_callback(python_string_method);

    env.render_named_str("chat_template", template, Value::Object(context))
        .map_err(|e| AppError::InvalidInput(format!("failed to render chat template: {e}")))
}

fn python_string_method(
    _state: &State,
    value: &minijinja::Value,
    method: &str,
    args: &[minijinja::Value],
) -> Result<minijinja::Value, JinjaError> {
    let Some(input) = value.as_str() else {
        return Err(JinjaError::from(JinjaErrorKind::UnknownMethod));
    };

    match method {
        "startswith" => {
            let (prefix,): (String,) = from_args(args)?;
            Ok(minijinja::Value::from(input.starts_with(&prefix)))
        }
        "endswith" => {
            let (suffix,): (String,) = from_args(args)?;
            Ok(minijinja::Value::from(input.ends_with(&suffix)))
        }
        "split" => {
            let (separator, maxsplit): (Option<String>, Option<usize>) = from_args(args)?;
            Ok(minijinja::Value::from(split_string(
                input,
                separator.as_deref(),
                maxsplit,
            )?))
        }
        "lstrip" => {
            let (chars,): (Option<String>,) = from_args(args)?;
            Ok(minijinja::Value::from(strip_string(
                input,
                chars.as_deref(),
                StripSide::Left,
            )))
        }
        "rstrip" => {
            let (chars,): (Option<String>,) = from_args(args)?;
            Ok(minijinja::Value::from(strip_string(
                input,
                chars.as_deref(),
                StripSide::Right,
            )))
        }
        _ => Err(JinjaError::from(JinjaErrorKind::UnknownMethod)),
    }
}

fn split_string(
    input: &str,
    separator: Option<&str>,
    maxsplit: Option<usize>,
) -> Result<Vec<String>, JinjaError> {
    let parts: Vec<&str> = match separator {
        Some("") => {
            return Err(JinjaError::new(
                JinjaErrorKind::InvalidOperation,
                "empty separator",
            ))
        }
        Some(separator) => match maxsplit {
            Some(maxsplit) => input.splitn(maxsplit + 1, separator).collect(),
            None => input.split(separator).collect(),
        },
        None => match maxsplit {
            Some(maxsplit) => input.split_whitespace().take(maxsplit + 1).collect(),
            None => input.split_whitespace().collect(),
        },
    };

    Ok(parts.into_iter().map(ToOwned::to_owned).collect())
}

#[derive(Debug, Clone, Copy)]
enum StripSide {
    Left,
    Right,
}

fn strip_string(input: &str, chars: Option<&str>, side: StripSide) -> String {
    let should_strip = |ch: char| match chars {
        Some(chars) => chars.contains(ch),
        None => ch.is_whitespace(),
    };

    match side {
        StripSide::Left => input.trim_start_matches(should_strip),
        StripSide::Right => input.trim_end_matches(should_strip),
    }
    .to_string()
}

fn extract_chat_template_value(value: &Value) -> AppResult<String> {
    match value {
        Value::String(template) => Ok(template.clone()),
        Value::Object(map) => extract_named_chat_template(map),
        other => Err(AppError::InvalidInput(format!(
            "tokenizer_config.json field 'chat_template' must be a string or object, got {other}"
        ))),
    }
}

fn extract_named_chat_template(map: &Map<String, Value>) -> AppResult<String> {
    if let Some(Value::String(template)) = map.get("default") {
        return Ok(template.clone());
    }

    let mut string_templates = map
        .iter()
        .filter_map(|(_, value)| value.as_str().map(ToOwned::to_owned));

    if let Some(template) = string_templates.next() {
        if string_templates.next().is_none() {
            return Ok(template);
        }
    }

    Err(AppError::InvalidInput(
        "tokenizer_config.json field 'chat_template' is an object; it must contain a string 'default' template or exactly one string template"
            .to_string(),
    ))
}

fn extract_special_token(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(token)) => Some(token.clone()),
        Some(Value::Object(map)) => map
            .get("content")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        _ => None,
    }
}

fn raise_exception(message: String) -> Result<String, JinjaError> {
    Err(JinjaError::new(JinjaErrorKind::InvalidOperation, message))
}

fn tokenizer_config_candidates(cache_dir: &str, model_id: &str) -> Vec<PathBuf> {
    snapshot_candidates(cache_dir, model_id, |snapshot, out| {
        let path = snapshot.join("tokenizer_config.json");
        if path.is_file() {
            out.push(path);
        }
    })
}

fn jinja_template_candidates(cache_dir: &str, model_id: &str) -> Vec<PathBuf> {
    let mut candidates = snapshot_candidates(cache_dir, model_id, |snapshot, out| {
        let Ok(entries) = fs::read_dir(snapshot) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "jinja") {
                out.push(path);
            }
        }
    });

    candidates.sort_by_key(|a| template_candidate_rank(a));
    candidates
}

fn snapshot_candidates(
    cache_dir: &str,
    model_id: &str,
    collect: impl Fn(&Path, &mut Vec<PathBuf>),
) -> Vec<PathBuf> {
    let cache_root = Path::new(cache_dir);
    let cache_id = model_id.replace('/', "--");
    let mut candidates = Vec::new();

    for snapshots in [
        cache_root
            .join(format!("models--{cache_id}"))
            .join("snapshots"),
        cache_root.join(format!("ms--{cache_id}")).join("snapshots"),
    ] {
        let Ok(entries) = fs::read_dir(snapshots) else {
            continue;
        };

        for entry in entries.flatten() {
            collect(&entry.path(), &mut candidates);
        }
    }

    candidates.sort();
    candidates
}

fn template_candidate_rank(path: &Path) -> (u8, String) {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let rank = match filename {
        "chat_template.jinja" => 0,
        "template.jinja" => 1,
        _ => 2,
    };

    (rank, path.display().to_string())
}
