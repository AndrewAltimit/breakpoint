use serde::Deserialize;

use breakpoint_core::overlay::config::OverlayRoomConfig;

/// Top-level server configuration, loaded from `breakpoint.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub web_root: String,
    pub auth: AuthFileConfig,
    pub overlay: OverlayDefaults,
    pub github: Option<GitHubConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8080".to_string(),
            web_root: "web".to_string(),
            auth: AuthFileConfig::default(),
            overlay: OverlayDefaults::default(),
            github: None,
        }
    }
}

/// Auth section of the config file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AuthFileConfig {
    pub bearer_token: Option<String>,
    pub github_webhook_secret: Option<String>,
}

/// Default overlay settings applied to new rooms.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OverlayDefaults {
    pub room_config: OverlayRoomConfig,
}

/// GitHub integration configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GitHubConfig {
    pub enabled: bool,
    pub token: Option<String>,
    pub repos: Vec<String>,
    pub poll_interval_secs: u64,
    pub agent_patterns: Vec<String>,
}

impl Default for GitHubConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: None,
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

impl ServerConfig {
    /// Load config from `breakpoint.toml` if it exists, then apply env var overrides.
    pub fn load() -> Self {
        let mut config = match std::fs::read_to_string("breakpoint.toml") {
            Ok(content) => match toml::from_str::<ServerConfig>(&content) {
                Ok(cfg) => {
                    tracing::info!("Loaded configuration from breakpoint.toml");
                    cfg
                },
                Err(e) => {
                    tracing::warn!("Failed to parse breakpoint.toml: {e}, using defaults");
                    ServerConfig::default()
                },
            },
            Err(_) => {
                tracing::info!("No breakpoint.toml found, using defaults");
                ServerConfig::default()
            },
        };

        // Environment variable overrides
        if let Ok(addr) = std::env::var("BREAKPOINT_LISTEN_ADDR")
            && !addr.is_empty()
        {
            config.listen_addr = addr;
        }
        if let Ok(root) = std::env::var("BREAKPOINT_WEB_ROOT")
            && !root.is_empty()
        {
            config.web_root = root;
        }
        if let Ok(token) = std::env::var("BREAKPOINT_API_TOKEN")
            && !token.is_empty()
        {
            config.auth.bearer_token = Some(token);
        }
        if let Ok(secret) = std::env::var("BREAKPOINT_GITHUB_SECRET")
            && !secret.is_empty()
        {
            config.auth.github_webhook_secret = Some(secret);
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.listen_addr, "0.0.0.0:8080");
        assert_eq!(cfg.web_root, "web");
        assert!(cfg.auth.bearer_token.is_none());
        assert!(cfg.github.is_none());
    }

    #[test]
    fn parse_minimal_toml() {
        let toml_str = r#"
listen_addr = "127.0.0.1:9090"
web_root = "/var/www"

[auth]
bearer_token = "secret123"
"#;
        let cfg: ServerConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.listen_addr, "127.0.0.1:9090");
        assert_eq!(cfg.web_root, "/var/www");
        assert_eq!(cfg.auth.bearer_token.as_deref(), Some("secret123"));
    }

    #[test]
    fn parse_full_toml() {
        let toml_str = r#"
listen_addr = "0.0.0.0:3000"
web_root = "dist"

[auth]
bearer_token = "mytoken"
github_webhook_secret = "webhooksecret"

[github]
enabled = true
token = "ghp_xxx"
repos = ["owner/repo1", "owner/repo2"]
poll_interval_secs = 60
agent_patterns = ["*[bot]"]
"#;
        let cfg: ServerConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.listen_addr, "0.0.0.0:3000");
        let gh = cfg.github.unwrap();
        assert!(gh.enabled);
        assert_eq!(gh.repos.len(), 2);
        assert_eq!(gh.poll_interval_secs, 60);
    }
}
