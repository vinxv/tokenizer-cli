/// API contract tests — request/response shapes and error handling.
/// These tests verify the API surface WITHOUT requiring real tokenizer backends.
mod common;

// ── /v1/token/count ───────────────────────────────────────────────────────────

/// An unknown model should return 404 with a structured error body.
#[tokio::test]
async fn count_unknown_model_returns_404() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .post(format!("{}/v1/token/count", srv.base_url))
        .json(&serde_json::json!({
            "model": "does-not-exist/fake-model",
            "text": "Hello, world!"
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 404);
    let body: serde_json::Value = res.json().await.expect("json parse");
    assert!(body["error"]["code"].is_string());
    assert!(body["error"]["message"].is_string());
}

/// Missing required fields should return a 4xx client error.
#[tokio::test]
async fn count_missing_text_field_returns_error() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .post(format!("{}/v1/token/count", srv.base_url))
        .json(&serde_json::json!({ "model": "some-model" })) // missing "text"
        .send()
        .await
        .expect("request failed");

    assert!(
        res.status().is_client_error(),
        "expected 4xx, got {}",
        res.status()
    );
}

// ── /v1/token/encode ──────────────────────────────────────────────────────────

/// An unknown model should return 404.
#[tokio::test]
async fn encode_unknown_model_returns_404() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .post(format!("{}/v1/token/encode", srv.base_url))
        .json(&serde_json::json!({
            "model": "does-not-exist/fake-model",
            "text": "Hello"
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 404);
}

// ── /v1/token/decode ──────────────────────────────────────────────────────────

/// An unknown model should return 404.
#[tokio::test]
async fn decode_unknown_model_returns_404() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .post(format!("{}/v1/token/decode", srv.base_url))
        .json(&serde_json::json!({
            "model": "does-not-exist/fake-model",
            "ids": [9906]
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 404);
}
