use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::ConnectInfo;
use axum::extract::FromRequest;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use breakpoint_core::game_trait::PlayerId;
use breakpoint_core::net::messages::{AlertClaimedMsg, JoinRoomMsg, MessageType, ServerMessage};
use breakpoint_core::net::protocol::{
    PROTOCOL_VERSION, decode_client_message, decode_message_type, encode_server_message,
};
use breakpoint_core::room::RoomState;

use crate::state::{AppState, ConnectionGuard, IpConnectionGuard};

pub async fn ws_handler(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, StatusCode> {
    let max_ws = state.config.limits.max_ws_connections;
    let current = state.ws_connection_count.load(Ordering::Relaxed);
    if current >= max_ws {
        tracing::warn!(current, max = max_ws, "WS connection limit reached");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Per-IP connection limit
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let max_per_ip = state.config.limits.max_ws_per_ip;
    let ip_guard =
        IpConnectionGuard::try_acquire(ip, Arc::clone(&state.ws_per_ip), max_per_ip).await;
    let Some(ip_guard) = ip_guard else {
        tracing::warn!(%ip, max_per_ip, "Per-IP WS connection limit reached");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    };

    // Perform WebSocket upgrade manually
    let ws = WebSocketUpgrade::from_request(request, &state)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    Ok(ws
        .on_upgrade(move |socket| handle_socket(socket, state, ip_guard))
        .into_response())
}

async fn handle_socket(socket: WebSocket, state: AppState, _ip_guard: IpConnectionGuard) {
    let _guard = ConnectionGuard::new(Arc::clone(&state.ws_connection_count));
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Wait for the first message: must be a JoinRoom.
    let first_msg = match ws_receiver.next().await {
        Some(Ok(Message::Binary(data))) => data,
        _ => return,
    };

    let Ok(client_msg) = decode_client_message(&first_msg) else {
        return;
    };

    let join = match client_msg {
        breakpoint_core::net::messages::ClientMessage::JoinRoom(j) => j,
        _ => return,
    };

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

    // Attempt join (reconnect or normal)
    let result = match attempt_join(&join, &state).await {
        Some(r) => r,
        None => {
            send_join_error(&mut ws_sender, "Invalid player name").await;
            return;
        },
    };

    let (room_code, player_id, rx) = match result {
        JoinResult::Success {
            room_code,
            player_id,
            session_token,
            room_state,
            rx,
        } => {
            let Ok(response) = crate::room_manager::RoomManager::make_join_response(
                player_id,
                &room_code,
                room_state,
                &session_token,
            ) else {
                tracing::warn!("Failed to encode JoinRoomResponse");
                return;
            };

            if ws_sender
                .send(Message::Binary(response.into()))
                .await
                .is_err()
            {
                return;
            }

            (room_code, player_id, rx)
        },
        JoinResult::Error(err) => {
            send_join_error(&mut ws_sender, &err).await;
            return;
        },
    };

    // Broadcast player list
    {
        let rooms = state.rooms.read().await;
        rooms.broadcast_player_list(&room_code);
    }

    spawn_writer(ws_sender, rx);

    // Read loop: relay incoming messages
    read_loop(&mut ws_receiver, &state, &room_code, player_id).await;

    // Player disconnected — clean up
    let mut rooms = state.rooms.write().await;
    let destroyed = rooms.leave_room(&room_code, player_id);
    if destroyed.is_none() {
        rooms.broadcast_player_list(&room_code);
    }
    drop(rooms);

    tracing::info!(
        player_id,
        room_code = %room_code,
        "Player disconnected"
    );
}

enum JoinResult {
    Success {
        room_code: String,
        player_id: PlayerId,
        session_token: String,
        room_state: RoomState,
        rx: mpsc::Receiver<Bytes>,
    },
    Error(String),
}

async fn attempt_join(join: &JoinRoomMsg, state: &AppState) -> Option<JoinResult> {
    // Try session-based reconnection first
    if let Some(ref token) = join.session_token {
        let (tx, rx) = mpsc::channel::<Bytes>(state.config.limits.player_message_buffer);
        let mut rooms = state.rooms.write().await;
        match rooms.reconnect(token, tx) {
            Ok((code, pid, new_token)) => {
                let room_state = rooms.get_room_state(&code).unwrap_or(RoomState::Lobby);
                drop(rooms);
                tracing::info!(player_id = pid, room = %code, "Player reconnected via session");
                return Some(JoinResult::Success {
                    room_code: code,
                    player_id: pid,
                    session_token: new_token,
                    room_state,
                    rx,
                });
            },
            Err(e) => {
                drop(rooms);
                tracing::debug!(error = %e, "Session reconnect failed, trying normal join");
            },
        }
    }

    // Normal join path
    let (tx, rx) = mpsc::channel::<Bytes>(state.config.limits.player_message_buffer);

    // Validate player name
    let name = join.player_name.trim().to_string();
    if name.is_empty() || name.len() > 32 || name.chars().any(|c| c.is_control()) {
        return None; // signals name validation failure
    }

    let mut rooms = state.rooms.write().await;

    if join.room_code.is_empty() {
        // Create new room
        let (code, pid, token) = rooms.create_room(name, join.player_color, tx);
        drop(rooms);
        Some(JoinResult::Success {
            room_code: code,
            player_id: pid,
            session_token: token,
            room_state: RoomState::Lobby,
            rx,
        })
    } else {
        // Validate room code format before lookup
        if !breakpoint_core::room::is_valid_room_code(&join.room_code) {
            drop(rooms);
            return Some(JoinResult::Error("Invalid room code".to_string()));
        }

        // Join existing room
        match rooms.join_room(&join.room_code, name, join.player_color, tx) {
            Ok((pid, token)) => {
                let room_state = rooms
                    .get_room_state(&join.room_code)
                    .unwrap_or(RoomState::Lobby);
                let code = join.room_code.clone();
                drop(rooms);
                Some(JoinResult::Success {
                    room_code: code,
                    player_id: pid,
                    session_token: token,
                    room_state,
                    rx,
                })
            },
            Err(err) => {
                drop(rooms);
                Some(JoinResult::Error(err))
            },
        }
    }
}

async fn send_join_error(
    ws_sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    error: &str,
) {
    if let Ok(response) = crate::room_manager::RoomManager::make_join_error(error)
        && let Err(e) = ws_sender.send(Message::Binary(response.into())).await
    {
        tracing::warn!(error = %e, "Failed to send join error response");
    }
}

fn spawn_writer(
    mut ws_sender: futures::stream::SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<Bytes>,
) {
    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if ws_sender
                .send(Message::Binary(data.to_vec().into()))
                .await
                .is_err()
            {
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

        // AddBot: leader adds a bot player to the lobby
        if msg_type == MessageType::AddBot {
            let mut rooms = state.rooms.write().await;
            match rooms.add_bot(room_code, player_id) {
                Ok(bot_id) => {
                    tracing::info!(player_id, room_code, bot_id, "Bot added");
                    rooms.broadcast_player_list(room_code);
                },
                Err(e) => {
                    tracing::warn!(player_id, room_code, error = %e, "Failed to add bot");
                },
            }
            continue;
        }

        // RemoveBot: leader removes a bot player from the lobby
        if msg_type == MessageType::RemoveBot {
            if let Ok(breakpoint_core::net::messages::ClientMessage::RemoveBot(req)) =
                decode_client_message(&data)
            {
                let mut rooms = state.rooms.write().await;
                match rooms.remove_bot(room_code, req.player_id, player_id) {
                    Ok(()) => {
                        tracing::info!(player_id, room_code, bot_id = req.player_id, "Bot removed");
                        rooms.broadcast_player_list(room_code);
                    },
                    Err(e) => {
                        tracing::warn!(player_id, room_code, error = %e, "Failed to remove bot");
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
