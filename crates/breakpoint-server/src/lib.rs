pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod event_store;
pub mod game_loop;
pub mod room_manager;
pub mod sse;
pub mod state;
pub mod webhooks;
pub mod ws;

use axum::Router;
use axum::http::HeaderValue;
use axum::middleware;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use breakpoint_core::net::messages::{AlertEventMsg, ServerMessage};
use breakpoint_core::net::protocol::encode_server_message;

use config::ServerConfig;
use state::AppState;

/// Build the Axum router and application state from a config.
pub fn build_app(config: ServerConfig) -> (Router<()>, AppState) {
    let web_root = config.web_root.clone();
    let state = AppState::new(config);

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

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/ws", axum::routing::get(ws::ws_handler))
        .route("/health", axum::routing::get(|| async { "ok" }))
        .nest("/api/v1", api_routes)
        .nest("/api/v1/webhooks", webhook_routes)
        .fallback_service(ServeDir::new(&web_root))
        .layer(cors)
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-xss-protection"),
            HeaderValue::from_static("0"),
        ))
        .with_state(state.clone());

    (app, state)
}

/// Background task that subscribes to the EventStore broadcast channel and
/// re-broadcasts each new event to all connected rooms via WSS.
pub fn spawn_event_broadcaster(state: AppState) {
    tokio::spawn(async move {
        // Subscribe while holding the read lock, then drop it
        let mut rx = {
            let store = state.event_store.read().await;
            store.subscribe()
        };

        let mut total_lagged: u64 = 0;

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
                    total_lagged += n;
                    tracing::warn!(skipped = n, total_lagged, "Event broadcaster lagged");
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("Event broadcast channel closed, stopping broadcaster");
                    break;
                },
            }
        }
    });
}

/// Background task that periodically removes idle rooms.
pub fn spawn_idle_room_cleanup(state: AppState) {
    let check_interval = state.config.rooms.idle_check_interval_secs;
    let idle_timeout = state.config.rooms.idle_timeout_secs;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(check_interval));
        let max_idle = std::time::Duration::from_secs(idle_timeout);
        loop {
            interval.tick().await;
            let mut rooms = state.rooms.write().await;
            let removed = rooms.cleanup_idle_rooms(max_idle);
            if removed > 0 {
                tracing::info!(removed, "Cleaned up idle rooms");
            }
        }
    });
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
