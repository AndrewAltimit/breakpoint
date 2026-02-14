use std::collections::HashMap;

use bevy::prelude::*;

use breakpoint_core::game_trait::PlayerId;

/// Tracks previous-frame velocity per ball for bounce detection.
#[derive(Resource, Default)]
pub struct BallVelocityTracker {
    pub prev_velocities: HashMap<PlayerId, Vec3>,
}

/// Attached to ball entities when a bounce is detected.
#[derive(Component)]
pub struct SquashStretch {
    pub timer: f32,
    pub magnitude: f32,
    /// Bounce axis (normalized) — stretch happens along this axis.
    pub axis: Vec3,
}

const SQUASH_DURATION: f32 = 0.15;
const MIN_BOUNCE_SPEED: f32 = 1.0;

/// Detect bounces by comparing velocity sign flips between frames.
pub fn bounce_detect_system(
    mut commands: Commands,
    mut tracker: ResMut<BallVelocityTracker>,
    active_game: Res<crate::game::ActiveGame>,
    ball_query: Query<(Entity, &crate::game::golf_plugin::BallEntity)>,
) {
    let state: Option<breakpoint_golf::GolfState> = crate::game::read_game_state(&active_game);
    let Some(state) = state else {
        return;
    };

    for (entity, ball_entity) in &ball_query {
        let pid = ball_entity.0;
        let Some(ball) = state.balls.get(&pid) else {
            continue;
        };

        let current_vel = Vec3::new(ball.velocity.x, 0.0, ball.velocity.z);
        let speed = current_vel.length();

        if let Some(&prev_vel) = tracker.prev_velocities.get(&pid) {
            // Check for sign flip on X or Z axis (wall/bumper bounce)
            let x_flip = prev_vel.x * current_vel.x < 0.0 && prev_vel.x.abs() > MIN_BOUNCE_SPEED;
            let z_flip = prev_vel.z * current_vel.z < 0.0 && prev_vel.z.abs() > MIN_BOUNCE_SPEED;

            if (x_flip || z_flip) && speed > MIN_BOUNCE_SPEED {
                let magnitude = (speed * 0.04).clamp(0.05, 0.4);
                let axis = if x_flip && z_flip {
                    current_vel.normalize_or(Vec3::X)
                } else if x_flip {
                    Vec3::X
                } else {
                    Vec3::Z
                };
                commands.entity(entity).insert(SquashStretch {
                    timer: SQUASH_DURATION,
                    magnitude,
                    axis,
                });
            }
        }

        tracker.prev_velocities.insert(pid, current_vel);
    }
}

/// Animate squash/stretch on ball entities, then remove the component.
pub fn squash_stretch_animate_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut SquashStretch, &mut Transform)>,
) {
    for (entity, mut ss, mut transform) in &mut query {
        ss.timer -= time.delta_secs();
        if ss.timer <= 0.0 {
            transform.scale = Vec3::ONE;
            commands.entity(entity).remove::<SquashStretch>();
            continue;
        }

        let t = 1.0 - (ss.timer / SQUASH_DURATION);
        // Squash along bounce axis, stretch perpendicular
        let squash = 1.0 - ss.magnitude * (1.0 - t);
        let stretch = 1.0 + ss.magnitude * 0.5 * (1.0 - t);

        // Apply scale: squash along the dominant bounce axis, stretch others
        let sx = if ss.axis.x.abs() > 0.5 {
            squash
        } else {
            stretch
        };
        let sz = if ss.axis.z.abs() > 0.5 {
            squash
        } else {
            stretch
        };
        transform.scale = Vec3::new(sx, stretch, sz);
    }
}
