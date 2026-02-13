#[allow(dead_code)]
mod common;

use common::{TestServer, sign_webhook};

fn pr_opened_payload() -> serde_json::Value {
    serde_json::json!({
        "action": "opened",
        "pull_request": {
            "number": 42,
            "title": "Add feature X",
            "html_url": "https://github.com/test/repo/pull/42",
            "merged": false,
            "base": {"ref": "main"}
        },
        "sender": {"login": "alice"},
        "repository": {"full_name": "test/repo"}
    })
}

fn workflow_failure_payload() -> serde_json::Value {
    serde_json::json!({
        "action": "completed",
        "workflow_run": {
            "name": "CI",
            "conclusion": "failure",
            "html_url": "https://github.com/test/repo/actions/runs/1",
            "head_branch": "main"
        },
        "sender": {"login": "bot"},
        "repository": {"full_name": "test/repo"}
    })
}

fn push_payload() -> serde_json::Value {
    serde_json::json!({
        "ref": "refs/heads/feature-branch",
        "commits": [{"id": "abc"}, {"id": "def"}],
        "compare": "https://github.com/test/repo/compare/abc...def",
        "sender": {"login": "bob"},
        "repository": {"full_name": "test/repo"}
    })
}

#[tokio::test]
async fn github_webhook_pr_opened() {
    let server = TestServer::with_auth("token", "webhook-secret").await;
    let client = reqwest::Client::new();

    let body = serde_json::to_vec(&pr_opened_payload()).unwrap();
    let sig = sign_webhook("webhook-secret", &body);

    let resp = client
        .post(format!("{}/api/v1/webhooks/github", server.base_url()))
        .header("x-github-event", "pull_request")
        .header("x-hub-signature-256", &sig)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["accepted"], 1);
    assert!(!json["event_ids"][0].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn github_webhook_invalid_signature_rejected() {
    let server = TestServer::with_auth("token", "webhook-secret").await;
    let client = reqwest::Client::new();

    let body = serde_json::to_vec(&pr_opened_payload()).unwrap();

    let resp = client
        .post(format!("{}/api/v1/webhooks/github", server.base_url()))
        .header("x-github-event", "pull_request")
        .header(
            "x-hub-signature-256",
            "sha256=0000000000000000000000000000000000000000000000000000000000000000",
        )
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn github_webhook_no_secret_allows_any() {
    // Server with no webhook secret configured
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    let body = serde_json::to_vec(&pr_opened_payload()).unwrap();

    // No signature header at all — should still succeed
    let resp = client
        .post(format!("{}/api/v1/webhooks/github", server.base_url()))
        .header("x-github-event", "pull_request")
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["accepted"], 1);
}

#[tokio::test]
async fn github_webhook_workflow_failure() {
    let server = TestServer::with_auth("token", "webhook-secret").await;
    let client = reqwest::Client::new();

    let body = serde_json::to_vec(&workflow_failure_payload()).unwrap();
    let sig = sign_webhook("webhook-secret", &body);

    let resp = client
        .post(format!("{}/api/v1/webhooks/github", server.base_url()))
        .header("x-github-event", "workflow_run")
        .header("x-hub-signature-256", &sig)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["accepted"], 1);
}

#[tokio::test]
async fn github_webhook_push_event() {
    let server = TestServer::with_auth("token", "webhook-secret").await;
    let client = reqwest::Client::new();

    let body = serde_json::to_vec(&push_payload()).unwrap();
    let sig = sign_webhook("webhook-secret", &body);

    let resp = client
        .post(format!("{}/api/v1/webhooks/github", server.base_url()))
        .header("x-github-event", "push")
        .header("x-hub-signature-256", &sig)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["accepted"], 1);
}
