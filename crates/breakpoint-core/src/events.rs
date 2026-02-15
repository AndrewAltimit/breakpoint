use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Priority tiers for alert events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    #[default]
    Ambient,
    Notice,
    Urgent,
    Critical,
}

/// Recognized event types for the overlay system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "pipeline.started")]
    PipelineStarted,
    #[serde(rename = "pipeline.succeeded")]
    PipelineSucceeded,
    #[serde(rename = "pipeline.failed")]
    PipelineFailed,
    #[serde(rename = "pr.opened")]
    PrOpened,
    #[serde(rename = "pr.reviewed")]
    PrReviewed,
    #[serde(rename = "pr.merged")]
    PrMerged,
    #[serde(rename = "pr.conflict")]
    PrConflict,
    #[serde(rename = "issue.opened")]
    IssueOpened,
    #[serde(rename = "issue.assigned")]
    IssueAssigned,
    #[serde(rename = "issue.closed")]
    IssueClosed,
    #[serde(rename = "review.requested")]
    ReviewRequested,
    #[serde(rename = "deploy.pending")]
    DeployPending,
    #[serde(rename = "deploy.completed")]
    DeployCompleted,
    #[serde(rename = "deploy.failed")]
    DeployFailed,
    #[serde(rename = "agent.started")]
    AgentStarted,
    #[serde(rename = "agent.completed")]
    AgentCompleted,
    #[serde(rename = "agent.blocked")]
    AgentBlocked,
    #[serde(rename = "agent.error")]
    AgentError,
    #[serde(rename = "security.alert")]
    SecurityAlert,
    #[serde(rename = "comment.added")]
    CommentAdded,
    #[serde(rename = "branch.pushed")]
    BranchPushed,
    #[serde(rename = "test.passed")]
    TestPassed,
    #[serde(rename = "test.failed")]
    TestFailed,
    #[serde(rename = "custom")]
    Custom,
}

/// A Breakpoint event from an external data source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event_type: EventType,
    pub source: String,
    #[serde(default)]
    pub priority: Priority,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub action_required: bool,
    #[serde(default)]
    pub group_key: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_event() -> Event {
        Event {
            id: "evt-1".to_string(),
            event_type: EventType::PrOpened,
            source: "github".to_string(),
            priority: Priority::Notice,
            title: "PR #1 opened".to_string(),
            body: Some("Description".to_string()),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            url: Some("https://github.com/test/pr/1".to_string()),
            actor: Some("alice".to_string()),
            tags: vec!["ci".to_string(), "test".to_string()],
            action_required: true,
            group_key: Some("repo:main".to_string()),
            expires_at: Some("2025-01-02T00:00:00Z".to_string()),
            metadata: {
                let mut m = HashMap::new();
                m.insert("key".to_string(), serde_json::json!("value"));
                m
            },
        }
    }

    #[test]
    fn priority_json_roundtrip() {
        for p in [
            Priority::Ambient,
            Priority::Notice,
            Priority::Urgent,
            Priority::Critical,
        ] {
            let json = serde_json::to_string(&p).unwrap();
            let back: Priority = serde_json::from_str(&json).unwrap();
            assert_eq!(p, back);
        }
    }

    #[test]
    fn priority_json_values() {
        assert_eq!(
            serde_json::to_string(&Priority::Ambient).unwrap(),
            "\"ambient\""
        );
        assert_eq!(
            serde_json::to_string(&Priority::Critical).unwrap(),
            "\"critical\""
        );
    }

    #[test]
    fn event_type_json_roundtrip() {
        let types = [
            EventType::PipelineStarted,
            EventType::PipelineSucceeded,
            EventType::PipelineFailed,
            EventType::PrOpened,
            EventType::PrReviewed,
            EventType::PrMerged,
            EventType::PrConflict,
            EventType::IssueOpened,
            EventType::IssueAssigned,
            EventType::IssueClosed,
            EventType::ReviewRequested,
            EventType::DeployPending,
            EventType::DeployCompleted,
            EventType::DeployFailed,
            EventType::AgentStarted,
            EventType::AgentCompleted,
            EventType::AgentBlocked,
            EventType::AgentError,
            EventType::SecurityAlert,
            EventType::CommentAdded,
            EventType::BranchPushed,
            EventType::TestPassed,
            EventType::TestFailed,
            EventType::Custom,
        ];
        for et in types {
            let json = serde_json::to_string(&et).unwrap();
            let back: EventType = serde_json::from_str(&json).unwrap();
            assert_eq!(et, back);
        }
    }

    #[test]
    fn event_type_serde_rename() {
        assert_eq!(
            serde_json::to_string(&EventType::PipelineFailed).unwrap(),
            "\"pipeline.failed\""
        );
        assert_eq!(
            serde_json::to_string(&EventType::PrOpened).unwrap(),
            "\"pr.opened\""
        );
        assert_eq!(
            serde_json::to_string(&EventType::AgentBlocked).unwrap(),
            "\"agent.blocked\""
        );
    }

    #[test]
    fn event_json_roundtrip() {
        let event = test_event();
        let json = serde_json::to_string(&event).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn event_msgpack_roundtrip() {
        let event = test_event();
        let bytes = rmp_serde::to_vec(&event).unwrap();
        let back: Event = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(event, back);
    }

    #[test]
    fn event_missing_optional_fields() {
        let json = r#"{
            "id": "evt-2",
            "event_type": "custom",
            "source": "test",
            "title": "Minimal event",
            "timestamp": "2025-01-01T00:00:00Z"
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert_eq!(event.id, "evt-2");
        assert_eq!(event.priority, Priority::Ambient); // default
        assert!(event.body.is_none());
        assert!(event.url.is_none());
        assert!(event.actor.is_none());
        assert!(event.tags.is_empty());
        assert!(!event.action_required);
        assert!(event.group_key.is_none());
        assert!(event.expires_at.is_none());
        assert!(event.metadata.is_empty());
    }

    #[test]
    fn event_with_metadata() {
        let mut event = test_event();
        event
            .metadata
            .insert("count".to_string(), serde_json::json!(42));
        event.metadata.insert(
            "nested".to_string(),
            serde_json::json!({"a": 1, "b": [2, 3]}),
        );
        let json = serde_json::to_string(&event).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(back.metadata["count"], serde_json::json!(42));
        assert_eq!(back.metadata["nested"]["a"], serde_json::json!(1));
    }

    #[test]
    fn priority_default_is_ambient() {
        assert_eq!(Priority::default(), Priority::Ambient);
    }
}
