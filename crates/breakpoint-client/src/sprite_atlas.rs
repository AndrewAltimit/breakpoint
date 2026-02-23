use std::collections::HashMap;

use glam::Vec4;

/// UV sub-region within a texture atlas.
#[derive(Debug, Clone, Copy)]
pub struct SpriteRegion {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

impl SpriteRegion {
    /// Convert to Vec4 for shader uniform (u0, v0, u1, v1).
    pub fn to_vec4(self) -> Vec4 {
        Vec4::new(self.u0, self.v0, self.u1, self.v1)
    }
}

/// Named sprite regions within a texture atlas.
pub struct SpriteSheet {
    regions: HashMap<&'static str, SpriteRegion>,
    atlas_width: f32,
    atlas_height: f32,
}

impl SpriteSheet {
    pub fn new(atlas_width: u32, atlas_height: u32) -> Self {
        Self {
            regions: HashMap::new(),
            atlas_width: atlas_width as f32,
            atlas_height: atlas_height as f32,
        }
    }

    /// Add a sprite region by pixel coordinates.
    pub fn add(&mut self, name: &'static str, x: u32, y: u32, w: u32, h: u32) {
        let region = SpriteRegion {
            u0: x as f32 / self.atlas_width,
            v0: y as f32 / self.atlas_height,
            u1: (x + w) as f32 / self.atlas_width,
            v1: (y + h) as f32 / self.atlas_height,
        };
        self.regions.insert(name, region);
    }

    /// Get a sprite region by name.
    pub fn get(&self, name: &str) -> Option<&SpriteRegion> {
        self.regions.get(name)
    }

    /// Get a sprite region, falling back to a 1x1 white pixel region.
    pub fn get_or_default(&self, name: &str) -> SpriteRegion {
        self.regions.get(name).copied().unwrap_or(SpriteRegion {
            u0: 0.0,
            v0: 0.0,
            u1: 1.0 / self.atlas_width,
            v1: 1.0 / self.atlas_height,
        })
    }
}

/// Animation: a sequence of sprite regions with timing.
pub struct SpriteAnimation {
    pub frames: Vec<SpriteRegion>,
    pub frame_duration: f32,
    pub looping: bool,
}

impl SpriteAnimation {
    /// Get the frame region at a given time.
    pub fn frame_at(&self, time: f32) -> &SpriteRegion {
        if self.frames.is_empty() {
            // Should not happen, but return a safe default.
            static DEFAULT: SpriteRegion = SpriteRegion {
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
            };
            return &DEFAULT;
        }
        let total = self.frames.len() as f32 * self.frame_duration;
        let t = if self.looping && total > 0.0 {
            time % total
        } else {
            time.min(total - self.frame_duration)
        };
        let idx = (t / self.frame_duration) as usize;
        let idx = idx.min(self.frames.len() - 1);
        &self.frames[idx]
    }
}

// ================================================================
// Atlas layout constants (1024x512)
// ================================================================

/// Atlas dimensions.
pub const ATLAS_W: u32 = 1024;
pub const ATLAS_H: u32 = 512;

// Y-range assignments:
// 0-63:    Player sprites (16x32) — 2 rows of 64 columns
// 64-159:  Enemy sprites (16x32) — 3 rows of 64 columns
// 160-287: Tile sprites (16x16) — per-theme, 8 rows x 64 cols
// 288-351: UI, props, HUD, power-ups (16x16 & 32x32)
// 352-415: Particles + combat VFX (8x8 & 32x32)
// 416-511: Reserved (caustic tex, foam, LUT strips)

/// Build the platformer sprite sheet with all named regions.
/// Atlas is 1024x512 with 16x32 characters, 16x16 tiles, and 8x8 particles.
pub fn build_platformer_atlas() -> SpriteSheet {
    let mut sheet = SpriteSheet::new(ATLAS_W, ATLAS_H);

    // ── Player sprites (16x32) — Y 0-63 ────────────────────
    add_player_sprites(&mut sheet);

    // ── Enemy sprites (16x32) — Y 64-159 ─────────────────
    add_enemy_sprites(&mut sheet);

    // ── Tile sprites (16x16) — Y 160-287 ─────────────────
    add_tile_sprites(&mut sheet);

    // ── Power-ups, HUD, props (16x16 & 32x32) — Y 288-351 ─
    add_ui_sprites(&mut sheet);

    // ── Particle + VFX sprites (8x8 & 32x32) — Y 352-415 ──
    add_particle_sprites(&mut sheet);

    // ── Combat VFX sprites (32x32) — Y 384-415 ─────────────
    add_combat_vfx_sprites(&mut sheet);

    sheet
}

// ================================================================
// Player sprites (16x32) — Y 0-63
// Row 0 (Y 0-31):  idle(8f), walk(8f), run(8f), jump(4f), fall(4f)
// Row 1 (Y 32-63): attack(8f), hurt(4f), death(6f), wall_slide(3f), crouch(3f), dash(4f)
// ================================================================

fn add_player_sprites(sheet: &mut SpriteSheet) {
    let w = 16u32;
    let h = 32u32;

    // Row 0 (Y=0)
    let mut x = 0u32;

    // Idle: 8 frames
    for i in 0..8u32 {
        let name = player_frame_name("player_idle_", i);
        sheet.add(name, x + i * w, 0, w, h);
    }
    x += 8 * w; // 256

    // Walk: 8 frames
    for i in 0..8u32 {
        let name = player_frame_name("player_walk_", i);
        sheet.add(name, x + i * w, 0, w, h);
    }
    x += 8 * w; // 512

    // Run: 8 frames
    for i in 0..8u32 {
        let name = player_frame_name("player_run_", i);
        sheet.add(name, x + i * w, 0, w, h);
    }
    x += 8 * w; // 768

    // Jump: 4 frames
    for i in 0..4u32 {
        let name = player_frame_name("player_jump_", i);
        sheet.add(name, x + i * w, 0, w, h);
    }
    x += 4 * w; // 896

    // Fall: 4 frames
    for i in 0..4u32 {
        let name = player_frame_name("player_fall_", i);
        sheet.add(name, x + i * w, 0, w, h);
    }

    // Row 1 (Y=64)
    x = 0;

    // Attack: 8 frames
    for i in 0..8u32 {
        let name = player_frame_name("player_attack_", i);
        sheet.add(name, x + i * w, h, w, h);
    }
    x += 8 * w; // 256

    // Hurt: 4 frames
    for i in 0..4u32 {
        let name = player_frame_name("player_hurt_", i);
        sheet.add(name, x + i * w, h, w, h);
    }
    x += 4 * w; // 384

    // Dead: 6 frames
    for i in 0..6u32 {
        let name = player_frame_name("player_dead_", i);
        sheet.add(name, x + i * w, h, w, h);
    }
    x += 6 * w; // 576

    // Wall slide: 3 frames
    for i in 0..3u32 {
        let name = player_frame_name("player_wall_slide_", i);
        sheet.add(name, x + i * w, h, w, h);
    }
    x += 3 * w; // 672

    // Crouch: 3 frames
    for i in 0..3u32 {
        let name = player_frame_name("player_crouch_", i);
        sheet.add(name, x + i * w, h, w, h);
    }
    x += 3 * w; // 768

    // Dash: 4 frames
    for i in 0..4u32 {
        let name = player_frame_name("player_dash_", i);
        sheet.add(name, x + i * w, h, w, h);
    }
}

/// Map index to static frame name for player sprites.
fn player_frame_name(prefix: &str, idx: u32) -> &'static str {
    match prefix {
        "player_idle_" => match idx {
            0 => "player_idle_0",
            1 => "player_idle_1",
            2 => "player_idle_2",
            3 => "player_idle_3",
            4 => "player_idle_4",
            5 => "player_idle_5",
            6 => "player_idle_6",
            _ => "player_idle_7",
        },
        "player_walk_" => match idx {
            0 => "player_walk_0",
            1 => "player_walk_1",
            2 => "player_walk_2",
            3 => "player_walk_3",
            4 => "player_walk_4",
            5 => "player_walk_5",
            6 => "player_walk_6",
            _ => "player_walk_7",
        },
        "player_run_" => match idx {
            0 => "player_run_0",
            1 => "player_run_1",
            2 => "player_run_2",
            3 => "player_run_3",
            4 => "player_run_4",
            5 => "player_run_5",
            6 => "player_run_6",
            _ => "player_run_7",
        },
        "player_jump_" => match idx {
            0 => "player_jump_0",
            1 => "player_jump_1",
            2 => "player_jump_2",
            _ => "player_jump_3",
        },
        "player_fall_" => match idx {
            0 => "player_fall_0",
            1 => "player_fall_1",
            2 => "player_fall_2",
            _ => "player_fall_3",
        },
        "player_attack_" => match idx {
            0 => "player_attack_0",
            1 => "player_attack_1",
            2 => "player_attack_2",
            3 => "player_attack_3",
            4 => "player_attack_4",
            5 => "player_attack_5",
            6 => "player_attack_6",
            _ => "player_attack_7",
        },
        "player_hurt_" => match idx {
            0 => "player_hurt_0",
            1 => "player_hurt_1",
            2 => "player_hurt_2",
            _ => "player_hurt_3",
        },
        "player_dead_" => match idx {
            0 => "player_dead_0",
            1 => "player_dead_1",
            2 => "player_dead_2",
            3 => "player_dead_3",
            4 => "player_dead_4",
            _ => "player_dead_5",
        },
        "player_wall_slide_" => match idx {
            0 => "player_wall_slide_0",
            1 => "player_wall_slide_1",
            _ => "player_wall_slide_2",
        },
        "player_crouch_" => match idx {
            0 => "player_crouch_0",
            1 => "player_crouch_1",
            _ => "player_crouch_2",
        },
        "player_dash_" => match idx {
            0 => "player_dash_0",
            1 => "player_dash_1",
            2 => "player_dash_2",
            _ => "player_dash_3",
        },
        _ => "player_idle_0",
    }
}

// ================================================================
// Enemy sprites (16x32) — Y 64-159
// Row 0 (Y 64-95):   Skeleton walk(4f), attack(3f), death(4f)
//                     Bat fly(4f), death(2f)
// Row 1 (Y 96-127):  Knight walk(4f), attack(3f), death(4f)
//                     Medusa float(4f), death(2f)
// Row 2 (Y 128-159): Ghost drift(4f), phase(3f), death(3f)
//                     Gargoyle perch(2f), swoop(4f), death(3f)
//                     Projectile(3f)
// ================================================================

fn add_enemy_sprites(sheet: &mut SpriteSheet) {
    let w = 16u32;
    let h = 32u32;

    // Row 0 (Y=64): Skeleton + Bat
    let y0 = 64u32;
    let mut x = 0u32;

    // Skeleton walk: 4 frames
    for i in 0..4u32 {
        let name = enemy_frame_name("skeleton_walk_", i);
        sheet.add(name, x + i * w, y0, w, h);
    }
    x += 4 * w;

    // Skeleton attack: 3 frames
    for i in 0..3u32 {
        let name = enemy_frame_name("skeleton_attack_", i);
        sheet.add(name, x + i * w, y0, w, h);
    }
    x += 3 * w;

    // Skeleton death: 4 frames
    for i in 0..4u32 {
        let name = enemy_frame_name("skeleton_death_", i);
        sheet.add(name, x + i * w, y0, w, h);
    }
    x += 4 * w;

    // Bat fly: 4 frames
    for i in 0..4u32 {
        let name = enemy_frame_name("bat_fly_", i);
        sheet.add(name, x + i * w, y0, w, h);
    }
    x += 4 * w;

    // Bat death: 2 frames
    for i in 0..2u32 {
        let name = enemy_frame_name("bat_death_", i);
        sheet.add(name, x + i * w, y0, w, h);
    }

    // Row 1 (Y=96): Knight + Medusa
    let y1 = 96u32;
    x = 0;

    // Knight walk: 4 frames
    for i in 0..4u32 {
        let name = enemy_frame_name("knight_walk_", i);
        sheet.add(name, x + i * w, y1, w, h);
    }
    x += 4 * w;

    // Knight attack: 3 frames
    for i in 0..3u32 {
        let name = enemy_frame_name("knight_attack_", i);
        sheet.add(name, x + i * w, y1, w, h);
    }
    x += 3 * w;

    // Knight death: 4 frames
    for i in 0..4u32 {
        let name = enemy_frame_name("knight_death_", i);
        sheet.add(name, x + i * w, y1, w, h);
    }
    x += 4 * w;

    // Medusa float: 4 frames
    for i in 0..4u32 {
        let name = enemy_frame_name("medusa_float_", i);
        sheet.add(name, x + i * w, y1, w, h);
    }
    x += 4 * w;

    // Medusa death: 2 frames
    for i in 0..2u32 {
        let name = enemy_frame_name("medusa_death_", i);
        sheet.add(name, x + i * w, y1, w, h);
    }

    // Row 2 (Y=128): Ghost + Gargoyle + Projectile
    let y2 = 128u32;
    x = 0;

    // Ghost drift: 4 frames
    for i in 0..4u32 {
        let name = enemy_frame_name("ghost_drift_", i);
        sheet.add(name, x + i * w, y2, w, h);
    }
    x += 4 * w;

    // Ghost phase: 3 frames
    for i in 0..3u32 {
        let name = enemy_frame_name("ghost_phase_", i);
        sheet.add(name, x + i * w, y2, w, h);
    }
    x += 3 * w;

    // Ghost death: 3 frames
    for i in 0..3u32 {
        let name = enemy_frame_name("ghost_death_", i);
        sheet.add(name, x + i * w, y2, w, h);
    }
    x += 3 * w;

    // Gargoyle perch: 2 frames
    for i in 0..2u32 {
        let name = enemy_frame_name("gargoyle_perch_", i);
        sheet.add(name, x + i * w, y2, w, h);
    }
    x += 2 * w;

    // Gargoyle swoop: 4 frames
    for i in 0..4u32 {
        let name = enemy_frame_name("gargoyle_swoop_", i);
        sheet.add(name, x + i * w, y2, w, h);
    }
    x += 4 * w;

    // Gargoyle death: 3 frames
    for i in 0..3u32 {
        let name = enemy_frame_name("gargoyle_death_", i);
        sheet.add(name, x + i * w, y2, w, h);
    }
    x += 3 * w;

    // Projectile: 3 frames (16x16 within the 32x64 row)
    sheet.add("projectile_0", x, y2, 16, 16);
    sheet.add("projectile_1", x + 16, y2, 16, 16);
    sheet.add("projectile_2", x + 32, y2, 16, 16);
}

fn enemy_frame_name(prefix: &str, idx: u32) -> &'static str {
    match prefix {
        "skeleton_walk_" => match idx {
            0 => "skeleton_walk_0",
            1 => "skeleton_walk_1",
            2 => "skeleton_walk_2",
            _ => "skeleton_walk_3",
        },
        "skeleton_attack_" => match idx {
            0 => "skeleton_attack_0",
            1 => "skeleton_attack_1",
            _ => "skeleton_attack_2",
        },
        "skeleton_death_" => match idx {
            0 => "skeleton_death_0",
            1 => "skeleton_death_1",
            2 => "skeleton_death_2",
            _ => "skeleton_death_3",
        },
        "bat_fly_" => match idx {
            0 => "bat_fly_0",
            1 => "bat_fly_1",
            2 => "bat_fly_2",
            _ => "bat_fly_3",
        },
        "bat_death_" => match idx {
            0 => "bat_death_0",
            _ => "bat_death_1",
        },
        "knight_walk_" => match idx {
            0 => "knight_walk_0",
            1 => "knight_walk_1",
            2 => "knight_walk_2",
            _ => "knight_walk_3",
        },
        "knight_attack_" => match idx {
            0 => "knight_attack_0",
            1 => "knight_attack_1",
            _ => "knight_attack_2",
        },
        "knight_death_" => match idx {
            0 => "knight_death_0",
            1 => "knight_death_1",
            2 => "knight_death_2",
            _ => "knight_death_3",
        },
        "medusa_float_" => match idx {
            0 => "medusa_float_0",
            1 => "medusa_float_1",
            2 => "medusa_float_2",
            _ => "medusa_float_3",
        },
        "medusa_death_" => match idx {
            0 => "medusa_death_0",
            _ => "medusa_death_1",
        },
        "ghost_drift_" => match idx {
            0 => "ghost_drift_0",
            1 => "ghost_drift_1",
            2 => "ghost_drift_2",
            _ => "ghost_drift_3",
        },
        "ghost_phase_" => match idx {
            0 => "ghost_phase_0",
            1 => "ghost_phase_1",
            _ => "ghost_phase_2",
        },
        "ghost_death_" => match idx {
            0 => "ghost_death_0",
            1 => "ghost_death_1",
            _ => "ghost_death_2",
        },
        "gargoyle_perch_" => match idx {
            0 => "gargoyle_perch_0",
            _ => "gargoyle_perch_1",
        },
        "gargoyle_swoop_" => match idx {
            0 => "gargoyle_swoop_0",
            1 => "gargoyle_swoop_1",
            2 => "gargoyle_swoop_2",
            _ => "gargoyle_swoop_3",
        },
        "gargoyle_death_" => match idx {
            0 => "gargoyle_death_0",
            1 => "gargoyle_death_1",
            _ => "gargoyle_death_2",
        },
        _ => "skeleton_walk_0",
    }
}

// ================================================================
// Tile sprites (16x16) — Y 160-287
// 4 visual groups × 16 bitmask tiles + shared tiles
//
// Layout at Y=160:
//   Row 0 (Y 160-175): Castle Interior tiles (16 bitmask + decoratives)
//   Row 1 (Y 176-191): Underground tiles (16 bitmask + decoratives)
//   Row 2 (Y 192-207): Sacred tiles (16 bitmask + decoratives)
//   Row 3 (Y 208-223): Fortress tiles (16 bitmask + decoratives)
//   Row 4 (Y 224-239): Shared tiles (platform, spikes, checkpoint, etc.)
//   Row 5-7 (Y 240-287): Decorative tiles, theme-specific props
// ================================================================

/// Tileset visual group for theme-based tile selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileGroup {
    CastleInterior,
    Underground,
    Sacred,
    Fortress,
}

fn add_tile_sprites(sheet: &mut SpriteSheet) {
    let tile = 16u32;

    // Per-group bitmask tiles: 16 tiles per group (4-neighbor: UDLR = 16 combos)
    // Each group starts at a known Y offset from the tile region base (320)
    add_bitmask_tiles(sheet, "castle", 0, 160, tile);
    add_bitmask_tiles(sheet, "underground", 0, 176, tile);
    add_bitmask_tiles(sheet, "sacred", 0, 192, tile);
    add_bitmask_tiles(sheet, "fortress", 0, 208, tile);

    // Theme-specific decorative tiles (after bitmask tiles in each row)
    let deco_x = 16 * tile; // X=256

    // Castle decoratives
    sheet.add("castle_bookshelf", deco_x, 160, tile, tile);
    sheet.add("castle_banner", deco_x + tile, 160, tile, tile);
    sheet.add("castle_pillar_top", deco_x + 2 * tile, 160, tile, tile);
    sheet.add("castle_pillar_mid", deco_x + 3 * tile, 160, tile, tile);

    // Underground decoratives
    sheet.add("underground_coffin", deco_x, 176, tile, tile);
    sheet.add("underground_bones", deco_x + tile, 176, tile, tile);
    sheet.add("underground_mushroom", deco_x + 2 * tile, 176, tile, tile);

    // Sacred decoratives
    sheet.add("sacred_altar", deco_x, 192, tile, tile);
    sheet.add("sacred_candle", deco_x + tile, 192, tile, tile);
    sheet.add("sacred_rune", deco_x + 2 * tile, 192, tile, tile);

    // Fortress decoratives
    sheet.add("fortress_weapon_rack", deco_x, 208, tile, tile);
    sheet.add("fortress_anvil", deco_x + tile, 208, tile, tile);
    sheet.add("fortress_shield", deco_x + 2 * tile, 208, tile, tile);

    // Shared tiles (Y=224)
    let sy = 224u32;
    let mut x = 0u32;

    // Platform: 3 variants
    sheet.add("platform_0", x, sy, tile, tile);
    sheet.add("platform_1", x + tile, sy, tile, tile);
    sheet.add("platform_2", x + 2 * tile, sy, tile, tile);
    x += 3 * tile;

    // Spikes: 2 variants
    sheet.add("spikes_0", x, sy, tile, tile);
    sheet.add("spikes_1", x + tile, sy, tile, tile);
    x += 2 * tile;

    // Checkpoint flags
    sheet.add("checkpoint_flag_down_0", x, sy, tile, tile);
    sheet.add("checkpoint_flag_down_1", x + tile, sy, tile, tile);
    sheet.add("checkpoint_flag_up_0", x + 2 * tile, sy, tile, tile);
    sheet.add("checkpoint_flag_up_1", x + 3 * tile, sy, tile, tile);
    x += 4 * tile;

    // Finish gate: 2 frames
    sheet.add("finish_gate_0", x, sy, tile, tile);
    sheet.add("finish_gate_1", x + tile, sy, tile, tile);
    x += 2 * tile;

    // Ladder
    sheet.add("ladder", x, sy, tile, tile);
    x += tile;

    // Breakable wall: 2 variants
    sheet.add("breakable_wall_0", x, sy, tile, tile);
    sheet.add("breakable_wall_1", x + tile, sy, tile, tile);
    x += 2 * tile;

    // Torch: 4 frames
    sheet.add("torch_0", x, sy, tile, tile);
    sheet.add("torch_1", x + tile, sy, tile, tile);
    sheet.add("torch_2", x + 2 * tile, sy, tile, tile);
    sheet.add("torch_3", x + 3 * tile, sy, tile, tile);
    x += 4 * tile;

    // Stained glass
    sheet.add("stained_glass", x, sy, tile, tile);
    x += tile;

    // Water tiles
    sheet.add("water_surface", x, sy, tile, tile);
    sheet.add("water_body", x + tile, sy, tile, tile);
    x += 2 * tile;

    // Decorative tiles
    sheet.add("cobweb", x, sy, tile, tile);
    sheet.add("chain_0", x + tile, sy, tile, tile);
    sheet.add("chain_1", x + 2 * tile, sy, tile, tile);

    // Legacy aliases: map old stone_brick_* names to castle bitmask tiles
    // so existing code that references stone_brick_top etc. still works.
    add_legacy_stone_aliases(sheet);
}

/// Add 16 bitmask tiles for a given visual group.
/// Bitmask: bit 0=Up, bit 1=Down, bit 2=Left, bit 3=Right (4 neighbors = 16 combos).
fn add_bitmask_tiles(sheet: &mut SpriteSheet, group: &str, base_x: u32, base_y: u32, tile: u32) {
    for i in 0..16u32 {
        let name = bitmask_tile_name(group, i);
        sheet.add(name, base_x + i * tile, base_y, tile, tile);
    }
}

/// Get the static name for a bitmask tile. Index 0-15 encodes neighbor presence.
fn bitmask_tile_name(group: &str, idx: u32) -> &'static str {
    match group {
        "castle" => match idx {
            0 => "castle_tile_0",
            1 => "castle_tile_1",
            2 => "castle_tile_2",
            3 => "castle_tile_3",
            4 => "castle_tile_4",
            5 => "castle_tile_5",
            6 => "castle_tile_6",
            7 => "castle_tile_7",
            8 => "castle_tile_8",
            9 => "castle_tile_9",
            10 => "castle_tile_10",
            11 => "castle_tile_11",
            12 => "castle_tile_12",
            13 => "castle_tile_13",
            14 => "castle_tile_14",
            _ => "castle_tile_15",
        },
        "underground" => match idx {
            0 => "underground_tile_0",
            1 => "underground_tile_1",
            2 => "underground_tile_2",
            3 => "underground_tile_3",
            4 => "underground_tile_4",
            5 => "underground_tile_5",
            6 => "underground_tile_6",
            7 => "underground_tile_7",
            8 => "underground_tile_8",
            9 => "underground_tile_9",
            10 => "underground_tile_10",
            11 => "underground_tile_11",
            12 => "underground_tile_12",
            13 => "underground_tile_13",
            14 => "underground_tile_14",
            _ => "underground_tile_15",
        },
        "sacred" => match idx {
            0 => "sacred_tile_0",
            1 => "sacred_tile_1",
            2 => "sacred_tile_2",
            3 => "sacred_tile_3",
            4 => "sacred_tile_4",
            5 => "sacred_tile_5",
            6 => "sacred_tile_6",
            7 => "sacred_tile_7",
            8 => "sacred_tile_8",
            9 => "sacred_tile_9",
            10 => "sacred_tile_10",
            11 => "sacred_tile_11",
            12 => "sacred_tile_12",
            13 => "sacred_tile_13",
            14 => "sacred_tile_14",
            _ => "sacred_tile_15",
        },
        _ => match idx {
            0 => "fortress_tile_0",
            1 => "fortress_tile_1",
            2 => "fortress_tile_2",
            3 => "fortress_tile_3",
            4 => "fortress_tile_4",
            5 => "fortress_tile_5",
            6 => "fortress_tile_6",
            7 => "fortress_tile_7",
            8 => "fortress_tile_8",
            9 => "fortress_tile_9",
            10 => "fortress_tile_10",
            11 => "fortress_tile_11",
            12 => "fortress_tile_12",
            13 => "fortress_tile_13",
            14 => "fortress_tile_14",
            _ => "fortress_tile_15",
        },
    }
}

/// Compute 4-neighbor bitmask for a stone brick tile.
/// bit 0 = solid above, bit 1 = solid below, bit 2 = solid left, bit 3 = solid right.
pub fn stone_brick_bitmask(
    course: &breakpoint_platformer::course_gen::Course,
    tx: i32,
    ty: i32,
) -> u32 {
    use breakpoint_platformer::course_gen::Tile;
    let is_solid = |dx: i32, dy: i32| -> bool {
        matches!(course.get_tile(tx + dx, ty + dy), Tile::StoneBrick)
    };
    let mut mask = 0u32;
    if is_solid(0, 1) {
        mask |= 1;
    } // up
    if is_solid(0, -1) {
        mask |= 2;
    } // down
    if is_solid(-1, 0) {
        mask |= 4;
    } // left
    if is_solid(1, 0) {
        mask |= 8;
    } // right
    mask
}

/// Get the bitmask tile name for a given group and bitmask value.
pub fn bitmask_tile_for_group(group: TileGroup, mask: u32) -> &'static str {
    let group_name = match group {
        TileGroup::CastleInterior => "castle",
        TileGroup::Underground => "underground",
        TileGroup::Sacred => "sacred",
        TileGroup::Fortress => "fortress",
    };
    bitmask_tile_name(group_name, mask.min(15))
}

/// Map room themes to visual tile groups.
pub fn room_theme_to_tile_group(theme: &breakpoint_platformer::course_gen::RoomTheme) -> TileGroup {
    use breakpoint_platformer::course_gen::RoomTheme;
    match theme {
        RoomTheme::Entrance
        | RoomTheme::Corridor
        | RoomTheme::GreatHall
        | RoomTheme::ThroneRoom => TileGroup::CastleInterior,
        RoomTheme::Crypt | RoomTheme::Dungeon => TileGroup::Underground,
        RoomTheme::Chapel | RoomTheme::Library => TileGroup::Sacred,
        RoomTheme::Armory | RoomTheme::Tower => TileGroup::Fortress,
    }
}

/// Legacy aliases so old code referencing stone_brick_top etc. still works.
fn add_legacy_stone_aliases(sheet: &mut SpriteSheet) {
    // Map old 8-variant names to nearest bitmask equivalents using castle tiles.
    // stone_brick_top = exposed top (no solid above) = mask where bit0=0
    // We alias to the bitmask tile with appropriate neighbor pattern.
    let copy_region = |sheet: &mut SpriteSheet, src: &str, dst: &'static str| {
        if let Some(r) = sheet.get(src).copied() {
            sheet.regions.insert(dst, r);
        }
    };

    // Bitmask: bit0=up, bit1=down, bit2=left, bit3=right
    // top exposed (no above, left+right present) = 0b1110 = 14
    copy_region(sheet, "castle_tile_14", "stone_brick_top");
    // inner (all surrounded) = 0b1111 = 15
    copy_region(sheet, "castle_tile_15", "stone_brick_inner");
    // left edge (no left, above+below+right) = 0b1011 = 11
    copy_region(sheet, "castle_tile_11", "stone_brick_left");
    // right edge (no right, above+below+left) = 0b0111 = 7
    copy_region(sheet, "castle_tile_7", "stone_brick_right");
    // top-left corner (no above, no left) = 0b1010 = 10
    copy_region(sheet, "castle_tile_10", "stone_brick_top_left");
    // top-right corner (no above, no right) = 0b0110 = 6
    copy_region(sheet, "castle_tile_6", "stone_brick_top_right");
    // bottom-left corner (no below, no left) = 0b1001 = 9
    copy_region(sheet, "castle_tile_9", "stone_brick_bottom_left");
    // bottom-right corner (no below, no right) = 0b0101 = 5
    copy_region(sheet, "castle_tile_5", "stone_brick_bottom_right");
}

// ================================================================
// UI / Power-up / Props sprites — Y 288-351
// ================================================================

fn add_ui_sprites(sheet: &mut SpriteSheet) {
    let tile = 16u32;
    let y = 288u32;

    // Power-ups (16x16)
    sheet.add("powerup_holy_water", 0, y, tile, tile);
    sheet.add("powerup_crucifix", tile, y, tile, tile);
    sheet.add("powerup_speed_boots", 2 * tile, y, tile, tile);
    sheet.add("powerup_double_jump", 3 * tile, y, tile, tile);
    sheet.add("powerup_armor", 4 * tile, y, tile, tile);
    sheet.add("powerup_invincibility", 5 * tile, y, tile, tile);
    sheet.add("powerup_whip_extend", 6 * tile, y, tile, tile);

    // Hearts (16x16)
    sheet.add("heart_full", 0, y + tile, tile, tile);
    sheet.add("heart_empty", tile, y + tile, tile, tile);

    // Props (16x16)
    sheet.add("prop_candelabra", 2 * tile, y + tile, tile, tile);
    sheet.add("prop_cross", 3 * tile, y + tile, tile, tile);
    sheet.add("prop_gravestone", 4 * tile, y + tile, tile, tile);

    // Health bar elements (32x8 — procedural bar frame)
    sheet.add("health_bar_frame", 0, y + 2 * tile, 32, 8);
    sheet.add("health_bar_fill", 32, y + 2 * tile, 32, 8);
}

// ================================================================
// Particle sprites (8x8) — Y 352-383
// ================================================================

fn add_particle_sprites(sheet: &mut SpriteSheet) {
    let y = 352u32;
    let mut x = 0u32;

    for i in 0..4u32 {
        let name = match i {
            0 => "particle_dust_0",
            1 => "particle_dust_1",
            2 => "particle_dust_2",
            _ => "particle_dust_3",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }
    x += 32;
    for i in 0..3u32 {
        let name = match i {
            0 => "particle_spark_0",
            1 => "particle_spark_1",
            _ => "particle_spark_2",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }
    x += 24;
    for i in 0..3u32 {
        let name = match i {
            0 => "particle_blood_0",
            1 => "particle_blood_1",
            _ => "particle_blood_2",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }
    x += 24;
    for i in 0..4u32 {
        let name = match i {
            0 => "particle_fire_0",
            1 => "particle_fire_1",
            2 => "particle_fire_2",
            _ => "particle_fire_3",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }
    x += 32;
    for i in 0..3u32 {
        let name = match i {
            0 => "particle_magic_0",
            1 => "particle_magic_1",
            _ => "particle_magic_2",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }
    x += 24;
    for i in 0..3u32 {
        let name = match i {
            0 => "particle_smoke_0",
            1 => "particle_smoke_1",
            _ => "particle_smoke_2",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }
    x += 24;
    for i in 0..3u32 {
        let name = match i {
            0 => "particle_debris_0",
            1 => "particle_debris_1",
            _ => "particle_debris_2",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }
    x += 24;
    for i in 0..3u32 {
        let name = match i {
            0 => "particle_water_0",
            1 => "particle_water_1",
            _ => "particle_water_2",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }
    x += 24;
    for i in 0..3u32 {
        let name = match i {
            0 => "particle_ember_0",
            1 => "particle_ember_1",
            _ => "particle_ember_2",
        };
        sheet.add(name, x + i * 8, y, 8, 8);
    }

    // Ambient particle types (Y=356)
    let ay = 356u32;
    sheet.add("particle_sparkle_0", 0, ay, 8, 8);
    sheet.add("particle_sparkle_1", 8, ay, 8, 8);
    sheet.add("particle_snowflake_0", 16, ay, 8, 8);
    sheet.add("particle_snowflake_1", 24, ay, 8, 8);
    sheet.add("particle_page_0", 32, ay, 8, 8);
    sheet.add("particle_page_1", 40, ay, 8, 8);
}

// ================================================================
// Combat VFX sprites (32x32) — Y 384-415
// ================================================================

fn add_combat_vfx_sprites(sheet: &mut SpriteSheet) {
    let vfx_y = 384u32;
    let size = 32u32;
    let mut x = 0u32;

    // Slash arc VFX: 5 frames
    for i in 0..5u32 {
        let name = match i {
            0 => "vfx_slash_0",
            1 => "vfx_slash_1",
            2 => "vfx_slash_2",
            3 => "vfx_slash_3",
            _ => "vfx_slash_4",
        };
        sheet.add(name, x + i * size, vfx_y, size, size);
    }
    x += 5 * size; // 320

    // Magic circle: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "vfx_magic_circle_0",
            1 => "vfx_magic_circle_1",
            2 => "vfx_magic_circle_2",
            _ => "vfx_magic_circle_3",
        };
        sheet.add(name, x + i * size, vfx_y, size, size);
    }
    x += 4 * size; // 576

    // Hit sparks: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "vfx_hit_spark_0",
            1 => "vfx_hit_spark_1",
            2 => "vfx_hit_spark_2",
            _ => "vfx_hit_spark_3",
        };
        sheet.add(name, x + i * size, vfx_y, size, size);
    }
}

// ================================================================
// Animation lookup table
// ================================================================

/// Build animation lookup table from the sprite sheet.
pub fn build_platformer_animations(sheet: &SpriteSheet) -> HashMap<&'static str, SpriteAnimation> {
    let mut anims = HashMap::new();

    let frames = |names: &[&str]| -> Vec<SpriteRegion> {
        names.iter().map(|n| sheet.get_or_default(n)).collect()
    };

    // Player animations (expanded)
    anims.insert(
        "player_idle",
        SpriteAnimation {
            frames: frames(&[
                "player_idle_0",
                "player_idle_1",
                "player_idle_2",
                "player_idle_3",
                "player_idle_4",
                "player_idle_5",
                "player_idle_6",
                "player_idle_7",
            ]),
            frame_duration: 0.15,
            looping: true,
        },
    );
    anims.insert(
        "player_walk",
        SpriteAnimation {
            frames: frames(&[
                "player_walk_0",
                "player_walk_1",
                "player_walk_2",
                "player_walk_3",
                "player_walk_4",
                "player_walk_5",
                "player_walk_6",
                "player_walk_7",
            ]),
            frame_duration: 0.1,
            looping: true,
        },
    );
    anims.insert(
        "player_run",
        SpriteAnimation {
            frames: frames(&[
                "player_run_0",
                "player_run_1",
                "player_run_2",
                "player_run_3",
                "player_run_4",
                "player_run_5",
                "player_run_6",
                "player_run_7",
            ]),
            frame_duration: 0.08,
            looping: true,
        },
    );
    anims.insert(
        "player_jump",
        SpriteAnimation {
            frames: frames(&[
                "player_jump_0",
                "player_jump_1",
                "player_jump_2",
                "player_jump_3",
            ]),
            frame_duration: 0.12,
            looping: true,
        },
    );
    anims.insert(
        "player_fall",
        SpriteAnimation {
            frames: frames(&[
                "player_fall_0",
                "player_fall_1",
                "player_fall_2",
                "player_fall_3",
            ]),
            frame_duration: 0.15,
            looping: true,
        },
    );
    anims.insert(
        "player_attack",
        SpriteAnimation {
            frames: frames(&[
                "player_attack_0",
                "player_attack_1",
                "player_attack_2",
                "player_attack_3",
                "player_attack_4",
                "player_attack_5",
                "player_attack_6",
                "player_attack_7",
            ]),
            frame_duration: 0.04,
            looping: false,
        },
    );
    anims.insert(
        "player_hurt",
        SpriteAnimation {
            frames: frames(&[
                "player_hurt_0",
                "player_hurt_1",
                "player_hurt_2",
                "player_hurt_3",
            ]),
            frame_duration: 0.12,
            looping: false,
        },
    );
    anims.insert(
        "player_dead",
        SpriteAnimation {
            frames: frames(&[
                "player_dead_0",
                "player_dead_1",
                "player_dead_2",
                "player_dead_3",
                "player_dead_4",
                "player_dead_5",
            ]),
            frame_duration: 0.15,
            looping: false,
        },
    );
    anims.insert(
        "player_wall_slide",
        SpriteAnimation {
            frames: frames(&[
                "player_wall_slide_0",
                "player_wall_slide_1",
                "player_wall_slide_2",
            ]),
            frame_duration: 0.15,
            looping: true,
        },
    );
    anims.insert(
        "player_crouch",
        SpriteAnimation {
            frames: frames(&["player_crouch_0", "player_crouch_1", "player_crouch_2"]),
            frame_duration: 0.15,
            looping: true,
        },
    );
    anims.insert(
        "player_dash",
        SpriteAnimation {
            frames: frames(&[
                "player_dash_0",
                "player_dash_1",
                "player_dash_2",
                "player_dash_3",
            ]),
            frame_duration: 0.06,
            looping: false,
        },
    );

    // Enemy animations
    add_enemy_animations(&mut anims, sheet);

    // Tile animations
    anims.insert(
        "torch",
        SpriteAnimation {
            frames: frames(&["torch_0", "torch_1", "torch_2", "torch_3"]),
            frame_duration: 0.15,
            looping: true,
        },
    );
    anims.insert(
        "checkpoint_flag_down",
        SpriteAnimation {
            frames: frames(&["checkpoint_flag_down_0", "checkpoint_flag_down_1"]),
            frame_duration: 0.3,
            looping: true,
        },
    );
    anims.insert(
        "checkpoint_flag_up",
        SpriteAnimation {
            frames: frames(&["checkpoint_flag_up_0", "checkpoint_flag_up_1"]),
            frame_duration: 0.3,
            looping: true,
        },
    );
    anims.insert(
        "finish_gate",
        SpriteAnimation {
            frames: frames(&["finish_gate_0", "finish_gate_1"]),
            frame_duration: 0.4,
            looping: true,
        },
    );
    anims.insert(
        "projectile",
        SpriteAnimation {
            frames: frames(&["projectile_0", "projectile_1", "projectile_2"]),
            frame_duration: 0.1,
            looping: true,
        },
    );
    anims.insert(
        "chain",
        SpriteAnimation {
            frames: frames(&["chain_0", "chain_1"]),
            frame_duration: 0.5,
            looping: true,
        },
    );

    // VFX animations
    anims.insert(
        "vfx_slash",
        SpriteAnimation {
            frames: frames(&[
                "vfx_slash_0",
                "vfx_slash_1",
                "vfx_slash_2",
                "vfx_slash_3",
                "vfx_slash_4",
            ]),
            frame_duration: 0.05,
            looping: false,
        },
    );
    anims.insert(
        "vfx_magic_circle",
        SpriteAnimation {
            frames: frames(&[
                "vfx_magic_circle_0",
                "vfx_magic_circle_1",
                "vfx_magic_circle_2",
                "vfx_magic_circle_3",
            ]),
            frame_duration: 0.1,
            looping: true,
        },
    );
    anims.insert(
        "vfx_hit_spark",
        SpriteAnimation {
            frames: frames(&[
                "vfx_hit_spark_0",
                "vfx_hit_spark_1",
                "vfx_hit_spark_2",
                "vfx_hit_spark_3",
            ]),
            frame_duration: 0.04,
            looping: false,
        },
    );

    anims
}

/// Add enemy animation entries to the map.
fn add_enemy_animations(anims: &mut HashMap<&'static str, SpriteAnimation>, sheet: &SpriteSheet) {
    let frames = |names: &[&str]| -> Vec<SpriteRegion> {
        names.iter().map(|n| sheet.get_or_default(n)).collect()
    };

    // Skeleton
    anims.insert(
        "skeleton_walk",
        SpriteAnimation {
            frames: frames(&[
                "skeleton_walk_0",
                "skeleton_walk_1",
                "skeleton_walk_2",
                "skeleton_walk_3",
            ]),
            frame_duration: 0.15,
            looping: true,
        },
    );
    anims.insert(
        "skeleton_attack",
        SpriteAnimation {
            frames: frames(&[
                "skeleton_attack_0",
                "skeleton_attack_1",
                "skeleton_attack_2",
            ]),
            frame_duration: 0.1,
            looping: false,
        },
    );
    anims.insert(
        "skeleton_death",
        SpriteAnimation {
            frames: frames(&[
                "skeleton_death_0",
                "skeleton_death_1",
                "skeleton_death_2",
                "skeleton_death_3",
            ]),
            frame_duration: 0.15,
            looping: false,
        },
    );

    // Bat
    anims.insert(
        "bat_fly",
        SpriteAnimation {
            frames: frames(&["bat_fly_0", "bat_fly_1", "bat_fly_2", "bat_fly_3"]),
            frame_duration: 0.12,
            looping: true,
        },
    );
    anims.insert(
        "bat_death",
        SpriteAnimation {
            frames: frames(&["bat_death_0", "bat_death_1"]),
            frame_duration: 0.2,
            looping: false,
        },
    );

    // Knight
    anims.insert(
        "knight_walk",
        SpriteAnimation {
            frames: frames(&[
                "knight_walk_0",
                "knight_walk_1",
                "knight_walk_2",
                "knight_walk_3",
            ]),
            frame_duration: 0.18,
            looping: true,
        },
    );
    anims.insert(
        "knight_attack",
        SpriteAnimation {
            frames: frames(&["knight_attack_0", "knight_attack_1", "knight_attack_2"]),
            frame_duration: 0.1,
            looping: false,
        },
    );
    anims.insert(
        "knight_death",
        SpriteAnimation {
            frames: frames(&[
                "knight_death_0",
                "knight_death_1",
                "knight_death_2",
                "knight_death_3",
            ]),
            frame_duration: 0.15,
            looping: false,
        },
    );

    // Medusa
    anims.insert(
        "medusa_float",
        SpriteAnimation {
            frames: frames(&[
                "medusa_float_0",
                "medusa_float_1",
                "medusa_float_2",
                "medusa_float_3",
            ]),
            frame_duration: 0.18,
            looping: true,
        },
    );
    anims.insert(
        "medusa_death",
        SpriteAnimation {
            frames: frames(&["medusa_death_0", "medusa_death_1"]),
            frame_duration: 0.2,
            looping: false,
        },
    );

    // Ghost (new enemy type)
    anims.insert(
        "ghost_drift",
        SpriteAnimation {
            frames: frames(&[
                "ghost_drift_0",
                "ghost_drift_1",
                "ghost_drift_2",
                "ghost_drift_3",
            ]),
            frame_duration: 0.2,
            looping: true,
        },
    );
    anims.insert(
        "ghost_phase",
        SpriteAnimation {
            frames: frames(&["ghost_phase_0", "ghost_phase_1", "ghost_phase_2"]),
            frame_duration: 0.15,
            looping: false,
        },
    );
    anims.insert(
        "ghost_death",
        SpriteAnimation {
            frames: frames(&["ghost_death_0", "ghost_death_1", "ghost_death_2"]),
            frame_duration: 0.15,
            looping: false,
        },
    );

    // Gargoyle (new enemy type)
    anims.insert(
        "gargoyle_perch",
        SpriteAnimation {
            frames: frames(&["gargoyle_perch_0", "gargoyle_perch_1"]),
            frame_duration: 0.5,
            looping: true,
        },
    );
    anims.insert(
        "gargoyle_swoop",
        SpriteAnimation {
            frames: frames(&[
                "gargoyle_swoop_0",
                "gargoyle_swoop_1",
                "gargoyle_swoop_2",
                "gargoyle_swoop_3",
            ]),
            frame_duration: 0.08,
            looping: false,
        },
    );
    anims.insert(
        "gargoyle_death",
        SpriteAnimation {
            frames: frames(&["gargoyle_death_0", "gargoyle_death_1", "gargoyle_death_2"]),
            frame_duration: 0.15,
            looping: false,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_region_to_vec4() {
        let r = SpriteRegion {
            u0: 0.0,
            v0: 0.25,
            u1: 0.5,
            v1: 0.75,
        };
        let v = r.to_vec4();
        assert!((v.x - 0.0).abs() < 1e-6);
        assert!((v.y - 0.25).abs() < 1e-6);
        assert!((v.z - 0.5).abs() < 1e-6);
        assert!((v.w - 0.75).abs() < 1e-6);
    }

    #[test]
    fn sprite_sheet_lookup() {
        let sheet = build_platformer_atlas();
        assert!(sheet.get("player_idle_0").is_some());
        assert!(sheet.get("stone_brick_top").is_some());
        assert!(sheet.get("nonexistent").is_none());
    }

    #[test]
    fn sprite_sheet_uv_coords() {
        let sheet = build_platformer_atlas();
        let r = sheet.get("player_idle_0").unwrap();
        // 0,0 -> 16,32 on 1024x512 atlas
        assert!((r.u0 - 0.0).abs() < 1e-6);
        assert!((r.v0 - 0.0).abs() < 1e-6);
        assert!((r.u1 - 16.0 / 1024.0).abs() < 1e-6);
        assert!((r.v1 - 32.0 / 512.0).abs() < 1e-6);
    }

    #[test]
    fn animation_frame_at_looping() {
        let anim = SpriteAnimation {
            frames: vec![
                SpriteRegion {
                    u0: 0.0,
                    v0: 0.0,
                    u1: 0.1,
                    v1: 0.1,
                },
                SpriteRegion {
                    u0: 0.1,
                    v0: 0.0,
                    u1: 0.2,
                    v1: 0.1,
                },
            ],
            frame_duration: 0.5,
            looping: true,
        };
        let f0 = anim.frame_at(0.0);
        assert!((f0.u0 - 0.0).abs() < 1e-6);
        let f1 = anim.frame_at(0.5);
        assert!((f1.u0 - 0.1).abs() < 1e-6);
        // Looping: 1.0 wraps to frame 0
        let f2 = anim.frame_at(1.0);
        assert!((f2.u0 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn animation_frame_at_non_looping() {
        let anim = SpriteAnimation {
            frames: vec![
                SpriteRegion {
                    u0: 0.0,
                    v0: 0.0,
                    u1: 0.1,
                    v1: 0.1,
                },
                SpriteRegion {
                    u0: 0.1,
                    v0: 0.0,
                    u1: 0.2,
                    v1: 0.1,
                },
            ],
            frame_duration: 0.5,
            looping: false,
        };
        // Beyond end should clamp to last frame
        let f = anim.frame_at(5.0);
        assert!((f.u0 - 0.1).abs() < 1e-6);
    }

    #[test]
    fn get_or_default_returns_fallback() {
        let sheet = SpriteSheet::new(512, 512);
        let r = sheet.get_or_default("missing");
        assert!(r.u0 >= 0.0);
        assert!(r.u1 > 0.0);
    }

    #[test]
    fn build_animations_has_all_player_anims() {
        let sheet = build_platformer_atlas();
        let anims = build_platformer_animations(&sheet);
        assert!(anims.contains_key("player_idle"));
        assert!(anims.contains_key("player_walk"));
        assert!(anims.contains_key("player_run"));
        assert!(anims.contains_key("player_jump"));
        assert!(anims.contains_key("player_fall"));
        assert!(anims.contains_key("player_attack"));
        assert!(anims.contains_key("player_hurt"));
        assert!(anims.contains_key("player_dead"));
        assert!(anims.contains_key("player_wall_slide"));
        assert!(anims.contains_key("player_crouch"));
        assert!(anims.contains_key("player_dash"));
    }

    #[test]
    fn build_animations_has_enemy_anims() {
        let sheet = build_platformer_atlas();
        let anims = build_platformer_animations(&sheet);
        assert!(anims.contains_key("skeleton_walk"));
        assert!(anims.contains_key("skeleton_death"));
        assert!(anims.contains_key("bat_fly"));
        assert!(anims.contains_key("bat_death"));
        assert!(anims.contains_key("knight_walk"));
        assert!(anims.contains_key("knight_death"));
        assert!(anims.contains_key("medusa_float"));
        assert!(anims.contains_key("medusa_death"));
        // New enemy types
        assert!(anims.contains_key("ghost_drift"));
        assert!(anims.contains_key("ghost_death"));
        assert!(anims.contains_key("gargoyle_perch"));
        assert!(anims.contains_key("gargoyle_swoop"));
        assert!(anims.contains_key("gargoyle_death"));
    }

    #[test]
    fn player_idle_animation_has_8_frames() {
        let sheet = build_platformer_atlas();
        let anims = build_platformer_animations(&sheet);
        assert_eq!(anims["player_idle"].frames.len(), 8);
    }

    #[test]
    fn particle_sprites_exist() {
        let sheet = build_platformer_atlas();
        assert!(sheet.get("particle_dust_0").is_some());
        assert!(sheet.get("particle_fire_0").is_some());
        assert!(sheet.get("particle_blood_0").is_some());
    }

    #[test]
    fn tile_variant_sprites_exist() {
        let sheet = build_platformer_atlas();
        assert!(sheet.get("stone_brick_top").is_some());
        assert!(sheet.get("stone_brick_inner").is_some());
        assert!(sheet.get("stone_brick_left").is_some());
        assert!(sheet.get("checkpoint_flag_down_0").is_some());
        assert!(sheet.get("checkpoint_flag_up_0").is_some());
        assert!(sheet.get("torch_0").is_some());
    }

    #[test]
    fn bitmask_tiles_exist_for_all_groups() {
        let sheet = build_platformer_atlas();
        for i in 0..16 {
            assert!(
                sheet.get(&format!("castle_tile_{i}")).is_some(),
                "castle_tile_{i} missing"
            );
            assert!(
                sheet.get(&format!("underground_tile_{i}")).is_some(),
                "underground_tile_{i} missing"
            );
            assert!(
                sheet.get(&format!("sacred_tile_{i}")).is_some(),
                "sacred_tile_{i} missing"
            );
            assert!(
                sheet.get(&format!("fortress_tile_{i}")).is_some(),
                "fortress_tile_{i} missing"
            );
        }
    }

    #[test]
    fn new_enemy_sprites_exist() {
        let sheet = build_platformer_atlas();
        assert!(sheet.get("ghost_drift_0").is_some());
        assert!(sheet.get("gargoyle_perch_0").is_some());
        assert!(sheet.get("gargoyle_swoop_0").is_some());
    }

    #[test]
    fn vfx_sprites_exist() {
        let sheet = build_platformer_atlas();
        assert!(sheet.get("vfx_slash_0").is_some());
        assert!(sheet.get("vfx_magic_circle_0").is_some());
        assert!(sheet.get("vfx_hit_spark_0").is_some());
    }

    #[test]
    fn combat_vfx_animations_exist() {
        let sheet = build_platformer_atlas();
        let anims = build_platformer_animations(&sheet);
        assert!(anims.contains_key("vfx_slash"));
        assert!(anims.contains_key("vfx_magic_circle"));
        assert!(anims.contains_key("vfx_hit_spark"));
    }

    #[test]
    fn atlas_dimensions_are_1024x512() {
        assert_eq!(ATLAS_W, 1024);
        assert_eq!(ATLAS_H, 512);
    }

    #[test]
    fn bitmask_tile_for_group_returns_valid() {
        let sheet = build_platformer_atlas();
        for mask in 0..16u32 {
            let name = bitmask_tile_for_group(TileGroup::CastleInterior, mask);
            assert!(sheet.get(name).is_some(), "Missing tile: {name}");
        }
    }
}
