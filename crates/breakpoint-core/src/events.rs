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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
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
