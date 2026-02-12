use bevy::prelude::*;

/// Application state machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, States)]
pub enum AppState {
    #[default]
    Lobby,
    InGame,
    #[allow(dead_code)]
    BetweenRounds,
    #[allow(dead_code)]
    GameOver,
}
