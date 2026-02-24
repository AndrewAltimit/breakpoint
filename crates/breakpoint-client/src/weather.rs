use glam::{Vec3, Vec4};

use crate::scene::{MaterialType, MeshType, Scene, Transform};

/// Maximum number of active rain drops (reduced from 120 for draw call budget).
const MAX_RAIN_DROPS: usize = 60;

/// Maximum number of ambient particles (reduced from 40 for draw call budget).
const MAX_AMBIENT_PARTICLES: usize = 20;

/// A single rain drop particle.
struct RainDrop {
    x: f32,
    y: f32,
    speed: f32,
    length: f32,
    active: bool,
}

/// Ambient particle (dust motes, embers, sparkles, etc.).
struct AmbientParticle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    alpha: f32,
    lifetime: f32,
    max_lifetime: f32,
    color: Vec4,
    active: bool,
}

/// Ambient particle type determined by room theme.
#[derive(Clone, Copy, PartialEq)]
pub enum AmbientType {
    None,
    DustMotes,
    GoldenSparkles,
    Embers,
    Snowflakes,
    FloatingPages,
    RoyalSparkles,
}

/// Weather system for rain, lightning, fog, and atmospheric effects.
pub struct WeatherSystem {
    drops: Vec<RainDrop>,
    ambient: Vec<AmbientParticle>,
    /// Whether rain is currently active.
    pub raining: bool,
    /// Ground fog density (0.0-1.0).
    pub fog_density: f32,
    /// Per-room fog color (RGB).
    pub fog_color: [f32; 3],
    /// Camera X for positioning rain relative to view.
    camera_x: f32,
    camera_y: f32,
    /// Lightning flash timer (counts down from flash duration).
    lightning_timer: f32,
    /// Lightning flash intensity (0.0-1.0).
    pub lightning_intensity: f32,
    /// Time until next lightning strike.
    next_lightning: f32,
    /// Current ambient particle type.
    pub ambient_type: AmbientType,
}

impl Default for WeatherSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl WeatherSystem {
    pub fn new() -> Self {
        let mut drops = Vec::with_capacity(MAX_RAIN_DROPS);
        for _ in 0..MAX_RAIN_DROPS {
            drops.push(RainDrop {
                x: 0.0,
                y: 0.0,
                speed: 0.0,
                length: 0.0,
                active: false,
            });
        }
        let mut ambient = Vec::with_capacity(MAX_AMBIENT_PARTICLES);
        for _ in 0..MAX_AMBIENT_PARTICLES {
            ambient.push(AmbientParticle {
                x: 0.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                size: 0.0,
                alpha: 0.0,
                lifetime: 0.0,
                max_lifetime: 1.0,
                color: Vec4::ZERO,
                active: false,
            });
        }
        Self {
            drops,
            ambient,
            raining: false,
            fog_density: 0.0,
            fog_color: [0.08, 0.06, 0.12],
            camera_x: 0.0,
            camera_y: 5.0,
            lightning_timer: 0.0,
            lightning_intensity: 0.0,
            next_lightning: 3.0 + fastrand::f32() * 5.0,
            ambient_type: AmbientType::None,
        }
    }

    /// Update camera position for rain positioning.
    pub fn set_camera(&mut self, x: f32, y: f32) {
        self.camera_x = x;
        self.camera_y = y;
    }

    /// Update all weather effects.
    pub fn tick(&mut self, dt: f32) {
        self.tick_rain(dt);
        self.tick_lightning(dt);
        self.tick_ambient(dt);
    }

    fn tick_rain(&mut self, dt: f32) {
        if !self.raining {
            for drop in &mut self.drops {
                drop.active = false;
            }
            return;
        }

        let view_half_w = 16.0;
        let view_half_h = 12.0;

        for drop in &mut self.drops {
            if !drop.active {
                drop.x = self.camera_x + (fastrand::f32() - 0.5) * view_half_w * 2.0;
                drop.y = self.camera_y + view_half_h;
                drop.speed = 14.0 + fastrand::f32() * 8.0;
                drop.length = 0.3 + fastrand::f32() * 0.3;
                drop.active = true;
                continue;
            }

            // Fall diagonally (slight wind)
            drop.y -= drop.speed * dt;
            drop.x += drop.speed * 0.08 * dt;

            if drop.y < self.camera_y - view_half_h {
                drop.active = false;
            }
        }
    }

    fn tick_lightning(&mut self, dt: f32) {
        if !self.raining {
            self.lightning_intensity = 0.0;
            return;
        }

        if self.lightning_timer > 0.0 {
            self.lightning_timer -= dt;
            // Flash + afterglow: bright flash for 0.1s, then fade 0.3s
            if self.lightning_timer > 0.3 {
                self.lightning_intensity = 1.0;
            } else {
                self.lightning_intensity = (self.lightning_timer / 0.3).max(0.0);
            }
        } else {
            self.lightning_intensity = 0.0;
            self.next_lightning -= dt;
            if self.next_lightning <= 0.0 {
                self.lightning_timer = 0.4; // 0.1s flash + 0.3s afterglow
                self.next_lightning = 4.0 + fastrand::f32() * 8.0;
            }
        }
    }

    fn tick_ambient(&mut self, dt: f32) {
        if self.ambient_type == AmbientType::None {
            for p in &mut self.ambient {
                p.active = false;
            }
            return;
        }

        let view_half_w = 14.0;
        let view_half_h = 10.0;

        let cam_x = self.camera_x;
        let cam_y = self.camera_y;
        let atype = self.ambient_type;
        for p in &mut self.ambient {
            if !p.active {
                spawn_ambient_particle(p, cam_x, cam_y, atype, view_half_w, view_half_h);
                continue;
            }

            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.lifetime -= dt;

            // Fade in/out
            let life_ratio = p.lifetime / p.max_lifetime;
            p.alpha = if life_ratio > 0.8 {
                (1.0 - life_ratio) * 5.0
            } else if life_ratio < 0.2 {
                life_ratio * 5.0
            } else {
                1.0
            };

            if p.lifetime <= 0.0
                || (p.x - self.camera_x).abs() > view_half_w + 2.0
                || (p.y - self.camera_y).abs() > view_half_h + 2.0
            {
                p.active = false;
            }
        }
    }
}

fn spawn_ambient_particle(
    p: &mut AmbientParticle,
    camera_x: f32,
    camera_y: f32,
    ambient_type: AmbientType,
    view_half_w: f32,
    view_half_h: f32,
) {
    p.x = camera_x + (fastrand::f32() - 0.5) * view_half_w * 2.0;
    p.y = camera_y + (fastrand::f32() - 0.5) * view_half_h * 2.0;
    p.lifetime = 2.0 + fastrand::f32() * 3.0;
    p.max_lifetime = p.lifetime;
    p.active = true;

    match ambient_type {
        AmbientType::DustMotes => {
            p.vx = (fastrand::f32() - 0.5) * 0.3;
            p.vy = (fastrand::f32() - 0.5) * 0.2;
            p.size = 0.04 + fastrand::f32() * 0.03;
            p.color = Vec4::new(0.5, 0.4, 0.35, 0.25);
        },
        AmbientType::GoldenSparkles => {
            p.vx = (fastrand::f32() - 0.5) * 0.2;
            p.vy = 0.2 + fastrand::f32() * 0.3;
            p.size = 0.03 + fastrand::f32() * 0.02;
            p.color = Vec4::new(1.0, 0.9, 0.5, 0.5);
        },
        AmbientType::Embers => {
            p.vx = (fastrand::f32() - 0.5) * 0.4;
            p.vy = 0.5 + fastrand::f32() * 0.8;
            p.size = 0.03 + fastrand::f32() * 0.03;
            p.color = Vec4::new(1.0, 0.45, 0.15, 0.65);
        },
        AmbientType::Snowflakes => {
            p.vx = 0.3 + fastrand::f32() * 0.5;
            p.vy = -(0.5 + fastrand::f32() * 0.3);
            p.size = 0.04 + fastrand::f32() * 0.04;
            p.color = Vec4::new(0.9, 0.9, 1.0, 0.4);
        },
        AmbientType::FloatingPages => {
            p.vx = (fastrand::f32() - 0.5) * 0.3;
            p.vy = -(0.1 + fastrand::f32() * 0.2);
            p.size = 0.06 + fastrand::f32() * 0.04;
            p.color = Vec4::new(0.9, 0.85, 0.7, 0.4);
        },
        AmbientType::RoyalSparkles => {
            p.vx = (fastrand::f32() - 0.5) * 0.2;
            p.vy = (fastrand::f32() - 0.3) * 0.3;
            p.size = 0.03 + fastrand::f32() * 0.02;
            p.color = Vec4::new(0.8, 0.5, 1.0, 0.5);
        },
        AmbientType::None => {},
    }
}

impl WeatherSystem {
    /// Render all weather effects into the scene.
    pub fn render(&self, scene: &mut Scene) {
        self.render_rain(scene);
        self.render_ambient(scene);
        self.render_fog(scene);
    }

    fn render_rain(&self, scene: &mut Scene) {
        if !self.raining {
            return;
        }

        for drop in &self.drops {
            if !drop.active {
                continue;
            }
            // Sprite-based rain droplets (slightly wider for visibility with fewer drops)
            scene.add(
                MeshType::Quad,
                MaterialType::Unlit {
                    color: Vec4::new(0.7, 0.75, 0.9, 0.4),
                },
                Transform::from_xyz(drop.x, drop.y, 0.2).with_scale(Vec3::new(
                    0.06,
                    drop.length * 1.2,
                    1.0,
                )),
            );
        }
    }

    fn render_ambient(&self, scene: &mut Scene) {
        for p in &self.ambient {
            if !p.active || p.alpha < 0.01 {
                continue;
            }
            let color = Vec4::new(p.color.x, p.color.y, p.color.z, p.color.w * p.alpha);
            scene.add(
                MeshType::Quad,
                MaterialType::Glow {
                    color,
                    intensity: 0.8,
                },
                Transform::from_xyz(p.x, p.y, 0.15).with_scale(Vec3::new(p.size, p.size, 1.0)),
            );
        }
    }

    fn render_fog(&self, scene: &mut Scene) {
        if self.fog_density < 0.01 {
            return;
        }
        // Ground fog layer covering the lower portion of the view
        scene.add(
            MeshType::Quad,
            MaterialType::FogLayer {
                density: self.fog_density,
                color: Vec4::new(self.fog_color[0], self.fog_color[1], self.fog_color[2], 0.5),
            },
            Transform::from_xyz(
                self.camera_x,
                self.camera_y - 3.0,
                crate::game::platformer_render::Z_FOG,
            )
            .with_scale(Vec3::new(40.0, 8.0, 1.0)),
        );
    }
}
