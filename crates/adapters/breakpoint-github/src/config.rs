/// Configuration for the GitHub Actions polling monitor.
#[derive(Debug, Clone)]
pub struct GitHubPollerConfig {
    /// GitHub personal access token for API authentication.
    pub token: String,
    /// Repositories to monitor in "owner/repo" format.
    pub repos: Vec<String>,
    /// Polling interval in seconds.
    pub poll_interval_secs: u64,
    /// Glob patterns for identifying agent/bot actors.
    pub agent_patterns: Vec<String>,
}

impl Default for GitHubPollerConfig {
    fn default() -> Self {
        Self {
            token: String::new(),
            repos: Vec::new(),
            poll_interval_secs: 30,
            agent_patterns: vec![
                "dependabot[bot]".to_string(),
                "github-actions[bot]".to_string(),
                "renovate[bot]".to_string(),
                "*[bot]".to_string(),
                "*-agent".to_string(),
            ],
        }
    }
}
