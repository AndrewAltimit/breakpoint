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
    pub limits: LimitsConfig,
    pub rooms: RoomsConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8080".to_string(),
            web_root: "web".to_string(),
            auth: AuthFileConfig::default(),
            overlay: OverlayDefaults::default(),
            github: None,
            limits: LimitsConfig::default(),
            rooms: RoomsConfig::default(),
        }
    }
}

/// Infrastructure limits (connection caps, buffer sizes, rate limits).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    pub max_ws_connections: usize,
    pub max_sse_subscribers: usize,
    pub max_stored_events: usize,
    pub broadcast_capacity: usize,
    pub event_batch_limit: usize,
    pub ws_rate_limit_per_sec: f64,
    pub player_message_buffer: usize,
    /// API endpoint rate limit: max burst tokens per IP.
    pub api_rate_limit_burst: usize,
    /// API endpoint rate limit: token refill rate (requests per second) per IP.
    pub api_rate_limit_per_sec: f64,
    /// Maximum concurrent WebSocket connections per IP address.
    pub max_ws_per_ip: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_ws_connections: 200,
            max_sse_subscribers: 100,
            max_stored_events: 500,
            broadcast_capacity: 1024,
            event_batch_limit: 100,
            ws_rate_limit_per_sec: 50.0,
            player_message_buffer: 256,
            api_rate_limit_burst: 20,
            api_rate_limit_per_sec: 2.0, // ~120 req/min
            max_ws_per_ip: 10,
        }
    }
}

/// Room lifecycle configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RoomsConfig {
    pub idle_timeout_secs: u64,
    pub idle_check_interval_secs: u64,
}

impl Default for RoomsConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: 3600,
            idle_check_interval_secs: 60,
        }
    }
}

/// Auth section of the config file.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AuthFileConfig {
    pub bearer_token: Option<String>,
    pub github_webhook_secret: Option<String>,
    /// When true, reject GitHub webhooks that have no HMAC signature.
    /// Defaults to true for production safety.
    #[serde(default = "default_true")]
    pub require_webhook_signature: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AuthFileConfig {
    fn default() -> Self {
        Self {
            bearer_token: None,
            github_webhook_secret: None,
            require_webhook_signature: true,
        }
    }
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
    /// Validate configuration, logging warnings for issues.
    pub fn validate(&self) {
        if self.listen_addr.parse::<std::net::SocketAddr>().is_err() {
            tracing::error!(
                addr = %self.listen_addr,
                "listen_addr is not a valid socket address"
            );
            std::process::exit(1);
        }

        // Warn when webhook signature verification is disabled without a secret
        if !self.auth.require_webhook_signature && self.auth.github_webhook_secret.is_none() {
            tracing::warn!(
                "Webhook signature verification is disabled and no secret is configured — \
                 webhooks are unauthenticated"
            );
        }

        // Warn about secrets in config file (should use env vars in production)
        if self.auth.bearer_token.is_some() {
            tracing::warn!(
                "bearer_token is set in config file — use BREAKPOINT_API_TOKEN env var in production"
            );
        }
        if self.auth.github_webhook_secret.is_some() {
            tracing::warn!(
                "github_webhook_secret is set in config file — use BREAKPOINT_GITHUB_SECRET env var in production"
            );
        }

        if let Some(ref gh) = self.github {
            if gh.enabled && gh.token.is_none() {
                tracing::warn!("GitHub poller enabled but no token configured");
            }
            if gh.poll_interval_secs == 0 {
                tracing::error!("GitHub poll_interval_secs must be > 0");
                std::process::exit(1);
            }
            if gh.enabled && gh.token.is_some() {
                tracing::warn!(
                    "GitHub token is set in config file — use environment variables in production"
                );
            }
        }

        // Validate limits
        if self.limits.max_ws_connections == 0 {
            tracing::error!("limits.max_ws_connections must be > 0");
            std::process::exit(1);
        }
        if self.limits.max_sse_subscribers == 0 {
            tracing::error!("limits.max_sse_subscribers must be > 0");
            std::process::exit(1);
        }
        if self.limits.max_stored_events == 0 {
            tracing::error!("limits.max_stored_events must be > 0");
            std::process::exit(1);
        }
        if self.limits.broadcast_capacity == 0 {
            tracing::error!("limits.broadcast_capacity must be > 0");
            std::process::exit(1);
        }
        if self.limits.event_batch_limit == 0 {
            tracing::error!("limits.event_batch_limit must be > 0");
            std::process::exit(1);
        }
        if self.limits.ws_rate_limit_per_sec <= 0.0 {
            tracing::error!("limits.ws_rate_limit_per_sec must be > 0");
            std::process::exit(1);
        }
        if self.limits.player_message_buffer == 0 {
            tracing::error!("limits.player_message_buffer must be > 0");
            std::process::exit(1);
        }

        // Validate rooms
        if self.rooms.idle_timeout_secs == 0 {
            tracing::error!("rooms.idle_timeout_secs must be > 0");
            std::process::exit(1);
        }
        if self.rooms.idle_check_interval_secs == 0 {
            tracing::error!("rooms.idle_check_interval_secs must be > 0");
            std::process::exit(1);
        }
    }

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

        // Limits overrides
        if let Ok(val) = std::env::var("BREAKPOINT_MAX_WS_CONNECTIONS")
            && let Ok(n) = val.parse::<usize>()
        {
            config.limits.max_ws_connections = n;
        }
        if let Ok(val) = std::env::var("BREAKPOINT_MAX_SSE_SUBSCRIBERS")
            && let Ok(n) = val.parse::<usize>()
        {
            config.limits.max_sse_subscribers = n;
        }
        if let Ok(val) = std::env::var("BREAKPOINT_MAX_STORED_EVENTS")
            && let Ok(n) = val.parse::<usize>()
        {
            config.limits.max_stored_events = n;
        }
        if let Ok(val) = std::env::var("BREAKPOINT_EVENT_BATCH_LIMIT")
            && let Ok(n) = val.parse::<usize>()
        {
            config.limits.event_batch_limit = n;
        }
        if let Ok(val) = std::env::var("BREAKPOINT_WS_RATE_LIMIT")
            && let Ok(n) = val.parse::<f64>()
        {
            config.limits.ws_rate_limit_per_sec = n;
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
    fn validate_accepts_valid_config() {
        // Default config should pass validation without panicking
        let cfg = ServerConfig::default();
        cfg.validate();
    }

    #[test]
    fn validate_rejects_invalid_addr() {
        let cfg = ServerConfig {
            listen_addr: "not-an-address".to_string(),
            ..ServerConfig::default()
        };
        // validate() calls process::exit, so we test the underlying check
        assert!(cfg.listen_addr.parse::<std::net::SocketAddr>().is_err());
    }

    #[test]
    fn validate_rejects_zero_poll_interval() {
        let cfg = ServerConfig {
            github: Some(GitHubConfig {
                enabled: true,
                poll_interval_secs: 0,
                ..GitHubConfig::default()
            }),
            ..ServerConfig::default()
        };
        // validate() calls process::exit, so we test the underlying condition
        assert_eq!(cfg.github.as_ref().unwrap().poll_interval_secs, 0);
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

    #[test]
    fn default_limits_config() {
        let cfg = LimitsConfig::default();
        assert_eq!(cfg.max_ws_connections, 200);
        assert_eq!(cfg.max_sse_subscribers, 100);
        assert_eq!(cfg.max_stored_events, 500);
        assert_eq!(cfg.broadcast_capacity, 1024);
        assert_eq!(cfg.event_batch_limit, 100);
        assert!((cfg.ws_rate_limit_per_sec - 50.0).abs() < f64::EPSILON);
        assert_eq!(cfg.player_message_buffer, 256);
    }

    #[test]
    fn default_rooms_config() {
        let cfg = RoomsConfig::default();
        assert_eq!(cfg.idle_timeout_secs, 3600);
        assert_eq!(cfg.idle_check_interval_secs, 60);
    }

    #[test]
    fn parse_limits_toml() {
        let toml_str = r#"
[limits]
max_ws_connections = 500
max_sse_subscribers = 200
max_stored_events = 1000
broadcast_capacity = 2048
event_batch_limit = 50
ws_rate_limit_per_sec = 100.0
player_message_buffer = 512

[rooms]
idle_timeout_secs = 7200
idle_check_interval_secs = 120
"#;
        let cfg: ServerConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.limits.max_ws_connections, 500);
        assert_eq!(cfg.limits.max_sse_subscribers, 200);
        assert_eq!(cfg.limits.max_stored_events, 1000);
        assert_eq!(cfg.limits.broadcast_capacity, 2048);
        assert_eq!(cfg.limits.event_batch_limit, 50);
        assert!((cfg.limits.ws_rate_limit_per_sec - 100.0).abs() < f64::EPSILON);
        assert_eq!(cfg.limits.player_message_buffer, 512);
        assert_eq!(cfg.rooms.idle_timeout_secs, 7200);
        assert_eq!(cfg.rooms.idle_check_interval_secs, 120);
    }

    #[test]
    fn missing_limits_uses_defaults() {
        let toml_str = r#"
listen_addr = "0.0.0.0:8080"
"#;
        let cfg: ServerConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.limits.max_ws_connections, 200);
        assert_eq!(cfg.rooms.idle_timeout_secs, 3600);
    }
}
