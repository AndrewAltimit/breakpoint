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
