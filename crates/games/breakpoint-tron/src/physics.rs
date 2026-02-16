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
}
