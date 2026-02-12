use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use breakpoint_core::game_trait::PlayerId;
use breakpoint_core::net::messages::{AlertClaimedMsg, MessageType, ServerMessage};
use breakpoint_core::net::protocol::{
    decode_client_message, decode_message_type, encode_server_message,
};
use breakpoint_core::room::RoomState;

use crate::state::AppState;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
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
            let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();

            let mut rooms = state.rooms.write().await;

            if join.room_code.is_empty() {
                // Create new room
                let (code, pid) = rooms.create_room(join.player_name, join.player_color, tx);
                let response = crate::room_manager::RoomManager::make_join_response(
                    pid,
                    &code,
                    RoomState::Lobby,
                );
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
                // Join existing room
                match rooms.join_room(&join.room_code, join.player_name, join.player_color, tx) {
                    Ok(pid) => {
                        let room_state = rooms
                            .get_room_state(&join.room_code)
                            .unwrap_or(RoomState::Lobby);
                        let response = crate::room_manager::RoomManager::make_join_response(
                            pid,
                            &join.room_code,
                            room_state,
                        );
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
                        let response = crate::room_manager::RoomManager::make_join_error(&err);
                        let _ = ws_sender.send(Message::Binary(response.into())).await;
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
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if ws_sender.send(Message::Binary(data.into())).await.is_err() {
                break;
            }
        }
    });
}

async fn read_loop(
    ws_receiver: &mut futures::stream::SplitStream<WebSocket>,
    state: &AppState,
    room_code: &str,
    player_id: PlayerId,
) {
    while let Some(Ok(msg)) = ws_receiver.next().await {
        let data = match msg {
            Message::Binary(d) => d.to_vec(),
            Message::Close(_) => break,
            _ => continue,
        };

        if data.is_empty() {
            continue;
        }

        let msg_type = match decode_message_type(&data) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let rooms = state.rooms.read().await;

        match msg_type {
            // Player inputs get relayed to the host
            MessageType::PlayerInput => {
                if let Some(host_id) = rooms.get_host_id(room_code)
                    && player_id != host_id
                {
                    rooms.send_to_player(room_code, host_id, data.to_vec());
                }
            },

            // Game state from host gets broadcast to all non-host players
            MessageType::GameState => {
                rooms.broadcast_to_room_except(room_code, player_id, &data);
            },

            // Game lifecycle messages from host get broadcast to all
            MessageType::GameStart | MessageType::RoundEnd | MessageType::GameEnd => {
                // Update room state based on message type
                drop(rooms);
                let mut rooms = state.rooms.write().await;
                match msg_type {
                    MessageType::GameStart => {
                        rooms.set_room_state(room_code, RoomState::InGame);
                    },
                    MessageType::RoundEnd => {
                        rooms.set_room_state(room_code, RoomState::BetweenRounds);
                    },
                    MessageType::GameEnd => {
                        rooms.set_room_state(room_code, RoomState::Lobby);
                    },
                    _ => {},
                }
                rooms.broadcast_to_room_except(room_code, player_id, &data);
                continue;
            },

            // Chat messages broadcast to all
            MessageType::ChatMessage => {
                rooms.broadcast_to_room(room_code, &data);
            },

            // Alert events, claimed, dismissed — broadcast to all
            MessageType::AlertEvent | MessageType::AlertClaimed | MessageType::AlertDismissed => {
                rooms.broadcast_to_room(room_code, &data);
            },

            // ClaimAlert from a client — record in EventStore and broadcast AlertClaimed
            MessageType::ClaimAlert => {
                if let Ok(breakpoint_core::net::messages::ClientMessage::ClaimAlert(claim)) =
                    decode_client_message(&data)
                {
                    let player_name = rooms
                        .get_player_name(room_code, claim.player_id)
                        .unwrap_or_else(|| format!("Player {}", claim.player_id));

                    drop(rooms);

                    // Record the claim in the event store
                    let now = {
                        let dur = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default();
                        format!("{}Z", dur.as_secs())
                    };
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

                    continue;
                }
            },

            // Player list updates from host broadcast to all
            MessageType::PlayerList | MessageType::RoomConfigMsg => {
                rooms.broadcast_to_room(room_code, &data);
            },

            _ => {},
        }
    }
}
