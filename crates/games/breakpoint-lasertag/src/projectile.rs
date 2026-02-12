use crate::arena::{ArenaWall, WallType};

/// Laser travel speed in units/second.
pub const LASER_SPEED: f32 = 40.0;
/// Stun duration in seconds.
pub const STUN_DURATION: f32 = 1.5;
/// Fire cooldown in seconds.
pub const FIRE_COOLDOWN: f32 = 0.4;
/// Maximum bounces off reflective walls.
pub const MAX_BOUNCES: u8 = 2;
/// Player collision radius.
pub const PLAYER_RADIUS: f32 = 0.6;

/// Result of a laser raycast.
#[derive(Debug, Clone)]
pub struct LaserHitResult {
    /// Path segments the laser traveled (start/end pairs).
    pub segments: Vec<(f32, f32, f32, f32)>,
    /// Player ID that was hit, if any.
    pub hit_player: Option<u64>,
    /// Total distance traveled.
    pub total_distance: f32,
}

/// Perform a laser raycast from origin in aim_direction, checking walls and players.
///
/// `players` is a list of (id, x, z) for potential hit targets.
/// `shooter_id` is excluded from hit detection.
/// `team_ids` contains IDs on the same team as the shooter (excluded from hits).
#[allow(clippy::too_many_arguments)]
pub fn raycast_laser(
    origin_x: f32,
    origin_z: f32,
    aim_angle: f32,
    walls: &[ArenaWall],
    players: &[(u64, f32, f32)],
    shooter_id: u64,
    team_ids: &[u64],
    max_distance: f32,
) -> LaserHitResult {
    let mut segments = Vec::new();
    let mut cx = origin_x;
    let mut cz = origin_z;
    let mut dx = aim_angle.cos();
    let mut dz = aim_angle.sin();
    let mut remaining_distance = max_distance;
    let mut bounces = 0u8;
    let mut hit_player = None;
    let mut total_distance = 0.0;

    loop {
        // Find nearest wall intersection
        let mut nearest_wall_t = remaining_distance;
        let mut nearest_wall_idx: Option<usize> = None;
        let mut nearest_wall_normal = (0.0f32, 0.0f32);

        for (i, wall) in walls.iter().enumerate() {
            if let Some((t, nx, nz)) =
                ray_segment_intersection(cx, cz, dx, dz, wall.ax, wall.az, wall.bx, wall.bz)
                && t > 0.01
                && t < nearest_wall_t
            {
                nearest_wall_t = t;
                nearest_wall_idx = Some(i);
                nearest_wall_normal = (nx, nz);
            }
        }

        // Check player hits along this ray segment
        let segment_len = nearest_wall_t;
        if let Some((hit_t, pid)) =
            check_player_hits(cx, cz, dx, dz, segment_len, players, shooter_id, team_ids)
        {
            let end_x = cx + dx * hit_t;
            let end_z = cz + dz * hit_t;
            segments.push((cx, cz, end_x, end_z));
            total_distance += hit_t;
            hit_player = Some(pid);
            break;
        }

        // Move to wall intersection
        let end_x = cx + dx * nearest_wall_t;
        let end_z = cz + dz * nearest_wall_t;
        segments.push((cx, cz, end_x, end_z));
        total_distance += nearest_wall_t;
        remaining_distance -= nearest_wall_t;

        if remaining_distance <= 0.1 {
            break;
        }

        // Check if we hit a reflective wall and can bounce
        if let Some(wall_idx) = nearest_wall_idx
            && walls[wall_idx].wall_type == WallType::Reflective
            && bounces < MAX_BOUNCES
        {
            // Reflect direction
            let (nx, nz) = nearest_wall_normal;
            let dot = dx * nx + dz * nz;
            dx -= 2.0 * dot * nx;
            dz -= 2.0 * dot * nz;
            cx = end_x + dx * 0.01;
            cz = end_z + dz * 0.01;
            bounces += 1;
        } else {
            break;
        }
    }

    LaserHitResult {
        segments,
        hit_player,
        total_distance,
    }
}

/// Ray-segment intersection. Returns (t, normal_x, normal_z) if hit.
#[allow(clippy::too_many_arguments)]
fn ray_segment_intersection(
    ox: f32,
    oz: f32,
    dx: f32,
    dz: f32,
    ax: f32,
    az: f32,
    bx: f32,
    bz: f32,
) -> Option<(f32, f32, f32)> {
    let sx = bx - ax;
    let sz = bz - az;

    let denom = dx * sz - dz * sx;
    if denom.abs() < 1e-8 {
        return None; // parallel
    }

    let t = ((ax - ox) * sz - (az - oz) * sx) / denom;
    let u = ((ax - ox) * dz - (az - oz) * dx) / denom;

    if t > 0.0 && (0.0..=1.0).contains(&u) {
        // Normal: perpendicular to segment
        let len = (sx * sx + sz * sz).sqrt();
        if len < 1e-6 {
            return None;
        }
        let nx = -sz / len;
        let nz = sx / len;
        // Ensure normal faces the ray origin
        if nx * dx + nz * dz > 0.0 {
            Some((t, -nx, -nz))
        } else {
            Some((t, nx, nz))
        }
    } else {
        None
    }
}

/// Check for player hits along a ray segment. Returns (t, player_id) for nearest hit.
#[allow(clippy::too_many_arguments)]
fn check_player_hits(
    ox: f32,
    oz: f32,
    dx: f32,
    dz: f32,
    max_t: f32,
    players: &[(u64, f32, f32)],
    shooter_id: u64,
    team_ids: &[u64],
) -> Option<(f32, u64)> {
    let mut nearest: Option<(f32, u64)> = None;

    for &(pid, px, pz) in players {
        if pid == shooter_id || team_ids.contains(&pid) {
            continue;
        }

        // Line-circle intersection
        if let Some(t) = ray_circle_intersection(ox, oz, dx, dz, px, pz, PLAYER_RADIUS)
            && t > 0.01
            && t < max_t
            && (nearest.is_none() || t < nearest.unwrap().0)
        {
            nearest = Some((t, pid));
        }
    }

    nearest
}

/// Ray-circle intersection (2D). Returns nearest t if hit.
fn ray_circle_intersection(
    ox: f32,
    oz: f32,
    dx: f32,
    dz: f32,
    cx: f32,
    cz: f32,
    radius: f32,
) -> Option<f32> {
    let fx = ox - cx;
    let fz = oz - cz;
    let a = dx * dx + dz * dz;
    let b = 2.0 * (fx * dx + fz * dz);
    let c = fx * fx + fz * fz - radius * radius;
    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        return None;
    }

    let sqrt_d = discriminant.sqrt();
    let t1 = (-b - sqrt_d) / (2.0 * a);
    let t2 = (-b + sqrt_d) / (2.0 * a);

    if t1 > 0.0 {
        Some(t1)
    } else if t2 > 0.0 {
        Some(t2)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::{ArenaWall, WallType};

    #[test]
    fn laser_travels_straight() {
        let walls = vec![ArenaWall {
            ax: 100.0,
            az: -10.0,
            bx: 100.0,
            bz: 10.0,
            wall_type: WallType::Solid,
        }];
        let result = raycast_laser(0.0, 0.0, 0.0, &walls, &[], 0, &[], 200.0);
        assert_eq!(result.segments.len(), 1);
        assert!(result.hit_player.is_none());
    }

    #[test]
    fn laser_reflects_off_reflective_wall() {
        let walls = vec![
            ArenaWall {
                ax: 10.0,
                az: -20.0,
                bx: 10.0,
                bz: 20.0,
                wall_type: WallType::Reflective,
            },
            ArenaWall {
                ax: -20.0,
                az: -20.0,
                bx: -20.0,
                bz: 20.0,
                wall_type: WallType::Solid,
            },
        ];
        let result = raycast_laser(0.0, 0.0, 0.0, &walls, &[], 0, &[], 200.0);
        assert!(
            result.segments.len() >= 2,
            "Should have at least 2 segments after reflection"
        );
    }

    #[test]
    fn laser_hits_player() {
        let walls = vec![];
        let players = vec![(2, 5.0, 0.0)];
        let result = raycast_laser(0.0, 0.0, 0.0, &walls, &players, 1, &[], 200.0);
        assert_eq!(result.hit_player, Some(2));
    }

    #[test]
    fn laser_does_not_hit_shooter() {
        let walls = vec![];
        let players = vec![(1, 5.0, 0.0)];
        let result = raycast_laser(0.0, 0.0, 0.0, &walls, &players, 1, &[], 200.0);
        assert!(result.hit_player.is_none(), "Should not hit self");
    }

    #[test]
    fn laser_does_not_hit_teammate() {
        let walls = vec![];
        let players = vec![(2, 5.0, 0.0)];
        let result = raycast_laser(0.0, 0.0, 0.0, &walls, &players, 1, &[2], 200.0);
        assert!(result.hit_player.is_none(), "Should not hit teammate");
    }

    #[test]
    fn max_bounces_respected() {
        // Create a corridor of reflective walls that would bounce many times
        let walls = vec![
            ArenaWall {
                ax: 5.0,
                az: -20.0,
                bx: 5.0,
                bz: 20.0,
                wall_type: WallType::Reflective,
            },
            ArenaWall {
                ax: -5.0,
                az: -20.0,
                bx: -5.0,
                bz: 20.0,
                wall_type: WallType::Reflective,
            },
        ];
        let result = raycast_laser(0.0, 0.0, 0.1, &walls, &[], 0, &[], 500.0);
        // Should stop after MAX_BOUNCES + 1 segments
        assert!(result.segments.len() <= (MAX_BOUNCES as usize + 1));
    }
}
