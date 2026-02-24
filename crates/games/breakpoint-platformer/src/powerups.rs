use rand::Rng;
use serde::{Deserialize, Serialize};

use breakpoint_core::powerup;

/// Castlevania-style power-up types for the platformer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerUpKind {
    /// AOE clear around player (instant effect).
    HolyWater,
    /// Screen-wide clear of all nearby enemies (instant effect).
    Crucifix,
    /// 1.5x movement speed for 5 seconds.
    SpeedBoots,
    /// Grants permanent double-jump until death.
    DoubleJump,
    /// Permanently increases max HP by 1 (and heals that point).
    ArmorUp,
    /// Invincibility for 3 seconds.
    Invincibility,
    /// Extended whip attack range for 10 seconds.
    WhipExtend,
}

impl powerup::PowerUpKind for PowerUpKind {
    fn duration(&self) -> f32 {
        match self {
            PowerUpKind::HolyWater => 0.0,
            PowerUpKind::Crucifix => 0.0,
            PowerUpKind::SpeedBoots => 5.0,
            PowerUpKind::DoubleJump => f32::INFINITY,
            PowerUpKind::ArmorUp => f32::INFINITY,
            PowerUpKind::Invincibility => 3.0,
            PowerUpKind::WhipExtend => 10.0,
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

/// Select a power-up based on the player's relative position (Mario Kart-style rubber banding).
///
/// `quality` ranges from 0.0 (leader) to 1.0 (last place).
/// Leaders get weaker items, trailing players get stronger ones.
pub fn select_powerup_for_position(quality: f32, rng: &mut impl Rng) -> PowerUpKind {
    if quality < 0.3 {
        // Leader tier: moderate items
        let options = [
            PowerUpKind::HolyWater,
            PowerUpKind::DoubleJump,
            PowerUpKind::WhipExtend,
        ];
        options[rng.random_range(0..options.len())]
    } else if quality <= 0.7 {
        // Middle tier: balanced mix
        let options = [
            PowerUpKind::SpeedBoots,
            PowerUpKind::DoubleJump,
            PowerUpKind::HolyWater,
            PowerUpKind::WhipExtend,
        ];
        options[rng.random_range(0..options.len())]
    } else {
        // Last place tier: powerful items
        let options = [
            PowerUpKind::Crucifix,
            PowerUpKind::Invincibility,
            PowerUpKind::SpeedBoots,
            PowerUpKind::ArmorUp,
        ];
        options[rng.random_range(0..options.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn speed_boots_expires() {
        let mut pu = ActivePowerUp::new(PowerUpKind::SpeedBoots);
        assert!(!pu.is_expired());
        pu.tick(6.0);
        assert!(pu.is_expired());
    }

    #[test]
    fn double_jump_does_not_expire() {
        let mut pu = ActivePowerUp::new(PowerUpKind::DoubleJump);
        pu.tick(1000.0);
        assert!(!pu.is_expired(), "DoubleJump should not expire with time");
    }

    #[test]
    fn armor_up_does_not_expire() {
        let mut pu = ActivePowerUp::new(PowerUpKind::ArmorUp);
        pu.tick(1000.0);
        assert!(!pu.is_expired(), "ArmorUp should not expire with time");
    }

    #[test]
    fn invincibility_expires() {
        let mut pu = ActivePowerUp::new(PowerUpKind::Invincibility);
        assert!(!pu.is_expired());
        pu.tick(4.0);
        assert!(pu.is_expired());
    }

    #[test]
    fn whip_extend_expires() {
        let mut pu = ActivePowerUp::new(PowerUpKind::WhipExtend);
        assert!(!pu.is_expired());
        pu.tick(11.0);
        assert!(pu.is_expired());
    }

    #[test]
    fn holy_water_instant() {
        let pu = ActivePowerUp::new(PowerUpKind::HolyWater);
        assert!(pu.is_expired(), "HolyWater should be instant (0s duration)");
    }

    #[test]
    fn crucifix_instant() {
        let pu = ActivePowerUp::new(PowerUpKind::Crucifix);
        assert!(pu.is_expired(), "Crucifix should be instant (0s duration)");
    }

    #[test]
    fn leader_gets_moderate_items() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..50 {
            let kind = select_powerup_for_position(0.1, &mut rng);
            // Leaders should NOT get Crucifix, Invincibility, ArmorUp
            assert!(
                !matches!(
                    kind,
                    PowerUpKind::Crucifix | PowerUpKind::Invincibility | PowerUpKind::ArmorUp
                ),
                "Leader should not get powerful items, got {:?}",
                kind,
            );
        }
    }

    #[test]
    fn last_place_gets_powerful_items() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..50 {
            let kind = select_powerup_for_position(0.9, &mut rng);
            // Last place should NOT get HolyWater, DoubleJump, WhipExtend
            assert!(
                !matches!(
                    kind,
                    PowerUpKind::HolyWater | PowerUpKind::DoubleJump | PowerUpKind::WhipExtend
                ),
                "Last place should not get weak items, got {:?}",
                kind,
            );
        }
    }

    #[test]
    fn middle_tier_selection() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let kind = select_powerup_for_position(0.5, &mut rng);
            seen.insert(format!("{:?}", kind));
        }
        // Middle tier should produce at least 2 different kinds
        assert!(
            seen.len() >= 2,
            "Middle tier should produce variety, got {:?}",
            seen,
        );
    }
}
