use std::collections::HashMap;

use breakpoint_core::game_trait::PlayerId;
use serde::{Deserialize, Serialize};

use crate::physics::PlatformerPlayerState;

/// Rubber-banding factors applied per-player to keep the race competitive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubberBandFactor {
    /// Multiplier for enemy density near this player (higher = more enemies).
    pub enemy_density_mult: f32,
    /// Quality tier for power-up selection (0.0 = leader/weak items, 1.0 = last/strong items).
    pub powerup_quality: f32,
}

impl Default for RubberBandFactor {
    fn default() -> Self {
        Self {
            enemy_density_mult: 1.0,
            powerup_quality: 0.5,
        }
    }
}

/// Compute rubber-banding factors for all players based on their relative positions.
///
/// Players are ranked by x position (furthest ahead = rank 0 = leader).
/// Dead or eliminated players are ignored. If only 1 active player exists, defaults
/// are returned for everyone.
pub fn compute_rubber_band(
    players: &HashMap<PlayerId, PlatformerPlayerState>,
) -> HashMap<PlayerId, RubberBandFactor> {
    let mut result = HashMap::new();

    // Collect active players and their x positions
    let mut active: Vec<(PlayerId, f32)> = players
        .iter()
        .filter(|(_, p)| !p.eliminated && p.death_respawn_timer <= 0.0)
        .map(|(&id, p)| (id, p.x))
        .collect();

    // If 0 or 1 active player, return defaults for all
    if active.len() <= 1 {
        for &id in players.keys() {
            result.insert(id, RubberBandFactor::default());
        }
        return result;
    }

    // Sort by x position descending (leader first)
    active.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let n = active.len();
    for (rank, &(pid, _)) in active.iter().enumerate() {
        // Normalize rank to [0, 1] where 0 = leader, 1 = last
        let t = if n > 1 {
            rank as f32 / (n - 1) as f32
        } else {
            0.5
        };

        // Leader: enemy_density_mult = 1.5, powerup_quality = 0.0
        // Last:   enemy_density_mult = 0.7, powerup_quality = 1.0
        let enemy_density_mult = 1.5 + t * (0.7 - 1.5); // lerp from 1.5 to 0.7
        let powerup_quality = t; // 0.0 for leader, 1.0 for last

        result.insert(
            pid,
            RubberBandFactor {
                enemy_density_mult,
                powerup_quality,
            },
        );
    }

    // Assign defaults to inactive players (dead/eliminated)
    for &id in players.keys() {
        result.entry(id).or_insert_with(RubberBandFactor::default);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::PlatformerPlayerState;

    fn make_player_at_x(x: f32) -> PlatformerPlayerState {
        let mut p = PlatformerPlayerState::new(x, 5.0);
        p.eliminated = false;
        p
    }

    #[test]
    fn single_player_gets_defaults() {
        let mut players = HashMap::new();
        players.insert(1, make_player_at_x(10.0));

        let factors = compute_rubber_band(&players);

        let f = &factors[&1];
        assert!(
            (f.enemy_density_mult - 1.0).abs() < 0.01,
            "Single player should get default density"
        );
        assert!(
            (f.powerup_quality - 0.5).abs() < 0.01,
            "Single player should get default quality"
        );
    }

    #[test]
    fn leader_gets_harder_enemies_weaker_items() {
        let mut players = HashMap::new();
        players.insert(1, make_player_at_x(50.0)); // leader
        players.insert(2, make_player_at_x(10.0)); // last

        let factors = compute_rubber_band(&players);

        let leader = &factors[&1];
        let last = &factors[&2];

        assert!(
            leader.enemy_density_mult > last.enemy_density_mult,
            "Leader should face more enemies: {} vs {}",
            leader.enemy_density_mult,
            last.enemy_density_mult,
        );
        assert!(
            leader.powerup_quality < last.powerup_quality,
            "Leader should get weaker items: {} vs {}",
            leader.powerup_quality,
            last.powerup_quality,
        );
    }

    #[test]
    fn three_players_interpolation() {
        let mut players = HashMap::new();
        players.insert(1, make_player_at_x(50.0)); // leader
        players.insert(2, make_player_at_x(30.0)); // middle
        players.insert(3, make_player_at_x(10.0)); // last

        let factors = compute_rubber_band(&players);

        let leader = &factors[&1];
        let middle = &factors[&2];
        let last = &factors[&3];

        // Enemy density: leader > middle > last
        assert!(leader.enemy_density_mult > middle.enemy_density_mult);
        assert!(middle.enemy_density_mult > last.enemy_density_mult);

        // Powerup quality: leader < middle < last
        assert!(leader.powerup_quality < middle.powerup_quality);
        assert!(middle.powerup_quality < last.powerup_quality);
    }

    #[test]
    fn eliminated_players_ignored_in_ranking() {
        let mut players = HashMap::new();
        players.insert(1, make_player_at_x(50.0)); // leader
        let mut elim = make_player_at_x(100.0); // would be "leader" but eliminated
        elim.eliminated = true;
        players.insert(2, elim);
        players.insert(3, make_player_at_x(10.0)); // last

        let factors = compute_rubber_band(&players);

        // Player 1 is leader among active players
        assert!(
            factors[&1].powerup_quality < factors[&3].powerup_quality,
            "Active leader should have lower powerup quality than active last"
        );

        // Eliminated player gets default
        assert!(
            (factors[&2].enemy_density_mult - 1.0).abs() < 0.01,
            "Eliminated player should get default density"
        );
    }

    #[test]
    fn dead_players_ignored_in_ranking() {
        let mut players = HashMap::new();
        players.insert(1, make_player_at_x(50.0));
        let mut dead = make_player_at_x(100.0);
        dead.death_respawn_timer = 1.5; // currently dead, awaiting respawn
        players.insert(2, dead);
        players.insert(3, make_player_at_x(10.0));

        let factors = compute_rubber_band(&players);

        // Dead player gets default
        assert!(
            (factors[&2].enemy_density_mult - 1.0).abs() < 0.01,
            "Dead player should get default density"
        );
    }

    #[test]
    fn empty_players_returns_empty() {
        let players: HashMap<PlayerId, PlatformerPlayerState> = HashMap::new();
        let factors = compute_rubber_band(&players);
        assert!(factors.is_empty());
    }

    #[test]
    fn leader_density_is_1_5() {
        let mut players = HashMap::new();
        players.insert(1, make_player_at_x(50.0));
        players.insert(2, make_player_at_x(10.0));

        let factors = compute_rubber_band(&players);
        assert!(
            (factors[&1].enemy_density_mult - 1.5).abs() < 0.01,
            "Leader density should be 1.5, got {}",
            factors[&1].enemy_density_mult,
        );
    }

    #[test]
    fn last_density_is_0_7() {
        let mut players = HashMap::new();
        players.insert(1, make_player_at_x(50.0));
        players.insert(2, make_player_at_x(10.0));

        let factors = compute_rubber_band(&players);
        assert!(
            (factors[&2].enemy_density_mult - 0.7).abs() < 0.01,
            "Last place density should be 0.7, got {}",
            factors[&2].enemy_density_mult,
        );
    }
}
