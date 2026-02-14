use crate::course::{Bumper, Course, Vec3, Wall};

/// Ball radius in world units.
pub const BALL_RADIUS: f32 = 0.3;
/// Hole radius — ball sinks when center is within this distance.
pub const HOLE_RADIUS: f32 = 0.6;
/// Friction multiplier per tick (velocity *= FRICTION each tick).
/// At 10 Hz with 0.95 friction, a min-power ball stops in ~7s, max-power in ~12s.
pub const FRICTION: f32 = 0.95;
/// Maximum power a stroke can impart.
pub const MAX_POWER: f32 = 25.0;
/// Minimum velocity magnitude; below this the ball is considered stopped.
pub const MIN_VELOCITY: f32 = 0.1;
/// Physics substeps per tick for more accurate collision detection.
const SUBSTEPS: u32 = 4;

/// State of a single ball on the course.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct BallState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub is_sunk: bool,
}

impl BallState {
    pub fn new(spawn: Vec3) -> Self {
        Self {
            position: spawn,
            velocity: Vec3::ZERO,
            is_sunk: false,
        }
    }

    /// Whether the ball is effectively stationary.
    pub fn is_stopped(&self) -> bool {
        self.is_sunk || velocity_magnitude(&self.velocity) < MIN_VELOCITY
    }

    /// Apply a stroke impulse at the given angle (radians) and power (0..MAX_POWER).
    pub fn stroke(&mut self, angle: f32, power: f32) {
        if self.is_sunk || !self.is_stopped() {
            return;
        }
        let p = power.clamp(0.0, MAX_POWER);
        self.velocity.x = angle.cos() * p;
        self.velocity.z = angle.sin() * p;
    }

    /// Advance the ball by one tick on the given course.
    pub fn tick(&mut self, course: &Course) {
        if self.is_sunk {
            return;
        }

        let dt = 1.0 / SUBSTEPS as f32;
        for _ in 0..SUBSTEPS {
            if self.is_sunk {
                break;
            }

            // Move
            self.position.x += self.velocity.x * dt;
            self.position.z += self.velocity.z * dt;

            // Wall collisions
            for wall in &course.walls {
                self.collide_wall(wall);
            }

            // Bumper collisions
            for bumper in &course.bumpers {
                self.collide_bumper(bumper);
            }

            // Boundary clamping (safety net)
            self.clamp_to_bounds(course.width, course.depth);

            // Hole detection (ball near-stationary at hole)
            let dx = self.position.x - course.hole_position.x;
            let dz = self.position.z - course.hole_position.z;
            let dist = (dx * dx + dz * dz).sqrt();
            if dist < HOLE_RADIUS && velocity_magnitude(&self.velocity) < MAX_POWER * 0.4 {
                self.is_sunk = true;
                self.velocity = Vec3::ZERO;
                self.position = course.hole_position;
            }
        }

        // Apply friction
        self.velocity.x *= FRICTION;
        self.velocity.z *= FRICTION;

        // Stop if below threshold
        if velocity_magnitude(&self.velocity) < MIN_VELOCITY {
            self.velocity = Vec3::ZERO;
        }
    }

    fn collide_wall(&mut self, wall: &Wall) {
        // 2D line-segment collision on XZ plane
        let ax = wall.a.x;
        let az = wall.a.z;
        let bx = wall.b.x;
        let bz = wall.b.z;

        let dx = bx - ax;
        let dz = bz - az;
        let len_sq = dx * dx + dz * dz;
        if len_sq < 1e-6 {
            return;
        }

        // Project ball center onto wall segment
        let t = ((self.position.x - ax) * dx + (self.position.z - az) * dz) / len_sq;
        let t = t.clamp(0.0, 1.0);
        let closest_x = ax + t * dx;
        let closest_z = az + t * dz;

        let nx = self.position.x - closest_x;
        let nz = self.position.z - closest_z;
        let dist = (nx * nx + nz * nz).sqrt();

        if dist < BALL_RADIUS && dist > 1e-6 {
            // Normalize
            let inv = 1.0 / dist;
            let nx = nx * inv;
            let nz = nz * inv;

            // Push out
            let overlap = BALL_RADIUS - dist;
            self.position.x += nx * overlap;
            self.position.z += nz * overlap;

            // Reflect velocity
            let dot = self.velocity.x * nx + self.velocity.z * nz;
            if dot < 0.0 {
                self.velocity.x -= 2.0 * dot * nx;
                self.velocity.z -= 2.0 * dot * nz;
                // Slight energy loss on wall bounce
                self.velocity.x *= 0.9;
                self.velocity.z *= 0.9;
            }
        }
    }

    fn collide_bumper(&mut self, bumper: &Bumper) {
        let dx = self.position.x - bumper.position.x;
        let dz = self.position.z - bumper.position.z;
        let dist = (dx * dx + dz * dz).sqrt();
        let min_dist = BALL_RADIUS + bumper.radius;

        if dist < min_dist && dist > 1e-6 {
            let inv = 1.0 / dist;
            let nx = dx * inv;
            let nz = dz * inv;

            // Push out
            let overlap = min_dist - dist;
            self.position.x += nx * overlap;
            self.position.z += nz * overlap;

            // Bounce away at fixed speed
            self.velocity.x = nx * bumper.bounce_speed;
            self.velocity.z = nz * bumper.bounce_speed;
        }
    }

    fn clamp_to_bounds(&mut self, width: f32, depth: f32) {
        if self.position.x < BALL_RADIUS {
            self.position.x = BALL_RADIUS;
            self.velocity.x = self.velocity.x.abs();
        }
        if self.position.x > width - BALL_RADIUS {
            self.position.x = width - BALL_RADIUS;
            self.velocity.x = -self.velocity.x.abs();
        }
        if self.position.z < BALL_RADIUS {
            self.position.z = BALL_RADIUS;
            self.velocity.z = self.velocity.z.abs();
        }
        if self.position.z > depth - BALL_RADIUS {
            self.position.z = depth - BALL_RADIUS;
            self.velocity.z = -self.velocity.z.abs();
        }
    }
}

fn velocity_magnitude(v: &Vec3) -> f32 {
    (v.x * v.x + v.z * v.z).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::course::default_course;

    #[test]
    fn ball_stops_with_friction() {
        let course = default_course();
        let mut ball = BallState::new(course.spawn_point);
        ball.velocity = Vec3::new(5.0, 0.0, 0.0);

        for _ in 0..500 {
            ball.tick(&course);
        }

        assert!(
            ball.is_stopped(),
            "Ball should have stopped after many ticks, vel = {:?}",
            ball.velocity
        );
    }

    #[test]
    fn ball_reflects_off_wall() {
        let course = default_course();
        // Place ball near left wall, moving left
        let mut ball = BallState::new(Vec3::new(BALL_RADIUS + 0.1, 0.0, 5.0));
        ball.velocity = Vec3::new(-5.0, 0.0, 0.0);

        ball.tick(&course);

        // After collision with left wall, x-velocity should be positive
        assert!(
            ball.velocity.x > 0.0,
            "Ball should bounce off left wall, vx = {}",
            ball.velocity.x
        );
    }

    #[test]
    fn ball_sinks_in_hole() {
        let course = default_course();
        // Place ball at hole position with low velocity
        let mut ball = BallState::new(course.hole_position);
        ball.velocity = Vec3::new(0.1, 0.0, 0.0);

        ball.tick(&course);

        assert!(ball.is_sunk, "Ball should sink when near hole at low speed");
    }

    #[test]
    fn ball_does_not_sink_at_high_speed() {
        let course = default_course();
        // Place ball at hole position with high velocity
        let mut ball = BallState::new(course.hole_position);
        ball.velocity = Vec3::new(MAX_POWER * 0.5, 0.0, 0.0);

        ball.tick(&course);

        assert!(
            !ball.is_sunk,
            "Ball should not sink when moving fast over hole"
        );
    }

    #[test]
    fn stroke_only_when_stopped() {
        let course = default_course();
        let mut ball = BallState::new(course.spawn_point);
        ball.velocity = Vec3::new(5.0, 0.0, 0.0);

        // Should not apply stroke while moving
        ball.stroke(0.0, 10.0);
        assert!(
            (ball.velocity.x - 5.0).abs() < 0.01,
            "Stroke should not apply while ball is moving"
        );

        // Let ball stop
        for _ in 0..500 {
            ball.tick(&course);
        }
        assert!(ball.is_stopped());

        // Now stroke should work
        ball.stroke(0.0, 10.0);
        assert!(
            ball.velocity.x > 0.0,
            "Stroke should apply when ball is stopped"
        );
    }

    #[test]
    fn bumper_deflects_ball() {
        let course = default_course();
        let bumper = &course.bumpers[0];
        // Place ball right next to bumper, moving toward it
        let approach_x = bumper.position.x - bumper.radius - BALL_RADIUS + 0.1;
        let mut ball = BallState::new(Vec3::new(approach_x, 0.0, bumper.position.z));
        ball.velocity = Vec3::new(3.0, 0.0, 0.0);

        ball.tick(&course);

        // Ball should have been deflected away from bumper
        assert!(
            ball.position.x < bumper.position.x - bumper.radius || ball.velocity.x < 0.0,
            "Ball should be deflected by bumper"
        );
    }

    #[test]
    fn stroke_power_clamped() {
        let course = default_course();
        let mut ball = BallState::new(course.spawn_point);
        ball.stroke(0.0, MAX_POWER * 2.0);
        let speed = velocity_magnitude(&ball.velocity);
        assert!(
            (speed - MAX_POWER).abs() < 0.01,
            "Power should be clamped to MAX_POWER, got {speed}"
        );
    }
}
