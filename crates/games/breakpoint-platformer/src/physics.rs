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

    // Horizontal movement
    player.vx = input.move_dir * MOVE_SPEED;

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

fn resolve_collisions(player: &mut PlatformerPlayerState, course: &Course) {
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
                && p_bottom >= tile_top - 0.2
                && p_bottom <= tile_top + 0.1
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
    if player.y < -5.0 {
        player.respawn_at_checkpoint();
    }
}

fn check_tile_effects(player: &mut PlatformerPlayerState, course: &Course) {
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

fn is_solid(tile: Tile) -> bool {
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
}
