use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use breakpoint_core::game_trait::PlayerId;
use breakpoint_core::net::messages::{AlertClaimedMsg, MessageType, ServerMessage};
use breakpoint_core::net::protocol::{
    PROTOCOL_VERSION, decode_client_message, decode_message_type, encode_server_message,
};
use breakpoint_core::room::RoomState;

use crate::state::{AppState, ConnectionGuard};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, StatusCode> {
    let max_ws = state.config.limits.max_ws_connections;
    let current = state.ws_connection_count.load(Ordering::Relaxed);
    if current >= max_ws {
        tracing::warn!(current, max = max_ws, "WS connection limit reached");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state)))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let _guard = ConnectionGuard::new(Arc::clone(&state.ws_connection_count));
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Wait for the first message: must be a JoinRoom (with room_code empty to create,
    // or non-empty to join).
    let first_msg = match ws_receiver.next().await {
        Some(Ok(Message::Binary(data))) => data,
        _ => return,
    };

    let Ok(client_msg) = decode_client_message(&first_msg) else {
        return;
    };

    let (room_code, player_id) = match client_msg {
        breakpoint_core::net::messages::ClientMessage::JoinRoom(join) => {
            // Validate protocol version
            if join.protocol_version != 0 && join.protocol_version != PROTOCOL_VERSION {
                if let Ok(response) = crate::room_manager::RoomManager::make_join_error(&format!(
                    "Protocol version mismatch: client={}, server={}",
                    join.protocol_version, PROTOCOL_VERSION
                )) && let Err(e) = ws_sender.send(Message::Binary(response.into())).await
                {
                    tracing::warn!(error = %e, "Failed to send protocol mismatch error");
                }
                return;
            }

            // Validate player name
            let name = join.player_name.trim().to_string();
            if name.is_empty() || name.len() > 32 || name.chars().any(|c| c.is_control()) {
                if let Ok(response) =
                    crate::room_manager::RoomManager::make_join_error("Invalid player name")
                    && let Err(e) = ws_sender.send(Message::Binary(response.into())).await
                {
                    tracing::warn!(error = %e, "Failed to send invalid name error");
                }
                return;
            }

            let (tx, rx) = mpsc::channel::<Vec<u8>>(state.config.limits.player_message_buffer);

            let mut rooms = state.rooms.write().await;

            if join.room_code.is_empty() {
                // Create new room
                let (code, pid) = rooms.create_room(name, join.player_color, tx);
                let Ok(response) = crate::room_manager::RoomManager::make_join_response(
                    pid,
                    &code,
                    RoomState::Lobby,
                ) else {
                    tracing::warn!("Failed to encode JoinRoomResponse");
                    return;
                };
                drop(rooms);

                // Send response to this player
                if ws_sender
                    .send(Message::Binary(response.into()))
                    .await
                    .is_err()
                {
                    return;
                }

                // Broadcast player list
                let rooms = state.rooms.read().await;
                rooms.broadcast_player_list(&code);
                drop(rooms);

                spawn_writer(ws_sender, rx);
                (code, pid)
            } else {
                // Validate room code format before lookup
                if !breakpoint_core::room::is_valid_room_code(&join.room_code) {
                    if let Ok(response) =
                        crate::room_manager::RoomManager::make_join_error("Invalid room code")
                    {
                        drop(rooms);
                        if let Err(e) = ws_sender.send(Message::Binary(response.into())).await {
                            tracing::warn!(error = %e, "Failed to send invalid room code error");
                        }
                    } else {
                        drop(rooms);
                    }
                    return;
                }

                // Join existing room
                match rooms.join_room(&join.room_code, name, join.player_color, tx) {
                    Ok(pid) => {
                        let room_state = rooms
                            .get_room_state(&join.room_code)
                            .unwrap_or(RoomState::Lobby);
                        let Ok(response) = crate::room_manager::RoomManager::make_join_response(
                            pid,
                            &join.room_code,
                            room_state,
                        ) else {
                            tracing::warn!("Failed to encode JoinRoomResponse");
                            return;
                        };
                        let code = join.room_code.clone();
                        drop(rooms);

                        if ws_sender
                            .send(Message::Binary(response.into()))
                            .await
                            .is_err()
                        {
                            return;
                        }

                        let rooms = state.rooms.read().await;
                        rooms.broadcast_player_list(&code);
                        drop(rooms);

                        spawn_writer(ws_sender, rx);
                        (code, pid)
                    },
                    Err(err) => {
                        if let Ok(response) =
                            crate::room_manager::RoomManager::make_join_error(&err)
                            && let Err(e) = ws_sender.send(Message::Binary(response.into())).await
                        {
                            tracing::warn!(error = %e, "Failed to send join error response");
                        }
                        return;
                    },
                }
            }
        },
        _ => return,
    };

    // Read loop: relay incoming messages
    read_loop(&mut ws_receiver, &state, &room_code, player_id).await;

    // Player disconnected — clean up
    let mut rooms = state.rooms.write().await;
    let destroyed = rooms.leave_room(&room_code, player_id);
    if destroyed.is_none() {
        // Room still exists, broadcast updated player list
        rooms.broadcast_player_list(&room_code);
    }
    drop(rooms);

    tracing::info!(
        player_id,
        room_code = %room_code,
        "Player disconnected"
    );
}

fn spawn_writer(
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

/// Per-connection rate limiter (token bucket).
struct RateLimiter {
    tokens: f64,
    last_refill: tokio::time::Instant,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
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

    /// Returns true if the message is allowed; false if rate-limited.
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

async fn read_loop(
    ws_receiver: &mut futures::stream::SplitStream<WebSocket>,
    state: &AppState,
    room_code: &str,
    player_id: PlayerId,
) {
    let rate = state.config.limits.ws_rate_limit_per_sec;
    let mut rate_limiter = RateLimiter::new(rate, rate);

    while let Some(Ok(msg)) = ws_receiver.next().await {
        let data = match msg {
            Message::Binary(d) => d.to_vec(),
            Message::Close(_) => break,
            _ => continue,
        };

        // Rate limit: drop messages that exceed per-connection rate
        if !rate_limiter.allow() {
            tracing::warn!(player_id, room_code, "Rate limited");
            continue;
        }

        // Drop oversized messages
        if data.len() > breakpoint_core::net::protocol::MAX_MESSAGE_SIZE {
            continue;
        }

        if data.is_empty() {
            continue;
        }

        let msg_type = match decode_message_type(&data) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Server-authoritative: reject lifecycle messages from clients.
        // GameState, GameStart, RoundEnd, GameEnd are server-only.
        if matches!(
            msg_type,
            MessageType::GameState
                | MessageType::GameStart
                | MessageType::RoundEnd
                | MessageType::GameEnd
        ) {
            tracing::warn!(
                player_id,
                room_code,
                ?msg_type,
                "Rejected server-only message from client"
            );
            continue;
        }

        // RequestGameStart: client asks the server to start a game
        if msg_type == MessageType::RequestGameStart {
            if let Ok(breakpoint_core::net::messages::ClientMessage::RequestGameStart(req)) =
                decode_client_message(&data)
            {
                let mut rooms = state.rooms.write().await;
                match rooms.start_game(room_code, &req.game_name, player_id, &state.game_registry) {
                    Ok(()) => {
                        tracing::info!(
                            player_id,
                            room_code,
                            game = %req.game_name,
                            "Game started"
                        );
                    },
                    Err(e) => {
                        tracing::warn!(
                            player_id,
                            room_code,
                            game = %req.game_name,
                            error = %e,
                            "Failed to start game"
                        );
                    },
                }
            }
            continue;
        }

        // ClaimAlert needs special lock handling (read→drop→write→read)
        if msg_type == MessageType::ClaimAlert {
            if let Ok(breakpoint_core::net::messages::ClientMessage::ClaimAlert(claim)) =
                decode_client_message(&data)
            {
                // Reject spoofed claims
                if claim.player_id != player_id {
                    continue;
                }

                let player_name = {
                    let rooms = state.rooms.read().await;
                    rooms
                        .get_player_name(room_code, claim.player_id)
                        .unwrap_or_else(|| format!("Player {}", claim.player_id))
                };

                // Record the claim in the event store
                let now = breakpoint_core::time::timestamp_now();
                {
                    let mut store = state.event_store.write().await;
                    store.claim(&claim.event_id, player_name.clone(), now);
                }

                // Build and broadcast AlertClaimed to the room
                let msg = ServerMessage::AlertClaimed(AlertClaimedMsg {
                    event_id: claim.event_id,
                    claimed_by: claim.player_id,
                });
                if let Ok(encoded) = encode_server_message(&msg) {
                    let rooms = state.rooms.read().await;
                    rooms.broadcast_to_room(room_code, &encoded);
                }
            }
            continue;
        }

        // All other messages use a read lock
        let rooms = state.rooms.read().await;

        match msg_type {
            // Player inputs routed to the server game session
            MessageType::PlayerInput => {
                if let Ok(breakpoint_core::net::messages::ClientMessage::PlayerInput(pi)) =
                    decode_client_message(&data)
                {
                    rooms.route_player_input(room_code, player_id, pi.tick, pi.input_data);
                }
            },

            // Chat messages broadcast to all (cap at 1024 bytes, valid UTF-8, no control chars)
            MessageType::ChatMessage => {
                if data.len() <= 1024 {
                    // Decode and validate content length at the application level
                    if let Ok(breakpoint_core::net::messages::ClientMessage::ChatMessage(cm)) =
                        decode_client_message(&data)
                    {
                        if cm.content.len() > 1024 {
                            tracing::debug!(
                                player_id,
                                room_code,
                                "Chat message content exceeds 1024 chars"
                            );
                            continue;
                        }
                        if cm.content.chars().any(|c| c.is_control() && c != '\n') {
                            continue;
                        }
                        rooms.broadcast_to_room(room_code, &data);
                    }
                }
            },

            // Alert events, claimed, dismissed — broadcast to all
            MessageType::AlertEvent | MessageType::AlertClaimed | MessageType::AlertDismissed => {
                rooms.broadcast_to_room(room_code, &data);
            },

            // Player list updates broadcast to all
            MessageType::PlayerList | MessageType::RoomConfigMsg => {
                rooms.broadcast_to_room(room_code, &data);
            },

            // Overlay config broadcast to all
            MessageType::OverlayConfig => {
                rooms.broadcast_to_room(room_code, &data);
            },

            _ => {},
        }
    }
}
