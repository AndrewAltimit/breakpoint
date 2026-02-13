use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Trait for game-specific power-up kind enums.
pub trait PowerUpKind: Clone + Copy + PartialEq + Serialize + DeserializeOwned {
    /// Duration in seconds for this power-up. Use `f32::INFINITY` for permanent effects.
    fn duration(&self) -> f32;
}

/// Active power-up effect on a player, generic over the kind enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct ActivePowerUp<K: PowerUpKind> {
    pub kind: K,
    pub remaining: f32,
}

impl<K: PowerUpKind> ActivePowerUp<K> {
    pub fn new(kind: K) -> Self {
        Self {
            remaining: kind.duration(),
            kind,
        }
    }

    pub fn tick(&mut self, dt: f32) {
        if self.remaining.is_finite() {
            self.remaining -= dt;
        }
    }

    pub fn is_expired(&self) -> bool {
        self.remaining <= 0.0
    }
}
