use breakpoint_core::game_trait::PlayerId;

use super::{CycleState, Direction, TronInput, TurnDirection, WallSegment};
use crate::collision::nearest_wall_distance;
use crate::config::TronConfig;

/// Apply a turn to the cycle (90 degrees left or right).
pub fn apply_turn(cycle: &mut CycleState, turn: TurnDirection, config: &TronConfig) {
    if cycle.turn_cooldown > 0.0 || turn == TurnDirection::None {
        return;
    }

    cycle.direction = match (cycle.direction, turn) {
        (Direction::North, TurnDirection::Left) => Direction::West,
        (Direction::North, TurnDirection::Right) => Direction::East,
        (Direction::South, TurnDirection::Left) => Direction::East,
        (Direction::South, TurnDirection::Right) => Direction::West,
        (Direction::East, TurnDirection::Left) => Direction::North,
        (Direction::East, TurnDirection::Right) => Direction::South,
        (Direction::West, TurnDirection::Left) => Direction::South,
        (Direction::West, TurnDirection::Right) => Direction::North,
        (_, TurnDirection::None) => return,
    };

    // Speed penalty for turning
    cycle.speed *= 1.0 - config.turn_speed_penalty;
    cycle.turn_cooldown = config.turn_delay;
}

/// Apply brake to the cycle.
pub fn apply_brake(cycle: &mut CycleState, dt: f32, config: &TronConfig) {
    if cycle.brake_fuel > 0.0 {
        cycle.brake_fuel = (cycle.brake_fuel - config.brake_drain_rate * dt).max(0.0);
        cycle.speed *= config.brake_speed_mult.powf(dt);
    }
}

/// Regenerate brake fuel when not braking.
pub fn regen_brake(cycle: &mut CycleState, dt: f32, config: &TronConfig) {
    cycle.brake_fuel = (cycle.brake_fuel + config.brake_regen_rate * dt).min(config.brake_fuel_max);
}

/// Compute wall acceleration (grinding) based on proximity to walls.
pub fn wall_acceleration(
    cycle: &CycleState,
    cycle_owner_id: PlayerId,
    walls: &[WallSegment],
    arena_width: f32,
    arena_depth: f32,
    config: &TronConfig,
) -> f32 {
    let Some(dist) = nearest_wall_distance(
        cycle,
        cycle_owner_id,
        walls,
        arena_width,
        arena_depth,
        config.grind_distance,
    ) else {
        return 0.0;
    };

    // Closer = more acceleration, inversely proportional
    // At collision_distance: max boost. At grind_distance: no boost.
    let range = config.grind_distance - config.collision_distance;
    if range <= 0.0 {
        return 0.0;
    }

    let normalized = ((dist - config.collision_distance) / range).clamp(0.0, 1.0);
    let boost_factor = 1.0 - normalized; // 1.0 at closest, 0.0 at threshold

    let max_accel = config.base_speed * (config.grind_max_multiplier - 1.0);
    boost_factor * max_accel
}

/// Update cycle position based on its direction and speed.
/// Returns the new wall segment endpoint if the cycle moved.
#[allow(clippy::too_many_arguments)]
pub fn update_cycle(
    cycle: &mut CycleState,
    cycle_owner_id: PlayerId,
    input: &TronInput,
    walls: &[WallSegment],
    arena_width: f32,
    arena_depth: f32,
    dt: f32,
    config: &TronConfig,
) -> Option<(f32, f32)> {
    if !cycle.alive {
        return None;
    }

    // Turn cooldown
    cycle.turn_cooldown = (cycle.turn_cooldown - dt).max(0.0);

    // Apply turn
    match input.turn {
        TurnDirection::Left => apply_turn(cycle, TurnDirection::Left, config),
        TurnDirection::Right => apply_turn(cycle, TurnDirection::Right, config),
        TurnDirection::None => {},
    }

    // Braking
    if input.brake {
        apply_brake(cycle, dt, config);
    } else {
        regen_brake(cycle, dt, config);
    }

    // Wall acceleration (grinding)
    let accel = wall_acceleration(
        cycle,
        cycle_owner_id,
        walls,
        arena_width,
        arena_depth,
        config,
    );
    cycle.speed += accel * dt;

    // Speed decay toward base speed (skip recovery when braking)
    if cycle.speed > config.base_speed {
        cycle.speed = (cycle.speed - config.speed_decay_rate * dt).max(config.base_speed);
    } else if cycle.speed < config.base_speed && !input.brake {
        // Fast recovery if below base speed (but not while braking)
        cycle.speed = (cycle.speed + config.speed_decay_rate * 2.0 * dt).min(config.base_speed);
    }

    // Clamp speed
    cycle.speed = cycle.speed.clamp(config.base_speed * 0.3, config.max_speed);

    // Move
    let distance = cycle.speed * dt;
    let (dx, dz) = match cycle.direction {
        Direction::North => (0.0, -distance),
        Direction::South => (0.0, distance),
        Direction::East => (distance, 0.0),
        Direction::West => (-distance, 0.0),
    };

    let old_x = cycle.x;
    let old_z = cycle.z;
    cycle.x += dx;
    cycle.z += dz;

    // Return the previous position as the start of the current segment
    if (old_x - cycle.x).abs() > 0.001 || (old_z - cycle.z).abs() > 0.001 {
        Some((old_x, old_z))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_cycle() -> CycleState {
        CycleState {
            x: 250.0,
            z: 250.0,
            direction: Direction::East,
            speed: 20.0,
            rubber: 0.5,
            brake_fuel: 3.0,
            alive: true,
            trail_start_index: 0,
            turn_cooldown: 0.0,
            kills: 0,
            died: false,
            is_suicide: false,
        }
    }

    fn no_input() -> TronInput {
        TronInput {
            turn: TurnDirection::None,
            brake: false,
        }
    }

    #[test]
    fn cycle_moves_forward() {
        let mut cycle = default_cycle();
        let config = TronConfig::default();
        let input = no_input();
        let x_before = cycle.x;

        update_cycle(&mut cycle, 1, &input, &[], 500.0, 500.0, 0.05, &config);

        assert!(cycle.x > x_before, "Cycle should move east");
    }

    #[test]
    fn turn_changes_direction() {
        let mut cycle = default_cycle();
        let config = TronConfig::default();

        apply_turn(&mut cycle, TurnDirection::Left, &config);
        assert_eq!(cycle.direction, Direction::North);

        cycle.turn_cooldown = 0.0;
        apply_turn(&mut cycle, TurnDirection::Right, &config);
        assert_eq!(cycle.direction, Direction::East);
    }

    #[test]
    fn turn_cooldown_prevents_rapid_turns() {
        let mut cycle = default_cycle();
        let config = TronConfig::default();

        apply_turn(&mut cycle, TurnDirection::Left, &config);
        assert_eq!(cycle.direction, Direction::North);

        // Should be blocked by cooldown
        apply_turn(&mut cycle, TurnDirection::Left, &config);
        assert_eq!(
            cycle.direction,
            Direction::North,
            "Should not turn during cooldown"
        );
    }

    #[test]
    fn brake_reduces_speed() {
        let mut cycle = default_cycle();
        let config = TronConfig::default();
        let speed_before = cycle.speed;

        apply_brake(&mut cycle, 0.05, &config);
        assert!(cycle.speed < speed_before, "Braking should reduce speed");
    }

    #[test]
    fn brake_depletes_fuel() {
        let mut cycle = default_cycle();
        let config = TronConfig::default();
        let fuel_before = cycle.brake_fuel;

        apply_brake(&mut cycle, 1.0, &config);
        assert!(
            cycle.brake_fuel < fuel_before,
            "Braking should consume fuel"
        );
    }

    #[test]
    fn brake_regen_when_not_braking() {
        let mut cycle = default_cycle();
        cycle.brake_fuel = 0.0;
        let config = TronConfig::default();

        regen_brake(&mut cycle, 1.0, &config);
        assert!(
            cycle.brake_fuel > 0.0,
            "Brake fuel should regenerate when not braking"
        );
    }

    // ================================================================
    // Phase 3: Grinding mechanic tests
    // ================================================================

    #[test]
    fn grind_boost_near_arena_boundary() {
        // Cycle near left arena boundary should get a speed boost
        let cycle = CycleState {
            x: 2.0, // close to left wall (within grind_distance=8.0)
            z: 250.0,
            ..default_cycle()
        };
        let config = TronConfig::default();
        let accel = wall_acceleration(&cycle, 1, &[], 500.0, 500.0, &config);
        assert!(
            accel > 0.0,
            "Cycle near arena boundary should get grind boost, got {accel}"
        );
    }

    #[test]
    fn grind_boost_increases_with_proximity() {
        let config = TronConfig::default();

        // Cycle at distance 2.0 from boundary
        let close_cycle = CycleState {
            x: 2.0,
            z: 250.0,
            ..default_cycle()
        };
        let close_accel = wall_acceleration(&close_cycle, 1, &[], 500.0, 500.0, &config);

        // Cycle at distance 6.0 from boundary
        let far_cycle = CycleState {
            x: 6.0,
            z: 250.0,
            ..default_cycle()
        };
        let far_accel = wall_acceleration(&far_cycle, 1, &[], 500.0, 500.0, &config);

        assert!(
            close_accel > far_accel,
            "Closer cycle should get more boost: close={close_accel}, far={far_accel}"
        );
    }

    #[test]
    fn no_grind_boost_beyond_threshold() {
        // Cycle far from any wall (center of arena)
        let cycle = CycleState {
            x: 250.0,
            z: 250.0,
            ..default_cycle()
        };
        let config = TronConfig::default();
        let accel = wall_acceleration(&cycle, 1, &[], 500.0, 500.0, &config);
        assert!(
            accel == 0.0,
            "Cycle far from walls should get no boost, got {accel}"
        );
    }

    #[test]
    fn grind_boost_from_parallel_trail_wall() {
        let config = TronConfig::default();
        let walls = vec![WallSegment {
            x1: 100.0,
            z1: 240.0, // vertical wall at x=100 (z: 240..260)
            x2: 100.0,
            z2: 260.0,
            owner_id: 2, // different owner
            is_active: false,
        }];

        // Cycle 3 units away, moving north (vertical wall = parallel)
        let cycle_nearby = CycleState {
            x: 103.0,
            z: 250.0,
            direction: Direction::North,
            ..default_cycle()
        };
        let accel = wall_acceleration(&cycle_nearby, 1, &walls, 500.0, 500.0, &config);
        assert!(
            accel > 0.0,
            "Cycle near parallel trail wall should get boost, got {accel}"
        );
    }

    #[test]
    fn no_grind_from_perpendicular_trail_wall() {
        let config = TronConfig::default();
        // Horizontal wall at z=253 — perpendicular to North/South movement
        let walls = vec![WallSegment {
            x1: 90.0,
            z1: 253.0,
            x2: 110.0,
            z2: 253.0,
            owner_id: 2,
            is_active: false,
        }];
        // Center cycle in arena to avoid boundary effects
        let cycle_center = CycleState {
            x: 250.0,
            z: 250.0,
            direction: Direction::North,
            ..default_cycle()
        };
        let accel_center = wall_acceleration(&cycle_center, 1, &walls, 500.0, 500.0, &config);
        assert!(
            accel_center == 0.0,
            "Perpendicular trail wall should not give grind boost, got {accel_center}"
        );
    }

    #[test]
    fn no_grind_from_own_active_segment() {
        let config = TronConfig::default();
        // Cycle's own active segment should be ignored
        let walls = vec![WallSegment {
            x1: 103.0,
            z1: 240.0,
            x2: 103.0,
            z2: 260.0,
            owner_id: 1, // same owner
            is_active: true,
        }];
        let cycle = CycleState {
            x: 250.0,
            z: 250.0,
            direction: Direction::North,
            ..default_cycle()
        };
        let accel = wall_acceleration(&cycle, 1, &walls, 500.0, 500.0, &config);
        assert!(
            accel == 0.0,
            "Own active segment should not give grind boost, got {accel}"
        );
    }

    // ================================================================
    // Phase 6: Property-based tests (proptest)
    // ================================================================

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn cycle_speed_stays_bounded(
                initial_speed in 5.0f32..200.0,
                dt in 0.01f32..0.1,
                brake in proptest::bool::ANY,
            ) {
                let config = TronConfig::default();
                let mut cycle = CycleState {
                    x: 250.0,
                    z: 250.0,
                    direction: Direction::East,
                    speed: initial_speed,
                    rubber: 0.5,
                    brake_fuel: 3.0,
                    alive: true,
                    trail_start_index: 0,
                    turn_cooldown: 0.0,
                    kills: 0,
                    died: false,
                    is_suicide: false,
                };
                let input = TronInput {
                    turn: TurnDirection::None,
                    brake,
                };

                update_cycle(&mut cycle, 1, &input, &[], 500.0, 500.0, dt, &config);

                prop_assert!(
                    cycle.speed >= config.base_speed * 0.3,
                    "Speed {} below minimum {}",
                    cycle.speed,
                    config.base_speed * 0.3
                );
                prop_assert!(
                    cycle.speed <= config.max_speed,
                    "Speed {} above maximum {}",
                    cycle.speed,
                    config.max_speed
                );
            }

            #[test]
            fn cycle_position_changes_each_tick(
                x in 50.0f32..450.0,
                z in 50.0f32..450.0,
                dt in 0.01f32..0.1,
            ) {
                let config = TronConfig::default();
                let mut cycle = CycleState {
                    x,
                    z,
                    direction: Direction::East,
                    speed: config.base_speed,
                    rubber: 0.5,
                    brake_fuel: 3.0,
                    alive: true,
                    trail_start_index: 0,
                    turn_cooldown: 0.0,
                    kills: 0,
                    died: false,
                    is_suicide: false,
                };
                let input = TronInput {
                    turn: TurnDirection::None,
                    brake: false,
                };
                let old_x = cycle.x;

                update_cycle(&mut cycle, 1, &input, &[], 500.0, 500.0, dt, &config);

                prop_assert!(
                    cycle.x > old_x,
                    "Cycle moving East should increase x: old={old_x}, new={}",
                    cycle.x
                );
            }

            #[test]
            fn brake_fuel_stays_bounded(
                fuel in 0.0f32..3.0,
                dt in 0.01f32..1.0,
                brake in proptest::bool::ANY,
            ) {
                let config = TronConfig::default();
                let mut cycle = CycleState {
                    x: 250.0,
                    z: 250.0,
                    direction: Direction::East,
                    speed: config.base_speed,
                    rubber: 0.5,
                    brake_fuel: fuel,
                    alive: true,
                    trail_start_index: 0,
                    turn_cooldown: 0.0,
                    kills: 0,
                    died: false,
                    is_suicide: false,
                };

                if brake {
                    apply_brake(&mut cycle, dt, &config);
                } else {
                    regen_brake(&mut cycle, dt, &config);
                }

                prop_assert!(
                    cycle.brake_fuel >= 0.0,
                    "Brake fuel {} should never go negative",
                    cycle.brake_fuel
                );
                prop_assert!(
                    cycle.brake_fuel <= config.brake_fuel_max,
                    "Brake fuel {} should not exceed max {}",
                    cycle.brake_fuel,
                    config.brake_fuel_max
                );
            }
        }
    }
}
