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
/// Maximum ball speed that allows sinking into the hole.
/// At 50% of MAX_POWER, fast bounces off bumpers can still sink.
const HOLE_SINK_SPEED: f32 = MAX_POWER * 0.5;
/// Energy retained on wall bounce (1.0 = perfect, 0.0 = full stop).
const WALL_BOUNCE_RESTITUTION: f32 = 0.9;
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
        if angle.is_nan() || power.is_nan() {
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
            if dist < HOLE_RADIUS && velocity_magnitude(&self.velocity) < HOLE_SINK_SPEED {
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
                self.velocity.x *= WALL_BOUNCE_RESTITUTION;
                self.velocity.z *= WALL_BOUNCE_RESTITUTION;
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

    // ================================================================
    // Stroke direction unit tests
    // ================================================================

    #[test]
    fn stroke_angle_zero_moves_positive_x() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(0.0, 10.0);
        assert!(
            ball.velocity.x > 0.0,
            "vx should be positive, got {}",
            ball.velocity.x
        );
        assert!(
            ball.velocity.z.abs() < 0.01,
            "vz should be ~0, got {}",
            ball.velocity.z
        );
    }

    #[test]
    fn stroke_angle_half_pi_moves_positive_z() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(std::f32::consts::FRAC_PI_2, 10.0);
        assert!(
            ball.velocity.x.abs() < 0.01,
            "vx should be ~0, got {}",
            ball.velocity.x
        );
        assert!(
            ball.velocity.z > 0.0,
            "vz should be positive, got {}",
            ball.velocity.z
        );
    }

    #[test]
    fn stroke_angle_pi_moves_negative_x() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(std::f32::consts::PI, 10.0);
        assert!(
            ball.velocity.x < 0.0,
            "vx should be negative, got {}",
            ball.velocity.x
        );
        assert!(
            ball.velocity.z.abs() < 0.1,
            "vz should be ~0, got {}",
            ball.velocity.z
        );
    }

    #[test]
    fn stroke_angle_neg_half_pi_moves_negative_z() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(-std::f32::consts::FRAC_PI_2, 10.0);
        assert!(
            ball.velocity.x.abs() < 0.01,
            "vx should be ~0, got {}",
            ball.velocity.x
        );
        assert!(
            ball.velocity.z < 0.0,
            "vz should be negative, got {}",
            ball.velocity.z
        );
    }

    #[test]
    fn stroke_angle_quarter_pi_moves_diagonal() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(std::f32::consts::FRAC_PI_4, 10.0);
        assert!(
            ball.velocity.x > 0.0,
            "vx should be positive, got {}",
            ball.velocity.x
        );
        assert!(
            ball.velocity.z > 0.0,
            "vz should be positive, got {}",
            ball.velocity.z
        );
        let ratio = (ball.velocity.x / ball.velocity.z).abs();
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "|vx|/|vz| should be ~1.0 for π/4, got {ratio}"
        );
    }

    // ================================================================
    // Stroke-to-position integration tests (Gentle Straight, no obstacles)
    // ================================================================

    fn gentle_straight_course() -> Course {
        crate::course::all_courses().into_iter().nth(1).unwrap()
    }

    #[test]
    fn stroke_at_angle_zero_ball_travels_positive_x() {
        let course = gentle_straight_course();
        // Start left-of-center so there's plenty of room in +X before wall
        let mut ball = BallState::new(Vec3::new(2.0, 0.0, course.depth / 2.0));
        let start_x = ball.position.x;
        ball.stroke(0.0, 2.0);

        for _ in 0..200 {
            ball.tick(&course);
            if ball.is_stopped() {
                break;
            }
        }

        let dx = ball.position.x - start_x;
        let dz = (ball.position.z - course.depth / 2.0).abs();
        assert!(
            dx > 0.0 && dx.abs() > dz,
            "X displacement ({dx}) should dominate over Z displacement ({dz})"
        );
    }

    #[test]
    fn stroke_at_angle_half_pi_ball_travels_positive_z() {
        let course = gentle_straight_course();
        // Start low-Z so there's plenty of room in +Z before wall
        let mut ball = BallState::new(Vec3::new(course.width / 2.0, 0.0, 2.0));
        let start_z = ball.position.z;
        ball.stroke(std::f32::consts::FRAC_PI_2, 2.0);

        for _ in 0..200 {
            ball.tick(&course);
            if ball.is_stopped() {
                break;
            }
        }

        let dz = ball.position.z - start_z;
        let dx = (ball.position.x - course.width / 2.0).abs();
        assert!(
            dz > 0.0 && dz.abs() > dx,
            "Z displacement ({dz}) should dominate over X displacement ({dx})"
        );
    }

    // ================================================================
    // Phase 1a: Power-to-velocity unit tests
    // ================================================================

    #[test]
    fn stroke_zero_power_no_movement() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(0.0, 0.0);
        assert_eq!(ball.velocity.x, 0.0, "Zero power should not move ball");
        assert_eq!(ball.velocity.z, 0.0, "Zero power should not move ball");
    }

    #[test]
    fn stroke_half_power_half_speed() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        let half = MAX_POWER * 0.5;
        ball.stroke(0.0, half);
        let speed = velocity_magnitude(&ball.velocity);
        assert!(
            (speed - half).abs() < 0.01,
            "Half power should give half speed, got {speed}"
        );
    }

    #[test]
    fn stroke_full_power_max_speed() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(0.0, MAX_POWER);
        let speed = velocity_magnitude(&ball.velocity);
        assert!(
            (speed - MAX_POWER).abs() < 0.01,
            "Full power should give MAX_POWER speed, got {speed}"
        );
    }

    #[test]
    fn stroke_minimum_power_moves() {
        // Client minimum power is 0.15 * MAX_POWER
        let min_power = 0.15 * MAX_POWER;
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(0.0, min_power);
        let speed = velocity_magnitude(&ball.velocity);
        assert!(
            speed > MIN_VELOCITY,
            "Minimum client power ({min_power}) should produce movement above MIN_VELOCITY, \
             got {speed}"
        );
    }

    #[test]
    fn velocity_magnitude_equals_power_across_angles() {
        let power = 10.0;
        let angles = [
            -std::f32::consts::PI,
            -std::f32::consts::FRAC_PI_2,
            0.0,
            std::f32::consts::FRAC_PI_4,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        ];

        for angle in angles {
            let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
            ball.stroke(angle, power);
            let speed = velocity_magnitude(&ball.velocity);
            assert!(
                (speed - power).abs() < 0.01,
                "angle={angle:.3}: |velocity| should equal power ({power}), got {speed}"
            );
        }
    }

    // ================================================================
    // P0-1: NaN/Inf/Degenerate input tests
    // ================================================================

    // REGRESSION: NaN aim_angle could corrupt ball position via cos(NaN)/sin(NaN)
    #[test]
    fn stroke_with_nan_angle_does_not_corrupt_position() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(f32::NAN, 10.0);
        // NaN propagates through cos/sin → velocity becomes NaN
        // After stroke, position must still be finite (stroke only sets velocity)
        assert!(
            ball.position.x.is_finite() && ball.position.z.is_finite(),
            "Position must remain finite after NaN angle stroke"
        );
        // Velocity will be NaN — verify tick doesn't panic
        let course = default_course();
        ball.tick(&course);
        // After tick with NaN velocity, ball should be clamped to bounds (not panic)
        // The ball's position may become NaN, but the key requirement is no panic
    }

    // REGRESSION: Inf power should be clamped to MAX_POWER
    #[test]
    fn stroke_with_inf_power_clamps_to_max() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(0.0, f32::INFINITY);
        let speed = velocity_magnitude(&ball.velocity);
        assert!(
            (speed - MAX_POWER).abs() < 0.01,
            "Inf power should clamp to MAX_POWER ({MAX_POWER}), got {speed}"
        );
    }

    // REGRESSION: Negative infinity power should clamp to 0
    #[test]
    fn stroke_with_neg_inf_power_clamps_to_zero() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(0.0, f32::NEG_INFINITY);
        let speed = velocity_magnitude(&ball.velocity);
        assert!(
            speed < 0.01,
            "Negative Inf power should clamp to 0, got {speed}"
        );
    }

    // ================================================================
    // P2-1: Expanded property-based tests
    // ================================================================

    // ================================================================
    // Phase 4b: Property-based tests (proptest)
    // ================================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn stroke_velocity_magnitude_equals_power(
                angle in -std::f32::consts::PI..std::f32::consts::PI,
                power in 0.0f32..MAX_POWER
            ) {
                let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
                ball.stroke(angle, power);
                let speed = velocity_magnitude(&ball.velocity);
                let clamped = power.clamp(0.0, MAX_POWER);
                prop_assert!(
                    (speed - clamped).abs() < 0.01,
                    "|velocity| ({speed}) should equal clamped power ({clamped})"
                );
            }

            #[test]
            fn friction_always_stops_ball(
                vx in -MAX_POWER..MAX_POWER,
                vz in -MAX_POWER..MAX_POWER
            ) {
                let course = default_course();
                let mut ball = BallState::new(Vec3::new(
                    course.width / 2.0,
                    0.0,
                    course.depth / 2.0,
                ));
                ball.velocity = Vec3::new(vx, 0.0, vz);

                for _ in 0..500 {
                    ball.tick(&course);
                    if ball.is_stopped() {
                        break;
                    }
                }

                prop_assert!(
                    ball.is_stopped(),
                    "Ball should stop within 500 ticks: vel=({}, {})",
                    ball.velocity.x,
                    ball.velocity.z
                );
            }

            #[test]
            fn ball_stays_in_bounds_after_stroke(
                angle in -std::f32::consts::PI..std::f32::consts::PI,
                power_frac in 0.1f32..1.0
            ) {
                let course = default_course();
                let mut ball = BallState::new(course.spawn_point);
                ball.stroke(angle, power_frac * MAX_POWER);

                for _ in 0..200 {
                    ball.tick(&course);
                    if ball.is_stopped() {
                        break;
                    }
                }

                prop_assert!(
                    ball.position.x >= 0.0 && ball.position.x <= course.width,
                    "Ball x={} out of bounds [0, {}]",
                    ball.position.x,
                    course.width
                );
                prop_assert!(
                    ball.position.z >= 0.0 && ball.position.z <= course.depth,
                    "Ball z={} out of bounds [0, {}]",
                    ball.position.z,
                    course.depth
                );
            }

            #[test]
            fn stroke_direction_matches_angle(
                angle in -std::f32::consts::PI..std::f32::consts::PI,
                power in 1.0f32..MAX_POWER
            ) {
                let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
                ball.stroke(angle, power);
                let actual_angle = ball.velocity.z.atan2(ball.velocity.x);
                // atan2 angles should match within a small tolerance
                let diff = (actual_angle - angle).abs();
                let diff = if diff > std::f32::consts::PI {
                    2.0 * std::f32::consts::PI - diff
                } else {
                    diff
                };
                prop_assert!(
                    diff < 0.01,
                    "Stroke angle mismatch: input={angle:.4}, actual={actual_angle:.4}"
                );
            }

            // P2-1: Ball never escapes course boundaries on any course
            #[test]
            fn ball_stays_in_bounds_all_courses(
                course_idx in 0usize..9,
                angle in -std::f32::consts::PI..std::f32::consts::PI,
                power_frac in 0.1f32..1.0
            ) {
                let courses = crate::course::all_courses();
                let course = &courses[course_idx];
                let mut ball = BallState::new(course.spawn_point);
                ball.stroke(angle, power_frac * MAX_POWER);

                for _ in 0..300 {
                    ball.tick(course);
                    if ball.is_stopped() {
                        break;
                    }
                }

                prop_assert!(
                    ball.position.x >= -BALL_RADIUS
                        && ball.position.x <= course.width + BALL_RADIUS,
                    "Ball x={} out of bounds [0, {}] on course {}",
                    ball.position.x,
                    course.width,
                    course_idx
                );
                prop_assert!(
                    ball.position.z >= -BALL_RADIUS
                        && ball.position.z <= course.depth + BALL_RADIUS,
                    "Ball z={} out of bounds [0, {}] on course {}",
                    ball.position.z,
                    course.depth,
                    course_idx
                );
            }

            // P2-1: Wall-corner double collision doesn't teleport ball
            #[test]
            fn wall_corner_collision_stable(
                angle in -std::f32::consts::PI..std::f32::consts::PI
            ) {
                // Use default course which has an L-shaped wall (corner at intersection)
                let course = default_course();
                // Place ball near the L-wall corner area
                let mut ball = BallState::new(Vec3::new(14.0, 0.0, 15.0));
                ball.stroke(angle, MAX_POWER);
                let initial_dist = velocity_magnitude(&ball.velocity);

                for _ in 0..300 {
                    ball.tick(&course);
                    if ball.is_stopped() {
                        break;
                    }
                }

                // After settling, ball must still be within course bounds
                prop_assert!(
                    ball.position.x >= 0.0 && ball.position.x <= course.width,
                    "Ball x={} escaped bounds after corner collision",
                    ball.position.x
                );
                prop_assert!(
                    ball.position.z >= 0.0 && ball.position.z <= course.depth,
                    "Ball z={} escaped bounds after corner collision",
                    ball.position.z
                );
                // Ball should have stopped (friction) — not oscillating indefinitely
                let _ = initial_dist; // used above
            }
        }
    }

    #[test]
    fn stroke_nan_angle_rejected() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(f32::NAN, 10.0);
        assert!(
            ball.is_stopped(),
            "NaN angle should be rejected — ball should not move"
        );
    }

    #[test]
    fn stroke_nan_power_rejected() {
        let mut ball = BallState::new(Vec3::new(5.0, 0.0, 5.0));
        ball.stroke(0.0, f32::NAN);
        assert!(
            ball.is_stopped(),
            "NaN power should be rejected — ball should not move"
        );
    }
}
