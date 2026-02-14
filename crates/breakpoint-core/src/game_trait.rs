use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Unique identifier for a player in the game.
pub type PlayerId = u64;

/// Core trait that all Breakpoint games must implement.
///
/// The runtime manages networking, overlay, and player tracking;
/// the game only handles game-specific logic and rendering.
pub trait BreakpointGame: Send + Sync {
    /// Game metadata for the lobby selection screen.
    fn metadata(&self) -> GameMetadata;

    /// Called once when the game is selected and players are ready.
    fn init(&mut self, players: &[super::player::Player], config: &GameConfig);

    /// Called each frame. Returns a list of game events.
    fn update(&mut self, dt: f32, inputs: &PlayerInputs) -> Vec<GameEvent>;

    /// Serialize the authoritative game state for network broadcast.
    fn serialize_state(&self) -> Vec<u8>;

    /// Apply authoritative state received from the host.
    fn apply_state(&mut self, state: &[u8]);

    /// Apply a remote player's input to the authoritative simulation.
    fn apply_input(&mut self, player_id: PlayerId, input: &[u8]);

    /// Called when a new player joins mid-game.
    fn player_joined(&mut self, player: &super::player::Player);

    /// Called when a player disconnects.
    fn player_left(&mut self, player_id: PlayerId);

    /// Simulation tick rate in Hz. Different games may run at different rates.
    fn tick_rate(&self) -> f32 {
        10.0
    }

    /// Hint for the number of rounds this game wants to play (e.g. 9 holes for golf).
    /// The framework uses this to set `round_count` in the initial `GameConfig`.
    fn round_count_hint(&self) -> u8 {
        1
    }

    /// Whether the game supports the overlay pausing gameplay.
    fn supports_pause(&self) -> bool {
        true
    }

    /// Called when the overlay requests a pause (critical alert).
    fn pause(&mut self);

    /// Called when gameplay should resume after a pause.
    fn resume(&mut self);

    /// Whether the current round/match is complete.
    fn is_round_complete(&self) -> bool;

    /// Final scores for the completed round.
    fn round_results(&self) -> Vec<PlayerScore>;
}

/// Game metadata for the lobby selection screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMetadata {
    pub name: String,
    pub description: String,
    pub min_players: u8,
    pub max_players: u8,
    pub estimated_round_duration: Duration,
}

/// Configuration for a game session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub round_count: u8,
    pub round_duration: Duration,
    pub custom: HashMap<String, serde_json::Value>,
}

/// Collected inputs from all players for a single tick.
pub struct PlayerInputs {
    pub inputs: HashMap<PlayerId, Vec<u8>>,
}

/// Events emitted by a game during update (scoring, elimination, round end).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    ScoreUpdate { player_id: PlayerId, score: i32 },
    RoundComplete,
}

/// Score entry for a player at the end of a round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerScore {
    pub player_id: PlayerId,
    pub score: i32,
}

/// Generates the 5 boilerplate `BreakpointGame` methods that are identical across all games:
/// `serialize_state`, `apply_state`, `pause`, `resume`, `is_round_complete`.
///
/// Requires the implementing struct to have `state: $StateType` and `paused: bool` fields,
/// and `$StateType` to have a `round_complete: bool` field.
#[macro_export]
macro_rules! breakpoint_game_boilerplate {
    (state_type: $StateType:ty) => {
        fn serialize_state(&self) -> Vec<u8> {
            rmp_serde::to_vec(&self.state).expect("game state serialization must succeed")
        }

        fn apply_state(&mut self, state: &[u8]) {
            if let Ok(s) = rmp_serde::from_slice::<$StateType>(state) {
                self.state = s;
            }
        }

        fn pause(&mut self) {
            self.paused = true;
        }

        fn resume(&mut self) {
            self.paused = false;
        }

        fn is_round_complete(&self) -> bool {
            self.state.round_complete
        }
    };
}
