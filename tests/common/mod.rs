/// Test helpers: spawn the server and build a preconfigured reqwest client.
use std::net::SocketAddr;

use reqwest::Client;
use tokio::net::TcpListener;

use tokenizer_svc::{
    config::{Config, ModelSource},
    routes::build_router,
    state::AppState,
};

/// A running test server.
pub struct TestServer {
    pub base_url: String,
    pub client: Client,
}

/// Spawn a real Axum test server on an OS-assigned random port.
///
/// The server runs in the background for the lifetime of the returned handle.
///
/// Relevant environment variables:
/// - `TOKSVC_ALLOW_ONLINE=1`  — enable on-demand downloads
/// - `TOKSVC_TEST_CACHE_DIR`   — reuse an existing tokenizer cache without wiping it
/// - `HF_TOKEN`               — HuggingFace bearer token
/// - `TOKSVC_MS_TOKEN`        — ModelScope SDK token (`m_session_id` cookie)
/// - `TOKSVC_MODEL_SOURCE`    — `huggingface` (default) or `modelscope`
pub async fn spawn_server() -> TestServer {
    let model_source = std::env::var("TOKSVC_MODEL_SOURCE")
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "modelscope" => Some(ModelSource::ModelScope),
            "huggingface" => Some(ModelSource::HuggingFace),
            _ => None,
        })
        .unwrap_or_default();

    let test_cache_dir = std::env::var("TOKSVC_TEST_CACHE_DIR").ok();

    let cfg = Config {
        host: "127.0.0.1".to_string(),
        port: 0, // OS picks a free port
        cache_dir: test_cache_dir
            .clone()
            .unwrap_or_else(|| "/tmp/toksvc-test-cache".to_string()),
        hf_token: std::env::var("HF_TOKEN").ok(),
        allow_online_download: std::env::var("TOKSVC_ALLOW_ONLINE")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false),
        model_source,
        ms_token: std::env::var("TOKSVC_MS_TOKEN").ok(),
    };

    // Wipe the default throwaway cache to avoid state leakage between test
    // runs. When a caller supplies TOKSVC_TEST_CACHE_DIR, it is intentionally
    // treated as a pre-populated cache and preserved.
    if test_cache_dir.is_none() {
        let _ = std::fs::remove_dir_all(&cfg.cache_dir);
    }
    let _ = std::fs::create_dir_all(&cfg.cache_dir);

    let state = AppState::new(&cfg).await.expect("AppState::new failed");
    let app = build_router(state, false);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind failed");
    let addr: SocketAddr = listener.local_addr().expect("local_addr");
    let base_url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("reqwest::Client build");

    TestServer { base_url, client }
}
