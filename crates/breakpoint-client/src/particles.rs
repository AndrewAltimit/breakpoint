use glam::Vec4;

use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::sprite_atlas::{SpriteRegion, SpriteSheet};

/// Maximum number of active particles (oldest recycled when full).
const MAX_PARTICLES: usize = 256;

/// A single visual particle.
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    lifetime: f32,
    max_lifetime: f32,
    sprite: SpriteRegion,
    tint: Vec4,
    size: f32,
    gravity: f32,
    active: bool,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            lifetime: 0.0,
            max_lifetime: 1.0,
            sprite: SpriteRegion {
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
            },
            tint: Vec4::ONE,
            size: 0.2,
            gravity: 0.0,
            active: false,
        }
    }
}

/// Types of particle effects that can be emitted.
pub enum ParticleEffect {
    DustLanding,
    SparkHit,
    BloodDamage,
    TorchFire,
    EnemyDeath,
    PowerUpCollect,
    CheckpointActivate,
    GenericBurst { color: Vec4, count: u8 },
}

/// Lightweight particle system for visual effects.
pub struct ParticleSystem {
    particles: Vec<Particle>,
    /// Ring-buffer index for recycling oldest particles.
    next_slot: usize,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSystem {
    pub fn new() -> Self {
        let mut particles = Vec::with_capacity(MAX_PARTICLES);
        for _ in 0..MAX_PARTICLES {
            particles.push(Particle::default());
        }
        Self {
            particles,
            next_slot: 0,
        }
    }

    /// Emit particles for a given effect at world position (x, y).
    pub fn emit(&mut self, effect: ParticleEffect, x: f32, y: f32, sheet: &SpriteSheet) {
        match effect {
            ParticleEffect::DustLanding => self.emit_dust(x, y, sheet),
            ParticleEffect::SparkHit => self.emit_sparks(x, y, sheet),
            ParticleEffect::BloodDamage => self.emit_blood(x, y, sheet),
            ParticleEffect::TorchFire => self.emit_fire(x, y, sheet),
            ParticleEffect::EnemyDeath => self.emit_enemy_death(x, y, sheet),
            ParticleEffect::PowerUpCollect => self.emit_powerup(x, y, sheet),
            ParticleEffect::CheckpointActivate => self.emit_checkpoint(x, y, sheet),
            ParticleEffect::GenericBurst { color, count } => {
                self.emit_burst(x, y, color, count, sheet);
            },
        }
    }

    /// Update all particles by dt seconds.
    pub fn tick(&mut self, dt: f32) {
        for p in &mut self.particles {
            if !p.active {
                continue;
            }
            p.lifetime -= dt;
            if p.lifetime <= 0.0 {
                p.active = false;
                continue;
            }
            p.vy += p.gravity * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }
    }

    /// Add all active particles to the scene.
    pub fn render(&self, scene: &mut Scene) {
        for p in &self.particles {
            if !p.active {
                continue;
            }
            // Alpha fades linearly over lifetime
            let alpha = (p.lifetime / p.max_lifetime).clamp(0.0, 1.0);
            let tint = Vec4::new(p.tint.x, p.tint.y, p.tint.z, p.tint.w * alpha);
            scene.add(
                MeshType::Quad,
                MaterialType::Sprite {
                    atlas_id: 0,
                    sprite_rect: p.sprite.to_vec4(),
                    tint,
                    flip_x: false,
                },
                Transform::from_xyz(p.x, p.y, 0.1).with_scale(glam::Vec3::new(p.size, p.size, 1.0)),
            );
        }
    }

    /// Allocate a particle slot (recycles oldest when full).
    fn alloc(&mut self) -> &mut Particle {
        let idx = self.next_slot;
        self.next_slot = (self.next_slot + 1) % MAX_PARTICLES;
        let p = &mut self.particles[idx];
        *p = Particle::default();
        p.active = true;
        p
    }

    fn emit_dust(&mut self, x: f32, y: f32, sheet: &SpriteSheet) {
        for i in 0..4 {
            let p = self.alloc();
            p.x = x + rand_spread(0.3);
            p.y = y;
            p.vx = rand_spread(1.0);
            p.vy = 0.5 + fastrand::f32() * 0.5;
            p.lifetime = 0.3 + fastrand::f32() * 0.2;
            p.max_lifetime = p.lifetime;
            p.sprite = sheet.get_or_default(dust_frame(i));
            p.tint = Vec4::new(0.8, 0.75, 0.65, 0.8);
            p.size = 0.15 + fastrand::f32() * 0.1;
            p.gravity = -2.0;
        }
    }

    fn emit_sparks(&mut self, x: f32, y: f32, sheet: &SpriteSheet) {
        for i in 0..5 {
            let p = self.alloc();
            p.x = x;
            p.y = y;
            let angle = fastrand::f32() * std::f32::consts::TAU;
            let speed = 2.0 + fastrand::f32() * 2.0;
            p.vx = angle.cos() * speed;
            p.vy = angle.sin() * speed;
            p.lifetime = 0.2 + fastrand::f32() * 0.15;
            p.max_lifetime = p.lifetime;
            p.sprite = sheet.get_or_default(spark_frame(i));
            p.tint = Vec4::new(1.0, 0.9, 0.3, 1.0);
            p.size = 0.1 + fastrand::f32() * 0.08;
            p.gravity = -3.0;
        }
    }

    fn emit_blood(&mut self, x: f32, y: f32, sheet: &SpriteSheet) {
        for i in 0..6 {
            let p = self.alloc();
            p.x = x + rand_spread(0.2);
            p.y = y + rand_spread(0.3);
            let angle = fastrand::f32() * std::f32::consts::TAU;
            let speed = 1.5 + fastrand::f32() * 1.5;
            p.vx = angle.cos() * speed;
            p.vy = angle.sin() * speed;
            p.lifetime = 0.4 + fastrand::f32() * 0.2;
            p.max_lifetime = p.lifetime;
            p.sprite = sheet.get_or_default(blood_frame(i));
            p.tint = Vec4::new(0.9, 0.1, 0.1, 1.0);
            p.size = 0.12 + fastrand::f32() * 0.08;
            p.gravity = -5.0;
        }
    }

    fn emit_fire(&mut self, x: f32, y: f32, sheet: &SpriteSheet) {
        for i in 0..3 {
            let p = self.alloc();
            p.x = x + rand_spread(0.15);
            p.y = y + 0.3;
            p.vx = rand_spread(0.3);
            p.vy = 1.0 + fastrand::f32() * 0.5;
            p.lifetime = 0.3 + fastrand::f32() * 0.2;
            p.max_lifetime = p.lifetime;
            p.sprite = sheet.get_or_default(fire_frame(i));
            p.tint = Vec4::new(1.0, 0.7, 0.2, 0.9);
            p.size = 0.1 + fastrand::f32() * 0.1;
            p.gravity = 1.0;
        }
    }

    fn emit_enemy_death(&mut self, x: f32, y: f32, sheet: &SpriteSheet) {
        for i in 0..8 {
            let p = self.alloc();
            p.x = x + rand_spread(0.3);
            p.y = y + rand_spread(0.5);
            let angle = fastrand::f32() * std::f32::consts::TAU;
            let speed = 2.0 + fastrand::f32() * 2.0;
            p.vx = angle.cos() * speed;
            p.vy = angle.sin() * speed;
            p.lifetime = 0.5 + fastrand::f32() * 0.3;
            p.max_lifetime = p.lifetime;
            p.sprite = sheet.get_or_default(smoke_frame(i));
            p.tint = Vec4::new(0.6, 0.5, 0.7, 0.9);
            p.size = 0.15 + fastrand::f32() * 0.1;
            p.gravity = 1.0;
        }
    }

    fn emit_powerup(&mut self, x: f32, y: f32, sheet: &SpriteSheet) {
        for i in 0..6 {
            let p = self.alloc();
            p.x = x;
            p.y = y;
            let angle = (i as f32 / 6.0) * std::f32::consts::TAU;
            let speed = 1.5 + fastrand::f32();
            p.vx = angle.cos() * speed;
            p.vy = angle.sin() * speed;
            p.lifetime = 0.4 + fastrand::f32() * 0.2;
            p.max_lifetime = p.lifetime;
            p.sprite = sheet.get_or_default(magic_frame(i));
            p.tint = Vec4::new(0.3, 1.0, 0.5, 1.0);
            p.size = 0.12;
            p.gravity = 0.0;
        }
    }

    fn emit_checkpoint(&mut self, x: f32, y: f32, sheet: &SpriteSheet) {
        for i in 0..5 {
            let p = self.alloc();
            p.x = x + rand_spread(0.2);
            p.y = y;
            p.vx = rand_spread(0.5);
            p.vy = 2.0 + fastrand::f32() * 1.0;
            p.lifetime = 0.5 + fastrand::f32() * 0.3;
            p.max_lifetime = p.lifetime;
            p.sprite = sheet.get_or_default(spark_frame(i));
            p.tint = Vec4::new(1.0, 0.9, 0.3, 1.0);
            p.size = 0.1;
            p.gravity = -1.0;
        }
    }

    fn emit_burst(&mut self, x: f32, y: f32, color: Vec4, count: u8, sheet: &SpriteSheet) {
        for i in 0..count {
            let p = self.alloc();
            p.x = x;
            p.y = y;
            let angle = (i as f32 / count as f32) * std::f32::consts::TAU;
            let speed = 2.0 + fastrand::f32() * 1.5;
            p.vx = angle.cos() * speed;
            p.vy = angle.sin() * speed;
            p.lifetime = 0.4 + fastrand::f32() * 0.2;
            p.max_lifetime = p.lifetime;
            p.sprite = sheet.get_or_default(magic_frame(i as usize));
            p.tint = color;
            p.size = 0.12 + fastrand::f32() * 0.08;
            p.gravity = -1.0;
        }
    }
}

/// Random spread value in [-half, +half].
fn rand_spread(half: f32) -> f32 {
    (fastrand::f32() - 0.5) * 2.0 * half
}

/// Cycle through dust particle sprite frames.
fn dust_frame(i: usize) -> &'static str {
    match i % 4 {
        0 => "particle_dust_0",
        1 => "particle_dust_1",
        2 => "particle_dust_2",
        _ => "particle_dust_3",
    }
}

fn spark_frame(i: usize) -> &'static str {
    match i % 3 {
        0 => "particle_spark_0",
        1 => "particle_spark_1",
        _ => "particle_spark_2",
    }
}

fn blood_frame(i: usize) -> &'static str {
    match i % 3 {
        0 => "particle_blood_0",
        1 => "particle_blood_1",
        _ => "particle_blood_2",
    }
}

fn fire_frame(i: usize) -> &'static str {
    match i % 4 {
        0 => "particle_fire_0",
        1 => "particle_fire_1",
        2 => "particle_fire_2",
        _ => "particle_fire_3",
    }
}

fn smoke_frame(i: usize) -> &'static str {
    match i % 3 {
        0 => "particle_smoke_0",
        1 => "particle_smoke_1",
        _ => "particle_smoke_2",
    }
}

fn magic_frame(i: usize) -> &'static str {
    match i % 3 {
        0 => "particle_magic_0",
        1 => "particle_magic_1",
        _ => "particle_magic_2",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite_atlas::build_platformer_atlas;

    #[test]
    fn particle_system_new_has_capacity() {
        let ps = ParticleSystem::new();
        assert_eq!(ps.particles.len(), MAX_PARTICLES);
    }

    #[test]
    fn particle_system_tick_deactivates_expired() {
        let mut ps = ParticleSystem::new();
        let sheet = build_platformer_atlas();
        ps.emit(ParticleEffect::DustLanding, 5.0, 3.0, &sheet);

        let active_before = ps.particles.iter().filter(|p| p.active).count();
        assert!(active_before > 0);

        // Tick past all lifetimes
        for _ in 0..20 {
            ps.tick(0.1);
        }

        let active_after = ps.particles.iter().filter(|p| p.active).count();
        assert_eq!(active_after, 0);
    }

    #[test]
    fn particle_system_recycles_slots() {
        let mut ps = ParticleSystem::new();
        let sheet = build_platformer_atlas();

        // Emit more particles than the cap
        for i in 0..300 {
            ps.emit(ParticleEffect::SparkHit, i as f32, 0.0, &sheet);
        }

        // Should not exceed MAX_PARTICLES
        assert_eq!(ps.particles.len(), MAX_PARTICLES);
    }

    #[test]
    fn generic_burst_emits_correct_count() {
        let mut ps = ParticleSystem::new();
        let sheet = build_platformer_atlas();
        ps.emit(
            ParticleEffect::GenericBurst {
                color: Vec4::ONE,
                count: 10,
            },
            0.0,
            0.0,
            &sheet,
        );
        let active = ps.particles.iter().filter(|p| p.active).count();
        assert_eq!(active, 10);
    }
}
