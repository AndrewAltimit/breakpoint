use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use glam::{Vec3, Vec4};

use crate::scene::{MaterialType, MeshType, Scene, SceneLighting, Transform};
use crate::sprite_atlas::{
    SpriteAnimation, SpriteRegion, SpriteSheet, bitmask_tile_for_group,
    build_platformer_animations, build_platformer_atlas, room_theme_to_tile_group,
    stone_brick_bitmask,
};
use crate::theme::Theme;

/// Predefined player color palettes for multiplayer differentiation.
/// Each entry: (body_r, body_g, body_b) — applied as a tint multiplier.
const PLAYER_PALETTES: [[f32; 3]; 8] = [
    [1.0, 0.85, 0.65], // P1: warm gold/bronze (Belmont-style)
    [0.6, 0.7, 0.9],   // P2: steel blue
    [0.75, 0.2, 0.25], // P3: dark crimson
    [0.3, 0.55, 0.35], // P4: forest green
    [0.55, 0.35, 0.7], // P5: royal purple
    [0.75, 0.75, 0.8], // P6: silver
    [0.9, 0.5, 0.2],   // P7: flame orange
    [0.35, 0.6, 0.6],  // P8: shadow teal
];

/// Sprite atlas ID.
const ATLAS_ID: u8 = 0;

// MBAACC-style Z-layer constants (painter's algorithm).
const Z_BG_TILES: f32 = -1.0;
const Z_WATER: f32 = -0.8;
const Z_SHADOWS: f32 = -0.5;
const Z_ENEMIES: f32 = 0.0;
const Z_PLAYERS: f32 = 0.1;
const Z_EFFECTS: f32 = 0.5;
/// Fog layer Z (used by weather system).
pub const Z_FOG: f32 = 1.0;
const Z_HUD: f32 = 2.0;

/// Per-player visual state for squash/stretch animation.
struct PlayerVisualState {
    prev_anim: breakpoint_platformer::physics::AnimState,
    time_since_transition: f32,
    was_falling: bool,
}

/// Global visual state tracker per player ID.
fn visual_states() -> &'static Mutex<HashMap<u64, PlayerVisualState>> {
    static STATES: OnceLock<Mutex<HashMap<u64, PlayerVisualState>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cached tile sprite data: (SpriteRegion, room_tint_rgb).
/// Avoids per-frame HashMap lookups and bitmask recomputation for static tiles.
struct TileCache {
    /// Flat array indexed by ty * width + tx. None = not cached (animated/water/empty tile).
    entries: Vec<Option<(SpriteRegion, [f32; 3])>>,
    width: u32,
    height: u32,
}

impl TileCache {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            width: 0,
            height: 0,
        }
    }

    /// Get cached data for a tile, or None if not cached.
    fn get(&self, tx: i32, ty: i32) -> Option<&(SpriteRegion, [f32; 3])> {
        if tx < 0 || ty < 0 || tx as u32 >= self.width || ty as u32 >= self.height {
            return None;
        }
        let idx = ty as u32 * self.width + tx as u32;
        self.entries.get(idx as usize).and_then(|e| e.as_ref())
    }

    /// Rebuild the cache for a new course.
    fn rebuild(&mut self, course: &breakpoint_platformer::course_gen::Course) {
        use breakpoint_platformer::course_gen::Tile;
        self.width = course.width;
        self.height = course.height;
        let total = (self.width * self.height) as usize;
        self.entries.clear();
        self.entries.resize(total, None);

        let sheet = atlas();
        for ty in 0..self.height as i32 {
            for tx in 0..self.width as i32 {
                let tile = course.get_tile(tx, ty);
                // Only cache static (non-animated) tile sprite regions
                let region = match &tile {
                    Tile::Empty | Tile::PowerUpSpawn | Tile::Water => continue,
                    Tile::StoneBrick => Some(stone_brick_region(sheet, course, tx, ty)),
                    Tile::Platform => Some(sheet.get_or_default("platform_0")),
                    Tile::Spikes => Some(sheet.get_or_default("spikes_0")),
                    Tile::Checkpoint => Some(sheet.get_or_default("checkpoint_flag_down_0")),
                    Tile::Ladder => Some(sheet.get_or_default("ladder")),
                    Tile::BreakableWall => Some(sheet.get_or_default("breakable_wall_0")),
                    Tile::DecoStainedGlass => Some(sheet.get_or_default("stained_glass")),
                    Tile::DecoCobweb => Some(sheet.get_or_default("cobweb")),
                    // Animated tiles can't be cached (depend on time)
                    Tile::Finish | Tile::DecoTorch | Tile::DecoChain => continue,
                };
                if let Some(region) = region {
                    let rt = room_tile_tint(course.room_theme_at_tile(tx, ty));
                    let idx = (ty as u32 * self.width + tx as u32) as usize;
                    self.entries[idx] = Some((region, rt));
                }
            }
        }
    }
}

/// Global tile cache — rebuilt when the course changes.
fn tile_cache() -> &'static Mutex<TileCache> {
    static CACHE: OnceLock<Mutex<TileCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(TileCache::new()))
}

/// Hit freeze state for anime-style impact pauses.
struct HitFreezeState {
    /// Remaining freeze time in seconds.
    remaining: f32,
    /// Previous frame's enemy alive states, keyed by enemy ID.
    prev_enemy_alive: HashMap<u16, bool>,
}

/// Global hit freeze tracker.
fn hit_freeze() -> &'static Mutex<HitFreezeState> {
    static STATE: OnceLock<Mutex<HitFreezeState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(HitFreezeState {
            remaining: 0.0,
            prev_enemy_alive: HashMap::new(),
        })
    })
}

/// Duration of hit freeze in seconds (~2 frames at 60fps).
const HIT_FREEZE_DURATION: f32 = 0.033;

/// Compute squash/stretch scale for a player based on their movement state.
fn squash_stretch_scale(
    player: &breakpoint_platformer::physics::PlatformerPlayerState,
    pid: u64,
    dt: f32,
) -> (f32, f32) {
    use breakpoint_platformer::physics::AnimState;

    let mut states = visual_states().lock().unwrap_or_else(|e| e.into_inner());
    let vs = states.entry(pid).or_insert_with(|| PlayerVisualState {
        prev_anim: player.anim_state,
        time_since_transition: 0.0,
        was_falling: false,
    });

    // Detect state transitions
    if vs.prev_anim != player.anim_state {
        vs.was_falling = vs.prev_anim == AnimState::Fall;
        vs.prev_anim = player.anim_state;
        vs.time_since_transition = 0.0;
    } else {
        vs.time_since_transition += dt;
    }

    let is_running = player.anim_state == AnimState::Walk
        && player.active_powerup == Some(breakpoint_platformer::powerups::PowerUpKind::SpeedBoots);

    match player.anim_state {
        AnimState::Jump => (0.85, 1.2), // Stretch upward
        AnimState::Fall => (0.9, 1.15), // Slight stretch
        AnimState::Idle if vs.was_falling && vs.time_since_transition < 0.15 => {
            // Landing squash with spring-back
            let t = vs.time_since_transition / 0.15;
            let squash = 1.0 + (1.0 - t) * 0.12;
            let stretch = 1.0 - (1.0 - t) * 0.15;
            (squash, stretch)
        },
        AnimState::Walk if is_running => {
            // More pronounced bob when running
            let bob = (player.anim_time * 16.0).sin() * 0.04;
            (1.0 + bob, 1.0 - bob * 0.5)
        },
        AnimState::Walk => {
            // Subtle sine bob
            let bob = (player.anim_time * 12.0).sin() * 0.03;
            (1.0 + bob, 1.0 - bob)
        },
        _ => (1.0, 1.0),
    }
}

/// Cached sprite sheet — built once on first call.
pub fn atlas() -> &'static SpriteSheet {
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
    z: f32,
    w: f32,
    h: f32,
    tint: Vec4,
    flip_x: bool,
    outline: f32,
    blend_mode: crate::scene::BlendMode,
}

/// Helper: add a sprite quad from a SpriteRegion directly.
fn add_sprite_region(scene: &mut Scene, region: &SpriteRegion, params: &SpriteParams) {
    add_sprite_region_with_dissolve(scene, region, params, 0.0);
}

/// Helper: add a sprite quad with dissolve effect.
/// Non-dissolve sprites are written directly to the scene batch buffer,
/// bypassing RenderObject creation (avoids frustum cull + sort overhead).
fn add_sprite_region_with_dissolve(
    scene: &mut Scene,
    region: &SpriteRegion,
    params: &SpriteParams,
    dissolve: f32,
) {
    if dissolve == 0.0 {
        scene.add_batch_sprite(
            params.x,
            params.y,
            params.z,
            params.w,
            params.h,
            region.to_vec4(),
            params.tint,
            params.flip_x,
            params.outline,
            params.blend_mode,
        );
    } else {
        scene.add(
            MeshType::Quad,
            MaterialType::Sprite {
                atlas_id: ATLAS_ID,
                sprite_rect: region.to_vec4(),
                tint: params.tint,
                flip_x: params.flip_x,
                dissolve,
                outline: params.outline,
                blend_mode: params.blend_mode,
            },
            Transform::from_xyz(params.x, params.y, params.z)
                .with_scale(Vec3::new(params.w, params.h, 1.0)),
        );
    }
}

/// Helper: add a sprite quad by name (defaults: z=Z_BG_TILES, no outline, normal blend).
fn add_sprite(scene: &mut Scene, name: &str, x: f32, y: f32, w: f32, h: f32, tint: Vec4) {
    let region = atlas().get_or_default(name);
    add_sprite_region(
        scene,
        &region,
        &SpriteParams {
            x,
            y,
            z: Z_BG_TILES,
            w,
            h,
            tint,
            flip_x: false,
            outline: 0.0,
            blend_mode: crate::scene::BlendMode::Normal,
        },
    );
}

/// Get the sprite region for a player animation frame.
/// Uses full player state to select contextual animations (run, wall-slide, etc.)
fn player_sprite_region(
    player: &breakpoint_platformer::physics::PlatformerPlayerState,
    course: &breakpoint_platformer::course_gen::Course,
) -> SpriteRegion {
    use breakpoint_platformer::physics::{AnimState, PLAYER_WIDTH, TILE_SIZE};
    use breakpoint_platformer::powerups::PowerUpKind;

    let anims = animations();
    let key = match player.anim_state {
        AnimState::Idle => "player_idle",
        AnimState::Walk => {
            if player.active_powerup == Some(PowerUpKind::SpeedBoots) {
                "player_run"
            } else {
                "player_walk"
            }
        },
        AnimState::Jump => "player_jump",
        AnimState::Fall => {
            // Detect wall-slide: falling while touching a solid wall
            let half_w = PLAYER_WIDTH / 2.0;
            let tx_l = ((player.x - half_w - 0.05) / TILE_SIZE).floor() as i32;
            let tx_r = ((player.x + half_w + 0.05) / TILE_SIZE).floor() as i32;
            let ty = (player.y / TILE_SIZE).floor() as i32;
            let touching_wall = breakpoint_platformer::physics::is_solid(course.get_tile(tx_l, ty))
                || breakpoint_platformer::physics::is_solid(course.get_tile(tx_r, ty));
            if touching_wall && player.vy < -0.5 {
                "player_wall_slide"
            } else {
                "player_fall"
            }
        },
        AnimState::Attack => "player_attack",
        AnimState::Hurt => "player_hurt",
        AnimState::Dead => "player_dead",
    };
    // Fall back to the base key if the contextual animation doesn't exist
    match anims.get(key) {
        Some(anim) => *anim.frame_at(player.anim_time),
        None => {
            // Fallback chain: try base state, then default
            let fallback = match player.anim_state {
                AnimState::Walk => "player_walk",
                AnimState::Fall => "player_fall",
                _ => "player_idle",
            };
            anims
                .get(fallback)
                .map(|a| *a.frame_at(player.anim_time))
                .unwrap_or_else(|| atlas().get_or_default("player_idle_0"))
        },
    }
}

/// Simplified player_sprite_region for cases without course context (death respawn).
fn player_sprite_region_simple(
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
            EnemyType::Ghost => "ghost_death",
            EnemyType::Gargoyle => "gargoyle_death",
        };
        return anims.get(key).map(|a| *a.frame_at(death_time));
    }

    let key = match etype {
        EnemyType::Skeleton => "skeleton_walk",
        EnemyType::Bat => "bat_fly",
        EnemyType::Knight => "knight_walk",
        EnemyType::Medusa => "medusa_float",
        EnemyType::Ghost => "ghost_drift",
        EnemyType::Gargoyle => "gargoyle_perch",
    };
    anims.get(key).map(|a| *a.frame_at(anim_time))
}

/// Map tile type to sprite name, with auto-tiling for stone bricks.
/// Returns true if this tile should be rendered as a water material (not sprite).
fn is_water_tile(tile: &breakpoint_platformer::course_gen::Tile) -> bool {
    matches!(tile, breakpoint_platformer::course_gen::Tile::Water)
}

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
        Tile::Empty | Tile::PowerUpSpawn | Tile::Water => None,
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
        Tile::DecoCobweb => Some(sheet.get_or_default("cobweb")),
        Tile::DecoChain => {
            let phase = tx as f32 * 0.5 + ty as f32 * 1.1;
            let anims = animations();
            anims
                .get("chain")
                .map(|a| *a.frame_at(time + phase))
                .or_else(|| Some(sheet.get_or_default("chain_0")))
        },
    }
}

/// Auto-tile selection for stone bricks: 16-tile bitmask with per-room theme groups.
fn stone_brick_region(
    sheet: &SpriteSheet,
    course: &breakpoint_platformer::course_gen::Course,
    tx: i32,
    ty: i32,
) -> SpriteRegion {
    let mask = stone_brick_bitmask(course, tx, ty);
    let room_theme = course.room_theme_at_tile(tx, ty);
    let group = room_theme_to_tile_group(&room_theme);
    let name = bitmask_tile_for_group(group, mask);
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
    state: &breakpoint_platformer::PlatformerState,
    theme: &Theme,
    dt: f32,
    camera_x: f32,
    camera_y: f32,
    time: f32,
) {
    // Hit freeze: detect enemy kills and pause rendering for impact weight.
    {
        let mut freeze = hit_freeze().lock().unwrap_or_else(|e| e.into_inner());
        if freeze.remaining > 0.0 {
            freeze.remaining -= dt;
            if freeze.remaining > 0.0 {
                // Keep previous frame's scene — don't clear or rebuild.
                return;
            }
        }
        // Check for new enemy kills (alive→dead transitions).
        let mut triggered = false;
        for enemy in &state.enemies {
            let was_alive = freeze
                .prev_enemy_alive
                .get(&enemy.id)
                .copied()
                .unwrap_or(true);
            if was_alive && !enemy.alive {
                triggered = true;
            }
            freeze.prev_enemy_alive.insert(enemy.id, enemy.alive);
        }
        if triggered {
            freeze.remaining = HIT_FREEZE_DURATION;
        }
    }

    scene.clear();

    let tile_size = breakpoint_platformer::physics::TILE_SIZE;

    // Rebuild tile cache if course changed (new game or first frame)
    {
        let mut cache = tile_cache().lock().unwrap_or_else(|e| e.into_inner());
        if cache.width != state.course.width || cache.height != state.course.height {
            cache.rebuild(&state.course);
        }
    }

    // Determine camera room theme for background and lighting
    let _camera_theme = state
        .course
        .room_theme_at_tile((camera_x / tile_size) as i32, (camera_y / tile_size) as i32);

    let white = Vec4::ONE;

    // Tile culling: only render visible columns and rows.
    // Camera is at z=20, FOV=45°: visible half-width ≈ 15.5, half-height ≈ 8.7 at z=0.
    // Add 2-tile margin for smooth scrolling.
    let visible_half_x = 17.0;
    let visible_half_y = 11.0;
    let min_col = ((camera_x - visible_half_x) / tile_size).floor().max(0.0) as u32;
    let max_col = ((camera_x + visible_half_x) / tile_size)
        .ceil()
        .min(state.course.width as f32) as u32;
    let min_row = ((camera_y - visible_half_y) / tile_size).floor().max(0.0) as u32;
    let max_row = ((camera_y + visible_half_y) / tile_size)
        .ceil()
        .min(state.course.height as f32) as u32;

    // Collect torch lights for dynamic lighting
    scene.lighting = collect_torch_lights(
        state,
        tile_size,
        min_col,
        max_col,
        min_row,
        max_row,
        time,
        theme.platformer.torch_ambient,
    );

    // Render course tiles
    let wc = &theme.platformer.water_color;
    let water_color = Vec4::new(wc[0], wc[1], wc[2], wc[3]);
    render_tiles(
        scene,
        state,
        tile_size,
        min_col,
        max_col,
        min_row,
        max_row,
        time,
        water_color,
    );

    // God rays for Chapel rooms (from stained glass light sources)
    render_godrays(scene, state, tile_size, camera_x, camera_y);

    // Render enemies
    render_enemies(scene, state, tile_size, theme, time);

    // Render enemy projectiles
    render_projectiles(scene, state, tile_size, time);

    // Render players
    render_players(scene, state, tile_size, white, time, dt);

    // Render uncollected powerups
    render_powerups(scene, state, tile_size, white);
}

/// Per-room tile tint for atmospheric coloring of stone/brick surfaces.
fn room_tile_tint(theme: breakpoint_platformer::course_gen::RoomTheme) -> [f32; 3] {
    use breakpoint_platformer::course_gen::RoomTheme;
    match theme {
        // Castle Interior rooms: gray-mauve stone (>1.0 boosts sprite brightness)
        RoomTheme::Entrance
        | RoomTheme::Corridor
        | RoomTheme::GreatHall
        | RoomTheme::ThroneRoom => [1.20, 1.10, 1.25],
        // Underground rooms: teal-green stone
        RoomTheme::Crypt | RoomTheme::Dungeon => [0.90, 1.10, 1.00],
        // Sacred rooms: warm sandstone
        RoomTheme::Chapel | RoomTheme::Library => [1.30, 1.15, 0.95],
        // Fortress rooms: blue-gray steel
        RoomTheme::Armory | RoomTheme::Tower => [1.05, 1.05, 1.15],
    }
}

/// Render course tiles within the visible column and row range.
#[allow(clippy::too_many_arguments)]
fn render_tiles(
    scene: &mut Scene,
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    min_col: u32,
    max_col: u32,
    min_row: u32,
    max_row: u32,
    time: f32,
    water_color: Vec4,
) {
    let cache = tile_cache().lock().unwrap_or_else(|e| e.into_inner());

    for y in min_row..max_row {
        for x in min_col..max_col {
            let tx = x as i32;
            let ty = y as i32;
            let tile = state.course.get_tile(tx, ty);

            // Water tiles use a special material
            if is_water_tile(&tile) {
                let wx = x as f32 * tile_size + tile_size / 2.0;
                let wy = y as f32 * tile_size + tile_size / 2.0;
                let above = state.course.get_tile(tx, ty + 1);
                let depth = if is_water_tile(&above) { 0.8 } else { 0.4 };
                scene.add(
                    MeshType::Quad,
                    MaterialType::Water {
                        color: water_color,
                        depth,
                        wave_speed: 3.0,
                    },
                    Transform::from_xyz(wx, wy, Z_WATER)
                        .with_scale(Vec3::new(tile_size, tile_size, 1.0)),
                );
                continue;
            }

            let wx = x as f32 * tile_size + tile_size / 2.0;
            let wy = y as f32 * tile_size + tile_size / 2.0;

            // Try cache first (static tiles: stone, platform, spikes, etc.)
            if let Some((region, rt)) = cache.get(tx, ty) {
                let tint = Vec4::new(rt[0], rt[1], rt[2], 1.0);
                add_sprite_region(
                    scene,
                    region,
                    &SpriteParams {
                        x: wx,
                        y: wy,
                        z: Z_BG_TILES,
                        w: tile_size,
                        h: tile_size,
                        tint,
                        flip_x: false,
                        outline: 0.0,
                        blend_mode: crate::scene::BlendMode::Normal,
                    },
                );
                continue;
            }

            // Animated or uncached tiles: compute per-frame
            let Some(region) = tile_sprite_region(&tile, &state.course, tx, ty, time) else {
                continue;
            };
            let rt = room_tile_tint(state.course.room_theme_at_tile(tx, ty));
            let tint = Vec4::new(rt[0], rt[1], rt[2], 1.0);
            add_sprite_region(
                scene,
                &region,
                &SpriteParams {
                    x: wx,
                    y: wy,
                    z: Z_BG_TILES,
                    w: tile_size,
                    h: tile_size,
                    tint,
                    flip_x: false,
                    outline: 0.0,
                    blend_mode: crate::scene::BlendMode::Normal,
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
        // Dissolve dying enemies instead of simple alpha fade
        let (tint, dissolve) = if !enemy.alive {
            let death_time = breakpoint_platformer::enemies::RESPAWN_DELAY - enemy.respawn_timer;
            let dissolve_amount = (death_time / 0.6).clamp(0.0, 1.0);
            (enemy_tint, dissolve_amount)
        } else {
            (enemy_tint, 0.0)
        };
        // Shadow underneath enemy
        add_sprite_region(
            scene,
            &region,
            &SpriteParams {
                x: enemy.x,
                y: enemy.y - tile_size * 0.4,
                z: Z_SHADOWS,
                w: tile_size * 1.2,
                h: tile_size * 2.0 * 0.3,
                tint: Vec4::new(0.0, 0.0, 0.0, 0.35),
                flip_x: !enemy.facing_right,
                outline: 0.0,
                blend_mode: crate::scene::BlendMode::Normal,
            },
        );
        // Enemy sprite
        add_sprite_region_with_dissolve(
            scene,
            &region,
            &SpriteParams {
                x: enemy.x,
                y: enemy.y,
                z: Z_ENEMIES,
                w: tile_size,
                h: tile_size * 2.0,
                tint,
                flip_x: !enemy.facing_right,
                outline: 1.0,
                blend_mode: crate::scene::BlendMode::Normal,
            },
            dissolve,
        );
    }
}

/// Render enemy projectiles with trailing afterimages and glow.
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

        // Trailing afterimages (3 behind projectile direction)
        let dx = proj.vx.signum();
        for i in 1..=3u8 {
            let offset = f32::from(i) * tile_size * 0.15 * -dx;
            let alpha = 0.25 - f32::from(i) * 0.07;
            add_sprite_region(
                scene,
                &region,
                &SpriteParams {
                    x: proj.x + offset,
                    y: proj.y,
                    z: Z_EFFECTS,
                    w: tile_size * 0.5,
                    h: tile_size * 0.5,
                    tint: Vec4::new(0.4, 0.9, 0.3, alpha),
                    flip_x: false,
                    outline: 0.0,
                    blend_mode: crate::scene::BlendMode::Additive,
                },
            );
        }

        // Glow aura behind projectile
        scene.add(
            MeshType::Quad,
            MaterialType::Glow {
                color: Vec4::new(0.3, 0.7, 0.2, 0.4),
                intensity: 1.2,
            },
            Transform::from_xyz(proj.x, proj.y, -0.05).with_scale(Vec3::new(
                tile_size * 0.8,
                tile_size * 0.8,
                1.0,
            )),
        );

        // Main projectile sprite
        add_sprite_region(
            scene,
            &region,
            &SpriteParams {
                x: proj.x,
                y: proj.y,
                z: Z_EFFECTS,
                w: tile_size * 0.5,
                h: tile_size * 0.5,
                tint: Vec4::new(0.4, 0.9, 0.3, 1.0),
                flip_x: false,
                outline: 0.0,
                blend_mode: crate::scene::BlendMode::Normal,
            },
        );
    }
}

/// Get per-player color palette tint based on player index.
fn player_palette(pid: u64) -> Vec4 {
    let idx = (pid as usize) % PLAYER_PALETTES.len();
    let [r, g, b] = PLAYER_PALETTES[idx];
    Vec4::new(r, g, b, 1.0)
}

/// Render players with animation-based sprites, VFX, and HP hearts.
fn render_players(
    scene: &mut Scene,
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    white: Vec4,
    time: f32,
    dt: f32,
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

        let region = player_sprite_region(player, &state.course);
        let base_tint = player_palette(*pid);
        let tint = inv_tint.unwrap_or(base_tint);

        // Squash/stretch scaling based on movement state
        let (sx, sy) = squash_stretch_scale(player, *pid, dt);

        // Shadow underneath player
        add_sprite_region(
            scene,
            &region,
            &SpriteParams {
                x: player.x,
                y: player.y - tile_size * 0.4,
                z: Z_SHADOWS,
                w: tile_size * 1.2,
                h: tile_size * 2.0 * 0.3,
                tint: Vec4::new(0.0, 0.0, 0.0, 0.35),
                flip_x: !player.facing_right,
                outline: 0.0,
                blend_mode: crate::scene::BlendMode::Normal,
            },
        );

        // 16x32 sprites: render at 2.0x tile height
        add_sprite_region(
            scene,
            &region,
            &SpriteParams {
                x: player.x,
                y: player.y,
                z: Z_PLAYERS,
                w: tile_size * sx,
                h: tile_size * 2.0 * sy,
                tint,
                flip_x: !player.facing_right,
                outline: 1.0,
                blend_mode: crate::scene::BlendMode::Normal,
            },
        );

        render_player_effects(scene, player, tile_size, time, &state.course);
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
    let region = player_sprite_region_simple(&player.anim_state, player.anim_time);
    add_sprite_region(
        scene,
        &region,
        &SpriteParams {
            x: player.x,
            y: player.y,
            z: Z_PLAYERS,
            w: tile_size,
            h: tile_size * 2.0,
            tint: Vec4::new(1.0, 1.0, 1.0, fade_alpha),
            flip_x: !player.facing_right,
            outline: 1.0,
            blend_mode: crate::scene::BlendMode::Normal,
        },
    );
}

/// Render VFX for a player: attack trail, speed boots trail, invincibility glow.
fn render_player_effects(
    scene: &mut Scene,
    player: &breakpoint_platformer::physics::PlatformerPlayerState,
    tile_size: f32,
    time: f32,
    course: &breakpoint_platformer::course_gen::Course,
) {
    use breakpoint_platformer::physics::AnimState;
    use breakpoint_platformer::powerups::PowerUpKind;

    // Anime-style slash arc during attack
    if player.anim_state == AnimState::Attack {
        let attack_duration = 0.35; // matches game ATTACK_DURATION
        let progress = (player.anim_time / attack_duration).clamp(0.0, 1.0);
        let dir = if player.facing_right { 1.0 } else { -1.0 };
        let angle = if player.facing_right {
            -0.5 // Sweep from upper-right
        } else {
            std::f32::consts::PI - 0.5 // Mirrored for left-facing
        };
        scene.add(
            MeshType::Quad,
            MaterialType::SlashArc {
                progress,
                angle,
                color: Vec4::new(1.0, 0.7, 0.3, 0.9),
            },
            Transform::from_xyz(
                player.x + dir * tile_size * 0.5,
                player.y + tile_size * 0.2,
                0.15,
            )
            .with_scale(Vec3::new(tile_size * 2.2, tile_size * 2.2, 1.0)),
        );
    }

    // Magic circle when activating Holy Water or Crucifix power-ups
    if player.powerup_timer > 0.0 {
        let is_magic_powerup = matches!(
            player.active_powerup,
            Some(PowerUpKind::HolyWater) | Some(PowerUpKind::Crucifix)
        );
        if is_magic_powerup {
            let circle_color = match player.active_powerup {
                Some(PowerUpKind::HolyWater) => Vec4::new(0.15, 0.4, 0.9, 0.7),
                Some(PowerUpKind::Crucifix) => Vec4::new(1.0, 0.9, 0.4, 0.8),
                _ => Vec4::new(1.0, 1.0, 1.0, 0.5),
            };
            scene.add(
                MeshType::Quad,
                MaterialType::MagicCircle {
                    rotation: time * 2.0,
                    pulse: (time * 4.0).sin() * 0.5 + 0.5,
                    color: circle_color,
                },
                Transform::from_xyz(player.x, player.y - tile_size * 0.3, 0.12)
                    .with_scale(Vec3::new(tile_size * 2.0, tile_size * 2.0, 1.0)),
            );
        }
    }

    // Speed boots trail: 4 trailing afterimages with green tint
    if player.active_powerup == Some(PowerUpKind::SpeedBoots) {
        let region = player_sprite_region(player, course);
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
                    z: Z_EFFECTS,
                    w: tile_size,
                    h: tile_size * 2.0,
                    tint: Vec4::new(1.0, 0.6, 0.2, alpha),
                    flip_x: !player.facing_right,
                    outline: 0.0,
                    blend_mode: crate::scene::BlendMode::Additive,
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
                tile_size * 3.0,
                1.0,
            )),
        );
    }
}

/// Render animated health bar above a player (replacing floating hearts).
fn render_player_hearts(
    scene: &mut Scene,
    player: &breakpoint_platformer::physics::PlatformerPlayerState,
    pid: u64,
    tile_size: f32,
    _white: Vec4,
) {
    if player.max_hp == 0 {
        return;
    }
    let fill = player.hp as f32 / player.max_hp as f32;
    let bar_y = player.y + tile_size * 1.6;
    let bar_w = tile_size * 1.0;
    let bar_h = tile_size * 0.15;
    // Color based on fill level: green -> yellow -> red
    let bar_color = if fill > 0.6 {
        Vec4::new(0.3, 0.9, 0.3, 0.9)
    } else if fill > 0.3 {
        Vec4::new(0.9, 0.8, 0.2, 0.9)
    } else {
        Vec4::new(0.9, 0.2, 0.2, 0.9)
    };
    // Tint with player palette
    let palette = player_palette(pid);
    let final_color = Vec4::new(
        bar_color.x * palette.x,
        bar_color.y * palette.y,
        bar_color.z * palette.z,
        bar_color.w,
    );
    scene.add(
        MeshType::Quad,
        MaterialType::HealthBar {
            fill,
            color: final_color,
        },
        Transform::from_xyz(player.x, bar_y, Z_HUD).with_scale(Vec3::new(bar_w, bar_h, 1.0)),
    );
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

/// Render god rays for Chapel rooms (stained glass light beams).
fn render_godrays(
    scene: &mut Scene,
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    camera_x: f32,
    camera_y: f32,
) {
    use breakpoint_platformer::course_gen::{RoomTheme, Tile};

    // Only render god rays in Chapel rooms
    let center_theme = state
        .course
        .room_theme_at_tile((camera_x / tile_size) as i32, (camera_y / tile_size) as i32);
    if !matches!(center_theme, RoomTheme::Chapel) {
        return;
    }

    // Find torch positions in the visible area (these act as stained glass windows)
    let half_x = 12.0;
    let half_y = 10.0;
    let min_col = ((camera_x - half_x) / tile_size).floor().max(0.0) as u32;
    let max_col = ((camera_x + half_x) / tile_size)
        .ceil()
        .min(state.course.width as f32) as u32;
    let min_row = ((camera_y - half_y) / tile_size).floor().max(0.0) as u32;
    let max_row = ((camera_y + half_y) / tile_size)
        .ceil()
        .min(state.course.height as f32) as u32;

    let mut count = 0u8;
    for y in min_row..max_row {
        for x in min_col..max_col {
            if count >= 4 {
                return;
            }
            if state.course.get_tile(x as i32, y as i32) == Tile::DecoTorch {
                let wx = x as f32 * tile_size + tile_size / 2.0;
                let wy = y as f32 * tile_size + tile_size / 2.0;
                // God ray quad below the window, angled down
                scene.add(
                    MeshType::Quad,
                    MaterialType::GodRays {
                        intensity: 0.3,
                        color: Vec4::new(1.0, 0.9, 0.6, 0.25),
                    },
                    Transform::from_xyz(wx, wy - tile_size * 2.0, 0.15).with_scale(Vec3::new(
                        tile_size * 3.0,
                        tile_size * 6.0,
                        1.0,
                    )),
                );
                count += 1;
            }
        }
    }
}

/// Per-room ambient color (RGB) for atmospheric tinting.
fn room_ambient_color(theme: breakpoint_platformer::course_gen::RoomTheme) -> [f32; 3] {
    use breakpoint_platformer::course_gen::RoomTheme;
    match theme {
        RoomTheme::Entrance => [0.90, 0.78, 0.55],   // warm amber
        RoomTheme::Corridor => [0.72, 0.70, 0.68],   // dim stone
        RoomTheme::GreatHall => [0.85, 0.72, 0.50],  // golden
        RoomTheme::Library => [0.78, 0.68, 0.50],    // warm brown
        RoomTheme::Armory => [0.78, 0.50, 0.32],     // forge red
        RoomTheme::Chapel => [0.85, 0.75, 0.50],     // sacred gold
        RoomTheme::Crypt => [0.50, 0.55, 0.85],      // cold blue
        RoomTheme::Tower => [0.75, 0.75, 0.85],      // open sky
        RoomTheme::Dungeon => [0.55, 0.58, 0.48],    // sickly green
        RoomTheme::ThroneRoom => [0.68, 0.45, 0.85], // royal purple
    }
}

/// Per-room torch light color (RGB) for colored fire.
fn torch_light_color(theme: breakpoint_platformer::course_gen::RoomTheme) -> [f32; 3] {
    use breakpoint_platformer::course_gen::RoomTheme;
    match theme {
        RoomTheme::Armory => [1.0, 0.5, 0.2],     // forge orange
        RoomTheme::Crypt => [0.4, 0.5, 1.0],      // ghostly blue
        RoomTheme::Chapel => [1.0, 0.9, 0.6],     // warm candlelight
        RoomTheme::Dungeon => [0.5, 0.8, 0.3],    // sickly green
        RoomTheme::ThroneRoom => [0.8, 0.5, 1.0], // royal purple
        _ => [1.0, 0.65, 0.3],                    // distinctly orange fire
    }
}

/// Per-room color grading: (shadow_tint, highlight_tint, contrast, saturation).
fn room_color_grading(
    theme: breakpoint_platformer::course_gen::RoomTheme,
) -> ([f32; 3], [f32; 3], f32, f32) {
    use breakpoint_platformer::course_gen::RoomTheme;
    // Neutral contrast (1.0), GBA-saturated (1.05), lighter shadow tints to preserve dark detail
    match theme {
        RoomTheme::Entrance => ([0.90, 0.85, 0.78], [1.0, 0.9, 0.75], 1.0, 1.05),
        RoomTheme::Corridor => ([0.85, 0.85, 0.90], [0.9, 0.88, 0.95], 1.0, 1.0),
        RoomTheme::GreatHall => ([0.90, 0.85, 0.78], [1.0, 0.9, 0.75], 1.0, 1.05),
        RoomTheme::Library => ([0.88, 0.84, 0.76], [1.0, 0.85, 0.7], 1.0, 1.05),
        RoomTheme::Armory => ([0.92, 0.78, 0.72], [1.0, 0.75, 0.6], 1.0, 1.05),
        RoomTheme::Chapel => ([0.90, 0.86, 0.78], [1.0, 0.95, 0.8], 1.0, 1.05),
        RoomTheme::Crypt => ([0.78, 0.82, 0.92], [0.8, 0.85, 1.0], 1.0, 1.0),
        RoomTheme::Tower => ([0.86, 0.86, 0.92], [0.95, 0.95, 1.0], 1.0, 1.05),
        RoomTheme::Dungeon => ([0.82, 0.84, 0.78], [0.85, 0.9, 0.8], 1.0, 1.0),
        RoomTheme::ThroneRoom => ([0.86, 0.78, 0.92], [0.9, 0.75, 1.0], 1.0, 1.05),
    }
}

/// Per-room ambient particle type for atmospheric effects.
pub fn room_theme_ambient_type(
    theme: breakpoint_platformer::course_gen::RoomTheme,
) -> crate::weather::AmbientType {
    use crate::weather::AmbientType;
    use breakpoint_platformer::course_gen::RoomTheme;
    match theme {
        RoomTheme::Entrance | RoomTheme::Corridor | RoomTheme::GreatHall => AmbientType::DustMotes,
        RoomTheme::Crypt | RoomTheme::Dungeon => AmbientType::DustMotes,
        RoomTheme::Chapel => AmbientType::GoldenSparkles,
        RoomTheme::Armory => AmbientType::Embers,
        RoomTheme::Tower => AmbientType::Snowflakes,
        RoomTheme::Library => AmbientType::FloatingPages,
        RoomTheme::ThroneRoom => AmbientType::Embers,
    }
}

/// Per-room weather configuration: (raining, fog_density, fog_color_rgb).
pub fn room_theme_weather(
    theme: breakpoint_platformer::course_gen::RoomTheme,
) -> (bool, f32, [f32; 3]) {
    use breakpoint_platformer::course_gen::RoomTheme;
    match theme {
        RoomTheme::Tower => (true, 0.0, [0.10, 0.10, 0.15]), // open sky — rain, no fog
        RoomTheme::Crypt => (false, 0.6, [0.08, 0.10, 0.18]), // thick cold blue fog
        RoomTheme::Dungeon => (false, 0.4, [0.10, 0.12, 0.08]), // sickly green fog
        RoomTheme::Corridor => (false, 0.25, [0.10, 0.08, 0.12]), // misty purple haze
        RoomTheme::Entrance => (false, 0.1, [0.12, 0.10, 0.08]), // warm haze
        RoomTheme::GreatHall => (false, 0.1, [0.12, 0.10, 0.08]), // warm haze
        _ => (false, 0.0, [0.08, 0.06, 0.12]),               // default dark purple
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_torch_lights(
    state: &breakpoint_platformer::PlatformerState,
    tile_size: f32,
    min_col: u32,
    max_col: u32,
    min_row: u32,
    max_row: u32,
    time: f32,
    torch_ambient: f32,
) -> SceneLighting {
    use breakpoint_platformer::course_gen::Tile;

    let mut lights: Vec<[f32; 4]> = Vec::with_capacity(32);
    let mut light_colors: Vec<[f32; 4]> = Vec::with_capacity(32);

    // Determine the dominant room theme near the camera center
    let center_col = (min_col + max_col) / 2;
    let center_row = (min_row + max_row) / 2;
    let center_theme = state
        .course
        .room_theme_at_tile(center_col as i32, center_row as i32);

    let torch_rgb = torch_light_color(center_theme);

    for y in min_row..max_row {
        for x in min_col..max_col {
            if lights.len() >= 32 {
                break;
            }
            if state.course.get_tile(x as i32, y as i32) == Tile::DecoTorch {
                let wx = x as f32 * tile_size + tile_size / 2.0;
                let wy = y as f32 * tile_size + tile_size / 2.0;
                // Per-torch flicker using position hash
                let hash = (x as f32) * 7.3 + (y as f32) * 13.1;
                let intensity = 1.8 + 0.4 * (time * 8.0 + hash).sin();
                let radius = 14.0;
                lights.push([wx, wy, intensity, radius]);
                light_colors.push([torch_rgb[0], torch_rgb[1], torch_rgb[2], 0.0]);
            }
        }
    }

    // Dark atmosphere when torches are present, fully lit otherwise
    let ambient = if lights.is_empty() {
        1.0
    } else {
        torch_ambient
    };

    let ambient_color = room_ambient_color(center_theme);
    let (grade_shadows, grade_highlights, grade_contrast, saturation) =
        room_color_grading(center_theme);

    let (_, _, fog_color) = room_theme_weather(center_theme);

    SceneLighting {
        lights,
        light_colors,
        ambient,
        ambient_color,
        grade_shadows,
        grade_highlights,
        grade_contrast,
        saturation,
        // GBA Castlevania color ramp: mauve shadows → bronze midtones → gold highlights
        ramp_shadow: [0.40, 0.35, 0.48],
        ramp_mid: [0.85, 0.68, 0.45],
        ramp_highlight: [1.0, 0.90, 0.35],
        posterize: 48.0, // Subtle banding for GBA-style look
        fog_color,
    }
}

/// 6-layer parallax configuration: (scroll_factor, z_depth, v_start, v_height, alpha).
#[allow(dead_code)]
const PARALLAX_LAYERS: [(f32, f32, f32, f32, f32); 6] = [
    (0.05, -6.0, 0.0, 1.0 / 6.0, 0.45),       // Layer 0: far sky/void
    (0.15, -5.0, 1.0 / 6.0, 1.0 / 6.0, 0.55), // Layer 1: distant architecture
    (0.3, -3.5, 2.0 / 6.0, 1.0 / 6.0, 0.65),  // Layer 2: mid architecture
    (0.6, -1.5, 3.0 / 6.0, 1.0 / 6.0, 0.75),  // Layer 3: near architecture
    (1.2, 0.4, 4.0 / 6.0, 1.0 / 6.0, 0.12),   // Layer 4: foreground pillars
    (1.5, 0.5, 5.0 / 6.0, 1.0 / 6.0, 0.08),   // Layer 5: close dust/mist
];

/// Add parallax background layers (6 layers) to the scene.
/// Scrolls in both X and Y directions for 2D exploration.
#[allow(dead_code)]
fn add_parallax_layers(
    scene: &mut Scene,
    camera_x: f32,
    camera_y: f32,
    camera_theme: breakpoint_platformer::course_gen::RoomTheme,
) {
    // Use the main sprite atlas (ID 0) — background content is there
    let atlas = ATLAS_ID;

    // Pull parallax tint from the room's atmospheric theme
    let ambient = room_ambient_color(camera_theme);

    for &(scroll_factor, z, v_start, v_height, alpha) in &PARALLAX_LAYERS {
        let layer_y = camera_y * scroll_factor + 5.0 * (1.0 - scroll_factor);
        let tint = Vec4::new(ambient[0], ambient[1], ambient[2], alpha);
        scene.add(
            MeshType::Quad,
            MaterialType::Parallax {
                atlas_id: atlas,
                layer_rect: Vec4::new(0.0, v_start, 1.0, v_start + v_height),
                scroll_factor,
                tint,
            },
            Transform::from_xyz(camera_x, layer_y, z).with_scale(Vec3::new(50.0, 40.0, 1.0)),
        );
    }
}
