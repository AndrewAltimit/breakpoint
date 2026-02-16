use crate::arena::{ArenaWall, WallType};
use serde::{Deserialize, Serialize};

/// Laser travel speed in units/second.
pub const LASER_SPEED: f32 = 40.0;
/// Stun duration in seconds.
pub const STUN_DURATION: f32 = 1.5;
/// Fire cooldown in seconds.
pub const FIRE_COOLDOWN: f32 = 0.4;
/// Cooldown multiplier when RapidFire power-up is active.
pub const RAPIDFIRE_COOLDOWN_MULT: f32 = 0.4;
/// Maximum bounces off reflective walls.
pub const MAX_BOUNCES: u8 = 2;
/// Player collision radius.
pub const PLAYER_RADIUS: f32 = 0.6;

/// Configurable laser tag physics parameters, loadable from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserTagPhysicsConfig {
    pub laser_speed: f32,
    pub stun_duration: f32,
    pub fire_cooldown: f32,
    pub rapidfire_cooldown_mult: f32,
    pub max_bounces: u8,
    pub player_radius: f32,
    pub move_speed: f32,
    pub powerup_respawn_time: f32,
}

impl Default for LaserTagPhysicsConfig {
    fn default() -> Self {
        Self {
            laser_speed: LASER_SPEED,
            stun_duration: STUN_DURATION,
            fire_cooldown: FIRE_COOLDOWN,
            rapidfire_cooldown_mult: RAPIDFIRE_COOLDOWN_MULT,
            max_bounces: MAX_BOUNCES,
            player_radius: PLAYER_RADIUS,
            move_speed: 8.0,
            powerup_respawn_time: 15.0,
        }
    }
}

/// Top-level laser tag game configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserTagConfig {
    pub physics: LaserTagPhysicsConfig,
    pub round_duration_secs: f32,
    pub tick_rate_hz: f32,
}

impl Default for LaserTagConfig {
    fn default() -> Self {
        Self {
            physics: LaserTagPhysicsConfig::default(),
            round_duration_secs: 180.0,
            tick_rate_hz: 20.0,
        }
    }
}

impl LaserTagConfig {
    /// Load config from a TOML file. Falls back to defaults if the file is missing
    /// or unparseable.
    pub fn load() -> Self {
        let path = std::env::var("BREAKPOINT_LASERTAG_CONFIG")
            .unwrap_or_else(|_| "config/lasertag.toml".to_string());
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<LaserTagConfig>(&content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::warn!("Failed to parse {path}: {e}, using defaults");
                    LaserTagConfig::default()
                },
            },
            Err(_) => LaserTagConfig::default(),
        }
    }
}

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
pub(crate) fn ray_segment_intersection(
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
pub(crate) fn check_player_hits(
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
pub(crate) fn ray_circle_intersection(
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

    // ================================================================
    // Phase 2b: Ray-segment intersection tests
    // ================================================================

    #[test]
    fn ray_segment_near_parallel_returns_none() {
        // Ray nearly parallel to segment — denom close to 0
        let result = ray_segment_intersection(
            0.0, 0.0, // origin
            1.0, 0.0, // direction: +X
            0.0, 5.0, // segment start
            10.0, 5.0, // segment end (horizontal, parallel to ray)
        );
        assert!(result.is_none(), "Parallel ray-segment should return None");
    }

    #[test]
    fn ray_segment_hits_at_endpoint_start() {
        // Ray aimed at the start of the segment (u ≈ 0)
        let result = ray_segment_intersection(
            0.0, 0.0, // origin
            0.0, 1.0, // direction: +Z
            -5.0, 5.0, // segment start
            5.0, 5.0, // segment end
        );
        assert!(result.is_some(), "Ray should hit segment at u=0.5");
        let (t, _, _) = result.unwrap();
        assert!((t - 5.0).abs() < 0.1, "t should be ~5.0, got {t}");
    }

    #[test]
    fn ray_segment_misses_past_endpoint() {
        // Ray aimed past the segment endpoint (u > 1)
        let result = ray_segment_intersection(
            0.0, 0.0, // origin
            0.0, 1.0, // direction: +Z
            5.0, 5.0, // segment from (5,5) to (10,5)
            10.0, 5.0,
        );
        assert!(
            result.is_none(),
            "Ray missing segment past endpoint should return None"
        );
    }

    #[test]
    fn ray_segment_degenerate_zero_length() {
        // Zero-length segment
        let result = ray_segment_intersection(
            0.0, 0.0, // origin
            1.0, 0.0, // direction
            5.0, 5.0, // degenerate segment (same start/end)
            5.0, 5.0,
        );
        assert!(result.is_none(), "Zero-length segment should return None");
    }

    #[test]
    fn ray_segment_normal_faces_ray_origin() {
        // Ray going +X, hits a vertical segment
        let result = ray_segment_intersection(
            0.0, 0.0, // origin
            1.0, 0.0, // direction: +X
            5.0, -5.0, // vertical segment at x=5
            5.0, 5.0,
        );
        assert!(result.is_some());
        let (_, nx, nz) = result.unwrap();
        // Normal should face back toward origin (negative X direction)
        let dot = nx * 1.0 + nz * 0.0;
        assert!(
            dot < 0.0,
            "Normal should face ray origin: dot(normal, ray_dir) = {dot}"
        );
    }

    #[test]
    fn ray_segment_perpendicular_hit() {
        // Simple perpendicular hit: ray +X hitting vertical segment at x=10
        let result = ray_segment_intersection(
            0.0, 0.0, // origin
            1.0, 0.0, // direction: +X
            10.0, -5.0, // vertical segment at x=10
            10.0, 5.0,
        );
        assert!(result.is_some());
        let (t, nx, _nz) = result.unwrap();
        assert!(
            (t - 10.0).abs() < 0.1,
            "t should be ~10.0 for perpendicular hit, got {t}"
        );
        assert!(
            nx < 0.0,
            "Normal x should face -X (toward origin), got {nx}"
        );
    }

    // ================================================================
    // Phase 2c: Ray-circle intersection tests
    // ================================================================

    #[test]
    fn ray_circle_direct_center_hit() {
        // Ray aimed directly at circle center
        let result = ray_circle_intersection(
            0.0, 0.0, // origin
            1.0, 0.0, // direction: +X
            10.0, 0.0, // circle center
            1.0, // radius
        );
        assert!(result.is_some(), "Direct center hit should return Some");
        let t = result.unwrap();
        // Should hit at distance - radius = 10 - 1 = 9
        assert!(
            (t - 9.0).abs() < 0.1,
            "t should be ~9.0 (distance minus radius), got {t}"
        );
    }

    #[test]
    fn ray_circle_tangent_near_miss() {
        // Ray that just barely misses the circle (passes tangentially outside)
        let result = ray_circle_intersection(
            0.0, 0.0, // origin
            1.0, 0.0, // direction: +X
            10.0, 1.1, // circle center offset just beyond radius
            1.0, // radius
        );
        assert!(result.is_none(), "Tangent near-miss should return None");
    }

    #[test]
    fn ray_circle_clear_miss() {
        // Ray parallel to circle edge, far away
        let result = ray_circle_intersection(
            0.0, 0.0, // origin
            1.0, 0.0, // direction: +X
            10.0, 10.0, // circle far away in Z
            1.0,
        );
        assert!(result.is_none(), "Clear miss should return None");
    }

    #[test]
    fn ray_circle_starts_inside() {
        // Origin inside circle — should return exit point (t2)
        let result = ray_circle_intersection(
            10.0, 0.0, // origin inside circle centered at (10,0)
            1.0, 0.0, // direction: +X
            10.0, 0.0, // circle center
            5.0, // radius
        );
        assert!(
            result.is_some(),
            "Ray starting inside should return exit point"
        );
        let t = result.unwrap();
        assert!(t > 0.0, "t should be positive for exit point, got {t}");
    }

    #[test]
    fn ray_circle_moving_away() {
        // Ray moving away from circle (behind origin)
        let result = ray_circle_intersection(
            0.0, 0.0, // origin
            -1.0, 0.0, // direction: -X (away from circle)
            10.0, 0.0, // circle center at +X
            1.0,
        );
        assert!(
            result.is_none(),
            "Ray moving away from circle should return None"
        );
    }

    #[test]
    fn ray_circle_glancing_hit() {
        // Ray that barely intersects the circle
        let result = ray_circle_intersection(
            0.0, 0.0, // origin
            1.0, 0.0, // direction: +X
            10.0, 0.95, // circle center offset just within radius
            1.0,
        );
        assert!(result.is_some(), "Glancing hit should return Some");
    }

    // ================================================================
    // Phase 2d: Multi-bounce & hit ordering tests
    // ================================================================

    #[test]
    fn laser_reflect_then_hit_player() {
        // Player behind a reflective wall, reachable by bouncing off it
        let walls = vec![ArenaWall {
            ax: 10.0,
            az: -20.0,
            bx: 10.0,
            bz: 20.0,
            wall_type: WallType::Reflective,
        }];
        // Player at (-5, 0) — behind the shooter, reachable via reflection
        let players = vec![(2, -5.0, 0.0)];
        // Shoot +X, reflect off wall at x=10, then laser goes -X toward player
        let result = raycast_laser(0.0, 0.0, 0.0, &walls, &players, 1, &[], 200.0);
        assert_eq!(
            result.hit_player,
            Some(2),
            "Player should be hit via reflection"
        );
        assert!(
            result.segments.len() >= 2,
            "Should have at least 2 segments (before and after reflection)"
        );
    }

    #[test]
    fn laser_double_bounce_hit() {
        // Two reflective walls forming a corridor for double bounce
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
        // Shoot at slight angle → bounce off right wall → bounce off left wall → continue
        let result = raycast_laser(0.0, 0.0, 0.1, &walls, &[], 0, &[], 200.0);
        assert!(
            result.segments.len() == 3,
            "Should have 3 segments for double bounce, got {}",
            result.segments.len()
        );
    }

    #[test]
    fn laser_hits_nearest_of_two_players() {
        let walls = vec![];
        // Two players in line along +X, nearest should be hit
        let players = vec![(2, 5.0, 0.0), (3, 10.0, 0.0)];
        let result = raycast_laser(0.0, 0.0, 0.0, &walls, &players, 1, &[], 200.0);
        assert_eq!(
            result.hit_player,
            Some(2),
            "Nearest player (id=2 at x=5) should be hit, not id=3 at x=10"
        );
    }

    #[test]
    fn laser_grazing_angle_reflection() {
        // Near-parallel ray to reflective wall
        let walls = vec![ArenaWall {
            ax: 10.0,
            az: -20.0,
            bx: 10.0,
            bz: 20.0,
            wall_type: WallType::Reflective,
        }];
        // Very shallow angle (nearly parallel)
        let result = raycast_laser(0.0, 0.0, 0.05, &walls, &[], 0, &[], 500.0);
        // Should still reflect (2 segments) or travel past if too shallow to hit
        assert!(
            !result.segments.is_empty(),
            "Grazing angle should produce at least 1 segment"
        );
    }

    #[test]
    fn laser_solid_wall_stops_no_bounce() {
        // Solid wall should block the laser, not reflect it
        let walls = vec![ArenaWall {
            ax: 10.0,
            az: -20.0,
            bx: 10.0,
            bz: 20.0,
            wall_type: WallType::Solid,
        }];
        let result = raycast_laser(0.0, 0.0, 0.0, &walls, &[], 0, &[], 200.0);
        assert_eq!(
            result.segments.len(),
            1,
            "Solid wall should stop laser (1 segment only), got {}",
            result.segments.len()
        );
    }

    // ================================================================
    // Phase 4c: Property-based tests (proptest)
    // ================================================================

    mod proptests {
        use super::*;
        use crate::arena::{ArenaSize, generate_arena};
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn raycast_distance_never_exceeds_max(
                aim_angle in -std::f32::consts::PI..std::f32::consts::PI
            ) {
                let arena = generate_arena(ArenaSize::Default);
                let max_dist = 100.0;
                let result = raycast_laser(
                    25.0, 25.0, aim_angle, &arena.walls, &[], 0, &[], max_dist,
                );
                prop_assert!(
                    result.total_distance <= max_dist + 1.0,
                    "Total distance ({}) should not exceed max ({})",
                    result.total_distance,
                    max_dist
                );
            }

            #[test]
            fn raycast_segments_form_continuous_path(
                aim_angle in -std::f32::consts::PI..std::f32::consts::PI
            ) {
                let arena = generate_arena(ArenaSize::Default);
                let result = raycast_laser(
                    25.0, 25.0, aim_angle, &arena.walls, &[], 0, &[], 100.0,
                );
                for i in 1..result.segments.len() {
                    let (_, _, prev_ex, prev_ez) = result.segments[i - 1];
                    let (cur_sx, cur_sz, _, _) = result.segments[i];
                    let gap = ((prev_ex - cur_sx).powi(2) + (prev_ez - cur_sz).powi(2)).sqrt();
                    prop_assert!(
                        gap < 1.0,
                        "Gap between segment {} end and segment {} start: {gap}",
                        i - 1,
                        i
                    );
                }
            }

            #[test]
            fn ray_aimed_at_center_always_hits(
                distance in 5.0f32..50.0,
                radius in 0.5f32..3.0
            ) {
                // Ray from origin aimed at circle center should always hit
                let result = ray_circle_intersection(
                    0.0, 0.0,
                    1.0, 0.0,
                    distance, 0.0,
                    radius,
                );
                prop_assert!(
                    result.is_some(),
                    "Ray aimed at center should always hit: distance={distance}, radius={radius}"
                );
                let t = result.unwrap();
                prop_assert!(
                    (t - (distance - radius)).abs() < 0.1,
                    "t ({t}) should be ~distance-radius ({})",
                    distance - radius
                );
            }

            // P2-1: All aim angles produce finite laser segments
            #[test]
            fn all_angles_produce_finite_segments(
                angle in -std::f32::consts::PI..std::f32::consts::PI,
                ox in 5.0f32..45.0,
                oz in 5.0f32..45.0
            ) {
                let arena = generate_arena(ArenaSize::Default);
                let result = raycast_laser(
                    ox, oz, angle, &arena.walls, &[], 0, &[], 100.0,
                );
                for (i, &(sx, sz, ex, ez)) in result.segments.iter().enumerate() {
                    prop_assert!(
                        sx.is_finite() && sz.is_finite() && ex.is_finite() && ez.is_finite(),
                        "Segment {i} has non-finite coords: ({sx}, {sz}) -> ({ex}, {ez})"
                    );
                }
                prop_assert!(
                    result.total_distance.is_finite(),
                    "Total distance should be finite: {}",
                    result.total_distance
                );
            }

            // P2-1: Reflected laser never exceeds MAX_RANGE total distance
            #[test]
            fn reflected_laser_within_max_range(
                angle in -std::f32::consts::PI..std::f32::consts::PI
            ) {
                let arena = generate_arena(ArenaSize::Default);
                let max_range = 100.0;
                let result = raycast_laser(
                    25.0, 25.0, angle, &arena.walls, &[], 0, &[], max_range,
                );
                // Sum actual segment lengths
                let actual_dist: f32 = result
                    .segments
                    .iter()
                    .map(|&(sx, sz, ex, ez)| ((ex - sx).powi(2) + (ez - sz).powi(2)).sqrt())
                    .sum();
                prop_assert!(
                    actual_dist <= max_range + 2.0,
                    "Summed segment length ({actual_dist:.2}) exceeds max range ({max_range})"
                );
            }
        }
    }
}
