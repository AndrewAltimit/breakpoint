use glam::Vec3;

/// Screen shake effect state.
#[derive(Default)]
pub struct ScreenShake {
    pub timer: f32,
    pub intensity: f32,
    pub offset: Vec3,
}

impl ScreenShake {
    pub fn trigger(&mut self, intensity: f32, duration: f32) {
        self.intensity = intensity;
        self.timer = duration;
    }

    pub fn tick(&mut self, dt: f32) {
        if self.timer <= 0.0 {
            self.offset = Vec3::ZERO;
            return;
        }
        self.timer -= dt;
        let factor = (self.timer / 0.3).min(1.0);
        self.offset = Vec3::new(
            (fastrand::f32() - 0.5) * 2.0 * self.intensity * factor,
            (fastrand::f32() - 0.5) * 2.0 * self.intensity * factor,
            0.0,
        );
        if self.timer <= 0.0 {
            self.timer = 0.0;
            self.offset = Vec3::ZERO;
        }
    }
}
