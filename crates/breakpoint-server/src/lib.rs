pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod event_store;
pub mod game_loop;
pub mod health;
pub mod rate_limit;
pub mod room_manager;
pub mod sse;
pub mod state;
pub mod webhooks;
pub mod ws;

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::extract::ConnectInfo;
use axum::http::HeaderValue;
use axum::middleware;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;

use breakpoint_core::net::messages::{AlertEventMsg, ServerMessage};
use breakpoint_core::net::protocol::encode_server_message;

use config::ServerConfig;
use state::AppState;

/// Build the Axum router and application state from a config.
pub fn build_app(config: ServerConfig) -> (Router<()>, AppState) {
    let web_root = config.web_root.clone();
    let state = AppState::new(config);

    // API routes (behind bearer auth + rate limiting + request timeout)
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
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api_rate_limit_layer,
        ))
        .layer(ServiceBuilder::new().layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        )));

    // Webhook routes (NOT behind bearer auth — uses its own HMAC verification + rate limiting)
    let webhook_routes = Router::new()
        .route(
            "/github",
            axum::routing::post(webhooks::github::github_webhook),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api_rate_limit_layer,
        ))
        .layer(ServiceBuilder::new().layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        )));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Static file serving with Cache-Control headers for immutable assets.
    // WASM bundles, JS, and CSS are fingerprinted by wasm-pack, so long
    // cache lifetimes are safe. HTML is short-cached to pick up new deploys.
    let static_service = ServeDir::new(&web_root);

    let app = Router::new()
        .route("/ws", axum::routing::get(ws::ws_handler))
        .route("/health", axum::routing::get(health::health_check))
        .route("/health/ready", axum::routing::get(health::readiness_check))
        .nest("/api/v1", api_routes)
        .nest("/api/v1/webhooks", webhook_routes)
        .fallback_service(static_service)
        .layer(axum::middleware::from_fn(cache_control_middleware))
        .layer(CompressionLayer::new())
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
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; \
                 script-src 'self' 'wasm-unsafe-eval'; \
                 style-src 'self'; \
                 connect-src 'self' wss: ws:; \
                 img-src 'self' data:; \
                 font-src 'self'; \
                 object-src 'none'; \
                 base-uri 'self'; \
                 form-action 'self'; \
                 frame-ancestors 'none'",
            ),
        ))
        .with_state(state.clone());

    (app, state)
}

/// Background task that subscribes to the EventStore broadcast channel and
/// re-broadcasts each new event to all connected rooms via WSS.
pub fn spawn_event_broadcaster(state: AppState) {
    let shutdown = state.shutdown.clone();
    tokio::spawn(async move {
        // Subscribe while holding the read lock, then drop it
        let mut rx = {
            let store = state.event_store.read().await;
            store.subscribe()
        };

        let mut total_lagged: u64 = 0;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("Event broadcaster shutting down");
                    break;
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            let msg = ServerMessage::AlertEvent(
                                Box::new(AlertEventMsg { event }),
                            );
                            match encode_server_message(&msg) {
                                Ok(data) => {
                                    let rooms = state.rooms.read().await;
                                    rooms.broadcast_to_all_rooms(&data);
                                },
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "Failed to encode AlertEvent for broadcast"
                                    );
                                },
                            }
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            total_lagged += n;
                            tracing::warn!(
                                skipped = n, total_lagged,
                                "Event broadcaster lagged"
                            );
                        },
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!(
                                "Event broadcast channel closed, stopping broadcaster"
                            );
                            break;
                        },
                    }
                }
            }
        }
    });
}

/// Background task that periodically removes idle rooms.
pub fn spawn_idle_room_cleanup(state: AppState) {
    let check_interval = state.config.rooms.idle_check_interval_secs;
    let idle_timeout = state.config.rooms.idle_timeout_secs;
    let shutdown = state.shutdown.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(check_interval));
        let max_idle = std::time::Duration::from_secs(idle_timeout);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("Idle room cleanup shutting down");
                    break;
                }
                _ = interval.tick() => {
                    let mut rooms = state.rooms.write().await;
                    let removed = rooms.cleanup_idle_rooms(max_idle);
                    if removed > 0 {
                        tracing::info!(removed, "Cleaned up idle rooms");
                    }
                }
            }
        }
    });
}

/// Middleware that sets Cache-Control headers based on response content type.
/// `.wasm`, `.js`, `.css` files use `no-cache` so the browser always revalidates
/// against `Last-Modified` but can still use its cached copy when unchanged.
/// Image assets (`.png`, `.svg`) get a longer cache since they change infrequently.
async fn cache_control_middleware(
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;

    // Skip API routes, WebSocket, and health check
    if path.starts_with("/api/") || path.starts_with("/ws") || path == "/health" {
        return response;
    }

    let cache_value = if path.ends_with(".png") || path.ends_with(".svg") {
        // Image assets: cache for 1 day
        HeaderValue::from_static("public, max-age=86400")
    } else if path.ends_with(".wasm") || path.ends_with(".js") || path.ends_with(".css") {
        // Code assets: always revalidate (uses Last-Modified/ETag)
        HeaderValue::from_static("no-cache")
    } else {
        // HTML and other files: short cache
        HeaderValue::from_static("public, max-age=300")
    };

    response
        .headers_mut()
        .insert(axum::http::header::CACHE_CONTROL, cache_value);
    response
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

/// Middleware that enforces per-IP rate limiting on API endpoints.
async fn api_rate_limit_layer(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    if !state.api_rate_limiter.check_rate_limit(ip).await {
        tracing::warn!(%ip, "API rate limit exceeded");
        return Err(axum::http::StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(request).await)
}

/// Background task that periodically cleans up stale rate limiter entries.
pub fn spawn_rate_limit_cleanup(state: AppState) {
    let shutdown = state.shutdown.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("Rate limiter cleanup shutting down");
                    break;
                }
                _ = interval.tick() => {
                    state
                        .api_rate_limiter
                        .cleanup(std::time::Duration::from_secs(300))
                        .await;
                }
            }
        }
    });
}
