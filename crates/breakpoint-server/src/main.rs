mod room_manager;
mod state;
mod ws;

use axum::Router;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let state = AppState::new();

    let app = Router::new()
        .route("/ws", axum::routing::get(ws::ws_handler))
        .fallback_service(ServeDir::new("web"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Failed to bind to port 8080");

    tracing::info!("Breakpoint server listening on 0.0.0.0:8080");

    axum::serve(listener, app).await.expect("Server error");
}
