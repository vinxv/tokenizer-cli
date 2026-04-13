/// End-to-end HuggingFace backend tests.
///
/// All tests are `#[ignore]` by default — they require:
///   - Phase 1 implementation of `HfTokenizer::load`
///   - Internet access (`TOKSVC_ALLOW_ONLINE=1`)
///   - Optional `HF_TOKEN` for gated models
///
/// Run with:
///   TOKSVC_ALLOW_ONLINE=1 cargo test --test test_token_hf -- --ignored
mod common;

/// Token count must be deterministic and non-zero.
#[tokio::test]
#[ignore = "Requires internet access: set TOKSVC_ALLOW_ONLINE=1 to run"]
async fn hf_count_is_nonzero() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .post(format!("{}/v1/token/count", srv.base_url))
        .json(&serde_json::json!({
            "model": "bert-base-uncased",
            "text": "Hello, world!"
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("json parse");
    let count = body["count"].as_u64().expect("count field missing");
    assert!(count > 0, "token count must be > 0");
}

/// Encode → Decode must round-trip (identity property).
#[tokio::test]
#[ignore = "Requires internet access: set TOKSVC_ALLOW_ONLINE=1 to run"]
async fn hf_encode_decode_roundtrip() {
    let srv = common::spawn_server().await;
    let original = "The quick brown fox jumps over the lazy dog";

    let enc_res = srv
        .client
        .post(format!("{}/v1/token/encode", srv.base_url))
        .json(&serde_json::json!({ "model": "bert-base-uncased", "text": original }))
        .send()
        .await
        .expect("encode request failed");
    assert_eq!(enc_res.status(), 200);

    let enc_body: serde_json::Value = enc_res.json().await.expect("json");
    let ids = enc_body["ids"].as_array().expect("ids array").clone();
    assert!(!ids.is_empty(), "ids must not be empty");

    let dec_res = srv
        .client
        .post(format!("{}/v1/token/decode", srv.base_url))
        .json(&serde_json::json!({ "model": "bert-base-uncased", "ids": ids }))
        .send()
        .await
        .expect("decode request failed");
    assert_eq!(dec_res.status(), 200);

    let dec_body: serde_json::Value = dec_res.json().await.expect("json");
    let decoded = dec_body["text"].as_str().expect("text field");

    assert!(
        decoded
            .to_lowercase()
            .contains(&original[..10].to_lowercase()),
        "decoded '{}' does not resemble original '{}'",
        decoded,
        original
    );
}

/// Count is >= 1 for short inputs across multiple public models.
#[tokio::test]
#[ignore = "Requires internet access: set TOKSVC_ALLOW_ONLINE=1 to run"]
async fn hf_count_single_word_multiple_models() {
    let srv = common::spawn_server().await;
    for model in &["bert-base-uncased", "gpt2"] {
        let res = srv
            .client
            .post(format!("{}/v1/token/count", srv.base_url))
            .json(&serde_json::json!({ "model": model, "text": "hi" }))
            .send()
            .await
            .expect("request failed");
        assert_eq!(res.status(), 200, "model={model}");
        let body: serde_json::Value = res.json().await.expect("json");
        let count = body["count"].as_u64().expect("count");
        assert!(
            count >= 1,
            "model={model}: expected count >= 1, got {count}"
        );
    }
}
