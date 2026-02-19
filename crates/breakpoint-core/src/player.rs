use std::path::Path;

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
    #[serde(default)]
    pub is_bot: bool,
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

// ---------------------------------------------------------------------------
// Configurable player color palette
// ---------------------------------------------------------------------------

/// Default path for the player colors TOML file.
const DEFAULT_CONFIG_PATH: &str = "config/player_colors.toml";

/// Environment variable that overrides the config file path.
const ENV_VAR: &str = "BREAKPOINT_PLAYER_COLORS";

/// Configurable player color palette loaded from TOML.
///
/// When no config file is present (or it is unparseable), the palette
/// falls back to the same 8 built-in colors defined in
/// [`PlayerColor::PALETTE`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerColorConfig {
    /// Ordered list of `(r, g, b)` color triples.
    pub colors: Vec<(u8, u8, u8)>,
}

impl Default for PlayerColorConfig {
    fn default() -> Self {
        Self {
            colors: PlayerColor::PALETTE
                .iter()
                .map(|c| (c.r, c.g, c.b))
                .collect(),
        }
    }
}

impl PlayerColorConfig {
    /// Load the color palette from disk or environment.
    ///
    /// Resolution order:
    /// 1. File at the path given by `BREAKPOINT_PLAYER_COLORS` env var
    /// 2. File at the default path `config/player_colors.toml`
    /// 3. Built-in defaults
    ///
    /// Parse errors are logged at `warn` level and the built-in
    /// defaults are returned.
    pub fn load() -> Self {
        let path = std::env::var(ENV_VAR).unwrap_or_else(|_| DEFAULT_CONFIG_PATH.to_string());
        Self::load_from_path(&path)
    }

    /// Load from a specific file path, falling back to defaults on any
    /// error.
    pub fn load_from_path(path: &str) -> Self {
        let p = Path::new(path);
        if !p.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(p) {
            Ok(contents) => match toml::from_str::<PlayerColorConfig>(&contents) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::warn!(
                        path = %path,
                        error = %e,
                        "Failed to parse player color config; using defaults",
                    );
                    Self::default()
                },
            },
            Err(e) => {
                tracing::warn!(
                    path = %path,
                    error = %e,
                    "Failed to read player color config file; using defaults",
                );
                Self::default()
            },
        }
    }

    /// Return the palette as a `Vec<PlayerColor>`.
    pub fn palette(&self) -> Vec<PlayerColor> {
        self.colors
            .iter()
            .map(|&(r, g, b)| PlayerColor { r, g, b })
            .collect()
    }

    /// Retrieve a color by index (wrapping around if index exceeds
    /// palette length). Falls back to `PlayerColor::default()` if the
    /// palette is empty.
    pub fn color_at(&self, index: usize) -> PlayerColor {
        if self.colors.is_empty() {
            return PlayerColor::default();
        }
        let (r, g, b) = self.colors[index % self.colors.len()];
        PlayerColor { r, g, b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_returns_eight_colors() {
        let cfg = PlayerColorConfig::default();
        assert_eq!(cfg.colors.len(), 8);
        // First color should be Red (255, 87, 87)
        assert_eq!(cfg.colors[0], (255, 87, 87));
        // Last color should be Pink (255, 107, 175)
        assert_eq!(cfg.colors[7], (255, 107, 175));
    }

    #[test]
    fn toml_roundtrip() {
        let cfg = PlayerColorConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).expect("serialize to TOML");
        let deserialized: PlayerColorConfig =
            toml::from_str(&toml_str).expect("deserialize from TOML");
        assert_eq!(cfg.colors, deserialized.colors);
    }

    #[test]
    fn json_roundtrip() {
        let cfg = PlayerColorConfig::default();
        let json_str = serde_json::to_string(&cfg).expect("serialize to JSON");
        let deserialized: PlayerColorConfig =
            serde_json::from_str(&json_str).expect("deserialize from JSON");
        assert_eq!(cfg.colors, deserialized.colors);
    }

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let cfg = PlayerColorConfig::load_from_path("/nonexistent/path/colors.toml");
        assert_eq!(cfg.colors.len(), 8);
        assert_eq!(cfg.colors[0], (255, 87, 87));
    }

    #[test]
    fn load_from_invalid_toml_returns_defaults() {
        let dir = std::env::temp_dir().join("breakpoint_test_invalid_toml");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.toml");
        std::fs::write(&path, "this is not { valid toml !!!").unwrap();
        let cfg = PlayerColorConfig::load_from_path(path.to_str().unwrap());
        assert_eq!(cfg.colors.len(), 8);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_valid_toml_file() {
        let dir = std::env::temp_dir().join("breakpoint_test_valid_toml");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("colors.toml");
        std::fs::write(&path, "colors = [[10, 20, 30], [40, 50, 60]]\n").unwrap();
        let cfg = PlayerColorConfig::load_from_path(path.to_str().unwrap());
        assert_eq!(cfg.colors.len(), 2);
        assert_eq!(cfg.colors[0], (10, 20, 30));
        assert_eq!(cfg.colors[1], (40, 50, 60));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn palette_conversion() {
        let cfg = PlayerColorConfig {
            colors: vec![(1, 2, 3), (4, 5, 6)],
        };
        let palette = cfg.palette();
        assert_eq!(palette.len(), 2);
        assert_eq!(palette[0], PlayerColor { r: 1, g: 2, b: 3 });
        assert_eq!(palette[1], PlayerColor { r: 4, g: 5, b: 6 });
    }

    #[test]
    fn color_at_wraps_index() {
        let cfg = PlayerColorConfig {
            colors: vec![(10, 20, 30), (40, 50, 60)],
        };
        assert_eq!(
            cfg.color_at(0),
            PlayerColor {
                r: 10,
                g: 20,
                b: 30
            }
        );
        assert_eq!(
            cfg.color_at(1),
            PlayerColor {
                r: 40,
                g: 50,
                b: 60
            }
        );
        // wraps around
        assert_eq!(
            cfg.color_at(2),
            PlayerColor {
                r: 10,
                g: 20,
                b: 30
            }
        );
        assert_eq!(
            cfg.color_at(3),
            PlayerColor {
                r: 40,
                g: 50,
                b: 60
            }
        );
    }

    #[test]
    fn color_at_empty_palette_returns_default() {
        let cfg = PlayerColorConfig { colors: vec![] };
        assert_eq!(cfg.color_at(0), PlayerColor::default());
    }

    #[test]
    fn default_palette_matches_hardcoded_palette() {
        let cfg = PlayerColorConfig::default();
        let palette = cfg.palette();
        assert_eq!(palette.len(), PlayerColor::PALETTE.len());
        for (i, color) in palette.iter().enumerate() {
            assert_eq!(*color, PlayerColor::PALETTE[i]);
        }
    }
}
