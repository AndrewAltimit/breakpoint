#[allow(dead_code)]
mod relay;

use std::sync::Arc;

use axum::Router;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::{RwLock, mpsc};
use tracing_subscriber::EnvFilter;

use breakpoint_core::net::messages::MessageType;
use breakpoint_core::net::protocol::decode_message_type;

use relay::{RelayState, SharedRelayState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let port = std::env::args()
        .nth(1)
        .and_then(|a| a.strip_prefix("--port=").map(String::from))
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8081);

    let max_rooms = std::env::args()
        .nth(2)
        .and_then(|a| a.strip_prefix("--max-rooms=").map(String::from))
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(100);

    let state: SharedRelayState = Arc::new(RwLock::new(RelayState::new(max_rooms)));

    let app = Router::new()
        .route("/relay", axum::routing::get(relay_ws_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind to {addr}: {e}"));

    tracing::info!("Breakpoint relay listening on {addr} (max rooms: {max_rooms})");

    axum::serve(listener, app)
        .await
        .expect("Relay server error");
}

async fn relay_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedRelayState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_relay_socket(socket, state))
}

async fn handle_relay_socket(socket: WebSocket, state: SharedRelayState) {
    let (ws_sender, mut ws_receiver) = socket.split();

    // Wait for first message to determine role (create or join)
    let first_msg = match ws_receiver.next().await {
        Some(Ok(Message::Binary(data))) => data.to_vec(),
        _ => return,
    };

    // Peek at message type — must be JoinRoom (0x02)
    let msg_type = match decode_message_type(&first_msg) {
        Ok(t) => t,
        Err(_) => return,
    };

    if msg_type != MessageType::JoinRoom {
        return;
    }

    // Try to deserialize the JoinRoom payload to get the room code
    let join = match breakpoint_core::net::protocol::decode_payload::<
        breakpoint_core::net::messages::JoinRoomMsg,
    >(&first_msg)
    {
        Ok(j) => j,
        Err(_) => return,
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>(256);

    if join.room_code.is_empty() {
        // Create a new room — this connection is the host
        let code = breakpoint_core::room::generate_room_code();
        let mut relay = state.write().await;
        if let Err(e) = relay.create_room(code.clone(), tx) {
            tracing::warn!(error = %e, "Failed to create relay room");
            return;
        }
        drop(relay);

        tracing::info!(room_code = %code, "Relay room created");

        // Forward original JoinRoom to "self" (host processes it locally)
        // The host doesn't need to receive it back — just start the writer
        spawn_relay_writer(ws_sender, rx);

        // Host read loop
        host_read_loop(&mut ws_receiver, &state, &code).await;

        // Host disconnected — destroy room
        let mut relay = state.write().await;
        relay.destroy_room(&code);
        tracing::info!(room_code = %code, "Relay room destroyed (host disconnected)");
    } else {
        // Join existing room as client
        let code = join.room_code.clone();
        let mut relay = state.write().await;
        let client_id = match relay.join_room(&code, tx) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(room_code = %code, error = %e, "Failed to join relay room");
                return;
            },
        };
        drop(relay);

        // Forward the original JoinRoom message to the host
        {
            let relay = state.read().await;
            relay.relay_to_host(&code, &first_msg);
        }

        tracing::info!(room_code = %code, client_id, "Client joined relay room");

        spawn_relay_writer(ws_sender, rx);

        // Client read loop
        client_read_loop(&mut ws_receiver, &state, &code, client_id).await;

        // Client disconnected — clean up
        let mut relay = state.write().await;
        relay.leave_room(&code, client_id);
        tracing::info!(room_code = %code, client_id, "Client left relay room");
    }
}

fn spawn_relay_writer(
    mut ws_sender: futures::stream::SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<Vec<u8>>,
) {
    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if ws_sender.send(Message::Binary(data.into())).await.is_err() {
                break;
            }
        }
    });
}

/// Per-connection rate limiter (token bucket), same pattern as the main server.
struct RateLimiter {
    tokens: f64,
    last_refill: tokio::time::Instant,
    max_tokens: f64,
    refill_rate: f64,
}

impl RateLimiter {
    fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            last_refill: tokio::time::Instant::now(),
            max_tokens,
            refill_rate,
        }
    }

    fn allow(&mut self) -> bool {
        let now = tokio::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Host read loop: messages from host go to all clients.
async fn host_read_loop(
    ws_receiver: &mut futures::stream::SplitStream<WebSocket>,
    state: &SharedRelayState,
    room_code: &str,
) {
    let mut rate_limiter = RateLimiter::new(100.0, 100.0);

    while let Some(Ok(msg)) = ws_receiver.next().await {
        let data = match msg {
            Message::Binary(d) => d.to_vec(),
            Message::Close(_) => break,
            _ => continue,
        };

        if data.is_empty() {
            continue;
        }

        if data.len() > breakpoint_core::net::protocol::MAX_MESSAGE_SIZE {
            tracing::warn!(
                room = room_code,
                size = data.len(),
                "Oversized host message dropped"
            );
            continue;
        }

        if !rate_limiter.allow() {
            tracing::warn!(room = room_code, "Host rate limited");
            continue;
        }

        // Protocol-agnostic: forward all host messages to clients
        let relay = state.read().await;
        relay.relay_to_clients(room_code, &data);
    }
}

/// Client read loop: messages from clients go to the host.
async fn client_read_loop(
    ws_receiver: &mut futures::stream::SplitStream<WebSocket>,
    state: &SharedRelayState,
    room_code: &str,
    client_id: u64,
) {
    let mut rate_limiter = RateLimiter::new(50.0, 50.0);

    while let Some(Ok(msg)) = ws_receiver.next().await {
        let data = match msg {
            Message::Binary(d) => d.to_vec(),
            Message::Close(_) => break,
            _ => continue,
        };

        if data.is_empty() {
            continue;
        }

        if data.len() > breakpoint_core::net::protocol::MAX_MESSAGE_SIZE {
            tracing::warn!(
                room = room_code,
                client_id,
                size = data.len(),
                "Oversized client message dropped"
            );
            continue;
        }

        if !rate_limiter.allow() {
            tracing::warn!(room = room_code, client_id, "Client rate limited");
            continue;
        }

        // Forward all client messages to the host
        let relay = state.read().await;
        relay.relay_to_host(room_code, &data);
    }
}
