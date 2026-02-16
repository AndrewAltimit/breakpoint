use breakpoint_core::game_trait::PlayerId;

use super::{CycleState, Direction, WallSegment};
use crate::config::TronConfig;

/// Result of a collision check.
pub struct CollisionResult {
    /// Whether the cycle is still alive after this check.
    pub alive: bool,
    /// If killed, whose wall caused the death (None = arena boundary or own wall).
    pub killer_id: Option<PlayerId>,
    /// Whether it was a suicide (hit own wall).
    pub is_suicide: bool,
}

/// Check if a cycle collides with arena boundaries.
pub fn check_arena_boundary(cycle: &CycleState, arena_width: f32, arena_depth: f32) -> bool {
    let margin = 0.1;
    cycle.x <= margin
        || cycle.x >= arena_width - margin
        || cycle.z <= margin
        || cycle.z >= arena_depth - margin
}

/// Check if a cycle collides with any wall segment.
/// Returns the CollisionResult with killer info.
pub fn check_wall_collision(
    cycle: &CycleState,
    cycle_owner_id: PlayerId,
    walls: &[WallSegment],
    config: &TronConfig,
) -> CollisionResult {
    let col_dist = config.collision_distance;

    for wall in walls {
        // Skip the active segment of our own trail (the one currently being drawn)
        if wall.owner_id == cycle_owner_id && wall.is_active {
            continue;
        }

        // Skip own segments whose endpoint is at the cycle's position (turn corners).
        // At low speeds the cycle may still be within collision distance of the
        // just-closed segment after a turn.
        if wall.owner_id == cycle_owner_id {
            let ex = cycle.x - wall.x2;
            let ez = cycle.z - wall.z2;
            if (ex * ex + ez * ez).sqrt() < col_dist * 3.0 {
                continue;
            }
        }

        let dist = point_to_segment_distance(cycle.x, cycle.z, wall.x1, wall.z1, wall.x2, wall.z2);

        if dist < col_dist {
            let is_suicide = wall.owner_id == cycle_owner_id;
            let killer_id = if is_suicide {
                None
            } else {
                Some(wall.owner_id)
            };
            return CollisionResult {
                alive: false,
                killer_id,
                is_suicide,
            };
        }
    }

    CollisionResult {
        alive: true,
        killer_id: None,
        is_suicide: false,
    }
}

/// Distance from point (px, pz) to line segment (x1, z1)-(x2, z2).
pub fn point_to_segment_distance(px: f32, pz: f32, x1: f32, z1: f32, x2: f32, z2: f32) -> f32 {
    let dx = x2 - x1;
    let dz = z2 - z1;
    let len_sq = dx * dx + dz * dz;

    if len_sq < 1e-8 {
        // Degenerate segment (point)
        let ddx = px - x1;
        let ddz = pz - z1;
        return (ddx * ddx + ddz * ddz).sqrt();
    }

    // Project point onto segment, clamped to [0, 1]
    let t = ((px - x1) * dx + (pz - z1) * dz) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let nearest_x = x1 + t * dx;
    let nearest_z = z1 + t * dz;

    let ddx = px - nearest_x;
    let ddz = pz - nearest_z;
    (ddx * ddx + ddz * ddz).sqrt()
}

/// Find the minimum distance from a cycle to any parallel wall segment within
/// the grind threshold. Returns the minimum distance, or None if no wall is near.
/// Skips the querying cycle's own active segment to avoid self-grinding.
pub fn nearest_wall_distance(
    cycle: &CycleState,
    cycle_owner_id: PlayerId,
    walls: &[WallSegment],
    arena_width: f32,
    arena_depth: f32,
    threshold: f32,
) -> Option<f32> {
    let mut min_dist = f32::MAX;

    // Check arena boundary walls
    let boundary_dists = [
        cycle.x,               // left wall
        arena_width - cycle.x, // right wall
        cycle.z,               // top wall
        arena_depth - cycle.z, // bottom wall
    ];
    for d in boundary_dists {
        if d < threshold && d < min_dist {
            min_dist = d;
        }
    }

    // Check trail walls (only parallel ones for grinding)
    for wall in walls {
        // Skip own active segment (the one currently being drawn)
        if wall.owner_id == cycle_owner_id && wall.is_active {
            continue;
        }

        let is_parallel = match cycle.direction {
            Direction::North | Direction::South => {
                // Cycle moving vertically, check vertical walls (same x)
                (wall.x1 - wall.x2).abs() < 0.1
            },
            Direction::East | Direction::West => {
                // Cycle moving horizontally, check horizontal walls (same z)
                (wall.z1 - wall.z2).abs() < 0.1
            },
        };

        if !is_parallel {
            continue;
        }

        let dist = point_to_segment_distance(cycle.x, cycle.z, wall.x1, wall.z1, wall.x2, wall.z2);
        if dist < threshold && dist < min_dist {
            min_dist = dist;
        }
    }

    if min_dist < f32::MAX {
        Some(min_dist)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_to_segment_horizontal() {
        // Horizontal segment from (0,0) to (10,0), point at (5, 3)
        let d = point_to_segment_distance(5.0, 3.0, 0.0, 0.0, 10.0, 0.0);
        assert!((d - 3.0).abs() < 0.01);
    }

    #[test]
    fn point_to_segment_endpoint() {
        // Point nearest to endpoint
        let d = point_to_segment_distance(12.0, 0.0, 0.0, 0.0, 10.0, 0.0);
        assert!((d - 2.0).abs() < 0.01);
    }

    #[test]
    fn point_to_segment_degenerate() {
        let d = point_to_segment_distance(3.0, 4.0, 0.0, 0.0, 0.0, 0.0);
        assert!((d - 5.0).abs() < 0.01);
    }

    #[test]
    fn arena_boundary_detection() {
        let cycle = CycleState {
            x: 0.05,
            z: 250.0,
            direction: Direction::West,
            speed: 20.0,
            rubber: 0.5,
            brake_fuel: 3.0,
            alive: true,
            trail_start_index: 0,
            turn_cooldown: 0.0,
            kills: 0,
            died: false,
            is_suicide: false,
        };
        assert!(check_arena_boundary(&cycle, 500.0, 500.0));
    }
}
