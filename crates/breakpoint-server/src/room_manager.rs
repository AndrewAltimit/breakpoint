use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use uuid::Uuid;

use breakpoint_core::game_trait::{GameId, PlayerId};
use breakpoint_core::net::messages::{JoinRoomResponseMsg, PlayerListMsg, ServerMessage};
use breakpoint_core::net::protocol::encode_server_message;
use breakpoint_core::player::{Player, PlayerColor};
use breakpoint_core::room::{Room, RoomState};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::game_loop::{
    GameBroadcast, GameCommand, GameSessionConfig, ServerGameRegistry, spawn_game_session,
};

/// Per-player sender for outbound WebSocket binary messages.
/// Bounded to 256 messages to prevent memory exhaustion from slow clients.
/// Uses `Bytes` for zero-copy cloning when broadcasting to multiple players.
pub type PlayerSender = mpsc::Sender<Bytes>;

/// Tracks a connected player's outbound channel.
struct ConnectedPlayer {
    sender: PlayerSender,
}

/// Session record for reconnection. When a player disconnects mid-game,
/// their session is preserved so they can rejoin within the TTL window.
struct DisconnectedSession {
    room_code: String,
    player_id: PlayerId,
    disconnected_at: Instant,
}

/// How long a disconnected session remains valid for reconnection.
const SESSION_TTL: Duration = Duration::from_secs(60);

/// Manages all active rooms and their connected players.
pub struct RoomManager {
    rooms: HashMap<String, RoomEntry>,
    next_player_id: PlayerId,
    /// Maps session_token → disconnected session info.
    sessions: HashMap<String, DisconnectedSession>,
}

struct RoomEntry {
    room: Room,
    connections: HashMap<PlayerId, ConnectedPlayer>,
    last_activity: Instant,
    /// Maps player_id → session_token for connected players.
    player_sessions: HashMap<PlayerId, String>,
    /// Channel to send commands to the active game tick loop.
    game_command_tx: Option<mpsc::UnboundedSender<GameCommand>>,
    /// Handle for the game tick loop task.
    game_task: Option<JoinHandle<()>>,
    /// Handle for the broadcast forwarder task.
    broadcast_task: Option<JoinHandle<()>>,
    /// Shared sender map for active game broadcasts. Updated on reconnection
    /// so the broadcast forwarder can reach reconnected clients.
    broadcast_senders: Arc<Mutex<HashMap<PlayerId, PlayerSender>>>,
}

impl Default for RoomManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomManager {
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
            next_player_id: 1,
            sessions: HashMap::new(),
        }
    }

    fn alloc_player_id(&mut self) -> PlayerId {
        let id = self.next_player_id;
        self.next_player_id += 1;
        id
    }

    fn generate_session_token() -> String {
        Uuid::new_v4().to_string()
    }

    /// Create a new room. Returns (room_code, player_id, session_token) for the host.
    pub fn create_room(
        &mut self,
        player_name: String,
        player_color: PlayerColor,
        sender: PlayerSender,
    ) -> (String, PlayerId, String) {
        let code = generate_unique_room_code(&self.rooms);
        let player_id = self.alloc_player_id();
        let session_token = Self::generate_session_token();
        let player = Player {
            id: player_id,
            display_name: player_name,
            color: player_color,
            is_leader: true,
            is_spectator: false,
            is_bot: false,
        };
        let room = Room::new(code.clone(), player);
        let mut connections = HashMap::new();
        connections.insert(player_id, ConnectedPlayer { sender });
        let mut player_sessions = HashMap::new();
        player_sessions.insert(player_id, session_token.clone());
        self.rooms.insert(
            code.clone(),
            RoomEntry {
                room,
                connections,
                last_activity: Instant::now(),
                player_sessions,
                game_command_tx: None,
                game_task: None,
                broadcast_task: None,
                broadcast_senders: Arc::new(Mutex::new(HashMap::new())),
            },
        );
        (code, player_id, session_token)
    }

    /// Join an existing room. Returns Ok((player_id, session_token)) or Err(reason).
    /// Players joining mid-game enter as spectators.
    pub fn join_room(
        &mut self,
        room_code: &str,
        player_name: String,
        player_color: PlayerColor,
        sender: PlayerSender,
    ) -> Result<(PlayerId, String), String> {
        // Validate room exists and is joinable
        {
            let entry = self
                .rooms
                .get(room_code)
                .ok_or_else(|| "Room not found".to_string())?;

            if entry.room.players.len() >= entry.room.config.max_players as usize {
                return Err("Room is full".to_string());
            }
        }

        let player_id = self.alloc_player_id();
        let session_token = Self::generate_session_token();
        let Some(entry) = self.rooms.get_mut(room_code) else {
            return Err("Room not found".to_string());
        };

        // Late-joiners (room not in Lobby) enter as spectators
        let is_spectator = entry.room.state != RoomState::Lobby;
        entry.last_activity = Instant::now();
        let player = Player {
            id: player_id,
            display_name: player_name,
            color: player_color,
            is_leader: false,
            is_spectator,
            is_bot: false,
        };

        entry.room.players.push(player);
        entry
            .connections
            .insert(player_id, ConnectedPlayer { sender });
        entry
            .player_sessions
            .insert(player_id, session_token.clone());

        Ok((player_id, session_token))
    }

    /// Attempt to reconnect using a session token. Returns
    /// Ok((room_code, player_id, new_session_token)) on success.
    pub fn reconnect(
        &mut self,
        session_token: &str,
        sender: PlayerSender,
    ) -> Result<(String, PlayerId, String), String> {
        let session = self
            .sessions
            .remove(session_token)
            .ok_or_else(|| "Invalid or expired session".to_string())?;

        // Check TTL
        if session.disconnected_at.elapsed() > SESSION_TTL {
            return Err("Session expired".to_string());
        }

        let entry = self
            .rooms
            .get_mut(&session.room_code)
            .ok_or_else(|| "Room no longer exists".to_string())?;

        // Verify the player still exists in the room's player list
        let player_exists = entry.room.players.iter().any(|p| p.id == session.player_id);
        if !player_exists {
            return Err("Player slot no longer available".to_string());
        }

        // Restore connection
        let new_token = Self::generate_session_token();
        entry.connections.insert(
            session.player_id,
            ConnectedPlayer {
                sender: sender.clone(),
            },
        );
        entry
            .player_sessions
            .insert(session.player_id, new_token.clone());
        entry.last_activity = Instant::now();

        // Update shared broadcast senders so the game loop can reach this client
        {
            let mut senders = entry.broadcast_senders.lock().unwrap();
            senders.insert(session.player_id, sender);
        }

        Ok((session.room_code, session.player_id, new_token))
    }

    /// Remove a player from their room. If the room is mid-game, the player's
    /// slot is preserved for reconnection via session token.
    /// Returns the room code if the room was destroyed (empty after leave).
    pub fn leave_room(&mut self, room_code: &str, player_id: PlayerId) -> Option<String> {
        let entry = self.rooms.get_mut(room_code)?;

        let is_in_game = entry.room.state != RoomState::Lobby;

        // Remove the connection (the WebSocket is gone)
        entry.connections.remove(&player_id);
        // Also remove from shared broadcast senders
        {
            let mut senders = entry.broadcast_senders.lock().unwrap();
            senders.remove(&player_id);
        }

        // If room is in-game, preserve the player slot for reconnection
        if is_in_game && let Some(token) = entry.player_sessions.remove(&player_id) {
            self.sessions.insert(
                token,
                DisconnectedSession {
                    room_code: room_code.to_string(),
                    player_id,
                    disconnected_at: Instant::now(),
                },
            );
            tracing::info!(
                player_id,
                room = room_code,
                "Player disconnected mid-game, session preserved for reconnection"
            );
            return None;
        }

        // Notify active game session about player leaving permanently
        if let Some(ref cmd_tx) = entry.game_command_tx
            && let Err(e) = cmd_tx.send(GameCommand::PlayerLeft { player_id })
        {
            tracing::debug!(player_id, room = room_code, error = %e, "Game session gone");
        }

        entry.player_sessions.remove(&player_id);
        entry.room.players.retain(|p| p.id != player_id);

        if entry.room.players.is_empty() {
            // Stop the game session if running
            if let Some(ref cmd_tx) = entry.game_command_tx
                && let Err(e) = cmd_tx.send(GameCommand::Stop)
            {
                tracing::debug!(room = room_code, error = %e, "Game session already stopped");
            }
            self.rooms.remove(room_code);
            return Some(room_code.to_string());
        }

        // If the host left, migrate to the next player
        if entry.room.leader_id == player_id
            && let Some(new_host) = entry.room.players.first()
        {
            entry.room.leader_id = new_host.id;
            for p in &mut entry.room.players {
                p.is_leader = p.id == entry.room.leader_id;
            }
        }

        None
    }

    /// Clean up expired disconnected sessions. Returns the number removed.
    pub fn cleanup_expired_sessions(&mut self) -> usize {
        let before = self.sessions.len();
        self.sessions
            .retain(|_, s| s.disconnected_at.elapsed() <= SESSION_TTL);

        // Also remove player slots from rooms for expired sessions
        // Note: We don't remove player entries from rooms here because the
        // game session manages its own player lifecycle. The session cleanup
        // just prevents stale tokens from being used.
        before - self.sessions.len()
    }

    /// Add a bot player to the room. Only the room leader can add bots, and
    /// the room must be in the Lobby state. Returns the bot's PlayerId.
    pub fn add_bot(&mut self, room_code: &str, requester_id: PlayerId) -> Result<PlayerId, String> {
        // Validate first with an immutable borrow
        {
            let entry = self
                .rooms
                .get(room_code)
                .ok_or_else(|| "Room not found".to_string())?;
            if entry.room.leader_id != requester_id {
                return Err("Only the room leader can add bots".to_string());
            }
            if entry.room.state != RoomState::Lobby {
                return Err("Can only add bots in lobby".to_string());
            }
            if entry.room.players.len() >= entry.room.config.max_players as usize {
                return Err("Room is full".to_string());
            }
        }

        let bot_id = self.alloc_player_id();
        let entry = self.rooms.get_mut(room_code).unwrap();
        let bot_number = entry.room.players.iter().filter(|p| p.is_bot).count() + 1;
        let color_index = entry.room.players.len();
        let color = PlayerColor::PALETTE[color_index % PlayerColor::PALETTE.len()];

        let bot = Player {
            id: bot_id,
            display_name: format!("Bot {bot_number}"),
            color,
            is_leader: false,
            is_spectator: false,
            is_bot: true,
        };
        entry.room.players.push(bot);
        entry.last_activity = Instant::now();

        Ok(bot_id)
    }

    /// Remove a bot player from the room. Only the room leader can remove bots.
    pub fn remove_bot(
        &mut self,
        room_code: &str,
        bot_id: PlayerId,
        requester_id: PlayerId,
    ) -> Result<(), String> {
        let entry = self
            .rooms
            .get_mut(room_code)
            .ok_or_else(|| "Room not found".to_string())?;

        if entry.room.leader_id != requester_id {
            return Err("Only the room leader can remove bots".to_string());
        }

        let is_bot = entry
            .room
            .players
            .iter()
            .any(|p| p.id == bot_id && p.is_bot);
        if !is_bot {
            return Err("Player is not a bot".to_string());
        }

        entry.room.players.retain(|p| p.id != bot_id);
        entry.last_activity = Instant::now();
        Ok(())
    }

    /// Get the list of players in a room.
    #[cfg(test)]
    pub fn get_players(&self, room_code: &str) -> Option<Vec<Player>> {
        self.rooms.get(room_code).map(|e| e.room.players.clone())
    }

    /// Get the host ID for a room.
    pub fn get_leader_id(&self, room_code: &str) -> Option<PlayerId> {
        self.rooms.get(room_code).map(|e| e.room.leader_id)
    }

    /// Get room state.
    pub fn get_room_state(&self, room_code: &str) -> Option<RoomState> {
        self.rooms.get(room_code).map(|e| e.room.state)
    }

    /// Update room state. Returns true if the transition was valid.
    /// Invalid transitions are logged and rejected.
    pub fn set_room_state(&mut self, room_code: &str, new_state: RoomState) -> bool {
        if let Some(entry) = self.rooms.get_mut(room_code) {
            let valid = matches!(
                (entry.room.state, new_state),
                (RoomState::Lobby, RoomState::InGame)
                    | (RoomState::InGame, RoomState::BetweenRounds)
                    | (RoomState::InGame, RoomState::Lobby)
                    | (RoomState::BetweenRounds, RoomState::InGame)
                    | (RoomState::BetweenRounds, RoomState::Lobby)
            );
            if valid {
                entry.room.state = new_state;
            } else {
                tracing::warn!(
                    room = room_code,
                    from = ?entry.room.state,
                    to = ?new_state,
                    "Invalid room state transition"
                );
            }
            valid
        } else {
            false
        }
    }

    /// Start a server-authoritative game session in a room.
    /// Returns Ok(()) on success, or Err(reason) if the game can't be started.
    pub fn start_game(
        &mut self,
        room_code: &str,
        game_name: &str,
        requester_id: PlayerId,
        registry: &std::sync::Arc<ServerGameRegistry>,
        rooms: crate::state::SharedRoomManager,
        custom: HashMap<String, serde_json::Value>,
    ) -> Result<(), String> {
        let entry = self
            .rooms
            .get_mut(room_code)
            .ok_or_else(|| "Room not found".to_string())?;

        // Only the room leader can start the game
        if entry.room.leader_id != requester_id {
            return Err("Only the room leader can start the game".to_string());
        }

        // Must be in Lobby state
        if entry.room.state != RoomState::Lobby {
            return Err("Game already in progress".to_string());
        }

        let game_id =
            GameId::from_str_opt(game_name).ok_or_else(|| format!("Unknown game: {game_name}"))?;

        let config = GameSessionConfig {
            game_id,
            players: entry.room.players.clone(),
            leader_id: entry.room.leader_id,
            round_count: 0, // Let the game decide via round_count_hint()
            round_duration: entry.room.config.round_duration,
            between_round_duration: entry.room.config.between_round_duration,
            custom,
        };

        let (cmd_tx, broadcast_rx, game_handle) = spawn_game_session(registry, config)
            .ok_or_else(|| format!("Failed to create game: {game_name}"))?;

        // Populate shared broadcast senders from current connections
        {
            let mut senders = entry.broadcast_senders.lock().unwrap();
            senders.clear();
            for (&id, conn) in &entry.connections {
                senders.insert(id, conn.sender.clone());
            }
        }
        let shared_senders = Arc::clone(&entry.broadcast_senders);
        let room_code_owned = room_code.to_string();
        let rooms_clone = rooms;
        let broadcast_handle = tokio::spawn(async move {
            forward_broadcasts(broadcast_rx, shared_senders, &room_code_owned).await;
            // Game ended — clean up room state and notify clients
            let mut mgr = rooms_clone.write().await;
            mgr.end_game_session(&room_code_owned);
            mgr.broadcast_player_list(&room_code_owned);
        });

        entry.game_command_tx = Some(cmd_tx);
        entry.game_task = Some(game_handle);
        entry.broadcast_task = Some(broadcast_handle);
        entry.room.state = RoomState::InGame;
        entry.last_activity = Instant::now();

        Ok(())
    }

    /// Route a player's input to the active game session.
    pub fn route_player_input(
        &self,
        room_code: &str,
        player_id: PlayerId,
        tick: u32,
        input_data: Vec<u8>,
    ) {
        if let Some(entry) = self.rooms.get(room_code)
            && let Some(ref cmd_tx) = entry.game_command_tx
            && let Err(e) = cmd_tx.send(GameCommand::PlayerInput {
                player_id,
                tick,
                input_data,
            })
        {
            tracing::debug!(player_id, room = room_code, error = %e, "Game session gone");
        }
    }

    /// Check if a room has an active game session.
    pub fn has_active_game(&self, room_code: &str) -> bool {
        self.rooms
            .get(room_code)
            .and_then(|e| e.game_command_tx.as_ref())
            .is_some()
    }

    /// Clean up a game session when it ends.
    pub fn end_game_session(&mut self, room_code: &str) {
        if let Some(entry) = self.rooms.get_mut(room_code) {
            if let Some(ref cmd_tx) = entry.game_command_tx
                && let Err(e) = cmd_tx.send(GameCommand::Stop)
            {
                tracing::debug!(room = room_code, error = %e, "Game session already stopped");
            }
            entry.game_command_tx = None;
            entry.game_task = None;
            entry.broadcast_task = None;
            entry.room.state = RoomState::Lobby;
        }
    }

    /// Send a raw binary message to a specific player.
    pub fn send_to_player(&self, room_code: &str, player_id: PlayerId, data: Bytes) {
        if let Some(entry) = self.rooms.get(room_code)
            && let Some(conn) = entry.connections.get(&player_id)
            && let Err(e) = conn.sender.try_send(data)
        {
            tracing::debug!(
                player_id, room = room_code, error = %e,
                "Failed to send to player (slow or disconnected)"
            );
        }
    }

    /// Broadcast raw binary data to all players in a room.
    /// Uses `Bytes` internally for zero-copy cloning across player channels.
    pub fn broadcast_to_room(&self, room_code: &str, data: &[u8]) {
        if let Some(entry) = self.rooms.get(room_code) {
            let bytes = Bytes::copy_from_slice(data);
            for (&pid, conn) in &entry.connections {
                if let Err(e) = conn.sender.try_send(bytes.clone()) {
                    tracing::debug!(
                        player_id = pid, room = room_code, error = %e,
                        "Skipping broadcast to slow client"
                    );
                }
            }
        }
    }

    /// Broadcast raw binary data to all players except one.
    pub fn broadcast_to_room_except(&self, room_code: &str, exclude: PlayerId, data: &[u8]) {
        if let Some(entry) = self.rooms.get(room_code) {
            let bytes = Bytes::copy_from_slice(data);
            for (&id, conn) in &entry.connections {
                if id != exclude
                    && let Err(e) = conn.sender.try_send(bytes.clone())
                {
                    tracing::debug!(
                        player_id = id, room = room_code, error = %e,
                        "Skipping broadcast to slow client"
                    );
                }
            }
        }
    }

    /// Build and broadcast a PlayerList update to everyone in the room.
    pub fn broadcast_player_list(&self, room_code: &str) {
        if let Some(entry) = self.rooms.get(room_code) {
            let msg = ServerMessage::PlayerList(PlayerListMsg {
                players: entry.room.players.clone(),
                leader_id: entry.room.leader_id,
            });
            if let Ok(data) = encode_server_message(&msg) {
                let bytes = Bytes::from(data);
                for (&pid, conn) in &entry.connections {
                    if let Err(e) = conn.sender.try_send(bytes.clone()) {
                        tracing::debug!(
                            player_id = pid, room = room_code, error = %e,
                            "Skipping player list broadcast to slow client"
                        );
                    }
                }
            }
        }
    }

    /// Build a JoinRoomResponse success message.
    pub fn make_join_response(
        player_id: PlayerId,
        room_code: &str,
        room_state: RoomState,
        session_token: &str,
    ) -> Result<Vec<u8>, breakpoint_core::net::protocol::ProtocolError> {
        let msg = ServerMessage::JoinRoomResponse(JoinRoomResponseMsg {
            success: true,
            player_id: Some(player_id),
            room_code: Some(room_code.to_string()),
            room_state: Some(room_state),
            error: None,
            session_token: Some(session_token.to_string()),
        });
        encode_server_message(&msg)
    }

    /// Build a JoinRoomResponse error message.
    pub fn make_join_error(
        error: &str,
    ) -> Result<Vec<u8>, breakpoint_core::net::protocol::ProtocolError> {
        let msg = ServerMessage::JoinRoomResponse(JoinRoomResponseMsg {
            success: false,
            player_id: None,
            room_code: None,
            room_state: None,
            error: Some(error.to_string()),
            session_token: None,
        });
        encode_server_message(&msg)
    }

    /// Broadcast raw binary data to all players in all rooms.
    /// Uses `Bytes` for zero-copy cloning across all player channels.
    pub fn broadcast_to_all_rooms(&self, data: &[u8]) {
        let bytes = Bytes::copy_from_slice(data);
        for (room_code, entry) in &self.rooms {
            for (&pid, conn) in &entry.connections {
                if let Err(e) = conn.sender.try_send(bytes.clone()) {
                    tracing::debug!(
                        player_id = pid, room = %room_code, error = %e,
                        "Skipping global broadcast to slow client"
                    );
                }
            }
        }
    }

    /// Look up a player's display name by room code and player id.
    pub fn get_player_name(&self, room_code: &str, player_id: PlayerId) -> Option<String> {
        self.rooms
            .get(room_code)?
            .room
            .players
            .iter()
            .find(|p| p.id == player_id)
            .map(|p| p.display_name.clone())
    }

    /// Touch room activity timestamp (call on any incoming message).
    pub fn touch_activity(&mut self, room_code: &str) {
        if let Some(entry) = self.rooms.get_mut(room_code) {
            entry.last_activity = Instant::now();
        }
    }

    /// Remove rooms that have been idle for longer than `max_idle`.
    /// Returns the number of rooms removed.
    pub fn cleanup_idle_rooms(&mut self, max_idle: Duration) -> usize {
        let now = Instant::now();
        let before = self.rooms.len();
        self.rooms
            .retain(|_, entry| now.duration_since(entry.last_activity) < max_idle);
        before - self.rooms.len()
    }

    /// Check if a room exists.
    #[cfg(test)]
    pub fn room_exists(&self, room_code: &str) -> bool {
        self.rooms.contains_key(room_code)
    }
}

/// Forward game broadcasts to all connected players in a room.
/// Uses a shared sender map so reconnected clients are included dynamically.
async fn forward_broadcasts(
    mut broadcast_rx: mpsc::UnboundedReceiver<crate::game_loop::GameBroadcast>,
    senders: Arc<Mutex<HashMap<PlayerId, PlayerSender>>>,
    room_code: &str,
) {
    while let Some(broadcast) = broadcast_rx.recv().await {
        match broadcast {
            GameBroadcast::EncodedMessage(data) => {
                let snapshot = senders.lock().unwrap().clone();
                for (&player_id, sender) in &snapshot {
                    if sender.try_send(data.clone()).is_err() {
                        tracing::debug!(
                            player_id,
                            room = room_code,
                            "Skipping broadcast to slow client (channel full or closed)"
                        );
                    }
                }
            },
            GameBroadcast::GameEnded => {
                tracing::info!(room = room_code, "Game session ended");
                break;
            },
        }
    }
}

/// Generate a unique room code, retrying on collision with existing rooms.
fn generate_unique_room_code(existing: &HashMap<String, RoomEntry>) -> String {
    loop {
        let code = breakpoint_core::room::generate_room_code();
        if !existing.contains_key(&code) {
            return code;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use breakpoint_core::player::PlayerColor;

    fn make_sender() -> (PlayerSender, mpsc::Receiver<Bytes>) {
        mpsc::channel(256)
    }

    #[test]
    fn create_room_returns_valid_code() {
        let mut mgr = RoomManager::new();
        let (tx, _rx) = make_sender();
        let (code, player_id, token) = mgr.create_room("Alice".into(), PlayerColor::default(), tx);
        assert!(breakpoint_core::room::is_valid_room_code(&code));
        assert_eq!(player_id, 1);
        assert!(!token.is_empty());
        assert!(mgr.room_exists(&code));
    }

    #[test]
    fn join_room_succeeds() {
        let mut mgr = RoomManager::new();
        let (tx1, _rx1) = make_sender();
        let (code, ..) = mgr.create_room("Alice".into(), PlayerColor::default(), tx1);

        let (tx2, _rx2) = make_sender();
        let result = mgr.join_room(&code, "Bob".into(), PlayerColor::PALETTE[1], tx2);
        assert!(result.is_ok());

        let players = mgr.get_players(&code).unwrap();
        assert_eq!(players.len(), 2);
    }

    #[test]
    fn join_nonexistent_room_fails() {
        let mut mgr = RoomManager::new();
        let (tx, _rx) = make_sender();
        let result = mgr.join_room("XXXX-0000", "Bob".into(), PlayerColor::default(), tx);
        assert!(result.is_err());
    }

    #[test]
    fn join_full_room_fails() {
        let mut mgr = RoomManager::new();
        let (tx1, _rx1) = make_sender();
        let (code, ..) = mgr.create_room("Alice".into(), PlayerColor::default(), tx1);

        // Fill the room (default max_players is 8, host is 1, so 7 more)
        for i in 0..7 {
            let (tx, _rx) = make_sender();
            let name = format!("Player{i}");
            mgr.join_room(&code, name, PlayerColor::default(), tx)
                .unwrap();
        }

        let (tx_extra, _rx_extra) = make_sender();
        let result = mgr.join_room(&code, "Extra".into(), PlayerColor::default(), tx_extra);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("full"));
    }

    #[test]
    fn leave_room_removes_player() {
        let mut mgr = RoomManager::new();
        let (tx1, _rx1) = make_sender();
        let (code, leader_id, _) = mgr.create_room("Alice".into(), PlayerColor::default(), tx1);

        let (tx2, _rx2) = make_sender();
        let (bob_id, _) = mgr
            .join_room(&code, "Bob".into(), PlayerColor::default(), tx2)
            .unwrap();

        mgr.leave_room(&code, bob_id);
        let players = mgr.get_players(&code).unwrap();
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].id, leader_id);
    }

    #[test]
    fn leave_room_destroys_empty_room() {
        let mut mgr = RoomManager::new();
        let (tx, _rx) = make_sender();
        let (code, leader_id, _) = mgr.create_room("Alice".into(), PlayerColor::default(), tx);

        let destroyed = mgr.leave_room(&code, leader_id);
        assert!(destroyed.is_some());
        assert!(!mgr.room_exists(&code));
    }

    #[test]
    fn host_migration_on_leave() {
        let mut mgr = RoomManager::new();
        let (tx1, _rx1) = make_sender();
        let (code, leader_id, _) = mgr.create_room("Alice".into(), PlayerColor::default(), tx1);

        let (tx2, _rx2) = make_sender();
        let (bob_id, _) = mgr
            .join_room(&code, "Bob".into(), PlayerColor::default(), tx2)
            .unwrap();

        mgr.leave_room(&code, leader_id);
        assert_eq!(mgr.get_leader_id(&code), Some(bob_id));
        let players = mgr.get_players(&code).unwrap();
        assert!(players[0].is_leader);
    }

    #[test]
    fn idle_room_cleanup_removes_stale_rooms() {
        let mut mgr = RoomManager::new();
        let (tx1, _rx1) = make_sender();
        let (code1, ..) = mgr.create_room("Alice".into(), PlayerColor::default(), tx1);

        let (tx2, _rx2) = make_sender();
        let (code2, ..) = mgr.create_room("Bob".into(), PlayerColor::default(), tx2);

        // Artificially age the first room
        mgr.rooms.get_mut(&code1).unwrap().last_activity =
            Instant::now() - Duration::from_secs(7200);

        let removed = mgr.cleanup_idle_rooms(Duration::from_secs(3600));
        assert_eq!(removed, 1);
        assert!(!mgr.room_exists(&code1));
        assert!(mgr.room_exists(&code2));
    }

    #[test]
    fn valid_state_transitions() {
        let mut mgr = RoomManager::new();
        let (tx, _rx) = make_sender();
        let (code, ..) = mgr.create_room("Alice".into(), PlayerColor::default(), tx);

        assert!(mgr.set_room_state(&code, RoomState::InGame));
        assert_eq!(mgr.get_room_state(&code), Some(RoomState::InGame));

        assert!(mgr.set_room_state(&code, RoomState::BetweenRounds));
        assert_eq!(mgr.get_room_state(&code), Some(RoomState::BetweenRounds));

        assert!(mgr.set_room_state(&code, RoomState::InGame));
        assert!(mgr.set_room_state(&code, RoomState::Lobby));
    }

    #[test]
    fn invalid_state_transition_rejected() {
        let mut mgr = RoomManager::new();
        let (tx, _rx) = make_sender();
        let (code, ..) = mgr.create_room("Alice".into(), PlayerColor::default(), tx);

        // Lobby → Lobby is invalid
        assert!(!mgr.set_room_state(&code, RoomState::Lobby));
        // Lobby → BetweenRounds is invalid
        assert!(!mgr.set_room_state(&code, RoomState::BetweenRounds));
        // State should remain unchanged
        assert_eq!(mgr.get_room_state(&code), Some(RoomState::Lobby));
    }

    #[test]
    fn room_code_format() {
        for _ in 0..100 {
            let code = breakpoint_core::room::generate_room_code();
            assert!(
                breakpoint_core::room::is_valid_room_code(&code),
                "Invalid room code: {code}"
            );
        }
    }

    #[test]
    fn session_reconnect_restores_player() {
        let mut mgr = RoomManager::new();
        let (tx1, _rx1) = make_sender();
        let (code, pid, token) = mgr.create_room("Alice".into(), PlayerColor::default(), tx1);

        // Set room to InGame so leave preserves the session
        mgr.set_room_state(&code, RoomState::InGame);

        // Simulate disconnect (leave room mid-game)
        mgr.leave_room(&code, pid);

        // Session should exist
        assert!(mgr.sessions.contains_key(&token));

        // Reconnect with the session token
        let (tx2, _rx2) = make_sender();
        let result = mgr.reconnect(&token, tx2);
        assert!(result.is_ok());
        let (recon_code, recon_pid, new_token) = result.unwrap();
        assert_eq!(recon_code, code);
        assert_eq!(recon_pid, pid);
        assert_ne!(new_token, token); // new token issued
    }

    #[test]
    fn session_invalid_token_rejected() {
        let mut mgr = RoomManager::new();
        let (tx, _rx) = make_sender();
        let result = mgr.reconnect("nonexistent-token", tx);
        assert!(result.is_err());
    }
}
