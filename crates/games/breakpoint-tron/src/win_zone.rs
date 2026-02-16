use serde::{Deserialize, Serialize};

use crate::config::TronConfig;

/// Expanding win zone that forces round resolution after timeout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinZone {
    /// Center X position.
    pub x: f32,
    /// Center Z position.
    pub z: f32,
    /// Current radius.
    pub radius: f32,
    /// Whether the win zone is currently active.
    pub active: bool,
}

impl Default for WinZone {
    fn default() -> Self {
        Self {
            x: 0.0,
            z: 0.0,
            radius: 0.0,
            active: false,
        }
    }
}

impl WinZone {
    /// Spawn the win zone at a random position within the arena.
    pub fn spawn(&mut self, arena_width: f32, arena_depth: f32) {
        // Place in center quarter of arena for fairness
        let margin = arena_width.min(arena_depth) * 0.25;
        self.x = arena_width / 2.0;
        self.z = arena_depth / 2.0;
        // Add some randomness with simple hash
        let hash = ((arena_width as u32)
            .wrapping_mul(31)
            .wrapping_add(arena_depth as u32)) as f32;
        self.x += (hash % margin) - margin / 2.0;
        self.z += ((hash * 1.7) % margin) - margin / 2.0;
        self.radius = 5.0;
        self.active = true;
    }

    /// Update the win zone (expand).
    pub fn update(&mut self, dt: f32, config: &TronConfig) {
        if self.active {
            self.radius += config.win_zone_expand_rate * dt;
        }
    }

    /// Check if a point is inside the win zone.
    pub fn contains(&self, x: f32, z: f32) -> bool {
        if !self.active {
            return false;
        }
        let dx = x - self.x;
        let dz = z - self.z;
        dx * dx + dz * dz <= self.radius * self.radius
    }
}

/// Check whether the win zone should appear based on round timer and last death time.
pub fn should_spawn_win_zone(
    round_timer: f32,
    time_since_last_death: f32,
    config: &TronConfig,
) -> bool {
    round_timer >= config.win_zone_delay && time_since_last_death >= config.win_zone_death_delay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_zone_spawn_and_contain() {
        let mut wz = WinZone::default();
        assert!(!wz.active);
        assert!(!wz.contains(250.0, 250.0));

        wz.spawn(500.0, 500.0);
        assert!(wz.active);
        assert!(wz.radius > 0.0);

        // Center should be within the zone
        assert!(wz.contains(wz.x, wz.z));
    }

    #[test]
    fn win_zone_expands() {
        let config = TronConfig::default();
        let mut wz = WinZone::default();
        wz.spawn(500.0, 500.0);
        let r_before = wz.radius;

        wz.update(1.0, &config);
        assert!(wz.radius > r_before, "Win zone should expand");
    }

    #[test]
    fn win_zone_timing() {
        let config = TronConfig::default();

        // Too early
        assert!(!should_spawn_win_zone(30.0, 40.0, &config));

        // Round time OK but recent death
        assert!(!should_spawn_win_zone(65.0, 10.0, &config));

        // Both conditions met
        assert!(should_spawn_win_zone(65.0, 35.0, &config));
    }
}
