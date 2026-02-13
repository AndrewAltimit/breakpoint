use serde::{Deserialize, Serialize};

use breakpoint_core::powerup;

/// Platformer power-up types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerUpKind {
    SpeedBoost,
    DoubleJump,
    Shield,
    Magnet,
}

impl powerup::PowerUpKind for PowerUpKind {
    fn duration(&self) -> f32 {
        match self {
            PowerUpKind::SpeedBoost => 3.0,
            PowerUpKind::DoubleJump => f32::INFINITY,
            PowerUpKind::Shield => f32::INFINITY,
            PowerUpKind::Magnet => 3.0,
        }
    }
}

/// Active power-up effect on a player.
pub type ActivePowerUp = powerup::ActivePowerUp<PowerUpKind>;

/// Spawned power-up on the course.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnedPowerUp {
    pub x: f32,
    pub y: f32,
    pub kind: PowerUpKind,
    pub collected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_boost_expires() {
        let mut pu = ActivePowerUp::new(PowerUpKind::SpeedBoost);
        assert!(!pu.is_expired());
        pu.tick(4.0);
        assert!(pu.is_expired());
    }

    #[test]
    fn shield_does_not_expire() {
        let mut pu = ActivePowerUp::new(PowerUpKind::Shield);
        pu.tick(100.0);
        assert!(!pu.is_expired(), "Shield should not expire with time");
    }

    #[test]
    fn double_jump_persists() {
        let mut pu = ActivePowerUp::new(PowerUpKind::DoubleJump);
        pu.tick(1000.0);
        assert!(!pu.is_expired());
    }
}
