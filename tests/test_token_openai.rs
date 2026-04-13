/// Tests for the OpenAI / tiktoken backend.
///
/// Contract tests (error routing) run without any backend implementation.
/// Value-correctness tests are `#[ignore]`d until Phase 2 lands.
mod common;

/// `gpt-4o` token count for a well-known string must match tiktoken ground truth.
///
/// Ground truth (Python): `tiktoken.encoding_for_model("gpt-4o").encode("Hello, world!")` → 4 tokens.
#[tokio::test]
#[ignore = "Requires OpenAiTokenizer::encode (Phase 2)"]
async fn openai_count_known_string() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .post(format!("{}/v1/token/count", srv.base_url))
        .json(&serde_json::json!({ "model": "gpt-4o", "text": "Hello, world!" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("json parse");
    assert_eq!(
        body["count"], 4,
        "gpt-4o should produce 4 tokens for 'Hello, world!'"
    );
}

/// Encode → Decode identity for `gpt-4` (cl100k_base).
#[tokio::test]
#[ignore = "Requires OpenAiTokenizer (Phase 2)"]
async fn openai_encode_decode_roundtrip() {
    let srv = common::spawn_server().await;
    let original = "The quick brown fox";

    let enc: serde_json::Value = srv
        .client
        .post(format!("{}/v1/token/encode", srv.base_url))
        .json(&serde_json::json!({ "model": "gpt-4", "text": original }))
        .send()
        .await
        .expect("encode")
        .json()
        .await
        .expect("json");

    let ids = enc["ids"].as_array().expect("ids").clone();

    let dec: serde_json::Value = srv
        .client
        .post(format!("{}/v1/token/decode", srv.base_url))
        .json(&serde_json::json!({ "model": "gpt-4", "ids": ids }))
        .send()
        .await
        .expect("decode")
        .json()
        .await
        .expect("json");

    assert_eq!(dec["text"], original);
}

/// Non-OpenAI model ids must not be silently swallowed by the OpenAI backend.
/// With online download disabled (default), this must 404.
#[tokio::test]
async fn openai_backend_does_not_claim_hf_models() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .post(format!("{}/v1/token/count", srv.base_url))
        .json(&serde_json::json!({ "model": "bert-base-uncased", "text": "test" }))
        .send()
        .await
        .expect("request");

    assert_eq!(res.status(), 404);
}
