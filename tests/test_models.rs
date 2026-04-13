/// Tests for `GET /v1/models`.
mod common;

/// An empty registry returns an empty list — not an error.
#[tokio::test]
async fn list_models_empty_on_startup() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .get(format!("{}/v1/models", srv.base_url))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("json parse");
    assert!(body["models"].is_array());
    assert_eq!(body["models"].as_array().unwrap().len(), 0);
}

/// After loading a model it appears in the `/v1/models` listing.
#[tokio::test]
#[ignore = "Requires internet access: set TOKSVC_ALLOW_ONLINE=1 to run"]
async fn list_models_after_hf_load() {
    let srv = common::spawn_server().await;

    // Trigger a load by calling count
    let res = srv
        .client
        .post(format!("{}/v1/token/count", srv.base_url))
        .json(&serde_json::json!({ "model": "bert-base-uncased", "text": "hello" }))
        .send()
        .await
        .expect("count request failed");
    assert_eq!(res.status(), 200, "count must succeed first");

    // Now the model should appear in the registry
    let models_res = srv
        .client
        .get(format!("{}/v1/models", srv.base_url))
        .send()
        .await
        .expect("models request failed");
    assert_eq!(models_res.status(), 200);

    let body: serde_json::Value = models_res.json().await.expect("json parse");
    let models = body["models"].as_array().expect("models array");
    assert!(
        models.iter().any(|m| m["model_id"] == "bert-base-uncased" && m["status"] == "loaded"),
        "bert-base-uncased should appear in /v1/models with status 'loaded' after being explicitly used"
    );
}
