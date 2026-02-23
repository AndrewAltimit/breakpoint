use serde::{Deserialize, Serialize};

use crate::combat::{ATTACK_COOLDOWN, ATTACK_DURATION, INVINCIBILITY_DURATION};
use crate::course_gen::{Course, Tile};
use crate::powerups::PowerUpKind;

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
/// Ladder climb speed (units/s).
const LADDER_SPEED: f32 = 5.0;

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
            round_duration_secs: 180.0,
            tick_rate_hz: 20.0,
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

/// Animation state for player rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimState {
    Idle,
    Walk,
    Jump,
    Fall,
    Attack,
    Hurt,
    Dead,
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
    pub last_checkpoint_id: u16,
    pub finished: bool,
    pub eliminated: bool,
    pub finish_time: Option<f32>,
    // Combat fields
    pub hp: u8,
    pub max_hp: u8,
    pub invincibility_timer: f32,
    pub attack_timer: f32,
    pub attack_cooldown: f32,
    pub deaths: u8,
    pub death_respawn_timer: f32,
    // Animation and facing
    pub facing_right: bool,
    pub anim_state: AnimState,
    pub anim_time: f32,
    // Active power-up (single slot for non-instant powerups)
    pub active_powerup: Option<PowerUpKind>,
    pub powerup_timer: f32,
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
            last_checkpoint_id: 0,
            finished: false,
            eliminated: false,
            finish_time: None,
            hp: 3,
            max_hp: 3,
            invincibility_timer: 0.0,
            attack_timer: 0.0,
            attack_cooldown: 0.0,
            deaths: 0,
            death_respawn_timer: 0.0,
            facing_right: true,
            anim_state: AnimState::Idle,
            anim_time: 0.0,
            active_powerup: None,
            powerup_timer: 0.0,
        }
    }

    pub fn respawn_at_checkpoint(&mut self) {
        self.x = self.last_checkpoint_x;
        self.y = self.last_checkpoint_y + 1.0;
        self.vx = 0.0;
        self.vy = 0.0;
        self.hp = self.max_hp;
        self.has_double_jump = false;
        self.jumps_remaining = 1;
        self.invincibility_timer = 0.0;
        self.attack_timer = 0.0;
        self.attack_cooldown = 0.0;
        self.death_respawn_timer = 0.0;
        self.anim_state = AnimState::Idle;
    }
}

/// Input from a single player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformerInput {
    pub move_dir: f32, // -1 (left), 0, +1 (right)
    pub jump: bool,
    pub use_powerup: bool,
    pub attack: bool,
}

impl Default for PlatformerInput {
    fn default() -> Self {
        Self {
            move_dir: 0.0,
            jump: false,
            use_powerup: false,
            attack: false,
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

    // Death respawn timer: skip all movement while dead
    if player.death_respawn_timer > 0.0 {
        player.death_respawn_timer -= dt;
        player.anim_state = AnimState::Dead;
        if player.death_respawn_timer <= 0.0 {
            player.respawn_at_checkpoint();
        }
        return;
    }

    // Tick invincibility timer
    if player.invincibility_timer > 0.0 {
        player.invincibility_timer -= dt;
        if player.invincibility_timer < 0.0 {
            player.invincibility_timer = 0.0;
        }
    }

    // Tick animation time
    player.anim_time += dt;

    // Attack state machine
    if player.attack_cooldown > 0.0 {
        player.attack_cooldown -= dt;
        if player.attack_cooldown < 0.0 {
            player.attack_cooldown = 0.0;
        }
    }
    if player.attack_timer > 0.0 {
        player.attack_timer -= dt;
        if player.attack_timer <= 0.0 {
            player.attack_timer = 0.0;
            player.attack_cooldown = ATTACK_COOLDOWN;
        }
    }
    // Start attack if requested and not in cooldown/active attack
    if input.attack && player.attack_timer <= 0.0 && player.attack_cooldown <= 0.0 {
        player.attack_timer = ATTACK_DURATION;
    }

    // Check if currently on a ladder
    let tx = (player.x / TILE_SIZE).floor() as i32;
    let ty = (player.y / TILE_SIZE).floor() as i32;
    let on_ladder = course.get_tile(tx, ty) == Tile::Ladder;

    // Horizontal movement (sanitize NaN/Inf)
    let move_dir = if input.move_dir.is_finite() {
        input.move_dir
    } else {
        0.0
    };

    // Update facing direction
    if move_dir > 0.01 {
        player.facing_right = true;
    } else if move_dir < -0.01 {
        player.facing_right = false;
    }

    if on_ladder {
        // Ladder movement: disable gravity, allow vertical movement
        player.vx = move_dir * MOVE_SPEED * 0.5; // Slower horizontal on ladder
        player.vy = 0.0;

        if input.jump {
            player.vy = LADDER_SPEED; // Climb up
        }
        // Can also move down by not pressing jump (just fall slowly or hold move_dir)
        // If not pressing anything, hold position
        if !input.jump && move_dir.abs() < 0.01 {
            player.vy = -LADDER_SPEED * 0.3; // Slow slide down when idle on ladder
        }

        // Jump off ladder with sufficient horizontal input
        if move_dir.abs() > 0.5 && input.jump {
            player.vy = JUMP_VELOCITY * 0.7;
            player.grounded = false;
            // Let normal physics take over below
        }
    } else {
        // Normal movement
        player.vx = move_dir * MOVE_SPEED;

        // Jump
        if input.jump && player.jumps_remaining > 0 {
            player.vy = JUMP_VELOCITY;
            player.jumps_remaining -= 1;
            player.grounded = false;
        }

        // Apply gravity
        player.vy += GRAVITY * dt;
    }

    // Move
    player.x += player.vx * dt;
    player.y += player.vy * dt;

    // Tile collisions
    resolve_collisions(player, course);

    // Check special tiles
    check_tile_effects(player, course);

    // Update animation state
    update_anim_state(player);
}

/// Update the player's animation state based on their current status.
fn update_anim_state(player: &mut PlatformerPlayerState) {
    // Attack overrides everything while active
    if player.attack_timer > 0.0 {
        player.anim_state = AnimState::Attack;
        return;
    }

    // Hurt overrides while in early invincibility frames
    if player.invincibility_timer > INVINCIBILITY_DURATION - 0.3 {
        player.anim_state = AnimState::Hurt;
        return;
    }

    // Airborne states
    if !player.grounded {
        if player.vy > 0.0 {
            player.anim_state = AnimState::Jump;
        } else {
            player.anim_state = AnimState::Fall;
        }
        return;
    }

    // Grounded states
    if player.vx.abs() > 0.1 {
        player.anim_state = AnimState::Walk;
    } else {
        player.anim_state = AnimState::Idle;
    }
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

    // Fall off bottom -> respawn via damage (not instant)
    if player.y < FALL_RESPAWN_Y {
        player.respawn_at_checkpoint();
    }
}

pub(crate) fn check_tile_effects(player: &mut PlatformerPlayerState, course: &Course) {
    let tx = (player.x / TILE_SIZE).floor() as i32;
    let ty = (player.y / TILE_SIZE).floor() as i32;

    match course.get_tile(tx, ty) {
        Tile::Spikes => {
            // Spikes deal 1 HP damage with invincibility, instead of instant respawn
            if player.invincibility_timer <= 0.0 {
                player.hp = player.hp.saturating_sub(1);
                if player.hp == 0 {
                    player.deaths += 1;
                    player.death_respawn_timer = crate::combat::DEATH_RESPAWN_TIMER;
                    player.vx = 0.0;
                    player.vy = 0.0;
                } else {
                    player.invincibility_timer = INVINCIBILITY_DURATION;
                    // Bounce player up slightly to avoid repeat damage
                    player.vy = JUMP_VELOCITY * 0.5;
                }
            }
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
    matches!(tile, Tile::StoneBrick | Tile::BreakableWall)
}

/// Check if an attack can break a breakable wall at the given tile coords.
/// Returns true if the wall was broken.
pub fn try_break_wall(course: &mut Course, tx: i32, ty: i32) -> bool {
    if course.get_tile(tx, ty) == Tile::BreakableWall && tx >= 0 && ty >= 0 {
        course.set_tile(tx as u32, ty as u32, Tile::Empty);
        return true;
    }
    false
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
        player.hp = 1;
        player.invincibility_timer = 0.5;
        player.attack_timer = 0.2;
        player.respawn_at_checkpoint();
        assert_eq!(player.x, 10.0);
        assert_eq!(player.y, 9.0); // last_checkpoint_y + 1.0
        assert_eq!(player.vx, 0.0);
        assert_eq!(player.vy, 0.0);
        assert_eq!(player.hp, player.max_hp, "HP should be restored to max");
        assert_eq!(
            player.invincibility_timer, 0.0,
            "Invincibility should clear"
        );
        assert_eq!(player.attack_timer, 0.0, "Attack timer should clear");
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

    #[test]
    fn new_player_has_3_hp() {
        let player = PlatformerPlayerState::new(0.0, 0.0);
        assert_eq!(player.hp, 3);
        assert_eq!(player.max_hp, 3);
    }

    #[test]
    fn new_player_starts_facing_right() {
        let player = PlatformerPlayerState::new(0.0, 0.0);
        assert!(player.facing_right);
    }

    #[test]
    fn new_player_starts_idle() {
        let player = PlatformerPlayerState::new(0.0, 0.0);
        assert_eq!(player.anim_state, AnimState::Idle);
    }

    #[test]
    fn attack_starts_on_input() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        let input = PlatformerInput {
            attack: true,
            ..Default::default()
        };

        tick_player(&mut player, &input, &course, 0.01);
        assert!(
            player.attack_timer > 0.0,
            "Attack timer should start on attack input"
        );
        assert_eq!(
            player.anim_state,
            AnimState::Attack,
            "Should be in Attack anim state"
        );
    }

    #[test]
    fn attack_cooldown_prevents_immediate_reattack() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);

        // Start attack
        let attack_input = PlatformerInput {
            attack: true,
            ..Default::default()
        };
        tick_player(&mut player, &attack_input, &course, 0.01);

        // Fast-forward past attack duration
        for _ in 0..50 {
            tick_player(&mut player, &PlatformerInput::default(), &course, 0.01);
        }

        // Attack should have ended and cooldown should be active
        assert!(
            player.attack_cooldown > 0.0 || player.attack_timer <= 0.0,
            "Should be in cooldown after attack ends"
        );
    }

    #[test]
    fn death_respawn_timer_prevents_movement() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.death_respawn_timer = 1.0;
        let x_before = player.x;
        let y_before = player.y;

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            ..Default::default()
        };
        tick_player(&mut player, &input, &course, 0.1);

        assert_eq!(player.x, x_before, "Dead player should not move x");
        assert_eq!(player.y, y_before, "Dead player should not move y");
        assert_eq!(player.anim_state, AnimState::Dead);
    }

    #[test]
    fn respawn_after_death_timer_expires() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.last_checkpoint_x = 10.0;
        player.last_checkpoint_y = 3.0;
        player.death_respawn_timer = 0.1;
        player.hp = 0;

        // Tick past the respawn timer
        tick_player(&mut player, &PlatformerInput::default(), &course, 0.2);

        assert_eq!(player.x, 10.0, "Should respawn at checkpoint x");
        assert_eq!(player.hp, player.max_hp, "HP should be restored");
    }

    #[test]
    fn facing_direction_updates_from_input() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        assert!(player.facing_right);

        // Move left
        let input = PlatformerInput {
            move_dir: -1.0,
            ..Default::default()
        };
        tick_player(&mut player, &input, &course, 0.01);
        assert!(!player.facing_right, "Should face left after moving left");

        // Move right
        let input = PlatformerInput {
            move_dir: 1.0,
            ..Default::default()
        };
        tick_player(&mut player, &input, &course, 0.01);
        assert!(player.facing_right, "Should face right after moving right");
    }

    #[test]
    fn invincibility_decrements() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(5.0, 5.0);
        player.invincibility_timer = 1.0;

        tick_player(&mut player, &PlatformerInput::default(), &course, 0.5);

        assert!(
            (player.invincibility_timer - 0.5).abs() < 0.1,
            "Invincibility should decrement: got {}",
            player.invincibility_timer,
        );
    }

    #[test]
    fn spikes_deal_damage_not_instant_respawn() {
        // Create a course with spikes
        let mut course = generate_course(42);
        // Place spikes at a known location
        course.set_tile(10, 2, Tile::Spikes);

        let mut player = PlatformerPlayerState::new(10.5, 2.5);
        player.hp = 3;
        player.invincibility_timer = 0.0;

        check_tile_effects(&mut player, &course);

        assert_eq!(player.hp, 2, "Spikes should deal 1 HP damage");
        assert!(
            player.invincibility_timer > 0.0,
            "Should get invincibility after spike damage"
        );
    }

    #[test]
    fn spikes_kill_at_1_hp() {
        let mut course = generate_course(42);
        course.set_tile(10, 2, Tile::Spikes);

        let mut player = PlatformerPlayerState::new(10.5, 2.5);
        player.hp = 1;
        player.invincibility_timer = 0.0;

        check_tile_effects(&mut player, &course);

        assert_eq!(player.hp, 0, "Player should die on spikes at 1 HP");
        assert!(
            player.death_respawn_timer > 0.0,
            "Should have respawn timer"
        );
        assert_eq!(player.deaths, 1);
    }

    #[test]
    fn spikes_ignored_when_invincible() {
        let mut course = generate_course(42);
        course.set_tile(10, 2, Tile::Spikes);

        let mut player = PlatformerPlayerState::new(10.5, 2.5);
        player.hp = 3;
        player.invincibility_timer = 1.0; // Already invincible

        check_tile_effects(&mut player, &course);

        assert_eq!(player.hp, 3, "Spikes should not damage invincible player");
    }

    #[test]
    fn is_solid_includes_stone_brick() {
        assert!(is_solid(Tile::StoneBrick));
    }

    #[test]
    fn is_solid_includes_breakable_wall() {
        assert!(is_solid(Tile::BreakableWall));
    }

    #[test]
    fn is_solid_excludes_empty() {
        assert!(!is_solid(Tile::Empty));
    }

    #[test]
    fn is_solid_excludes_platform() {
        assert!(!is_solid(Tile::Platform));
    }

    #[test]
    fn try_break_wall_breaks_breakable() {
        let mut course = generate_course(42);
        course.set_tile(10, 5, Tile::BreakableWall);

        assert!(try_break_wall(&mut course, 10, 5));
        assert_eq!(
            course.get_tile(10, 5),
            Tile::Empty,
            "Broken wall should become Empty"
        );
    }

    #[test]
    fn try_break_wall_ignores_non_breakable() {
        let mut course = generate_course(42);
        course.set_tile(10, 5, Tile::StoneBrick);

        assert!(!try_break_wall(&mut course, 10, 5));
        assert_eq!(
            course.get_tile(10, 5),
            Tile::StoneBrick,
            "StoneBrick should not break"
        );
    }

    /// Build a course with a floor (rows 0-1 stone brick) and optional extras.
    fn floor_course_with_extras(extras: &[(u32, u32, Tile)]) -> Course {
        let w = 20u32;
        let h = 20u32;
        let mut tiles = vec![Tile::Empty; (w * h) as usize];
        // Solid floor
        for x in 0..w {
            tiles[x as usize] = Tile::StoneBrick;
            tiles[w as usize + x as usize] = Tile::StoneBrick;
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
            enemy_spawns: Vec::new(),
            checkpoint_positions: Vec::new(),
        }
    }

    #[test]
    fn landing_on_solid_block_sets_grounded() {
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
        let course = floor_course_with_extras(&[(5, 5, Tile::StoneBrick)]);
        let mut player = PlatformerPlayerState::new(5.5, 3.0);
        player.grounded = true;
        player.jumps_remaining = 1;

        let input = PlatformerInput {
            jump: true,
            ..Default::default()
        };
        tick_player(&mut player, &input, &course, 0.02);
        assert!(player.vy >= 0.0);

        let no_jump = PlatformerInput::default();
        for _ in 0..50 {
            tick_player(&mut player, &no_jump, &course, 0.02);
        }
        assert!(player.y < 5.0, "Player should be below ceiling block");
    }

    #[test]
    fn horizontal_wall_collision_stops_vx() {
        let course = floor_course_with_extras(&[(8, 2, Tile::StoneBrick)]);
        let mut player = PlatformerPlayerState::new(7.0, 2.5 + PLAYER_HEIGHT / 2.0);
        player.grounded = true;

        let input = PlatformerInput {
            move_dir: 1.0,
            ..Default::default()
        };

        for _ in 0..20 {
            tick_player(&mut player, &input, &course, 0.02);
        }

        assert!(
            player.x < 8.0,
            "Player should be blocked by wall at x=8, got x={}",
            player.x
        );
    }

    #[test]
    fn platform_passthrough_from_below() {
        let course = floor_course_with_extras(&[(5, 5, Tile::Platform)]);
        let mut player = PlatformerPlayerState::new(5.5, 3.0);
        player.grounded = true;
        player.jumps_remaining = 1;
        player.vy = JUMP_VELOCITY;
        player.grounded = false;
        player.jumps_remaining = 0;

        let input = PlatformerInput::default();
        tick_player(&mut player, &input, &course, 0.02);

        assert!(
            player.vy > 0.0 || player.y > 5.0,
            "Player should pass through platform from below: vy={}, y={}",
            player.vy,
            player.y
        );
    }

    #[test]
    fn platform_landing_from_above() {
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

        assert!(
            player.y >= 5.0,
            "Player should land on platform at y>=5, got y={}",
            player.y
        );
    }

    #[test]
    fn fall_below_floor_respawns_at_checkpoint() {
        let w = 20u32;
        let h = 20u32;
        let mut tiles = vec![Tile::Empty; (w * h) as usize];
        for x in 0..w {
            if !(5..=6).contains(&x) {
                tiles[x as usize] = Tile::StoneBrick;
                tiles[w as usize + x as usize] = Tile::StoneBrick;
            }
        }
        let course = Course {
            width: w,
            height: h,
            tiles,
            spawn_x: 3.0,
            spawn_y: 3.0,
            enemy_spawns: Vec::new(),
            checkpoint_positions: Vec::new(),
        };

        let mut player = PlatformerPlayerState::new(5.5, 3.0);
        player.last_checkpoint_x = 3.0;
        player.last_checkpoint_y = 3.0;
        let input = PlatformerInput::default();

        for _ in 0..500 {
            tick_player(&mut player, &input, &course, 0.02);
            if player.x == 3.0 && player.y > 3.0 {
                break;
            }
        }

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

    #[test]
    fn checkpoint_forward_updates_position() {
        let course = floor_course_with_extras(&[(8, 2, Tile::Checkpoint)]);
        let mut player = PlatformerPlayerState::new(8.5, 2.5);
        player.last_checkpoint_x = 3.0;
        player.last_checkpoint_y = 3.0;

        check_tile_effects(&mut player, &course);

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

    #[test]
    fn nan_move_dir_treated_as_zero() {
        let course = generate_course(42);
        let mut player = PlatformerPlayerState::new(2.0, 2.0);
        player.grounded = true;

        let input = PlatformerInput {
            move_dir: f32::NAN,
            jump: false,
            use_powerup: false,
            attack: false,
        };
        tick_player(&mut player, &input, &course, 0.1);

        assert_eq!(
            player.vx, 0.0,
            "NaN move_dir should be sanitized to 0, resulting in vx=0"
        );
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

    // ================================================================
    // Property-based tests (proptest)
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

                for _ in 0..200 {
                    for _ in 0..SUBSTEPS {
                        tick_player(&mut player, &input, &course, 1.0 / SUBSTEPS as f32);
                    }
                }

                if player.grounded {
                    let half_h = PLAYER_HEIGHT / 2.0;
                    let foot_y = player.y - half_h;
                    prop_assert!(
                        foot_y >= -0.5,
                        "Grounded player feet y={foot_y} should be near a tile surface"
                    );
                }
            }

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
                    let course_extent = course.width as f32 * TILE_SIZE;
                    prop_assert!(
                        player.x >= -course_extent && player.x <= course_extent * 2.0,
                        "Player x={} teleported far beyond course bounds [0, {}]",
                        player.x,
                        course_extent
                    );
                }
            }

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

                    let jump_input = PlatformerInput {
                        jump: true,
                        ..Default::default()
                    };
                    for _ in 0..SUBSTEPS {
                        tick_player(&mut player, &jump_input, &course, 1.0 / SUBSTEPS as f32);
                    }

                    prop_assert!(
                        player.jumps_remaining < 2,
                        "After jumping, should have fewer jumps"
                    );

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
}
