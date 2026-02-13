use serde::{Deserialize, Serialize};

use breakpoint_core::powerup;

/// Laser Tag power-up types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaserPowerUpKind {
    RapidFire,
    Shield,
    SpeedBoost,
    WideBeam,
}

impl powerup::PowerUpKind for LaserPowerUpKind {
    fn duration(&self) -> f32 {
        match self {
            LaserPowerUpKind::RapidFire => 5.0,
            LaserPowerUpKind::Shield => f32::INFINITY,
            LaserPowerUpKind::SpeedBoost => 4.0,
            LaserPowerUpKind::WideBeam => 3.0,
        }
    }
}

/// Active power-up on a player.
pub type ActiveLaserPowerUp = powerup::ActivePowerUp<LaserPowerUpKind>;

/// Power-up spawn on the arena floor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnedLaserPowerUp {
    pub x: f32,
    pub z: f32,
    pub kind: LaserPowerUpKind,
    pub collected: bool,
    pub respawn_timer: f32,
}

/// Default respawn timer for power-ups.
pub const POWERUP_RESPAWN_TIME: f32 = 15.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_fire_expires() {
        let mut pu = ActiveLaserPowerUp::new(LaserPowerUpKind::RapidFire);
        assert!(!pu.is_expired());
        pu.tick(6.0);
        assert!(pu.is_expired());
    }

    #[test]
    fn shield_persists() {
        let mut pu = ActiveLaserPowerUp::new(LaserPowerUpKind::Shield);
        pu.tick(100.0);
        assert!(!pu.is_expired());
    }
}
