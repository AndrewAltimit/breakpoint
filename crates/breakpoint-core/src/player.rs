use serde::{Deserialize, Serialize};

use crate::game_trait::PlayerId;

/// A player connected to a Breakpoint room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub display_name: String,
    pub color: PlayerColor,
    pub is_leader: bool,
    pub is_spectator: bool,
}

/// Avatar color selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Default for PlayerColor {
    fn default() -> Self {
        Self::PALETTE[0]
    }
}

impl PlayerColor {
    /// Predefined palette colors for player selection.
    pub const PALETTE: &[PlayerColor] = &[
        PlayerColor {
            r: 255,
            g: 87,
            b: 87,
        }, // Red
        PlayerColor {
            r: 78,
            g: 205,
            b: 196,
        }, // Teal
        PlayerColor {
            r: 255,
            g: 195,
            b: 18,
        }, // Yellow
        PlayerColor {
            r: 130,
            g: 88,
            b: 255,
        }, // Purple
        PlayerColor {
            r: 46,
            g: 213,
            b: 115,
        }, // Green
        PlayerColor {
            r: 255,
            g: 148,
            b: 77,
        }, // Orange
        PlayerColor {
            r: 83,
            g: 152,
            b: 255,
        }, // Blue
        PlayerColor {
            r: 255,
            g: 107,
            b: 175,
        }, // Pink
    ];
}
