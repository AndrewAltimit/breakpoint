#[allow(dead_code)]
mod common;

use std::time::Duration;

use common::{TestServer, make_event};

#[tokio::test]
async fn sse_receives_posted_event() {
    let server = TestServer::new().await;
    let sse_url = format!("{}/api/v1/events/stream", server.base_url());
    let base_url = server.base_url();

    // Spawn a task that will post an event after a short delay
    let post_url = format!("{base_url}/api/v1/events");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let client = reqwest::Client::new();
        let event = make_event("sse-test-1");
        let _ = client.post(&post_url).json(&event).send().await;
    });

    // Connect to SSE and read chunks until we see our event or timeout
    let client = reqwest::Client::new();
    let sse_resp = client.get(&sse_url).send().await.unwrap();
    assert_eq!(sse_resp.status(), 200);

    let mut collected = String::new();
    let found = tokio::time::timeout(Duration::from_secs(3), async {
        let mut resp = sse_resp;
        loop {
            match resp.chunk().await {
                Ok(Some(bytes)) => {
                    collected.push_str(&String::from_utf8_lossy(&bytes));
                    if collected.contains("sse-test-1") {
                        return true;
                    }
                },
                _ => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        found,
        "SSE stream should contain the posted event ID, got: {collected}"
    );
}

#[tokio::test]
async fn sse_returns_503_when_at_capacity() {
    use breakpoint_server::config::{LimitsConfig, ServerConfig};

    let config = ServerConfig {
        limits: LimitsConfig {
            max_sse_subscribers: 1,
            ..LimitsConfig::default()
        },
        ..ServerConfig::default()
    };
    let server = TestServer::from_config(config).await;
    let client = reqwest::Client::new();
    let sse_url = format!("{}/api/v1/events/stream", server.base_url());

    // First SSE connection should succeed
    let resp1 = client.get(&sse_url).send().await.unwrap();
    assert_eq!(resp1.status(), 200);

    // Give it a moment to register
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Second SSE connection should be rejected
    let resp2 = client.get(&sse_url).send().await.unwrap();
    assert_eq!(
        resp2.status(),
        503,
        "Should reject when SSE subscriber limit reached"
    );
}
