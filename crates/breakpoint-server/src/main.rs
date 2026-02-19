use tracing_subscriber::EnvFilter;

use breakpoint_server::config::ServerConfig;
use breakpoint_server::{
    build_app, spawn_event_broadcaster, spawn_idle_room_cleanup, spawn_rate_limit_cleanup,
};

#[tokio::main]
async fn main() {
    let json_logs = std::env::var("BREAKPOINT_LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    if json_logs {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .json()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }

    let config = ServerConfig::load();
    config.validate();
    let listen_addr = config.listen_addr.clone();

    let (app, state) = build_app(config);

    // Spawn background task: broadcast new events to all rooms via WSS
    spawn_event_broadcaster(state.clone());

    // Spawn idle room cleanup (removes rooms with no activity for >1 hour)
    spawn_idle_room_cleanup(state.clone());

    // Spawn rate limiter cleanup (removes stale per-IP buckets every 5 minutes)
    spawn_rate_limit_cleanup(state.clone());

    // Conditionally spawn GitHub Actions poller
    #[cfg(feature = "github-poller")]
    if let Some(ref gh) = state.config.github
        && gh.enabled
    {
        if gh.token.is_some() {
            spawn_github_poller(&state, gh);
        } else {
            tracing::warn!(
                "GitHub poller is enabled but no token is configured; \
                 skipping poller startup. Set BREAKPOINT_GITHUB_TOKEN or \
                 github.token in breakpoint.toml."
            );
        }
    }

    let listener = match tokio::net::TcpListener::bind(&listen_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind to {listen_addr}: {e}");
            std::process::exit(1);
        },
    };

    tracing::info!("Breakpoint server listening on {listen_addr}");

    let shutdown_token = state.shutdown.clone();
    if let Err(e) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown_token))
    .await
    {
        tracing::error!("Server error: {e}");
        std::process::exit(1);
    }

    tracing::info!("Server shutdown complete");
}

/// Wait for SIGINT (Ctrl-C) or SIGTERM, then trigger cancellation.
async fn shutdown_signal(token: tokio_util::sync::CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received SIGINT, initiating graceful shutdown...");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, initiating graceful shutdown...");
        }
    }

    // Signal all background tasks to stop
    token.cancel();
}

/// Spawn the GitHub Actions polling monitor as a background task.
#[cfg(feature = "github-poller")]
fn spawn_github_poller(
    state: &breakpoint_server::state::AppState,
    gh: &breakpoint_server::config::GitHubConfig,
) {
    let poller_config = breakpoint_github::GitHubPollerConfig {
        token: gh.token.clone().unwrap_or_default(),
        repos: gh.repos.clone(),
        poll_interval_secs: gh.poll_interval_secs,
        agent_patterns: gh.agent_patterns.clone(),
    };
    let poller = breakpoint_github::GitHubPoller::new(poller_config);
    let event_store = state.event_store.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Poller task
    tokio::spawn(async move {
        poller.run(tx).await;
    });

    // Relay events from poller into EventStore
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let mut store = event_store.write().await;
            store.insert(event);
        }
    });

    tracing::info!("GitHub Actions poller started");
}
