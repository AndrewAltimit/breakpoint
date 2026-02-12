use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for a Breakpoint room.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomState {
    Lobby,
    InGame,
    BetweenRounds,
}
