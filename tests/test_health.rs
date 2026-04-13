/// `GET /ping` health-check — always passes regardless of backend state.
mod common;

#[tokio::test]
async fn ping_returns_ok() {
    let srv = common::spawn_server().await;
    let res = srv
        .client
        .get(format!("{}/ping", srv.base_url))
        .send()
        .await
        .expect("request failed");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("json parse");
    assert_eq!(body["status"], "ok");
}
