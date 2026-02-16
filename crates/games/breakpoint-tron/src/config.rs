use serde::{Deserialize, Serialize};

/// Data-driven configuration for the Tron game.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TronConfig {
    /// Base cycle speed (units/s).
    pub base_speed: f32,
    /// Maximum speed a cycle can reach via wall acceleration.
    pub max_speed: f32,
    /// Wall acceleration threshold distance (units). Walls within this distance boost speed.
    pub grind_distance: f32,
    /// Maximum speed bonus multiplier from wall grinding (e.g. 2.0 = 2x base speed).
    pub grind_max_multiplier: f32,
    /// Speed penalty fraction per turn (e.g. 0.05 = 5% reduction).
    pub turn_speed_penalty: f32,
    /// Minimum delay between turns (seconds).
    pub turn_delay: f32,
    /// Initial brake fuel.
    pub brake_fuel_max: f32,
    /// Brake fuel consumption rate per second.
    pub brake_drain_rate: f32,
    /// Brake fuel regeneration rate per second (when not braking).
    pub brake_regen_rate: f32,
    /// Brake speed multiplier (e.g. 0.5 = half speed while braking).
    pub brake_speed_mult: f32,
    /// Rubber amount: distance buffer before wall contact kills.
    pub rubber_max: f32,
    /// Rubber consumption rate when approaching walls head-on.
    pub rubber_drain_rate: f32,
    /// Arena width.
    pub arena_width: f32,
    /// Arena depth.
    pub arena_depth: f32,
    /// Round duration in seconds (game config).
    pub round_duration_secs: f32,
    /// Number of rounds per match.
    pub round_count: u8,
    /// Win zone appear delay (seconds since round start).
    pub win_zone_delay: f32,
    /// Time since last death before win zone can appear (seconds).
    pub win_zone_death_delay: f32,
    /// Win zone expansion rate (units/s).
    pub win_zone_expand_rate: f32,
    /// Speed decay rate toward base speed (units/s/s).
    pub speed_decay_rate: f32,
    /// Collision distance for cycle-to-wall checks.
    pub collision_distance: f32,
}

impl Default for TronConfig {
    fn default() -> Self {
        Self {
            base_speed: 20.0,
            max_speed: 60.0,
            grind_distance: 6.0,
            grind_max_multiplier: 2.0,
            turn_speed_penalty: 0.05,
            turn_delay: 0.1,
            brake_fuel_max: 3.0,
            brake_drain_rate: 1.0,
            brake_regen_rate: 0.5,
            brake_speed_mult: 0.5,
            rubber_max: 0.5,
            rubber_drain_rate: 10.0,
            arena_width: 500.0,
            arena_depth: 500.0,
            round_duration_secs: 120.0,
            round_count: 10,
            win_zone_delay: 60.0,
            win_zone_death_delay: 30.0,
            win_zone_expand_rate: 5.0,
            speed_decay_rate: 10.0,
            collision_distance: 0.5,
        }
    }
}

impl TronConfig {
    /// Load config from environment or TOML file, falling back to defaults.
    pub fn load() -> Self {
        if let Ok(path) = std::env::var("BREAKPOINT_TRON_CONFIG")
            && let Ok(contents) = std::fs::read_to_string(&path)
            && let Ok(config) = toml::from_str::<Self>(&contents)
        {
            return config;
        }
        if let Ok(contents) = std::fs::read_to_string("config/tron.toml")
            && let Ok(config) = toml::from_str::<Self>(&contents)
        {
            return config;
        }
        Self::default()
    }
}
