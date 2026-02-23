use glam::{Vec3, Vec4};

use crate::app::ActiveGame;
use crate::game::read_game_state;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::sprite_atlas::{SpriteSheet, build_platformer_atlas};
use crate::theme::Theme;

/// Cached sprite sheet — built once on first call.
fn atlas() -> &'static SpriteSheet {
    use std::sync::OnceLock;
    static SHEET: OnceLock<SpriteSheet> = OnceLock::new();
    SHEET.get_or_init(build_platformer_atlas)
}

const ATLAS_ID: u8 = 0;

/// Helper: add a sprite quad to the scene.
fn add_sprite(scene: &mut Scene, name: &str, x: f32, y: f32, w: f32, h: f32, tint: Vec4) {
    let sheet = atlas();
    let region = sheet.get_or_default(name);
    scene.add(
        MeshType::Quad,
        MaterialType::Sprite {
            atlas_id: ATLAS_ID,
            sprite_rect: region.to_vec4(),
            tint,
            flip_x: false,
        },
        Transform::from_xyz(x, y, 0.0).with_scale(Vec3::new(w, h, 1.0)),
    );
}

/// Sprite placement parameters for sprites that need flip control.
struct SpriteParams {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    tint: Vec4,
    flip_x: bool,
}

/// Helper: add a sprite quad with flip control.
fn add_sprite_ex(scene: &mut Scene, name: &str, params: &SpriteParams) {
    let sheet = atlas();
    let region = sheet.get_or_default(name);
    scene.add(
        MeshType::Quad,
        MaterialType::Sprite {
            atlas_id: ATLAS_ID,
            sprite_rect: region.to_vec4(),
            tint: params.tint,
            flip_x: params.flip_x,
        },
        Transform::from_xyz(params.x, params.y, 0.0).with_scale(Vec3::new(params.w, params.h, 1.0)),
    );
}

/// Pick player sprite name from animation state and time.
fn player_sprite_name(
    anim: &breakpoint_platformer::physics::AnimState,
    anim_time: f32,
) -> &'static str {
    use breakpoint_platformer::physics::AnimState;
    match anim {
        AnimState::Idle => {
            if (anim_time * 2.0) as i32 % 2 == 0 {
                "player_idle_0"
            } else {
                "player_idle_1"
            }
        },
        AnimState::Walk => {
            let frame = (anim_time * 8.0) as i32 % 4;
            match frame {
                0 => "player_walk_0",
                1 => "player_walk_1",
                2 => "player_walk_2",
                _ => "player_walk_3",
            }
        },
        AnimState::Jump => "player_jump",
        AnimState::Fall => "player_fall",
        AnimState::Attack => {
            let frame = (anim_time * 10.0) as i32 % 3;
            match frame {
                0 => "player_attack_0",
                1 => "player_attack_1",
                _ => "player_attack_2",
            }
        },
        AnimState::Hurt => "player_hurt",
        AnimState::Dead => "player_dead",
    }
}

/// Pick enemy sprite name from type and anim_time.
fn enemy_sprite_name(
    etype: &breakpoint_platformer::enemies::EnemyType,
    anim_time: f32,
) -> &'static str {
    use breakpoint_platformer::enemies::EnemyType;
    let frame = (anim_time * 4.0) as i32 % 2;
    match etype {
        EnemyType::Skeleton => {
            if frame == 0 {
                "skeleton_walk_0"
            } else {
                "skeleton_walk_1"
            }
        },
        EnemyType::Bat => {
            if frame == 0 {
                "bat_fly_0"
            } else {
                "bat_fly_1"
            }
        },
        EnemyType::Knight => {
            if frame == 0 {
                "knight_walk_0"
            } else {
                "knight_walk_1"
            }
        },
        EnemyType::Medusa => {
            if frame == 0 {
                "medusa_float_0"
            } else {
                "medusa_float_1"
            }
        },
    }
}

/// Map tile type to sprite name.
fn tile_sprite_name(tile: &breakpoint_platformer::course_gen::Tile) -> Option<&'static str> {
    use breakpoint_platformer::course_gen::Tile;
    match tile {
        Tile::Empty | Tile::PowerUpSpawn => None,
        Tile::StoneBrick => Some("stone_brick"),
        Tile::Platform => Some("platform"),
        Tile::Spikes => Some("spikes"),
        Tile::Checkpoint => Some("checkpoint_flag"),
        Tile::Finish => Some("finish_gate"),
        Tile::Ladder => Some("ladder"),
        Tile::BreakableWall => Some("breakable_wall"),
        Tile::DecoTorch => Some("torch"),
        Tile::DecoStainedGlass => Some("stained_glass"),
    }
}

/// Map power-up kind to sprite name.
fn powerup_sprite_name(kind: &breakpoint_platformer::powerups::PowerUpKind) -> &'static str {
    use breakpoint_platformer::powerups::PowerUpKind;
    match kind {
        PowerUpKind::HolyWater => "powerup_holy_water",
        PowerUpKind::Crucifix => "powerup_crucifix",
        PowerUpKind::SpeedBoots => "powerup_speed_boots",
        PowerUpKind::DoubleJump => "powerup_double_jump",
        PowerUpKind::ArmorUp => "powerup_armor",
        PowerUpKind::Invincibility => "powerup_invincibility",
        PowerUpKind::WhipExtend => "powerup_whip_extend",
    }
}

/// Sync the scene with the current platformer game state using flat sprites.
pub fn sync_platformer_scene(scene: &mut Scene, active: &ActiveGame, theme: &Theme, _dt: f32) {
    let state: Option<breakpoint_platformer::PlatformerState> = read_game_state(active);
    let Some(state) = state else {
        return;
    };

    scene.clear();

    let tile_size = breakpoint_platformer::physics::TILE_SIZE;
    let white = Vec4::ONE;

    // Render course tiles as sprite quads
    for y in 0..state.course.height {
        for x in 0..state.course.width {
            let tile = state.course.get_tile(x as i32, y as i32);
            let Some(sprite_name) = tile_sprite_name(&tile) else {
                continue;
            };
            let wx = x as f32 * tile_size + tile_size / 2.0;
            let wy = y as f32 * tile_size + tile_size / 2.0;
            add_sprite(scene, sprite_name, wx, wy, tile_size, tile_size, white);
        }
    }

    // Render enemies as animated sprite quads
    let enemy_tint = Vec4::new(
        theme.platformer.enemy_tint[0],
        theme.platformer.enemy_tint[1],
        theme.platformer.enemy_tint[2],
        1.0,
    );
    for enemy in &state.enemies {
        if !enemy.alive {
            continue;
        }
        let sprite_name = enemy_sprite_name(&enemy.enemy_type, enemy.anim_time);
        // Enemies are 16x32 sprites (1 tile wide, 2 tiles tall)
        add_sprite_ex(
            scene,
            sprite_name,
            &SpriteParams {
                x: enemy.x,
                y: enemy.y,
                w: tile_size,
                h: tile_size * 2.0,
                tint: enemy_tint,
                flip_x: !enemy.facing_right,
            },
        );
    }

    // Render enemy projectiles
    for proj in &state.projectiles {
        add_sprite(
            scene,
            "projectile",
            proj.x,
            proj.y,
            tile_size * 0.5,
            tile_size * 0.5,
            Vec4::new(1.0, 0.3, 0.9, 1.0),
        );
    }

    // Render players as animated sprite quads
    for (pid, player) in &state.players {
        if player.eliminated {
            continue;
        }
        // Blink during invincibility (alpha oscillation)
        if player.invincibility_timer > 0.0 {
            let blink = (player.invincibility_timer * 10.0) as i32;
            if blink % 2 == 0 {
                continue;
            }
        }
        // Don't render dead players awaiting respawn
        if player.death_respawn_timer > 0.0 {
            continue;
        }

        let sprite_name = player_sprite_name(&player.anim_state, player.anim_time);
        // Per-player tint color (unique per player ID)
        let tint = Vec4::new(
            ((*pid * 37) % 255) as f32 / 255.0 * 0.5 + 0.5,
            ((*pid * 73) % 255) as f32 / 255.0 * 0.5 + 0.5,
            ((*pid * 113) % 255) as f32 / 255.0 * 0.5 + 0.5,
            1.0,
        );
        // Players are 16x32 sprites (1 tile wide, 2 tiles tall)
        add_sprite_ex(
            scene,
            sprite_name,
            &SpriteParams {
                x: player.x,
                y: player.y,
                w: tile_size,
                h: tile_size * 2.0,
                tint,
                flip_x: !player.facing_right,
            },
        );

        // HP hearts above player
        let heart_size = tile_size * 0.3;
        let heart_y = player.y + tile_size * 1.3;
        let hearts_width = player.max_hp as f32 * heart_size * 1.2;
        let heart_start_x = player.x - hearts_width / 2.0 + heart_size / 2.0;
        for i in 0..player.max_hp {
            let hx = heart_start_x + i as f32 * heart_size * 1.2;
            let heart_name = if i < player.hp {
                "heart_full"
            } else {
                "heart_empty"
            };
            add_sprite(
                scene, heart_name, hx, heart_y, heart_size, heart_size, white,
            );
        }
    }

    // Render uncollected powerups
    for pu in &state.powerups {
        if pu.collected {
            continue;
        }
        let sprite_name = powerup_sprite_name(&pu.kind);
        add_sprite(
            scene,
            sprite_name,
            pu.x,
            pu.y,
            tile_size * 0.8,
            tile_size * 0.8,
            white,
        );
    }

    let _ = theme; // suppress unused warning
}
