use serde::{Deserialize, Serialize};

/// A spawn position with starting direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub x: f32,
    pub z: f32,
    pub direction: super::Direction,
}

/// Arena definition.
#[derive(Debug, Clone)]
pub struct Arena {
    pub width: f32,
    pub depth: f32,
    pub spawn_points: Vec<SpawnPoint>,
}

/// Generate spawn positions for N players evenly distributed around the arena perimeter,
/// facing inward.
pub fn create_arena(width: f32, depth: f32, player_count: usize) -> Arena {
    let mut spawn_points = Vec::with_capacity(player_count);
    let margin = 20.0;
    let cx = width / 2.0;
    let cz = depth / 2.0;

    for i in 0..player_count {
        let angle = std::f32::consts::TAU * (i as f32) / (player_count.max(1) as f32);

        // Place on a circle inside the arena
        let radius = (width.min(depth) / 2.0) - margin;
        let x = cx + radius * angle.cos();
        let z = cz + radius * angle.sin();

        // Face inward (toward center)
        let dx = cx - x;
        let dz = cz - z;
        let direction = if dx.abs() > dz.abs() {
            if dx > 0.0 {
                super::Direction::East
            } else {
                super::Direction::West
            }
        } else if dz > 0.0 {
            super::Direction::South
        } else {
            super::Direction::North
        };

        spawn_points.push(SpawnPoint { x, z, direction });
    }

    Arena {
        width,
        depth,
        spawn_points,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_arena_two_players() {
        let arena = create_arena(500.0, 500.0, 2);
        assert_eq!(arena.spawn_points.len(), 2, "Should have 2 spawn points");
        assert!((arena.width - 500.0).abs() < f32::EPSILON);
        assert!((arena.depth - 500.0).abs() < f32::EPSILON);
    }

    #[test]
    fn create_arena_eight_players() {
        let arena = create_arena(500.0, 500.0, 8);
        assert_eq!(arena.spawn_points.len(), 8, "Should have 8 spawn points");

        // All positions should be unique
        for i in 0..arena.spawn_points.len() {
            for j in (i + 1)..arena.spawn_points.len() {
                let a = &arena.spawn_points[i];
                let b = &arena.spawn_points[j];
                let dist = ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt();
                assert!(
                    dist > 1.0,
                    "Spawn points {i} and {j} are too close: distance = {dist}"
                );
            }
        }
    }

    #[test]
    fn create_arena_single_player() {
        let arena = create_arena(500.0, 500.0, 1);
        assert_eq!(arena.spawn_points.len(), 1, "Should have 1 spawn point");
        // Should not panic with a single player
    }

    #[test]
    fn spawn_points_within_arena_bounds() {
        for count in [1, 2, 4, 6, 8] {
            let arena = create_arena(500.0, 500.0, count);
            for (i, sp) in arena.spawn_points.iter().enumerate() {
                assert!(
                    sp.x >= 0.0 && sp.x <= arena.width,
                    "Spawn {i} x={} out of bounds [0, {}] (player_count={count})",
                    sp.x,
                    arena.width
                );
                assert!(
                    sp.z >= 0.0 && sp.z <= arena.depth,
                    "Spawn {i} z={} out of bounds [0, {}] (player_count={count})",
                    sp.z,
                    arena.depth
                );
            }
        }
    }

    #[test]
    fn spawn_points_face_inward() {
        let arena = create_arena(500.0, 500.0, 8);
        let cx = arena.width / 2.0;
        let cz = arena.depth / 2.0;

        for (i, sp) in arena.spawn_points.iter().enumerate() {
            // Vector from spawn point to center
            let to_center_x = cx - sp.x;
            let to_center_z = cz - sp.z;

            // Direction vector for the spawn direction
            let (dir_x, dir_z) = match sp.direction {
                super::super::Direction::North => (0.0_f32, -1.0_f32),
                super::super::Direction::South => (0.0, 1.0),
                super::super::Direction::East => (1.0, 0.0),
                super::super::Direction::West => (-1.0, 0.0),
            };

            // Dot product should be positive (facing toward center)
            let dot = dir_x * to_center_x + dir_z * to_center_z;
            assert!(
                dot > 0.0,
                "Spawn point {i} at ({}, {}) facing {:?} does not face center ({cx}, {cz}), \
                 dot product = {dot}",
                sp.x,
                sp.z,
                sp.direction
            );
        }
    }
}
