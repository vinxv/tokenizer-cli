mod common;

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Map, Value};
use tokenizer_svc::config::{Config, ModelSource};

const MODEL: &str = "local/chat";

#[tokio::test]
async fn chat_render_uses_jinja_file_and_counts_tokens() {
    let cache_dir = temp_cache_dir("jinja-file");
    write_model_cache(&cache_dir, None);

    let template_path = model_snapshot_dir(&cache_dir).join("chat_template.jinja");
    fs::write(
        &template_path,
        "{% for message in messages %}{{ message.role }}: {{ message.content }}\n{% endfor %}{% if add_generation_prompt %}assistant: {% endif %}",
    )
    .expect("write jinja template");

    let srv = common::spawn_server_with_config(test_config(&cache_dir)).await;
    let res = srv
        .client
        .post(format!("{}/v1/chat/render", srv.base_url))
        .json(&json!({
            "model": MODEL,
            "messages": [
                { "role": "system", "content": "You are terse" },
                { "role": "user", "content": "Hi" }
            ]
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.expect("json parse");
    let expected = "system: You are terse\nuser: Hi\nassistant: ";
    assert_eq!(body["text"], expected);
    assert_eq!(body["template_source"], "jinja_file");
    assert_eq!(body["template_name"], "chat_template.jinja");
    assert_eq!(
        body["count"].as_u64().expect("count"),
        expected.chars().count() as u64
    );
}

#[tokio::test]
async fn chat_render_uses_tokenizer_config_template_and_counts_tokens() {
    let cache_dir = temp_cache_dir("tokenizer-config");
    write_model_cache(
        &cache_dir,
        Some(json!({
            "chat_template": "{% for message in messages %}<{{ message.role }}>{{ message.content }}{{ eos_token }}{% endfor %}",
            "eos_token": "</s>"
        })),
    );

    let srv = common::spawn_server_with_config(test_config(&cache_dir)).await;
    let res = srv
        .client
        .post(format!("{}/v1/chat/render", srv.base_url))
        .json(&json!({
            "model": MODEL,
            "add_generation_prompt": false,
            "messages": [
                { "role": "user", "content": "Hello" }
            ]
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.expect("json parse");
    let expected = "<user>Hello</s>";
    assert_eq!(body["text"], expected);
    assert_eq!(body["template_source"], "tokenizer_config");
    assert_eq!(body["template_name"], "tokenizer_config.json");
    assert_eq!(
        body["count"].as_u64().expect("count"),
        expected.chars().count() as u64
    );
}

#[tokio::test]
async fn models_exposes_chat_template_capability() {
    let cache_dir = temp_cache_dir("model-capability");
    write_model_cache(
        &cache_dir,
        Some(json!({
            "chat_template": "{% for message in messages %}{{ message.content }}{% endfor %}"
        })),
    );

    let srv = common::spawn_server_with_config(test_config(&cache_dir)).await;
    let res = srv
        .client
        .get(format!("{}/v1/models", srv.base_url))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.expect("json parse");
    let model = body["models"]
        .as_array()
        .expect("models")
        .iter()
        .find(|entry| entry["model_id"] == MODEL)
        .expect("local model listed");

    assert_eq!(model["has_chat_template"], true);
    assert_eq!(model["chat_template_source"], "tokenizer_config");
    assert_eq!(model["chat_template_name"], "tokenizer_config.json");
}

#[tokio::test]
async fn encode_returns_token_texts() {
    let cache_dir = temp_cache_dir("token-texts");
    write_model_cache(&cache_dir, None);

    let srv = common::spawn_server_with_config(test_config(&cache_dir)).await;
    let res = srv
        .client
        .post(format!("{}/v1/token/encode", srv.base_url))
        .json(&json!({
            "model": MODEL,
            "text": "A b"
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.expect("json parse");
    let ids = body["ids"].as_array().expect("ids array");
    let tokens = body["tokens"].as_array().expect("tokens array");

    assert_eq!(tokens.len(), ids.len());
    assert_eq!(
        tokens
            .iter()
            .map(|token| token.as_str().expect("token text"))
            .collect::<String>(),
        "A b"
    );
}

#[tokio::test]
async fn chat_render_supports_qwen_python_string_methods_and_kwargs() {
    let cache_dir = temp_cache_dir("qwen-pycompat");
    write_model_cache(
        &cache_dir,
        Some(json!({
            "chat_template": r#"{%- set content = messages[0].content|trim -%}
{%- if not(content.startswith('<tool_response>') and content.endswith('</tool_response>')) -%}
{{- 'user ' + content + '\n' -}}
{%- endif -%}
{%- set content = messages[1].content -%}
{%- if '</think>' in content -%}
{%- set reasoning_content = content.split('</think>')[0].rstrip('\n').split('<think>')[-1].lstrip('\n') -%}
{{- reasoning_content + '\n' + content.split('</think>')[-1].lstrip('\n') -}}
{%- endif -%}
{%- if add_generation_prompt and enable_thinking is defined and enable_thinking is false -%}
{{- '\nNO' -}}
{%- endif -%}"#
        })),
    );

    let srv = common::spawn_server_with_config(test_config(&cache_dir)).await;
    let res = srv
        .client
        .post(format!("{}/v1/chat/render", srv.base_url))
        .json(&json!({
            "model": MODEL,
            "messages": [
                { "role": "user", "content": " Hi " },
                { "role": "assistant", "content": "<think>\nwhy\n</think>\nanswer" }
            ],
            "chat_template_kwargs": {
                "enable_thinking": false
            }
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.expect("json parse");
    assert_eq!(body["text"], "user Hi\nwhy\nanswer\nNO");
}

#[tokio::test]
async fn chat_render_errors_when_no_template_is_available() {
    let cache_dir = temp_cache_dir("missing-template");
    write_model_cache(&cache_dir, None);

    let srv = common::spawn_server_with_config(test_config(&cache_dir)).await;
    let res = srv
        .client
        .post(format!("{}/v1/chat/render", srv.base_url))
        .json(&json!({
            "model": MODEL,
            "messages": [
                { "role": "user", "content": "Hello" }
            ]
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.expect("json parse");
    assert_eq!(body["error"]["code"], "INVALID_INPUT");
    assert!(body["error"]["message"]
        .as_str()
        .expect("message")
        .contains("no chat template found"));
}

#[tokio::test]
async fn chat_render_ignores_client_template_path() {
    let cache_dir = temp_cache_dir("client-template-path");
    write_model_cache(&cache_dir, None);

    let outside_template = cache_dir.join("outside.jinja");
    fs::write(&outside_template, "SHOULD NOT RENDER").expect("write outside template");

    let srv = common::spawn_server_with_config(test_config(&cache_dir)).await;
    let res = srv
        .client
        .post(format!("{}/v1/chat/render", srv.base_url))
        .json(&json!({
            "model": MODEL,
            "template_path": outside_template,
            "messages": [
                { "role": "user", "content": "Hello" }
            ]
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.expect("json parse");
    assert_eq!(body["error"]["code"], "INVALID_INPUT");
}

fn test_config(cache_dir: &Path) -> Config {
    Config {
        host: "127.0.0.1".to_string(),
        port: 0,
        cache_dir: cache_dir.display().to_string(),
        hf_token: None,
        allow_online_download: false,
        model_source: ModelSource::ModelScope,
        ms_token: None,
    }
}

fn temp_cache_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "toksvc-chat-render-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create temp cache");
    path
}

fn write_model_cache(cache_dir: &Path, tokenizer_config: Option<Value>) {
    let snapshot = model_snapshot_dir(cache_dir);
    fs::create_dir_all(&snapshot).expect("create model snapshot");
    fs::write(snapshot.join("tokenizer.json"), tiny_tokenizer_json()).expect("write tokenizer");

    if let Some(config) = tokenizer_config {
        fs::write(
            snapshot.join("tokenizer_config.json"),
            serde_json::to_string_pretty(&config).expect("serialize tokenizer config"),
        )
        .expect("write tokenizer config");
    }
}

fn model_snapshot_dir(cache_dir: &Path) -> PathBuf {
    cache_dir
        .join("ms--local--chat")
        .join("snapshots")
        .join("master")
}

fn tiny_tokenizer_json() -> String {
    let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 <>|:/\n.,!?_-";
    let mut unique = BTreeSet::new();
    for ch in chars.chars() {
        unique.insert(ch);
    }

    let mut vocab = Map::new();
    for (id, ch) in unique.into_iter().enumerate() {
        vocab.insert(ch.to_string(), json!(id));
    }

    serde_json::to_string_pretty(&json!({
        "version": "1.0",
        "added_tokens": [],
        "normalizer": null,
        "pre_tokenizer": null,
        "model": {
            "type": "BPE",
            "vocab": vocab,
            "merges": []
        },
        "post_processor": null,
        "decoder": null
    }))
    .expect("serialize tokenizer")
}
