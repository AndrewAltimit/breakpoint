use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::game_trait::PlayerId;
use crate::player::Player;

/// Configuration for a Breakpoint room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomConfig {
    pub max_players: u8,
    pub round_count: u8,
    pub round_duration: Duration,
    pub between_round_duration: Duration,
    pub host_migration_enabled: bool,
    pub host_disconnect_grace_period: Duration,
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            max_players: 8,
            round_count: 9,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(30),
            host_migration_enabled: false,
            host_disconnect_grace_period: Duration::from_secs(60),
        }
    }
}

/// Current state of a room.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomState {
    #[default]
    Lobby,
    InGame,
    BetweenRounds,
}

/// A Breakpoint room containing players and game state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub code: String,
    pub config: RoomConfig,
    pub state: RoomState,
    pub players: Vec<Player>,
    pub host_id: PlayerId,
    pub current_round: u8,
}

impl Room {
    pub fn new(code: String, host: Player) -> Self {
        let host_id = host.id;
        Self {
            code,
            config: RoomConfig::default(),
            state: RoomState::Lobby,
            players: vec![host],
            host_id,
            current_round: 0,
        }
    }
}

/// Validates that a room code matches the ABCD-1234 format.
pub fn is_valid_room_code(code: &str) -> bool {
    if code.len() != 9 {
        return false;
    }
    let bytes = code.as_bytes();
    bytes[0..4].iter().all(|b| b.is_ascii_uppercase())
        && bytes[4] == b'-'
        && bytes[5..9].iter().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_room_codes() {
        assert!(is_valid_room_code("ABCD-1234"));
        assert!(is_valid_room_code("ZXYW-0000"));
        assert!(is_valid_room_code("GAME-9999"));
    }

    #[test]
    fn invalid_room_codes() {
        assert!(!is_valid_room_code(""));
        assert!(!is_valid_room_code("ABCD1234"));
        assert!(!is_valid_room_code("abcd-1234"));
        assert!(!is_valid_room_code("ABCD-123"));
        assert!(!is_valid_room_code("ABC-1234"));
        assert!(!is_valid_room_code("ABCD-123A"));
        assert!(!is_valid_room_code("1234-ABCD"));
    }
}
