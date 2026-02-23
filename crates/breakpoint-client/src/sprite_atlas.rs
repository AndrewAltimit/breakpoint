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

/// Build the platformer sprite sheet with all named regions.
/// Atlas is 512x512, sprites are 16x16 (tiles/items), 16x32 (characters), or 8x8 (particles).
pub fn build_platformer_atlas() -> SpriteSheet {
    let mut sheet = SpriteSheet::new(512, 512);

    // ── Player sprites (16x32) — Y 0-63 ───────────────────
    add_player_sprites(&mut sheet);

    // ── Enemy sprites (16x32) — Y 64-127 ──────────────────
    add_enemy_sprites(&mut sheet);

    // ── Tile sprites (16x16) — Y 128-191 ──────────────────
    add_tile_sprites(&mut sheet);

    // ── Power-ups, HUD, props (16x16) — Y 192-255 ────────
    add_ui_sprites(&mut sheet);

    // ── Particle sprites (8x8) — Y 256-383 ────────────────
    add_particle_sprites(&mut sheet);

    sheet
}

/// Add player animation frames to the sprite sheet.
fn add_player_sprites(sheet: &mut SpriteSheet) {
    // Idle: 6 frames
    for i in 0..6u32 {
        let name = match i {
            0 => "player_idle_0",
            1 => "player_idle_1",
            2 => "player_idle_2",
            3 => "player_idle_3",
            4 => "player_idle_4",
            _ => "player_idle_5",
        };
        sheet.add(name, i * 16, 0, 16, 32);
    }
    // Walk: 6 frames
    for i in 0..6u32 {
        let name = match i {
            0 => "player_walk_0",
            1 => "player_walk_1",
            2 => "player_walk_2",
            3 => "player_walk_3",
            4 => "player_walk_4",
            _ => "player_walk_5",
        };
        sheet.add(name, 96 + i * 16, 0, 16, 32);
    }
    // Jump: 3 frames
    for i in 0..3u32 {
        let name = match i {
            0 => "player_jump_0",
            1 => "player_jump_1",
            _ => "player_jump_2",
        };
        sheet.add(name, 192 + i * 16, 0, 16, 32);
    }
    // Fall: 3 frames
    for i in 0..3u32 {
        let name = match i {
            0 => "player_fall_0",
            1 => "player_fall_1",
            _ => "player_fall_2",
        };
        sheet.add(name, 240 + i * 16, 0, 16, 32);
    }
    // Attack: 6 frames
    for i in 0..6u32 {
        let name = match i {
            0 => "player_attack_0",
            1 => "player_attack_1",
            2 => "player_attack_2",
            3 => "player_attack_3",
            4 => "player_attack_4",
            _ => "player_attack_5",
        };
        sheet.add(name, 288 + i * 16, 0, 16, 32);
    }
    // Hurt: 2 frames
    sheet.add("player_hurt_0", 384, 0, 16, 32);
    sheet.add("player_hurt_1", 400, 0, 16, 32);
    // Dead: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "player_dead_0",
            1 => "player_dead_1",
            2 => "player_dead_2",
            _ => "player_dead_3",
        };
        sheet.add(name, 416 + i * 16, 0, 16, 32);
    }
}

/// Add enemy animation frames to the sprite sheet.
fn add_enemy_sprites(sheet: &mut SpriteSheet) {
    // Skeleton walk: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "skeleton_walk_0",
            1 => "skeleton_walk_1",
            2 => "skeleton_walk_2",
            _ => "skeleton_walk_3",
        };
        sheet.add(name, i * 16, 64, 16, 32);
    }
    // Skeleton attack: 3 frames
    for i in 0..3u32 {
        let name = match i {
            0 => "skeleton_attack_0",
            1 => "skeleton_attack_1",
            _ => "skeleton_attack_2",
        };
        sheet.add(name, 64 + i * 16, 64, 16, 32);
    }
    // Skeleton death: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "skeleton_death_0",
            1 => "skeleton_death_1",
            2 => "skeleton_death_2",
            _ => "skeleton_death_3",
        };
        sheet.add(name, 112 + i * 16, 64, 16, 32);
    }
    // Bat fly: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "bat_fly_0",
            1 => "bat_fly_1",
            2 => "bat_fly_2",
            _ => "bat_fly_3",
        };
        sheet.add(name, 176 + i * 16, 64, 16, 32);
    }
    // Bat death: 2 frames
    sheet.add("bat_death_0", 240, 64, 16, 32);
    sheet.add("bat_death_1", 256, 64, 16, 32);
    // Knight walk: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "knight_walk_0",
            1 => "knight_walk_1",
            2 => "knight_walk_2",
            _ => "knight_walk_3",
        };
        sheet.add(name, 272 + i * 16, 64, 16, 32);
    }
    // Knight attack: 3 frames
    for i in 0..3u32 {
        let name = match i {
            0 => "knight_attack_0",
            1 => "knight_attack_1",
            _ => "knight_attack_2",
        };
        sheet.add(name, 336 + i * 16, 64, 16, 32);
    }
    // Knight death: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "knight_death_0",
            1 => "knight_death_1",
            2 => "knight_death_2",
            _ => "knight_death_3",
        };
        sheet.add(name, 384 + i * 16, 64, 16, 32);
    }
    // Medusa float: 4 frames
    for i in 0..4u32 {
        let name = match i {
            0 => "medusa_float_0",
            1 => "medusa_float_1",
            2 => "medusa_float_2",
            _ => "medusa_float_3",
        };
        sheet.add(name, i * 16, 96, 16, 32);
    }
    // Medusa death: 2 frames
    sheet.add("medusa_death_0", 64, 96, 16, 32);
    sheet.add("medusa_death_1", 80, 96, 16, 32);
    // Projectile: 3 frames
    sheet.add("projectile_0", 96, 96, 16, 16);
    sheet.add("projectile_1", 112, 96, 16, 16);
    sheet.add("projectile_2", 128, 96, 16, 16);
}

/// Add tile sprites to the sprite sheet.
fn add_tile_sprites(sheet: &mut SpriteSheet) {
    // Stone brick variants
    sheet.add("stone_brick_top", 0, 128, 16, 16);
    sheet.add("stone_brick_inner", 16, 128, 16, 16);
    sheet.add("stone_brick_left", 32, 128, 16, 16);
    sheet.add("stone_brick_right", 48, 128, 16, 16);
    sheet.add("stone_brick_top_left", 64, 128, 16, 16);
    sheet.add("stone_brick_top_right", 80, 128, 16, 16);
    sheet.add("stone_brick_bottom_left", 96, 128, 16, 16);
    sheet.add("stone_brick_bottom_right", 112, 128, 16, 16);

    // Platform: 3 variants
    sheet.add("platform_0", 128, 128, 16, 16);
    sheet.add("platform_1", 144, 128, 16, 16);
    sheet.add("platform_2", 160, 128, 16, 16);

    // Spikes: 2 variants
    sheet.add("spikes_0", 176, 128, 16, 16);
    sheet.add("spikes_1", 192, 128, 16, 16);

    // Checkpoint flags
    sheet.add("checkpoint_flag_down_0", 0, 144, 16, 16);
    sheet.add("checkpoint_flag_down_1", 16, 144, 16, 16);
    sheet.add("checkpoint_flag_up_0", 32, 144, 16, 16);
    sheet.add("checkpoint_flag_up_1", 48, 144, 16, 16);

    // Finish gate: 2 frames
    sheet.add("finish_gate_0", 64, 144, 16, 16);
    sheet.add("finish_gate_1", 80, 144, 16, 16);

    // Ladder
    sheet.add("ladder", 96, 144, 16, 16);

    // Breakable wall: 2 variants
    sheet.add("breakable_wall_0", 112, 144, 16, 16);
    sheet.add("breakable_wall_1", 128, 144, 16, 16);

    // Torch: 4 frames
    sheet.add("torch_0", 144, 144, 16, 16);
    sheet.add("torch_1", 160, 144, 16, 16);
    sheet.add("torch_2", 176, 144, 16, 16);
    sheet.add("torch_3", 192, 144, 16, 16);

    // Stained glass
    sheet.add("stained_glass", 208, 144, 16, 16);
}

/// Add power-up, HUD, and prop sprites to the sprite sheet.
fn add_ui_sprites(sheet: &mut SpriteSheet) {
    sheet.add("powerup_holy_water", 0, 192, 16, 16);
    sheet.add("powerup_crucifix", 16, 192, 16, 16);
    sheet.add("powerup_speed_boots", 32, 192, 16, 16);
    sheet.add("powerup_double_jump", 48, 192, 16, 16);
    sheet.add("powerup_armor", 64, 192, 16, 16);
    sheet.add("powerup_invincibility", 80, 192, 16, 16);
    sheet.add("powerup_whip_extend", 96, 192, 16, 16);

    sheet.add("heart_full", 0, 208, 16, 16);
    sheet.add("heart_empty", 16, 208, 16, 16);

    sheet.add("prop_candelabra", 32, 208, 16, 16);
    sheet.add("prop_cross", 48, 208, 16, 16);
    sheet.add("prop_gravestone", 64, 208, 16, 16);
}

/// Add particle sprites (8x8) to the sprite sheet.
fn add_particle_sprites(sheet: &mut SpriteSheet) {
    let y = 256;
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
}

/// Build animation lookup table from the sprite sheet.
pub fn build_platformer_animations(sheet: &SpriteSheet) -> HashMap<&'static str, SpriteAnimation> {
    let mut anims = HashMap::new();

    // Helper to collect frames
    let frames = |names: &[&str]| -> Vec<SpriteRegion> {
        names.iter().map(|n| sheet.get_or_default(n)).collect()
    };

    // Player animations
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
            ]),
            frame_duration: 0.1,
            looping: true,
        },
    );
    anims.insert(
        "player_jump",
        SpriteAnimation {
            frames: frames(&["player_jump_0", "player_jump_1", "player_jump_2"]),
            frame_duration: 0.12,
            looping: true,
        },
    );
    anims.insert(
        "player_fall",
        SpriteAnimation {
            frames: frames(&["player_fall_0", "player_fall_1", "player_fall_2"]),
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
            ]),
            frame_duration: 0.05,
            looping: false,
        },
    );
    anims.insert(
        "player_hurt",
        SpriteAnimation {
            frames: frames(&["player_hurt_0", "player_hurt_1"]),
            frame_duration: 0.2,
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
            ]),
            frame_duration: 0.15,
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

    anims
}

/// Add enemy animation entries to the map.
fn add_enemy_animations(anims: &mut HashMap<&'static str, SpriteAnimation>, sheet: &SpriteSheet) {
    let frames = |names: &[&str]| -> Vec<SpriteRegion> {
        names.iter().map(|n| sheet.get_or_default(n)).collect()
    };

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
        // 0,0 -> 16,32 on 512x512 atlas
        assert!((r.u0 - 0.0).abs() < 1e-6);
        assert!((r.v0 - 0.0).abs() < 1e-6);
        assert!((r.u1 - 16.0 / 512.0).abs() < 1e-6);
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
        assert!(anims.contains_key("player_jump"));
        assert!(anims.contains_key("player_fall"));
        assert!(anims.contains_key("player_attack"));
        assert!(anims.contains_key("player_hurt"));
        assert!(anims.contains_key("player_dead"));
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
    }

    #[test]
    fn player_idle_animation_has_6_frames() {
        let sheet = build_platformer_atlas();
        let anims = build_platformer_animations(&sheet);
        assert_eq!(anims["player_idle"].frames.len(), 6);
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
}
