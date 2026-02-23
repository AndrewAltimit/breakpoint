use std::collections::HashMap;
use std::sync::OnceLock;

use glam::{Vec3, Vec4};

use crate::app::ActiveGame;
use crate::game::read_game_state;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::sprite_atlas::{
    SpriteAnimation, SpriteRegion, SpriteSheet, build_platformer_animations, build_platformer_atlas,
};
use crate::theme::Theme;

/// Background texture atlas ID.
const BG_ATLAS_ID: u8 = 1;
/// Sprite atlas ID.
const ATLAS_ID: u8 = 0;

/// Cached sprite sheet — built once on first call.
fn atlas() -> &'static SpriteSheet {
    static SHEET: OnceLock<SpriteSheet> = OnceLock::new();
    SHEET.get_or_init(build_platformer_atlas)
}

/// Cached animation table — built once on first call.
fn animations() -> &'static HashMap<&'static str, SpriteAnimation> {
    static ANIMS: OnceLock<HashMap<&'static str, SpriteAnimation>> = OnceLock::new();
    ANIMS.get_or_init(|| build_platformer_animations(atlas()))
}

/// Sprite placement parameters.
struct SpriteParams {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    tint: Vec4,
    flip_x: bool,
}

/// Helper: add a sprite quad from a SpriteRegion directly.
fn add_sprite_region(scene: &mut Scene, region: &SpriteRegion, params: &SpriteParams) {
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

/// Helper: add a sprite quad by name.
fn add_sprite(scene: &mut Scene, name: &str, x: f32, y: f32, w: f32, h: f32, tint: Vec4) {
    let region = atlas().get_or_default(name);
    add_sprite_region(
        scene,
        &region,
        &SpriteParams {
            x,
            y,
            w,
            h,
            tint,
            flip_x: false,
        },
    );
}

/// Get the sprite region for a player animation frame.
fn player_sprite_region(
    anim_state: &breakpoint_platformer::physics::AnimState,
    anim_time: f32,
) -> SpriteRegion {
    use breakpoint_platformer::physics::AnimState;
    let anims = animations();
    let key = match anim_state {
        AnimState::Idle => "player_idle",
        AnimState::Walk => "player_walk",
        AnimState::Jump => "player_jump",
        AnimState::Fall => "player_fall",
        AnimState::Attack => "player_attack",
        AnimState::Hurt => "player_hurt",
        AnimState::Dead => "player_dead",
    };
    match anims.get(key) {
        Some(anim) => *anim.frame_at(anim_time),
        None => atlas().get_or_default("player_idle_0"),
    }
}

/// Get the sprite region for an enemy animation frame.
fn enemy_sprite_region(
    etype: &breakpoint_platformer::enemies::EnemyType,
    anim_time: f32,
    alive: bool,
    respawn_timer: f32,
) -> Option<SpriteRegion> {
    use breakpoint_platformer::enemies::EnemyType;
    let anims = animations();

    if !alive {
        // Show death animation for the first 0.6s after death
        let death_time = breakpoint_platformer::enemies::RESPAWN_DELAY - respawn_timer;
        if death_time > 0.6 {
            return None; // Vanished
        }
        let key = match etype {
            EnemyType::Skeleton => "skeleton_death",
            EnemyType::Bat => "bat_death",
            EnemyType::Knight => "knight_death",
            EnemyType::Medusa => "medusa_death",
        };
        return anims.get(key).map(|a| *a.frame_at(death_time));
    }

    let key = match etype {
        EnemyType::Skeleton => "skeleton_walk",
        EnemyType::Bat => "bat_fly",
        EnemyType::Knight => "knight_walk",
        EnemyType::Medusa => "medusa_float",
    };
    anims.get(key).map(|a| *a.frame_at(anim_time))
}

/// Map tile type to sprite name, with auto-tiling for stone bricks.
fn tile_sprite_region(
    tile: &breakpoint_platformer::course_gen::Tile,
    course: &breakpoint_platformer::course_gen::Course,
    tx: i32,
    ty: i32,
    time: f32,
) -> Option<SpriteRegion> {
    use breakpoint_platformer::course_gen::Tile;
    let sheet = atlas();

    match tile {
        Tile::Empty | Tile::PowerUpSpawn => None,
        Tile::StoneBrick => Some(stone_brick_region(sheet, course, tx, ty)),
        Tile::Platform => Some(sheet.get_or_default("platform_0")),
        Tile::Spikes => Some(sheet.get_or_default("spikes_0")),
        Tile::Checkpoint => Some(sheet.get_or_default("checkpoint_flag_down_0")),
        Tile::Finish => {
            let anims = animations();
            anims
                .get("finish_gate")
                .map(|a| *a.frame_at(time))
                .or_else(|| Some(sheet.get_or_default("finish_gate_0")))
        },
        Tile::Ladder => Some(sheet.get_or_default("ladder")),
        Tile::BreakableWall => Some(sheet.get_or_default("breakable_wall_0")),
        Tile::DecoTorch => {
            // Animated torch with per-tile phase offset
            let phase = tx as f32 * 0.3 + ty as f32 * 0.7;
            let anims = animations();
            anims
                .get("torch")
                .map(|a| *a.frame_at(time + phase))
                .or_else(|| Some(sheet.get_or_default("torch_0")))
        },
        Tile::DecoStainedGlass => Some(sheet.get_or_default("stained_glass")),
    }
}

/// Auto-tile selection for stone bricks: check neighbors to pick edge variant.
fn stone_brick_region(
    sheet: &SpriteSheet,
    course: &breakpoint_platformer::course_gen::Course,
    tx: i32,
    ty: i32,
) -> SpriteRegion {
    use breakpoint_platformer::course_gen::Tile;
    let is_solid = |dx: i32, dy: i32| -> bool {
        matches!(course.get_tile(tx + dx, ty + dy), Tile::StoneBrick)
    };

    let above = is_solid(0, 1);
    let below = is_solid(0, -1);
    let left = is_solid(-1, 0);
    let right = is_solid(1, 0);

    let name = match (above, below, left, right) {
        // Exposed top edge
        (false, _, true, true) => "stone_brick_top",
        (false, _, false, true) => "stone_brick_top_left",
        (false, _, true, false) => "stone_brick_top_right",
        (false, _, false, false) => "stone_brick_top",
        // Left/right edges
        (true, _, false, true) => "stone_brick_left",
        (true, _, true, false) => "stone_brick_right",
        // Bottom corners
        (true, false, false, false) => "stone_brick_inner",
        // Fully surrounded or other
        _ => "stone_brick_inner",
    };
    sheet.get_or_default(name)
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
pub fn sync_platformer_scene(
    scene: &mut Scene,
    active: &ActiveGame,
    theme: &Theme,
    _dt: f32,
    camera_x: f32,
    time: f32,
) {
    let state: Option<breakpoint_platformer::PlatformerState> = read_game_state(active);
    let Some(state) = state else {
        return;
    };

    scene.clear();

    // Parallax background layers
    add_parallax_layers(scene, camera_x);

    let tile_size = breakpoint_platformer::physics::TILE_SIZE;
    let white = Vec4::ONE;

    // Tile culling: only render visible columns
    let visible_half = 15.0;
    let min_col = ((camera_x - visible_half) / tile_size).floor().max(0.0) as u32;
    let max_col = ((camera_x + visible_half) / tile_size)
        .ceil()
        .min(state.course.width as f32) as u32;

    // Render course tiles
    render_tiles(scene, &state, tile_size, white, min_col, max_col, time);

    // Render enemies
    render_enemies(scene, &state, tile_size, theme, time);

    // Render enemy projectiles
    render_projectiles(scene, &state, tile_size, time);

    // Render players
    render_players(scene, &state, tile_size, white, time);

    // Render uncollected powerups
    render_powerups(scene, &state, tile_size, white);
}

/// Render course tiles within the visible column range.
fn render_tiles(
    scene: &mut Scene,
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    white: Vec4,
    min_col: u32,
    max_col: u32,
    time: f32,
) {
    for y in 0..state.course.height {
        for x in min_col..max_col {
            let tile = state.course.get_tile(x as i32, y as i32);
            let Some(region) = tile_sprite_region(&tile, &state.course, x as i32, y as i32, time)
            else {
                continue;
            };
            let wx = x as f32 * tile_size + tile_size / 2.0;
            let wy = y as f32 * tile_size + tile_size / 2.0;
            add_sprite_region(
                scene,
                &region,
                &SpriteParams {
                    x: wx,
                    y: wy,
                    w: tile_size,
                    h: tile_size,
                    tint: white,
                    flip_x: false,
                },
            );
        }
    }
}

/// Render enemies with animation-driven sprites and death effects.
fn render_enemies(
    scene: &mut Scene,
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    theme: &Theme,
    _time: f32,
) {
    let enemy_tint = Vec4::new(
        theme.platformer.enemy_tint[0],
        theme.platformer.enemy_tint[1],
        theme.platformer.enemy_tint[2],
        1.0,
    );
    for enemy in &state.enemies {
        let Some(region) = enemy_sprite_region(
            &enemy.enemy_type,
            enemy.anim_time,
            enemy.alive,
            enemy.respawn_timer,
        ) else {
            continue;
        };
        // Fade out dying enemies
        let tint = if !enemy.alive {
            let death_time = breakpoint_platformer::enemies::RESPAWN_DELAY - enemy.respawn_timer;
            let alpha = (1.0 - death_time / 0.6).max(0.0);
            Vec4::new(enemy_tint.x, enemy_tint.y, enemy_tint.z, alpha)
        } else {
            enemy_tint
        };
        add_sprite_region(
            scene,
            &region,
            &SpriteParams {
                x: enemy.x,
                y: enemy.y,
                w: tile_size,
                h: tile_size * 2.0,
                tint,
                flip_x: !enemy.facing_right,
            },
        );
    }
}

/// Render enemy projectiles with animation.
fn render_projectiles(
    scene: &mut Scene,
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    time: f32,
) {
    let anims = animations();
    for proj in &state.projectiles {
        let region = anims
            .get("projectile")
            .map(|a| *a.frame_at(time))
            .unwrap_or_else(|| atlas().get_or_default("projectile_0"));
        add_sprite_region(
            scene,
            &region,
            &SpriteParams {
                x: proj.x,
                y: proj.y,
                w: tile_size * 0.5,
                h: tile_size * 0.5,
                tint: Vec4::new(1.0, 0.3, 0.9, 1.0),
                flip_x: false,
            },
        );
    }
}

/// Render players with animation-based sprites, VFX, and HP hearts.
fn render_players(
    scene: &mut Scene,
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    white: Vec4,
    time: f32,
) {
    for (pid, player) in &state.players {
        if player.eliminated {
            continue;
        }

        // Death/respawn: fade-in during last 0.3s before respawn
        if player.death_respawn_timer > 0.0 {
            render_death_respawn(scene, player, tile_size);
            continue;
        }

        // Golden pulsing tint during invincibility (instead of blink-skip)
        let inv_tint = if player.invincibility_timer > 0.0 {
            let alpha = 0.5 + 0.3 * (player.invincibility_timer * 8.0).sin();
            Some(Vec4::new(1.0, 0.9, 0.5, alpha))
        } else {
            None
        };

        let region = player_sprite_region(&player.anim_state, player.anim_time);
        let base_tint = Vec4::new(
            ((*pid * 37) % 255) as f32 / 255.0 * 0.5 + 0.5,
            ((*pid * 73) % 255) as f32 / 255.0 * 0.5 + 0.5,
            ((*pid * 113) % 255) as f32 / 255.0 * 0.5 + 0.5,
            1.0,
        );
        let tint = inv_tint.unwrap_or(base_tint);

        add_sprite_region(
            scene,
            &region,
            &SpriteParams {
                x: player.x,
                y: player.y,
                w: tile_size,
                h: tile_size * 2.0,
                tint,
                flip_x: !player.facing_right,
            },
        );

        render_player_effects(scene, player, tile_size, time);
        render_player_hearts(scene, player, *pid, tile_size, white);
    }
}

/// Render death/respawn transition: fade-in during last 0.3s before respawn.
fn render_death_respawn(
    scene: &mut Scene,
    player: &breakpoint_platformer::physics::PlatformerPlayerState,
    tile_size: f32,
) {
    if player.death_respawn_timer >= 0.3 {
        return; // Still fully dead, don't render
    }
    let fade_alpha = 1.0 - (player.death_respawn_timer / 0.3);
    let region = player_sprite_region(&player.anim_state, player.anim_time);
    add_sprite_region(
        scene,
        &region,
        &SpriteParams {
            x: player.x,
            y: player.y,
            w: tile_size,
            h: tile_size * 2.0,
            tint: Vec4::new(1.0, 1.0, 1.0, fade_alpha),
            flip_x: !player.facing_right,
        },
    );
}

/// Render VFX for a player: attack trail, speed boots trail, invincibility glow.
fn render_player_effects(
    scene: &mut Scene,
    player: &breakpoint_platformer::physics::PlatformerPlayerState,
    tile_size: f32,
    time: f32,
) {
    use breakpoint_platformer::physics::AnimState;
    use breakpoint_platformer::powerups::PowerUpKind;

    // Attack trail: 3 semi-transparent afterimage quads trailing behind
    if player.anim_state == AnimState::Attack {
        let region = player_sprite_region(&player.anim_state, player.anim_time);
        let dir = if player.facing_right { -1.0 } else { 1.0 };
        for i in 1..=3u8 {
            let offset = f32::from(i) * tile_size * 0.15 * dir;
            let alpha = 0.3 - f32::from(i) * 0.08;
            add_sprite_region(
                scene,
                &region,
                &SpriteParams {
                    x: player.x + offset,
                    y: player.y,
                    w: tile_size,
                    h: tile_size * 2.0,
                    tint: Vec4::new(1.0, 0.8, 0.4, alpha),
                    flip_x: !player.facing_right,
                },
            );
        }
    }

    // Speed boots trail: 4 trailing afterimages with green tint
    if player.active_powerup == Some(PowerUpKind::SpeedBoots) {
        let region = player_sprite_region(&player.anim_state, player.anim_time);
        let dir = if player.facing_right { -1.0 } else { 1.0 };
        for i in 1..=4u8 {
            let offset = f32::from(i) * tile_size * 0.2 * dir;
            let alpha = (0.25 - f32::from(i) * 0.05).max(0.03);
            add_sprite_region(
                scene,
                &region,
                &SpriteParams {
                    x: player.x + offset,
                    y: player.y,
                    w: tile_size,
                    h: tile_size * 2.0,
                    tint: Vec4::new(0.3, 1.0, 0.4, alpha),
                    flip_x: !player.facing_right,
                },
            );
        }
    }

    // Invincibility glow: pulsing Glow quad behind player
    if player.invincibility_timer > 0.0 {
        let pulse = 0.5 + 0.3 * (time * 6.0).sin();
        scene.add(
            MeshType::Quad,
            MaterialType::Glow {
                color: Vec4::new(1.0, 0.85, 0.3, pulse),
                intensity: 1.5,
            },
            Transform::from_xyz(player.x, player.y, -0.1).with_scale(Vec3::new(
                tile_size * 1.5,
                tile_size * 2.5,
                1.0,
            )),
        );
    }
}

/// Render HP hearts above a player.
fn render_player_hearts(
    scene: &mut Scene,
    player: &breakpoint_platformer::physics::PlatformerPlayerState,
    _pid: u64,
    tile_size: f32,
    white: Vec4,
) {
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

/// Render uncollected powerups.
fn render_powerups(
    scene: &mut Scene,
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    white: Vec4,
) {
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
}

/// Add 3 parallax background layers to the scene.
fn add_parallax_layers(scene: &mut Scene, camera_x: f32) {
    let layer_v = 1.0 / 3.0;

    let layers: [(f32, f32, f32); 3] = [
        (0.1, -5.0, 0.0),
        (0.3, -3.0, layer_v),
        (0.6, -1.0, layer_v * 2.0),
    ];

    for (scroll_factor, z, v_start) in layers {
        scene.add(
            MeshType::Quad,
            MaterialType::Parallax {
                atlas_id: BG_ATLAS_ID,
                layer_rect: Vec4::new(0.0, v_start, 1.0, v_start + layer_v),
                scroll_factor,
                tint: Vec4::ONE,
            },
            Transform::from_xyz(camera_x, 5.0, z).with_scale(Vec3::new(50.0, 30.0, 1.0)),
        );
    }
}
