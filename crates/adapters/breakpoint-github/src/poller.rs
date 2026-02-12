use std::collections::HashMap;

use serde::Deserialize;
use tokio::sync::mpsc;

use breakpoint_core::events::{Event, EventType, Priority};

use crate::agent_detect::AgentDetector;
use crate::config::GitHubPollerConfig;

/// GitHub Actions polling monitor.
pub struct GitHubPoller {
    config: GitHubPollerConfig,
    client: reqwest::Client,
    agent_detector: AgentDetector,
    /// Track active runs per repo to detect state changes.
    active_runs: HashMap<u64, RunState>,
    /// Rolling stats.
    stats: PollerStats,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct RunState {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    actor: String,
    repo: String,
    html_url: String,
}

#[derive(Debug, Default)]
struct PollerStats {
    success_24h: u32,
    failure_24h: u32,
}

impl PollerStats {
    fn pass_rate(&self) -> f32 {
        let total = self.success_24h + self.failure_24h;
        if total == 0 {
            return 100.0;
        }
        (self.success_24h as f32 / total as f32) * 100.0
    }
}

/// Partial GitHub API response for workflow runs.
#[derive(Debug, Deserialize)]
struct WorkflowRunsResponse {
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Debug, Deserialize)]
struct WorkflowRun {
    id: u64,
    name: Option<String>,
    status: String,
    conclusion: Option<String>,
    html_url: String,
    actor: Actor,
}

#[derive(Debug, Deserialize)]
struct Actor {
    login: String,
}

impl GitHubPoller {
    pub fn new(config: GitHubPollerConfig) -> Self {
        let agent_detector = AgentDetector::new(config.agent_patterns.clone());
        let client = reqwest::Client::builder()
            .user_agent("breakpoint-github-poller/0.1")
            .build()
            .expect("Failed to create HTTP client");
        Self {
            config,
            client,
            agent_detector,
            active_runs: HashMap::new(),
            stats: PollerStats::default(),
        }
    }

    /// Run the poller loop, sending events through the channel.
    pub async fn run(mut self, tx: mpsc::UnboundedSender<Event>) {
        let interval = std::time::Duration::from_secs(self.config.poll_interval_secs);
        loop {
            for repo in &self.config.repos.clone() {
                if let Err(e) = self.poll_repo(repo, &tx).await {
                    tracing::warn!(repo, error = %e, "Failed to poll repo");
                }
            }

            // Emit aggregate ticker event
            let active_count = self
                .active_runs
                .values()
                .filter(|r| r.status != "completed")
                .count();
            let aggregate = Event {
                id: format!("gh-agg-{}", uuid_simple()),
                event_type: EventType::Custom,
                source: "github-actions".to_string(),
                priority: Priority::Ambient,
                title: format!(
                    "CI: {:.0}% pass rate, {} active runs",
                    self.stats.pass_rate(),
                    active_count
                ),
                body: None,
                timestamp: now_iso(),
                url: None,
                actor: None,
                tags: vec!["aggregate".to_string()],
                action_required: false,
                group_key: Some("github:ci-aggregate".to_string()),
                expires_at: None,
                metadata: HashMap::new(),
            };
            let _ = tx.send(aggregate);

            tokio::time::sleep(interval).await;
        }
    }

    async fn poll_repo(
        &mut self,
        repo: &str,
        tx: &mpsc::UnboundedSender<Event>,
    ) -> Result<(), String> {
        let url = format!(
            "https://api.github.com/repos/{repo}/actions/runs?per_page=20&status=in_progress"
        );

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("GitHub API returned {}", resp.status()));
        }

        let runs: WorkflowRunsResponse = resp.json().await.map_err(|e| e.to_string())?;

        for run in runs.workflow_runs {
            let run_name = run.name.as_deref().unwrap_or("workflow");
            let is_agent = self.agent_detector.detect(&run.actor.login);

            if !self.active_runs.contains_key(&run.id) {
                // New run detected
                let mut metadata = HashMap::new();
                if is_agent {
                    metadata.insert("is_agent".to_string(), serde_json::Value::Bool(true));
                }
                metadata.insert(
                    "repo".to_string(),
                    serde_json::Value::String(repo.to_string()),
                );

                let event = Event {
                    id: format!("gh-run-{}", run.id),
                    event_type: EventType::PipelineStarted,
                    source: "github-actions".to_string(),
                    priority: Priority::Ambient,
                    title: format!("{run_name} started on {repo}"),
                    body: None,
                    timestamp: now_iso(),
                    url: Some(run.html_url.clone()),
                    actor: Some(run.actor.login.clone()),
                    tags: vec!["ci".to_string()],
                    action_required: false,
                    group_key: Some(format!("github:{repo}:runs")),
                    expires_at: None,
                    metadata,
                };
                let _ = tx.send(event);
            }

            self.active_runs.insert(
                run.id,
                RunState {
                    id: run.id,
                    name: run_name.to_string(),
                    status: run.status.clone(),
                    conclusion: run.conclusion.clone(),
                    actor: run.actor.login.clone(),
                    repo: repo.to_string(),
                    html_url: run.html_url.clone(),
                },
            );
        }

        // Also poll completed runs to detect transitions
        let completed_url = format!(
            "https://api.github.com/repos/{repo}/actions/runs?per_page=10&status=completed"
        );
        let completed_resp = self
            .client
            .get(&completed_url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if completed_resp.status().is_success() {
            let completed: WorkflowRunsResponse =
                completed_resp.json().await.map_err(|e| e.to_string())?;
            for run in completed.workflow_runs {
                if let Some(prev) = self.active_runs.remove(&run.id)
                    && prev.status != "completed"
                {
                    // Run just completed — emit event
                    let is_agent = self.agent_detector.detect(&run.actor.login);
                    let (event_type, priority) = match run.conclusion.as_deref() {
                        Some("success") => {
                            self.stats.success_24h += 1;
                            (EventType::PipelineSucceeded, Priority::Ambient)
                        },
                        Some("failure") => {
                            self.stats.failure_24h += 1;
                            (EventType::PipelineFailed, Priority::Notice)
                        },
                        _ => {
                            self.stats.failure_24h += 1;
                            (EventType::PipelineFailed, Priority::Ambient)
                        },
                    };

                    let run_name = run.name.as_deref().unwrap_or("workflow");
                    let conclusion = run.conclusion.as_deref().unwrap_or("unknown");

                    let mut metadata = HashMap::new();
                    if is_agent {
                        metadata.insert("is_agent".to_string(), serde_json::Value::Bool(true));
                    }

                    let event = Event {
                        id: format!("gh-run-{}-done", run.id),
                        event_type,
                        source: "github-actions".to_string(),
                        priority,
                        title: format!("{run_name} {conclusion} on {repo}"),
                        body: None,
                        timestamp: now_iso(),
                        url: Some(run.html_url.clone()),
                        actor: Some(run.actor.login.clone()),
                        tags: vec!["ci".to_string()],
                        action_required: conclusion == "failure",
                        group_key: None,
                        expires_at: None,
                        metadata,
                    };
                    let _ = tx.send(event);
                }
            }
        }

        Ok(())
    }
}

fn now_iso() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}Z", dur.as_secs())
}

fn uuid_simple() -> String {
    uuid::Uuid::new_v4().to_string()[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poller_stats_default() {
        let stats = PollerStats::default();
        assert_eq!(stats.pass_rate(), 100.0);
    }

    #[test]
    fn poller_stats_with_data() {
        let stats = PollerStats {
            success_24h: 9,
            failure_24h: 1,
        };
        assert!((stats.pass_rate() - 90.0).abs() < 0.01);
    }

    #[test]
    fn run_state_tracking() {
        let config = GitHubPollerConfig {
            token: "test".to_string(),
            ..GitHubPollerConfig::default()
        };
        let poller = GitHubPoller::new(config);
        assert!(poller.active_runs.is_empty());
    }
}
