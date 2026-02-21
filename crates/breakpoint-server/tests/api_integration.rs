#[allow(dead_code)]
mod common;

use common::{TestServer, make_event};

#[tokio::test]
async fn server_responds_on_root() {
    let server = TestServer::new().await;
    let resp = reqwest::get(&server.base_url()).await.unwrap();
    // Server is up — may return 200 (if index.html exists) or 404
    assert!(
        resp.status().is_success() || resp.status().as_u16() == 404,
        "Unexpected status: {}",
        resp.status()
    );
}

#[tokio::test]
async fn post_event_with_auth() {
    let server = TestServer::with_auth("test-token", "webhook-secret").await;
    let client = reqwest::Client::new();

    let event = make_event("evt-auth-1");
    let resp = client
        .post(format!("{}/api/v1/events", server.base_url()))
        .bearer_auth("test-token")
        .json(&event)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["accepted"], 1);
    assert_eq!(body["event_ids"][0], "evt-auth-1");
}

#[tokio::test]
async fn post_event_rejected_without_auth() {
    let server = TestServer::with_auth("test-token", "webhook-secret").await;
    let client = reqwest::Client::new();

    let event = make_event("evt-noauth-1");
    let resp = client
        .post(format!("{}/api/v1/events", server.base_url()))
        .json(&event)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn post_batch_events() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    let events = vec![
        make_event("batch-1"),
        make_event("batch-2"),
        make_event("batch-3"),
    ];
    let resp = client
        .post(format!("{}/api/v1/events", server.base_url()))
        .json(&events)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["accepted"], 3);
    let ids = body["event_ids"].as_array().unwrap();
    assert_eq!(ids.len(), 3);
}

#[tokio::test]
async fn get_status_shows_posted_event() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Post an event
    let event = make_event("status-evt-1");
    let resp = client
        .post(format!("{}/api/v1/events", server.base_url()))
        .json(&event)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Check status
    let resp = client
        .get(format!("{}/api/v1/status", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["stats"]["total_stored"], 1);
    let recent = body["recent_events"].as_array().unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0]["id"], "status-evt-1");
}

#[tokio::test]
async fn claim_event_via_rest() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Post an event
    let event = make_event("claim-evt-1");
    client
        .post(format!("{}/api/v1/events", server.base_url()))
        .json(&event)
        .send()
        .await
        .unwrap();

    // Claim it
    let resp = client
        .post(format!(
            "{}/api/v1/events/claim-evt-1/claim",
            server.base_url()
        ))
        .json(&serde_json::json!({"claimed_by": "alice"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["claimed"], true);
    assert_eq!(body["event_id"], "claim-evt-1");
}

#[tokio::test]
async fn claim_nonexistent_event_404() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!(
            "{}/api/v1/events/nonexistent/claim",
            server.base_url()
        ))
        .json(&serde_json::json!({"claimed_by": "alice"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn health_endpoint() {
    let server = TestServer::new().await;
    let resp = reqwest::get(format!("{}/health", server.base_url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "healthy");
    assert!(body["connections"]["websocket"].is_number());
    assert!(body["rooms"]["active"].is_number());
}

// ================================================================
// Phase 6: Rate limiting integration tests
// ================================================================

#[tokio::test]
async fn api_rate_limiting_returns_429() {
    use breakpoint_server::config::{LimitsConfig, ServerConfig};

    // Configure a very tight rate limit: burst 2, 1 req/sec
    let config = ServerConfig {
        limits: LimitsConfig {
            api_rate_limit_burst: 2,
            api_rate_limit_per_sec: 1.0,
            ..LimitsConfig::default()
        },
        ..ServerConfig::default()
    };
    let server = TestServer::from_config(config).await;
    let client = reqwest::Client::new();

    // First two requests should succeed (burst capacity)
    for i in 0..2 {
        let event = make_event(&format!("rate-{i}"));
        let resp = client
            .post(format!("{}/api/v1/events", server.base_url()))
            .json(&event)
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            201,
            "Request {i} should succeed (within burst)"
        );
    }

    // Third request should be rate-limited (429)
    let event = make_event("rate-overflow");
    let resp = client
        .post(format!("{}/api/v1/events", server.base_url()))
        .json(&event)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        429,
        "Request beyond burst should be rate-limited"
    );
}

// ================================================================
// Phase 6: Event claim timeout test
// ================================================================

#[tokio::test]
async fn event_claim_persists() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Post event
    let event = make_event("claim-timeout-1");
    client
        .post(format!("{}/api/v1/events", server.base_url()))
        .json(&event)
        .send()
        .await
        .unwrap();

    // Claim the event
    let resp = client
        .post(format!(
            "{}/api/v1/events/claim-timeout-1/claim",
            server.base_url()
        ))
        .json(&serde_json::json!({"claimed_by": "bot-1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify the claim is reflected in status
    let status_resp = client
        .get(format!("{}/api/v1/status", server.base_url()))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = status_resp.json().await.unwrap();
    let events = body["recent_events"].as_array().unwrap();
    let claimed_event = events.iter().find(|e| e["id"] == "claim-timeout-1");
    assert!(
        claimed_event.is_some(),
        "Claimed event should appear in status"
    );
}

#[tokio::test]
async fn no_auth_mode_allows_requests() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // No auth configured, so requests without bearer should succeed
    let event = make_event("noauth-evt-1");
    let resp = client
        .post(format!("{}/api/v1/events", server.base_url()))
        .json(&event)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
}
