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
/// Atlas is 256x256, sprites are 16x16 (tiles/items) or 16x32 (characters).
pub fn build_platformer_atlas() -> SpriteSheet {
    let mut sheet = SpriteSheet::new(256, 256);

    // Player sprites (16x32) — row 0
    sheet.add("player_idle_0", 0, 0, 16, 32);
    sheet.add("player_idle_1", 16, 0, 16, 32);
    sheet.add("player_walk_0", 32, 0, 16, 32);
    sheet.add("player_walk_1", 48, 0, 16, 32);
    sheet.add("player_walk_2", 64, 0, 16, 32);
    sheet.add("player_walk_3", 80, 0, 16, 32);
    sheet.add("player_jump", 96, 0, 16, 32);
    sheet.add("player_fall", 112, 0, 16, 32);
    sheet.add("player_attack_0", 128, 0, 16, 32);
    sheet.add("player_attack_1", 144, 0, 16, 32);
    sheet.add("player_attack_2", 160, 0, 16, 32);
    sheet.add("player_hurt", 176, 0, 16, 32);
    sheet.add("player_dead", 192, 0, 16, 32);

    // Enemy sprites (16x32) — row 2
    sheet.add("skeleton_walk_0", 0, 64, 16, 32);
    sheet.add("skeleton_walk_1", 16, 64, 16, 32);
    sheet.add("bat_fly_0", 32, 64, 16, 32);
    sheet.add("bat_fly_1", 48, 64, 16, 32);
    sheet.add("knight_walk_0", 64, 64, 16, 32);
    sheet.add("knight_walk_1", 80, 64, 16, 32);
    sheet.add("medusa_float_0", 96, 64, 16, 32);
    sheet.add("medusa_float_1", 112, 64, 16, 32);
    sheet.add("projectile", 128, 64, 16, 16);

    // Tile sprites (16x16) — row 6
    sheet.add("stone_brick", 0, 96, 16, 16);
    sheet.add("platform", 16, 96, 16, 16);
    sheet.add("spikes", 32, 96, 16, 16);
    sheet.add("checkpoint_flag", 48, 96, 16, 16);
    sheet.add("finish_gate", 64, 96, 16, 16);
    sheet.add("ladder", 80, 96, 16, 16);
    sheet.add("breakable_wall", 96, 96, 16, 16);
    sheet.add("torch", 112, 96, 16, 16);
    sheet.add("stained_glass", 128, 96, 16, 16);

    // Power-up sprites (16x16) — row 7
    sheet.add("powerup_holy_water", 0, 112, 16, 16);
    sheet.add("powerup_crucifix", 16, 112, 16, 16);
    sheet.add("powerup_speed_boots", 32, 112, 16, 16);
    sheet.add("powerup_double_jump", 48, 112, 16, 16);
    sheet.add("powerup_armor", 64, 112, 16, 16);
    sheet.add("powerup_invincibility", 80, 112, 16, 16);
    sheet.add("powerup_whip_extend", 96, 112, 16, 16);

    // HUD sprites (16x16) — row 8
    sheet.add("heart_full", 0, 128, 16, 16);
    sheet.add("heart_empty", 16, 128, 16, 16);

    sheet
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
        assert!(sheet.get("stone_brick").is_some());
        assert!(sheet.get("nonexistent").is_none());
    }

    #[test]
    fn sprite_sheet_uv_coords() {
        let sheet = build_platformer_atlas();
        let r = sheet.get("player_idle_0").unwrap();
        // 0,0 -> 16,32 on 256x256 atlas
        assert!((r.u0 - 0.0).abs() < 1e-6);
        assert!((r.v0 - 0.0).abs() < 1e-6);
        assert!((r.u1 - 16.0 / 256.0).abs() < 1e-6);
        assert!((r.v1 - 32.0 / 256.0).abs() < 1e-6);
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
        let sheet = SpriteSheet::new(256, 256);
        let r = sheet.get_or_default("missing");
        assert!(r.u0 >= 0.0);
        assert!(r.u1 > 0.0);
    }
}
