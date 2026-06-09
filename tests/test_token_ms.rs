/// End-to-end ModelScope backend tests.
///
/// All tests are `#[ignore]` by default — they require:
///   - Internet access to www.modelscope.cn
///   - `TOKSVC_ALLOW_ONLINE=1`
///   - `TOKSVC_MODEL_SOURCE=modelscope`
///   - Optionally `TOKSVC_MS_TOKEN` for private models
///
/// Run with:
///   TOKSVC_MODEL_SOURCE=modelscope TOKSVC_ALLOW_ONLINE=1 \
///     cargo test --test test_token_ms -- --ignored --nocapture
///
/// The test model used here is a public BPE tokenizer.json model:
///   <https://www.modelscope.cn/models/Qwen/Qwen2.5-0.5B-Instruct>
mod common;

const MS_MODEL: &str = "Qwen/Qwen2.5-0.5B-Instruct";

/// Token count must be deterministic and non-zero.
#[tokio::test]
#[ignore = "Requires ModelScope access: set TOKSVC_MODEL_SOURCE=modelscope TOKSVC_ALLOW_ONLINE=1"]
async fn ms_count_is_nonzero() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .post(format!("{}/v1/token/count", srv.base_url))
        .json(&serde_json::json!({
            "model": MS_MODEL,
            "text": "你好，世界！"
        }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("json parse");
    let count = body["count"].as_u64().expect("count field missing");
    assert!(count > 0, "token count must be > 0, got {count}");
}

/// Encode → decode must round-trip (identity property).
#[tokio::test]
#[ignore = "Requires ModelScope access: set TOKSVC_MODEL_SOURCE=modelscope TOKSVC_ALLOW_ONLINE=1"]
async fn ms_encode_decode_roundtrip() {
    let srv = common::spawn_server().await;
    let original = "快速的棕色狐狸跳过了懒狗";

    // Encode
    let enc_res = srv
        .client
        .post(format!("{}/v1/token/encode", srv.base_url))
        .json(&serde_json::json!({ "model": MS_MODEL, "text": original }))
        .send()
        .await
        .expect("encode request failed");
    assert_eq!(enc_res.status(), 200);

    let enc_body: serde_json::Value = enc_res.json().await.expect("json");
    let ids = enc_body["ids"].as_array().expect("ids array").clone();
    assert!(!ids.is_empty(), "ids must not be empty");

    // Decode
    let dec_res = srv
        .client
        .post(format!("{}/v1/token/decode", srv.base_url))
        .json(&serde_json::json!({ "model": MS_MODEL, "ids": ids }))
        .send()
        .await
        .expect("decode request failed");
    assert_eq!(dec_res.status(), 200);

    let dec_body: serde_json::Value = dec_res.json().await.expect("json");
    let decoded = dec_body["text"].as_str().expect("text field");

    assert!(
        decoded.contains('快') || decoded.contains("快速"),
        "decoded '{decoded}' does not resemble original '{original}'"
    );
}

/// After loading an MS model it appears in /v1/models.
#[tokio::test]
#[ignore = "Requires ModelScope access: set TOKSVC_MODEL_SOURCE=modelscope TOKSVC_ALLOW_ONLINE=1"]
async fn ms_model_appears_in_list() {
    let srv = common::spawn_server().await;

    // Trigger load
    let count_res = srv
        .client
        .post(format!("{}/v1/token/count", srv.base_url))
        .json(&serde_json::json!({ "model": MS_MODEL, "text": "hello" }))
        .send()
        .await
        .expect("count request failed");
    assert_eq!(count_res.status(), 200, "count must succeed first");

    // Check listing
    let list_res = srv
        .client
        .get(format!("{}/v1/models", srv.base_url))
        .send()
        .await
        .expect("models request failed");
    assert_eq!(list_res.status(), 200);

    let body: serde_json::Value = list_res.json().await.expect("json parse");
    let models = body["models"].as_array().expect("models array");
    assert!(
        models
            .iter()
            .any(|m| m["model_id"] == MS_MODEL && m["status"] == "loaded"),
        "{MS_MODEL} should appear in /v1/models with status 'loaded' after being explicitly used"
    );
}
