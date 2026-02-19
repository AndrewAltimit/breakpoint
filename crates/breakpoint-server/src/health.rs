use std::sync::atomic::Ordering;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::state::AppState;

/// Structured health check response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub connections: ConnectionInfo,
    pub rooms: RoomInfo,
}

#[derive(Serialize)]
pub struct ConnectionInfo {
    pub websocket: usize,
    pub sse: usize,
}

#[derive(Serialize)]
pub struct RoomInfo {
    pub active: usize,
    pub players: usize,
}

/// Structured health check endpoint. Returns server status, connection counts,
/// and room info as JSON.
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let ws = state.ws_connection_count.load(Ordering::Relaxed);
    let sse = state.sse_subscriber_count.load(Ordering::Relaxed);

    let (active_rooms, total_players) = {
        let rooms = state.rooms.read().await;
        rooms.stats()
    };

    Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
        connections: ConnectionInfo { websocket: ws, sse },
        rooms: RoomInfo {
            active: active_rooms,
            players: total_players,
        },
    })
}

/// Readiness check — verifies essential subsystems are initialized.
pub async fn readiness_check(State(state): State<AppState>) -> &'static str {
    // Verify game registry has at least one game registered
    let has_games = state.game_registry.available_games() > 0;
    if !has_games {
        return "not ready: no games registered";
    }

    // If we got here, config was loaded and state is initialized
    "ready"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_response_serializes() {
        let resp = HealthResponse {
            status: "healthy",
            version: "0.1.0",
            connections: ConnectionInfo {
                websocket: 5,
                sse: 2,
            },
            rooms: RoomInfo {
                active: 1,
                players: 3,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"healthy\""));
        assert!(json.contains("\"websocket\":5"));
        assert!(json.contains("\"active\":1"));
    }
}
