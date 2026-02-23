use glam::{Vec3, Vec4};

use crate::scene::{MaterialType, MeshType, Scene, Transform};

/// Maximum number of active rain drops.
const MAX_RAIN_DROPS: usize = 100;

/// A single rain drop particle.
struct RainDrop {
    x: f32,
    y: f32,
    speed: f32,
    length: f32,
    active: bool,
}

/// Weather system for rain and atmospheric effects.
pub struct WeatherSystem {
    drops: Vec<RainDrop>,
    /// Whether rain is currently active.
    pub raining: bool,
    /// Ground fog density (0.0-1.0).
    pub fog_density: f32,
    /// Camera X for positioning rain relative to view.
    camera_x: f32,
    camera_y: f32,
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
        Self {
            drops,
            raining: false,
            fog_density: 0.0,
            camera_x: 0.0,
            camera_y: 5.0,
        }
    }

    /// Update camera position for rain positioning.
    pub fn set_camera(&mut self, x: f32, y: f32) {
        self.camera_x = x;
        self.camera_y = y;
    }

    /// Update all rain drops.
    pub fn tick(&mut self, dt: f32) {
        if !self.raining {
            // Deactivate all drops when rain stops
            for drop in &mut self.drops {
                drop.active = false;
            }
            return;
        }

        let view_half_w = 16.0;
        let view_half_h = 12.0;

        for drop in &mut self.drops {
            if !drop.active {
                // Spawn at top of screen with random X
                drop.x = self.camera_x + (fastrand::f32() - 0.5) * view_half_w * 2.0;
                drop.y = self.camera_y + view_half_h;
                drop.speed = 12.0 + fastrand::f32() * 6.0;
                drop.length = 0.3 + fastrand::f32() * 0.3;
                drop.active = true;
                continue;
            }

            // Fall diagonally (slight wind)
            drop.y -= drop.speed * dt;
            drop.x += drop.speed * 0.1 * dt;

            // Recycle when below screen
            if drop.y < self.camera_y - view_half_h {
                drop.active = false;
            }
        }
    }

    /// Render rain drops into the scene.
    pub fn render(&self, scene: &mut Scene) {
        if !self.raining {
            return;
        }

        for drop in &self.drops {
            if !drop.active {
                continue;
            }
            // Thin diagonal line rendered as a narrow stretched quad
            scene.add(
                MeshType::Quad,
                MaterialType::Unlit {
                    color: Vec4::new(0.7, 0.75, 0.9, 0.35),
                },
                Transform::from_xyz(drop.x, drop.y, 0.2).with_scale(Vec3::new(
                    0.04,
                    drop.length,
                    1.0,
                )),
            );
        }
    }
}
