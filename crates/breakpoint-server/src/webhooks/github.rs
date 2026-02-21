use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use breakpoint_core::events::{Event, EventType, Priority};

use crate::auth::verify_github_signature;
use crate::state::AppState;

/// Response from the GitHub webhook handler.
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub accepted: usize,
    pub event_ids: Vec<String>,
}

/// POST /api/v1/webhooks/github — handle GitHub webhook payloads.
pub async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), (StatusCode, String)> {
    // Verify HMAC signature if secret is configured
    if let Some(ref secret) = state.auth.github_webhook_secret {
        let signature = headers
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .ok_or((
                StatusCode::UNAUTHORIZED,
                "Missing signature header".to_string(),
            ))?;

        if !verify_github_signature(signature, secret, &body) {
            return Err((StatusCode::UNAUTHORIZED, "Invalid signature".to_string()));
        }
    } else if state.auth.require_webhook_signature {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Webhook signature required but no secret configured".to_string(),
        ));
    } else {
        tracing::warn!("GitHub webhook accepted without HMAC verification (no secret configured)");
    }

    let gh_event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let payload: Value = serde_json::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")))?;

    let events = transform_github_event(gh_event, &payload);

    let mut event_ids = Vec::with_capacity(events.len());
    let mut store = state.event_store.write().await;
    for event in events {
        event_ids.push(event.id.clone());
        store.insert(event);
    }

    Ok((
        StatusCode::OK,
        Json(WebhookResponse {
            accepted: event_ids.len(),
            event_ids,
        }),
    ))
}

/// Transform a GitHub webhook event into Breakpoint events.
fn transform_github_event(gh_event: &str, payload: &Value) -> Vec<Event> {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let sender = payload
        .get("sender")
        .and_then(|s| s.get("login"))
        .and_then(|l| l.as_str())
        .unwrap_or("unknown");
    let repo = payload
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown");

    match gh_event {
        "workflow_run" => transform_workflow_run(action, payload, sender, repo),
        "pull_request" => transform_pull_request(action, payload, sender, repo),
        "push" => transform_push(payload, sender, repo),
        "issues" => transform_issues(action, payload, sender, repo),
        "issue_comment" => transform_issue_comment(payload, sender, repo),
        "deployment_status" => transform_deployment_status(payload, sender, repo),
        _ => vec![], // Unknown event type — accept silently
    }
}

fn transform_workflow_run(action: &str, payload: &Value, sender: &str, repo: &str) -> Vec<Event> {
    let workflow = &payload["workflow_run"];
    let name = workflow["name"].as_str().unwrap_or("workflow");
    let conclusion = workflow["conclusion"].as_str().unwrap_or("");
    let url = workflow["html_url"].as_str().map(String::from);
    let branch = workflow["head_branch"].as_str().unwrap_or("unknown");

    let (event_type, priority, title) = match (action, conclusion) {
        ("completed", "success") => (
            EventType::PipelineSucceeded,
            Priority::Ambient,
            format!("{name} succeeded on {repo}"),
        ),
        ("completed", "failure") => (
            EventType::PipelineFailed,
            Priority::Notice,
            format!("{name} failed on {repo}"),
        ),
        ("requested", _) | ("in_progress", _) => (
            EventType::PipelineStarted,
            Priority::Ambient,
            format!("{name} started on {repo}"),
        ),
        _ => return vec![],
    };

    vec![make_event(
        event_type,
        priority,
        title,
        sender,
        repo,
        url,
        vec![format!("repo:{repo}"), format!("branch:{branch}")],
    )]
}

fn transform_pull_request(action: &str, payload: &Value, sender: &str, repo: &str) -> Vec<Event> {
    let pr = &payload["pull_request"];
    let number = pr["number"].as_u64().unwrap_or(0);
    let pr_title = pr["title"].as_str().unwrap_or("PR");
    let url = pr["html_url"].as_str().map(String::from);
    let merged = pr["merged"].as_bool().unwrap_or(false);
    let base_ref = pr
        .get("base")
        .and_then(|b| b.get("ref"))
        .and_then(|r| r.as_str())
        .unwrap_or("unknown");

    let (event_type, priority, title) = match action {
        "opened" => (
            EventType::PrOpened,
            Priority::Notice,
            format!("PR #{number}: {pr_title}"),
        ),
        "closed" if merged => (
            EventType::PrMerged,
            Priority::Ambient,
            format!("PR #{number} merged: {pr_title}"),
        ),
        "review_requested" => (
            EventType::ReviewRequested,
            Priority::Notice,
            format!("Review requested on PR #{number}: {pr_title}"),
        ),
        _ => return vec![],
    };

    vec![make_event(
        event_type,
        priority,
        title,
        sender,
        repo,
        url,
        vec![format!("repo:{repo}"), format!("branch:{base_ref}")],
    )]
}

fn transform_push(payload: &Value, sender: &str, repo: &str) -> Vec<Event> {
    let git_ref = payload["ref"].as_str().unwrap_or("refs/heads/unknown");
    let branch = git_ref.strip_prefix("refs/heads/").unwrap_or(git_ref);
    let commits = payload["commits"].as_array().map(|c| c.len()).unwrap_or(0);
    let url = payload["compare"].as_str().map(String::from);

    vec![make_event(
        EventType::BranchPushed,
        Priority::Ambient,
        format!("{sender} pushed {commits} commit(s) to {branch}"),
        sender,
        repo,
        url,
        vec![format!("repo:{repo}"), format!("branch:{branch}")],
    )]
}

fn transform_issues(action: &str, payload: &Value, sender: &str, repo: &str) -> Vec<Event> {
    let issue = &payload["issue"];
    let number = issue["number"].as_u64().unwrap_or(0);
    let issue_title = issue["title"].as_str().unwrap_or("Issue");
    let url = issue["html_url"].as_str().map(String::from);

    let (event_type, priority, title) = match action {
        "opened" => (
            EventType::IssueOpened,
            Priority::Ambient,
            format!("Issue #{number}: {issue_title}"),
        ),
        "closed" => (
            EventType::IssueClosed,
            Priority::Ambient,
            format!("Issue #{number} closed: {issue_title}"),
        ),
        _ => return vec![],
    };

    vec![make_event(
        event_type,
        priority,
        title,
        sender,
        repo,
        url,
        vec![format!("repo:{repo}")],
    )]
}

fn transform_issue_comment(payload: &Value, sender: &str, repo: &str) -> Vec<Event> {
    let issue = &payload["issue"];
    let number = issue["number"].as_u64().unwrap_or(0);
    let url = payload
        .get("comment")
        .and_then(|c| c.get("html_url"))
        .and_then(|u| u.as_str())
        .map(String::from);

    vec![make_event(
        EventType::CommentAdded,
        Priority::Ambient,
        format!("{sender} commented on #{number}"),
        sender,
        repo,
        url,
        vec![format!("repo:{repo}")],
    )]
}

fn transform_deployment_status(payload: &Value, sender: &str, repo: &str) -> Vec<Event> {
    let ds = &payload["deployment_status"];
    let state_str = ds["state"].as_str().unwrap_or("");
    let env = ds["environment"].as_str().unwrap_or("production");
    let url = ds["target_url"].as_str().map(String::from);

    let (event_type, priority, title) = match state_str {
        "pending" => (
            EventType::DeployPending,
            Priority::Urgent,
            format!("Deploy pending to {env}"),
        ),
        "success" => (
            EventType::DeployCompleted,
            Priority::Ambient,
            format!("Deploy completed to {env}"),
        ),
        "failure" | "error" => (
            EventType::DeployFailed,
            Priority::Urgent,
            format!("Deploy failed to {env}"),
        ),
        _ => return vec![],
    };

    vec![make_event(
        event_type,
        priority,
        title,
        sender,
        repo,
        url,
        vec![format!("repo:{repo}"), format!("env:{env}")],
    )]
}

fn make_event(
    event_type: EventType,
    priority: Priority,
    title: String,
    actor: &str,
    repo: &str,
    url: Option<String>,
    tags: Vec<String>,
) -> Event {
    Event {
        id: Uuid::new_v4().to_string(),
        event_type,
        source: "github".to_string(),
        priority,
        title,
        body: None,
        timestamp: breakpoint_core::time::timestamp_now(),
        url,
        actor: Some(actor.to_string()),
        tags,
        action_required: false,
        group_key: Some(format!("github:{repo}")),
        expires_at: None,
        metadata: std::collections::HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_payload(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn workflow_run_success() {
        let payload = make_payload(
            r#"{
                "action": "completed",
                "workflow_run": {
                    "name": "CI",
                    "conclusion": "success",
                    "html_url": "https://github.com/test/repo/actions/runs/1",
                    "head_branch": "main"
                },
                "sender": {"login": "bot"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("workflow_run", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::PipelineSucceeded);
        assert_eq!(events[0].priority, Priority::Ambient);
        assert!(events[0].title.contains("CI succeeded"));
    }

    #[test]
    fn workflow_run_failure() {
        let payload = make_payload(
            r#"{
                "action": "completed",
                "workflow_run": {
                    "name": "CI",
                    "conclusion": "failure",
                    "html_url": "https://github.com/test/repo/actions/runs/1",
                    "head_branch": "main"
                },
                "sender": {"login": "bot"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("workflow_run", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::PipelineFailed);
        assert_eq!(events[0].priority, Priority::Notice);
    }

    #[test]
    fn workflow_run_started() {
        let payload = make_payload(
            r#"{
                "action": "requested",
                "workflow_run": {
                    "name": "CI",
                    "conclusion": null,
                    "html_url": "https://github.com/test/repo/actions/runs/1",
                    "head_branch": "main"
                },
                "sender": {"login": "bot"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("workflow_run", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::PipelineStarted);
    }

    #[test]
    fn pr_opened() {
        let payload = make_payload(
            r#"{
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
            }"#,
        );
        let events = transform_github_event("pull_request", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::PrOpened);
        assert_eq!(events[0].priority, Priority::Notice);
        assert!(events[0].title.contains("#42"));
    }

    #[test]
    fn pr_merged() {
        let payload = make_payload(
            r#"{
                "action": "closed",
                "pull_request": {
                    "number": 42,
                    "title": "Add feature X",
                    "html_url": "https://github.com/test/repo/pull/42",
                    "merged": true,
                    "base": {"ref": "main"}
                },
                "sender": {"login": "alice"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("pull_request", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::PrMerged);
    }

    #[test]
    fn pr_review_requested() {
        let payload = make_payload(
            r#"{
                "action": "review_requested",
                "pull_request": {
                    "number": 42,
                    "title": "Add feature X",
                    "html_url": "https://github.com/test/repo/pull/42",
                    "merged": false,
                    "base": {"ref": "main"}
                },
                "sender": {"login": "alice"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("pull_request", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::ReviewRequested);
        assert_eq!(events[0].priority, Priority::Notice);
    }

    #[test]
    fn push_event() {
        let payload = make_payload(
            r#"{
                "ref": "refs/heads/feature-branch",
                "commits": [{"id": "abc"}, {"id": "def"}],
                "compare": "https://github.com/test/repo/compare/abc...def",
                "sender": {"login": "bob"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("push", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::BranchPushed);
        assert!(events[0].title.contains("2 commit(s)"));
        assert!(events[0].title.contains("feature-branch"));
    }

    #[test]
    fn issue_opened() {
        let payload = make_payload(
            r#"{
                "action": "opened",
                "issue": {
                    "number": 10,
                    "title": "Bug report",
                    "html_url": "https://github.com/test/repo/issues/10"
                },
                "sender": {"login": "carol"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("issues", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::IssueOpened);
    }

    #[test]
    fn issue_closed() {
        let payload = make_payload(
            r#"{
                "action": "closed",
                "issue": {
                    "number": 10,
                    "title": "Bug report",
                    "html_url": "https://github.com/test/repo/issues/10"
                },
                "sender": {"login": "carol"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("issues", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::IssueClosed);
    }

    #[test]
    fn issue_comment() {
        let payload = make_payload(
            r#"{
                "issue": {"number": 10},
                "comment": {"html_url": "https://github.com/test/repo/issues/10#comment-1"},
                "sender": {"login": "dave"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("issue_comment", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::CommentAdded);
    }

    #[test]
    fn deployment_status_pending() {
        let payload = make_payload(
            r#"{
                "deployment_status": {
                    "state": "pending",
                    "environment": "production",
                    "target_url": "https://example.com/deploy/1"
                },
                "sender": {"login": "deployer"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("deployment_status", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::DeployPending);
        assert_eq!(events[0].priority, Priority::Urgent);
    }

    #[test]
    fn deployment_status_success() {
        let payload = make_payload(
            r#"{
                "deployment_status": {
                    "state": "success",
                    "environment": "staging",
                    "target_url": null
                },
                "sender": {"login": "deployer"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("deployment_status", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::DeployCompleted);
        assert_eq!(events[0].priority, Priority::Ambient);
    }

    #[test]
    fn deployment_status_failure() {
        let payload = make_payload(
            r#"{
                "deployment_status": {
                    "state": "failure",
                    "environment": "production",
                    "target_url": null
                },
                "sender": {"login": "deployer"},
                "repository": {"full_name": "test/repo"}
            }"#,
        );
        let events = transform_github_event("deployment_status", &payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::DeployFailed);
        assert_eq!(events[0].priority, Priority::Urgent);
    }

    #[test]
    fn unknown_event_type_returns_empty() {
        let payload =
            make_payload(r#"{"sender": {"login": "x"}, "repository": {"full_name": "y"}}"#);
        let events = transform_github_event("unknown_event", &payload);
        assert!(events.is_empty());
    }

    #[test]
    fn signature_verification_pass() {
        let secret = "webhook-secret";
        let body = br#"{"action":"opened"}"#;

        use hmac::Mac;
        let mut mac = <hmac::Hmac<sha2::Sha256>>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let result = mac.finalize().into_bytes();
        let sig = format!("sha256={}", hex::encode(result));

        assert!(verify_github_signature(&sig, secret, body));
    }

    #[test]
    fn signature_verification_fail() {
        assert!(!verify_github_signature(
            "sha256=0000000000000000000000000000000000000000000000000000000000000000",
            "webhook-secret",
            br#"{"action":"opened"}"#
        ));
    }
}
