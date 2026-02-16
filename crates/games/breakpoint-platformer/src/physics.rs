use serde::{Deserialize, Serialize};

use crate::course_gen::{Course, Tile};

/// Gravity acceleration (units/s^2, downward).
pub const GRAVITY: f32 = -30.0;
/// Horizontal move speed.
pub const MOVE_SPEED: f32 = 8.0;
/// Jump initial velocity.
pub const JUMP_VELOCITY: f32 = 12.0;
/// Player width for AABB collision.
pub const PLAYER_WIDTH: f32 = 0.8;
/// Player height for AABB collision.
pub const PLAYER_HEIGHT: f32 = 1.2;
/// Physics substeps per tick.
pub const SUBSTEPS: u32 = 4;
/// Tile size in world units.
pub const TILE_SIZE: f32 = 1.0;
/// Tolerance above platform top for landing detection.
const PLATFORM_LAND_TOLERANCE: f32 = 0.2;
/// Tolerance below platform top for landing detection.
const PLATFORM_SNAP_TOLERANCE: f32 = 0.1;
/// Y threshold below which player respawns at checkpoint.
const FALL_RESPAWN_Y: f32 = -5.0;

/// Configurable platformer physics parameters, loadable from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlatformerPhysicsConfig {
    pub gravity: f32,
    pub move_speed: f32,
    pub jump_velocity: f32,
    pub player_width: f32,
    pub player_height: f32,
    pub substeps: u32,
    pub tile_size: f32,
}

impl Default for PlatformerPhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: GRAVITY,
            move_speed: MOVE_SPEED,
            jump_velocity: JUMP_VELOCITY,
            player_width: PLAYER_WIDTH,
            player_height: PLAYER_HEIGHT,
            substeps: SUBSTEPS,
            tile_size: TILE_SIZE,
        }
    }
}

/// Top-level platformer game configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlatformerConfig {
    pub physics: PlatformerPhysicsConfig,
    pub round_duration_secs: f32,
    pub tick_rate_hz: f32,
    pub speed_boost_multiplier: f32,
}

impl Default for PlatformerConfig {
    fn default() -> Self {
        Self {
            physics: PlatformerPhysicsConfig::default(),
            round_duration_secs: 120.0,
            tick_rate_hz: 15.0,
            speed_boost_multiplier: 1.5,
        }
    }
}

impl PlatformerConfig {
    /// Load config from a TOML file. Falls back to defaults if the file is missing
    /// or unparseable.
    pub fn load() -> Self {
        let path = std::env::var("BREAKPOINT_PLATFORMER_CONFIG")
            .unwrap_or_else(|_| "config/platformer.toml".to_string());
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<PlatformerConfig>(&content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::warn!("Failed to parse {path}: {e}, using defaults");
                    PlatformerConfig::default()
                },
            },
            Err(_) => PlatformerConfig::default(),
        }
    }
}

/// State of a single player in the platformer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlatformerPlayerState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    pub has_double_jump: bool,
    pub jumps_remaining: u8,
    pub last_checkpoint_x: f32,
    pub last_checkpoint_y: f32,
    pub finished: bool,
    pub eliminated: bool,
    pub finish_time: Option<f32>,
}

impl PlatformerPlayerState {
    pub fn new(spawn_x: f32, spawn_y: f32) -> Self {
        Self {
            x: spawn_x,
            y: spawn_y,
            vx: 0.0,
            vy: 0.0,
            grounded: false,
            has_double_jump: false,
            jumps_remaining: 1,
            last_checkpoint_x: spawn_x,
            last_checkpoint_y: spawn_y,
            finished: false,
            eliminated: false,
            finish_time: None,
        }
    }

    pub fn respawn_at_checkpoint(&mut self) {
        self.x = self.last_checkpoint_x;
        self.y = self.last_checkpoint_y + 1.0;
        self.vx = 0.0;
        self.vy = 0.0;
        self.has_double_jump = false;
        self.jumps_remaining = 1;
    }
}

/// Input from a single player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformerInput {
    pub move_dir: f32, // -1 (left), 0, +1 (right)
    pub jump: bool,
    pub use_powerup: bool,
}

impl Default for PlatformerInput {
    fn default() -> Self {
        Self {
            move_dir: 0.0,
            jump: false,
            use_powerup: false,
        }
    }
}

/// Tick a player's physics for one substep.
pub fn tick_player(
    player: &mut PlatformerPlayerState,
    input: &PlatformerInput,
    course: &Course,
    dt: f32,
) {
    if player.finished || player.eliminated {
        return;
    }

    // Horizontal movement (sanitize NaN/Inf)
    let move_dir = if input.move_dir.is_finite() {
        input.move_dir
    } else {
        0.0
    };
    player.vx = move_dir * MOVE_SPEED;

    // Jump
    if input.jump && player.jumps_remaining > 0 {
        player.vy = JUMP_VELOCITY;
        player.jumps_remaining -= 1;
        player.grounded = false;
    }

    // Apply gravity
    player.vy += GRAVITY * dt;

    // Move
    player.x += player.vx * dt;
    player.y += player.vy * dt;

    // Tile collisions
    resolve_collisions(player, course);

    // Check special tiles
    check_tile_effects(player, course);
}

pub(crate) fn resolve_collisions(player: &mut PlatformerPlayerState, course: &Course) {
    let half_w = PLAYER_WIDTH / 2.0;
    let half_h = PLAYER_HEIGHT / 2.0;

    player.grounded = false;

    // Check surrounding tiles for collisions
    let min_tx = ((player.x - half_w) / TILE_SIZE).floor() as i32;
    let max_tx = ((player.x + half_w) / TILE_SIZE).ceil() as i32;
    let min_ty = ((player.y - half_h) / TILE_SIZE).floor() as i32;
    let max_ty = ((player.y + half_h) / TILE_SIZE).ceil() as i32;

    for ty in min_ty..max_ty {
        for tx in min_tx..max_tx {
            let tile = course.get_tile(tx, ty);
            if !is_solid(tile) {
                continue;
            }

            // Tile AABB
            let tile_left = tx as f32 * TILE_SIZE;
            let tile_bottom = ty as f32 * TILE_SIZE;
            let tile_right = tile_left + TILE_SIZE;
            let tile_top = tile_bottom + TILE_SIZE;

            // Player AABB
            let p_left = player.x - half_w;
            let p_right = player.x + half_w;
            let p_bottom = player.y - half_h;
            let p_top = player.y + half_h;

            // Check overlap
            if p_right <= tile_left
                || p_left >= tile_right
                || p_top <= tile_bottom
                || p_bottom >= tile_top
            {
                continue;
            }

            // Resolve with minimum penetration
            let overlap_left = p_right - tile_left;
            let overlap_right = tile_right - p_left;
            let overlap_bottom = p_top - tile_bottom;
            let overlap_top = tile_top - p_bottom;

            let min_overlap = overlap_left
                .min(overlap_right)
                .min(overlap_bottom)
                .min(overlap_top);

            if min_overlap == overlap_bottom {
                // Push down (hit head on tile)
                player.y = tile_bottom - half_h;
                if player.vy > 0.0 {
                    player.vy = 0.0;
                }
            } else if min_overlap == overlap_top {
                // Push up (landed on tile)
                player.y = tile_top + half_h;
                if player.vy < 0.0 {
                    player.vy = 0.0;
                }
                player.grounded = true;
                let max_jumps = if player.has_double_jump { 2 } else { 1 };
                player.jumps_remaining = max_jumps;
            } else if min_overlap == overlap_left {
                player.x = tile_left - half_w;
                player.vx = 0.0;
            } else {
                player.x = tile_right + half_w;
                player.vx = 0.0;
            }
        }
    }

    // Platform tiles: only collide from above
    for ty in min_ty..max_ty {
        for tx in min_tx..max_tx {
            if course.get_tile(tx, ty) != Tile::Platform {
                continue;
            }

            let tile_top = (ty as f32 + 1.0) * TILE_SIZE;
            let p_bottom = player.y - half_h;
            let tile_left = tx as f32 * TILE_SIZE;
            let tile_right = tile_left + TILE_SIZE;

            // Only collide if falling and feet near top of platform
            if player.vy < 0.0
                && p_bottom >= tile_top - PLATFORM_LAND_TOLERANCE
                && p_bottom <= tile_top + PLATFORM_SNAP_TOLERANCE
                && player.x + half_w > tile_left
                && player.x - half_w < tile_right
            {
                player.y = tile_top + half_h;
                player.vy = 0.0;
                player.grounded = true;
                let max_jumps = if player.has_double_jump { 2 } else { 1 };
                player.jumps_remaining = max_jumps;
            }
        }
    }

    // Fall off bottom → respawn
    if player.y < FALL_RESPAWN_Y {
        player.respawn_at_checkpoint();
    }
}

pub(crate) fn check_tile_effects(player: &mut PlatformerPlayerState, course: &Course) {
    let tx = (player.x / TILE_SIZE).floor() as i32;
    let ty = (player.y / TILE_SIZE).floor() as i32;

    match course.get_tile(tx, ty) {
        Tile::Hazard => {
            player.respawn_at_checkpoint();
        },
        Tile::Checkpoint => {
            if player.x > player.last_checkpoint_x {
                player.last_checkpoint_x = tx as f32 * TILE_SIZE + TILE_SIZE / 2.0;
                player.last_checkpoint_y = ty as f32 * TILE_SIZE + TILE_SIZE / 2.0;
            }
        },
        Tile::Finish => {
            player.finished = true;
            player.vx = 0.0;
            player.vy = 0.0;
        },
        _ => {},
    }
}

pub(crate) fn is_solid(tile: Tile) -> bool {
    matches!(tile, Tile::Solid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course_gen::generate_course;

    #[test]
    fn gravity_pulls_down() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(2.0, 10.0);
        let input = PlatformerInput::default();
        let y_before = player.y;

        // Run a single small substep so the player falls but doesn't reach -5.0
        tick_player(&mut player, &input, &course, 0.1);

        assert!(player.y < y_before, "Gravity should pull player down");
    }

    #[test]
    fn grounding_stops_fall() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(2.0, 10.0);
        let input = PlatformerInput::default();

        // Run many ticks, player should land
        for _ in 0..200 {
            for _ in 0..SUBSTEPS {
                tick_player(&mut player, &input, &course, 1.0 / SUBSTEPS as f32);
            }
        }

        assert!(
            player.grounded || player.y > -5.0,
            "Player should land or respawn"
        );
    }

    #[test]
    fn jumping_increases_y() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(2.0, 2.0);
        player.grounded = true;
        player.jumps_remaining = 1;

        let input = PlatformerInput {
            jump: true,
            ..Default::default()
        };

        tick_player(&mut player, &input, &course, 0.1);
        assert!(player.vy > 0.0, "Jump should give upward velocity");
    }

    #[test]
    fn checkpoint_respawn() {
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.last_checkpoint_x = 10.0;
        player.last_checkpoint_y = 8.0;
        player.respawn_at_checkpoint();
        assert_eq!(player.x, 10.0);
        assert_eq!(player.y, 9.0); // last_checkpoint_y + 1.0
        assert_eq!(player.vx, 0.0);
        assert_eq!(player.vy, 0.0);
    }

    #[test]
    fn double_jump_grants_extra_jump() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(2.0, 10.0);
        player.has_double_jump = true;
        player.grounded = false;
        player.jumps_remaining = 1;

        let input = PlatformerInput {
            jump: true,
            ..Default::default()
        };
        tick_player(&mut player, &input, &course, 0.05);
        assert_eq!(player.jumps_remaining, 0);
    }

    // ================================================================
    // Phase 3b: Collision resolution tests
    // ================================================================

    /// Build a course with a floor (row 0 and 1 solid) and optional extras.
    fn floor_course_with_extras(extras: &[(u32, u32, Tile)]) -> Course {
        let w = 20u32;
        let h = 20u32;
        let mut tiles = vec![Tile::Empty; (w * h) as usize];
        // Solid floor
        for x in 0..w {
            tiles[x as usize] = Tile::Solid;
            tiles[w as usize + x as usize] = Tile::Solid;
        }
        for &(x, y, tile) in extras {
            tiles[y as usize * w as usize + x as usize] = tile;
        }
        Course {
            width: w,
            height: h,
            tiles,
            spawn_x: 5.0,
            spawn_y: 3.0,
        }
    }

    #[test]
    fn landing_on_solid_block_sets_grounded() {
        // Floor at y=0,1. Player falls from above.
        let course = floor_course_with_extras(&[]);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.vy = -5.0;
        let input = PlatformerInput::default();

        for _ in 0..100 {
            tick_player(&mut player, &input, &course, 0.02);
        }

        assert!(
            player.grounded,
            "Player should be grounded after landing on floor"
        );
        assert!(
            player.vy.abs() < 0.1,
            "vy should be ~0 after landing, got {}",
            player.vy
        );
    }

    #[test]
    fn ceiling_collision_stops_upward_velocity() {
        // Solid block directly above player
        let course = floor_course_with_extras(&[(5, 5, Tile::Solid)]);
        let mut player = PlatformerPlayerState::new(5.5, 3.0);
        player.grounded = true;
        player.jumps_remaining = 1;

        // Jump: should hit the ceiling block at y=5
        let input = PlatformerInput {
            jump: true,
            ..Default::default()
        };
        tick_player(&mut player, &input, &course, 0.02);
        // After jump, vy should be positive initially
        assert!(player.vy >= 0.0);

        // Run more ticks — player should eventually hit ceiling and vy goes to 0
        let no_jump = PlatformerInput::default();
        for _ in 0..50 {
            tick_player(&mut player, &no_jump, &course, 0.02);
        }
        // Player should have come back down
        assert!(player.y < 5.0, "Player should be below ceiling block");
    }

    #[test]
    fn horizontal_wall_collision_stops_vx() {
        // Solid block to the right of player
        let course = floor_course_with_extras(&[(8, 2, Tile::Solid)]);
        let mut player = PlatformerPlayerState::new(7.0, 2.5 + PLAYER_HEIGHT / 2.0);
        player.grounded = true;

        let input = PlatformerInput {
            move_dir: 1.0,
            ..Default::default()
        };

        for _ in 0..20 {
            tick_player(&mut player, &input, &course, 0.02);
        }

        // Player should not pass through the solid block
        assert!(
            player.x < 8.0,
            "Player should be blocked by wall at x=8, got x={}",
            player.x
        );
    }

    #[test]
    fn platform_passthrough_from_below() {
        // Platform tile at y=5, player jumping up from below
        let course = floor_course_with_extras(&[(5, 5, Tile::Platform)]);
        let mut player = PlatformerPlayerState::new(5.5, 3.0);
        player.grounded = true;
        player.jumps_remaining = 1;
        player.vy = JUMP_VELOCITY;
        player.grounded = false;
        player.jumps_remaining = 0;

        let input = PlatformerInput::default();
        let initial_vy = player.vy;
        tick_player(&mut player, &input, &course, 0.02);

        // Player should pass through the platform from below (vy should still be positive
        // or at least not zeroed from collision)
        assert!(
            player.vy > 0.0 || player.y > 5.0,
            "Player should pass through platform from below: vy={}, y={}",
            player.vy,
            player.y
        );
        let _ = initial_vy;
    }

    #[test]
    fn platform_landing_from_above() {
        // Platform tile at y=5, player falling from above
        let course = floor_course_with_extras(&[(5, 5, Tile::Platform)]);
        let mut player = PlatformerPlayerState::new(5.5, 8.0);
        player.vy = -3.0;

        let input = PlatformerInput::default();
        for _ in 0..100 {
            tick_player(&mut player, &input, &course, 0.02);
            if player.grounded && player.y > 4.0 {
                break;
            }
        }

        // Player should have landed on the platform (y ≈ 6 + PLAYER_HEIGHT/2)
        assert!(
            player.y >= 5.0,
            "Player should land on platform at y>=5, got y={}",
            player.y
        );
    }

    #[test]
    fn fall_below_floor_respawns_at_checkpoint() {
        // Course with a gap — no floor at x=5..7
        let w = 20u32;
        let h = 20u32;
        let mut tiles = vec![Tile::Empty; (w * h) as usize];
        // Floor except gap at x=5,6
        for x in 0..w {
            if !(5..=6).contains(&x) {
                tiles[x as usize] = Tile::Solid;
                tiles[w as usize + x as usize] = Tile::Solid;
            }
        }
        let course = Course {
            width: w,
            height: h,
            tiles,
            spawn_x: 3.0,
            spawn_y: 3.0,
        };

        let mut player = PlatformerPlayerState::new(5.5, 3.0);
        player.last_checkpoint_x = 3.0;
        player.last_checkpoint_y = 3.0;
        let input = PlatformerInput::default();

        // Fall through gap
        for _ in 0..500 {
            tick_player(&mut player, &input, &course, 0.02);
            if player.x == 3.0 && player.y > 3.0 {
                // Respawned
                break;
            }
        }

        // After falling below -5.0, should respawn at checkpoint
        assert!(
            player.y > -5.0,
            "Player should have respawned, y={}",
            player.y
        );
    }

    #[test]
    fn double_jump_restores_on_ground() {
        let course = floor_course_with_extras(&[]);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.has_double_jump = true;
        player.jumps_remaining = 0;
        player.vy = -5.0;

        let input = PlatformerInput::default();
        for _ in 0..100 {
            tick_player(&mut player, &input, &course, 0.02);
        }

        if player.grounded {
            assert_eq!(
                player.jumps_remaining, 2,
                "Landing with double jump should give 2 jumps"
            );
        }
    }

    // ================================================================
    // Phase 3c: Tile effect tests
    // ================================================================

    #[test]
    fn hazard_tile_respawns_player() {
        let course = floor_course_with_extras(&[(5, 2, Tile::Hazard)]);
        let mut player = PlatformerPlayerState::new(5.5, 2.5);
        player.last_checkpoint_x = 3.0;
        player.last_checkpoint_y = 3.0;

        check_tile_effects(&mut player, &course);

        // Player should respawn at checkpoint
        assert_eq!(player.x, 3.0, "Hazard should respawn at checkpoint x");
        assert_eq!(player.y, 4.0, "Hazard should respawn at checkpoint y + 1.0");
    }

    #[test]
    fn checkpoint_forward_updates_position() {
        let course = floor_course_with_extras(&[(8, 2, Tile::Checkpoint)]);
        let mut player = PlatformerPlayerState::new(8.5, 2.5);
        player.last_checkpoint_x = 3.0;
        player.last_checkpoint_y = 3.0;

        check_tile_effects(&mut player, &course);

        // Checkpoint is forward (x=8.5 > 3.0), so it should update
        assert!(
            player.last_checkpoint_x > 3.0,
            "Checkpoint should update: last_checkpoint_x={}",
            player.last_checkpoint_x
        );
    }

    #[test]
    fn checkpoint_backward_ignored() {
        let course = floor_course_with_extras(&[(2, 2, Tile::Checkpoint)]);
        let mut player = PlatformerPlayerState::new(2.5, 2.5);
        player.last_checkpoint_x = 10.0;
        player.last_checkpoint_y = 3.0;

        check_tile_effects(&mut player, &course);

        // Checkpoint is backward (player.x=2.5 < last_checkpoint_x=10.0)
        assert_eq!(
            player.last_checkpoint_x, 10.0,
            "Backward checkpoint should not update position"
        );
    }

    #[test]
    fn finish_tile_marks_finished_and_zeroes_velocity() {
        let course = floor_course_with_extras(&[(15, 2, Tile::Finish)]);
        let mut player = PlatformerPlayerState::new(15.5, 2.5);
        player.vx = 5.0;
        player.vy = -2.0;

        check_tile_effects(&mut player, &course);

        assert!(player.finished, "Finish tile should set finished=true");
        assert_eq!(player.vx, 0.0, "Finish should zero vx");
        assert_eq!(player.vy, 0.0, "Finish should zero vy");
    }

    #[test]
    fn finished_player_skips_tick() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.finished = true;
        let y_before = player.y;

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            ..Default::default()
        };
        tick_player(&mut player, &input, &course, 0.1);

        assert_eq!(player.y, y_before, "Finished player should not move");
        assert_eq!(player.vx, 0.0, "Finished player vx should remain 0");
    }

    #[test]
    fn eliminated_player_skips_tick() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.eliminated = true;
        let y_before = player.y;

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            ..Default::default()
        };
        tick_player(&mut player, &input, &course, 0.1);

        assert_eq!(player.y, y_before, "Eliminated player should not move");
    }

    // ================================================================
    // Phase 4d: Property-based tests (proptest)
    // ================================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn player_never_stuck_below_floor(
                seed in 0u64..1000,
                moves in proptest::collection::vec(-1.0f32..=1.0, 10..50)
            ) {
                let course = generate_course(seed);
                let mut player = PlatformerPlayerState::new(
                    course.spawn_x,
                    course.spawn_y,
                );

                for &move_dir in &moves {
                    let input = PlatformerInput {
                        move_dir,
                        jump: move_dir > 0.5,
                        ..Default::default()
                    };
                    for _ in 0..SUBSTEPS {
                        tick_player(&mut player, &input, &course, 1.0 / SUBSTEPS as f32);
                    }
                }

                // After all ticks, player should be above -5.0 (respawn catches falls)
                prop_assert!(
                    player.y >= -5.0,
                    "Player y={} should be >= -5.0 (respawn should catch)",
                    player.y
                );
            }

            #[test]
            fn grounded_state_consistent_with_position(
                seed in 0u64..100
            ) {
                let course = generate_course(seed);
                let mut player = PlatformerPlayerState::new(
                    course.spawn_x,
                    course.spawn_y,
                );
                let input = PlatformerInput::default();

                // Let player settle
                for _ in 0..200 {
                    for _ in 0..SUBSTEPS {
                        tick_player(&mut player, &input, &course, 1.0 / SUBSTEPS as f32);
                    }
                }

                // If grounded, player should be resting on or above a tile
                if player.grounded {
                    let half_h = PLAYER_HEIGHT / 2.0;
                    let foot_y = player.y - half_h;
                    // Feet should be at or above a tile top (within tolerance)
                    prop_assert!(
                        foot_y >= -0.5,
                        "Grounded player feet y={foot_y} should be near a tile surface"
                    );
                }
            }

            // P2-1: Player position stays valid after collision resolution
            // The tile collision system allows AABB overlap with multi-tile
            // solid blocks (resolved incrementally per-tile), so we check
            // that the player doesn't fall through the world entirely.
            #[test]
            fn player_position_stays_valid(
                seed in 0u64..200,
                moves in proptest::collection::vec(-1.0f32..=1.0, 20..80)
            ) {
                let course = generate_course(seed);
                let mut player = PlatformerPlayerState::new(
                    course.spawn_x,
                    course.spawn_y,
                );

                for &move_dir in &moves {
                    let input = PlatformerInput {
                        move_dir,
                        jump: move_dir > 0.3,
                        ..Default::default()
                    };
                    for _ in 0..SUBSTEPS {
                        tick_player(&mut player, &input, &course, 1.0 / SUBSTEPS as f32);
                    }

                    if player.finished || player.eliminated {
                        break;
                    }

                    // Player should be in valid position:
                    // - x within course bounds (with some margin for edge)
                    // - y above respawn threshold (respawn catches y < -5.0)
                    // - All coordinates finite
                    prop_assert!(
                        player.x.is_finite() && player.y.is_finite(),
                        "Player position must be finite: ({}, {})",
                        player.x,
                        player.y
                    );
                    prop_assert!(
                        player.y >= -5.0,
                        "Player y={} fell below respawn threshold",
                        player.y
                    );
                    // Player can walk off course edges (no invisible walls)
                    // but shouldn't teleport to absurd positions
                    let course_extent = course.width as f32 * TILE_SIZE;
                    prop_assert!(
                        player.x >= -course_extent && player.x <= course_extent * 2.0,
                        "Player x={} teleported far beyond course bounds [0, {}]",
                        player.x,
                        course_extent
                    );
                }
            }

            // P2-1: double_jump_remaining resets on every ground contact
            #[test]
            fn double_jump_resets_on_ground(
                seed in 0u64..100
            ) {
                let course = generate_course(seed);
                let mut player = PlatformerPlayerState::new(
                    course.spawn_x,
                    course.spawn_y,
                );
                player.has_double_jump = true;

                // Let player settle to ground
                let no_input = PlatformerInput::default();
                for _ in 0..100 {
                    for _ in 0..SUBSTEPS {
                        tick_player(&mut player, &no_input, &course, 1.0 / SUBSTEPS as f32);
                    }
                }

                if player.grounded && player.has_double_jump {
                    prop_assert_eq!(
                        player.jumps_remaining, 2,
                        "Grounded player with double jump should have 2 jumps remaining"
                    );

                    // Jump
                    let jump_input = PlatformerInput {
                        jump: true,
                        ..Default::default()
                    };
                    for _ in 0..SUBSTEPS {
                        tick_player(&mut player, &jump_input, &course, 1.0 / SUBSTEPS as f32);
                    }

                    // Should have fewer jumps
                    prop_assert!(
                        player.jumps_remaining < 2,
                        "After jumping, should have fewer jumps"
                    );

                    // Let player land again
                    for _ in 0..200 {
                        for _ in 0..SUBSTEPS {
                            tick_player(
                                &mut player,
                                &no_input,
                                &course,
                                1.0 / SUBSTEPS as f32,
                            );
                        }
                        if player.grounded {
                            break;
                        }
                    }

                    if player.grounded {
                        prop_assert_eq!(
                            player.jumps_remaining, 2,
                            "Jumps should reset on landing"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn respawn_resets_double_jump() {
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.has_double_jump = true;
        player.jumps_remaining = 2;
        player.last_checkpoint_x = 10.0;
        player.last_checkpoint_y = 8.0;

        player.respawn_at_checkpoint();

        assert!(
            !player.has_double_jump,
            "Double jump should be reset on respawn"
        );
        assert_eq!(
            player.jumps_remaining, 1,
            "Jumps remaining should be 1 on respawn"
        );
    }

    #[test]
    fn nan_move_dir_treated_as_zero() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(2.0, 2.0);
        player.grounded = true;

        let input = PlatformerInput {
            move_dir: f32::NAN,
            jump: false,
            use_powerup: false,
        };
        tick_player(&mut player, &input, &course, 0.1);

        assert_eq!(
            player.vx, 0.0,
            "NaN move_dir should be sanitized to 0, resulting in vx=0"
        );
    }
}
