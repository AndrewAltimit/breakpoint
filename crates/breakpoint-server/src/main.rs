mod api;
mod auth;
mod config;
mod event_store;
mod room_manager;
mod sse;
mod state;
mod webhooks;
mod ws;

use axum::Router;
use axum::middleware;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use breakpoint_core::net::messages::{AlertEventMsg, ServerMessage};
use breakpoint_core::net::protocol::encode_server_message;

use config::ServerConfig;
use state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::load();
    let listen_addr = config.listen_addr.clone();
    let web_root = config.web_root.clone();

    let state = AppState::new(config);

    // Spawn background task: broadcast new events to all rooms via WSS
    spawn_event_broadcaster(state.clone());

    // Conditionally spawn GitHub Actions poller
    #[cfg(feature = "github-poller")]
    if let Some(ref gh) = state.config.github
        && gh.enabled
        && gh.token.is_some()
    {
        spawn_github_poller(&state);
    }

    // API routes (behind bearer auth middleware)
    let api_routes = Router::new()
        .route("/events", axum::routing::post(api::post_events))
        .route(
            "/events/{event_id}/claim",
            axum::routing::post(api::claim_event),
        )
        .route("/events/stream", axum::routing::get(sse::event_stream))
        .route("/status", axum::routing::get(api::get_status))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            bearer_auth_layer,
        ));

    // Webhook routes (NOT behind bearer auth — uses its own HMAC verification)
    let webhook_routes = Router::new().route(
        "/github",
        axum::routing::post(webhooks::github::github_webhook),
    );

    let app = Router::new()
        .route("/ws", axum::routing::get(ws::ws_handler))
        .nest("/api/v1", api_routes)
        .nest("/api/v1/webhooks", webhook_routes)
        .fallback_service(ServeDir::new(&web_root))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind to {listen_addr}: {e}"));

    tracing::info!("Breakpoint server listening on {listen_addr}");

    axum::serve(listener, app).await.expect("Server error");
}

/// Middleware wrapper that injects AuthConfig into request extensions for the
/// bearer auth middleware.
async fn bearer_auth_layer(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut request: axum::extract::Request,
    next: middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    request.extensions_mut().insert(state.auth.clone());
    auth::bearer_auth_middleware(request.headers().clone(), request, next).await
}

/// Spawn the GitHub Actions polling monitor as a background task.
#[cfg(feature = "github-poller")]
fn spawn_github_poller(state: &AppState) {
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

/// Background task that subscribes to the EventStore broadcast channel and
/// re-broadcasts each new event to all connected rooms via WSS.
fn spawn_event_broadcaster(state: AppState) {
    tokio::spawn(async move {
        // Subscribe while holding the read lock, then drop it
        let mut rx = {
            let store = state.event_store.read().await;
            store.subscribe()
        };

        loop {
            match rx.recv().await {
                Ok(event) => {
                    let msg = ServerMessage::AlertEvent(Box::new(AlertEventMsg { event }));
                    if let Ok(data) = encode_server_message(&msg) {
                        let rooms = state.rooms.read().await;
                        rooms.broadcast_to_all_rooms(&data);
                    }
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Event broadcaster lagged by {n} messages");
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("Event broadcast channel closed, stopping broadcaster");
                    break;
                },
            }
        }
    });
}
