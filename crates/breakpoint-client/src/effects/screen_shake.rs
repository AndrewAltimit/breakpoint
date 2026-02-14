use bevy::prelude::*;

/// Screen shake state — triggered by golf strokes, decays over time.
#[derive(Resource, Default)]
pub struct ScreenShake {
    pub intensity: f32,
    pub timer: f32,
    pub duration: f32,
    pub offset: Vec3,
}

/// Decays screen shake over time, computing a random offset each frame.
pub fn screen_shake_decay_system(mut shake: ResMut<ScreenShake>, time: Res<Time>) {
    if shake.timer <= 0.0 {
        shake.offset = Vec3::ZERO;
        return;
    }

    shake.timer -= time.delta_secs();
    if shake.timer <= 0.0 {
        shake.timer = 0.0;
        shake.offset = Vec3::ZERO;
        return;
    }

    let progress = shake.timer / shake.duration.max(0.001);
    let scale = shake.intensity * progress;

    // Random offset via fastrand — deterministic per frame, no seed needed
    let rx = (fastrand::f32() - 0.5) * 2.0;
    let ry = (fastrand::f32() - 0.5) * 2.0;
    shake.offset = Vec3::new(rx * scale, ry * scale * 0.5, 0.0);
}
