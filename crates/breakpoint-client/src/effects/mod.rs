use glam::{Vec3, Vec4};

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

/// Full-screen color flash effect (damage flash, pickup flash, etc.).
#[derive(Default)]
pub struct ScreenFlash {
    pub active: bool,
    pub timer: f32,
    pub duration: f32,
    pub color: Vec4,
}

impl ScreenFlash {
    /// Trigger a screen flash with the given color and duration.
    pub fn trigger(&mut self, color: Vec4, duration: f32) {
        self.active = true;
        self.timer = duration;
        self.duration = duration;
        self.color = color;
    }

    pub fn tick(&mut self, dt: f32) {
        if !self.active {
            return;
        }
        self.timer -= dt;
        if self.timer <= 0.0 {
            self.active = false;
            self.timer = 0.0;
        }
    }

    /// Current alpha (fades from 1.0 to 0.0 over duration).
    pub fn alpha(&self) -> f32 {
        if !self.active || self.duration <= 0.0 {
            return 0.0;
        }
        (self.timer / self.duration).clamp(0.0, 1.0) * self.color.w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_flash_fades_out() {
        let mut flash = ScreenFlash::default();
        flash.trigger(Vec4::new(1.0, 0.0, 0.0, 0.5), 0.5);
        assert!(flash.active);
        assert!(flash.alpha() > 0.0);

        flash.tick(0.3);
        assert!(flash.active);
        let a = flash.alpha();
        assert!(a > 0.0 && a < 0.5);

        flash.tick(0.3);
        assert!(!flash.active);
        assert!((flash.alpha() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn screen_flash_inactive_by_default() {
        let flash = ScreenFlash::default();
        assert!(!flash.active);
        assert!((flash.alpha() - 0.0).abs() < 1e-6);
    }
}
