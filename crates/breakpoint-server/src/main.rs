use tracing_subscriber::EnvFilter;

use breakpoint_server::config::ServerConfig;
use breakpoint_server::{build_app, spawn_event_broadcaster, spawn_idle_room_cleanup};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::load();
    config.validate();
    let listen_addr = config.listen_addr.clone();

    let (app, state) = build_app(config);

    // Spawn background task: broadcast new events to all rooms via WSS
    spawn_event_broadcaster(state.clone());

    // Spawn idle room cleanup (removes rooms with no activity for >1 hour)
    spawn_idle_room_cleanup(state.clone());

    // Conditionally spawn GitHub Actions poller
    #[cfg(feature = "github-poller")]
    if let Some(ref gh) = state.config.github
        && gh.enabled
        && gh.token.is_some()
    {
        spawn_github_poller(&state);
    }

    let listener = match tokio::net::TcpListener::bind(&listen_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind to {listen_addr}: {e}");
            std::process::exit(1);
        },
    };

    tracing::info!("Breakpoint server listening on {listen_addr}");

    axum::serve(listener, app).await.expect("Server error");
}

/// Spawn the GitHub Actions polling monitor as a background task.
#[cfg(feature = "github-poller")]
fn spawn_github_poller(state: &breakpoint_server::state::AppState) {
    let gh = state.config.github.as_ref().unwrap();
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
