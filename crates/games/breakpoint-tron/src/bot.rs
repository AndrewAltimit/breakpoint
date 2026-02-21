use breakpoint_core::game_trait::PlayerId;

use crate::collision::point_to_segment_distance;
use crate::config::TronConfig;
use crate::{CycleState, Direction, TronInput, TronState, TurnDirection, WallSegment};

/// Tron tick rate (must match TronCycles::tick_rate()).
const TICK_RATE: f32 = 20.0;

/// How many ticks of travel ahead to consider "danger zone".
const DANGER_LOOK_AHEAD_TICKS: f32 = 5.0;

/// Generate a bot input for the given player based on the current game state.
pub fn generate_bot_input(state: &TronState, bot_id: PlayerId, config: &TronConfig) -> TronInput {
    let Some(cycle) = state.players.get(&bot_id) else {
        return TronInput::default();
    };
    if !cycle.alive {
        return TronInput::default();
    }

    let danger_dist = cycle.speed * DANGER_LOOK_AHEAD_TICKS / TICK_RATE;
    let straight = open_distance(
        cycle,
        bot_id,
        cycle.direction,
        &state.wall_segments,
        config,
        state,
    );
    let left_dir = turn_left(cycle.direction);
    let right_dir = turn_right(cycle.direction);
    let left = open_distance(cycle, bot_id, left_dir, &state.wall_segments, config, state);
    let right = open_distance(
        cycle,
        bot_id,
        right_dir,
        &state.wall_segments,
        config,
        state,
    );

    let mut turn = TurnDirection::None;
    let mut brake = false;

    if straight < danger_dist {
        // 2-step lookahead: simulate moving in each direction, then evaluate
        let left_score = left
            + second_step_best(
                cycle,
                bot_id,
                left_dir,
                left,
                &state.wall_segments,
                config,
                state,
            );
        let right_score = right
            + second_step_best(
                cycle,
                bot_id,
                right_dir,
                right,
                &state.wall_segments,
                config,
                state,
            );

        if left_score >= right_score {
            turn = TurnDirection::Left;
        } else {
            turn = TurnDirection::Right;
        }

        // If all directions are bad, brake
        if left < danger_dist && right < danger_dist {
            brake = true;
        }
    }

    // Slight randomness to prevent deterministic behavior
    let noise = pseudo_random(bot_id, state.round_timer);
    if turn == TurnDirection::None && noise < 0.02 && straight > danger_dist * 3.0 {
        if noise < 0.01 {
            turn = TurnDirection::Left;
        } else {
            turn = TurnDirection::Right;
        }
    }

    TronInput { turn, brake }
}

/// 2-step lookahead: simulate moving in `first_dir` for a short distance,
/// then return the best open distance from the simulated position.
fn second_step_best(
    cycle: &CycleState,
    bot_id: PlayerId,
    first_dir: Direction,
    first_open: f32,
    walls: &[WallSegment],
    config: &TronConfig,
    state: &TronState,
) -> f32 {
    // Simulate position after a short travel in first_dir
    let look_dist = first_open.min(cycle.speed * 3.0 / TICK_RATE);
    let (dx, dz) = direction_delta(first_dir);
    let sim = CycleState {
        x: cycle.x + dx * look_dist,
        z: cycle.z + dz * look_dist,
        direction: first_dir,
        ..*cycle
    };

    let s = open_distance(&sim, bot_id, first_dir, walls, config, state);
    let l = open_distance(&sim, bot_id, turn_left(first_dir), walls, config, state);
    let r = open_distance(&sim, bot_id, turn_right(first_dir), walls, config, state);
    s.max(l).max(r)
}

/// Measure open distance in a given direction from the cycle's current position.
/// Steps along the direction checking for wall collisions and arena boundary.
fn open_distance(
    cycle: &CycleState,
    owner_id: PlayerId,
    direction: Direction,
    walls: &[WallSegment],
    config: &TronConfig,
    state: &TronState,
) -> f32 {
    let step = config.collision_distance * 2.0;
    let max_steps = 200;
    let (dx, dz) = direction_delta(direction);

    for i in 1..=max_steps {
        let dist = step * i as f32;
        let probe_x = cycle.x + dx * dist;
        let probe_z = cycle.z + dz * dist;

        // Check arena boundary
        let margin = 0.1;
        if probe_x <= margin
            || probe_x >= state.arena_width - margin
            || probe_z <= margin
            || probe_z >= state.arena_depth - margin
        {
            return dist;
        }

        // Check wall collisions
        for wall in walls {
            // Skip our own active segment (the one currently being drawn)
            if wall.owner_id == owner_id && wall.is_active {
                continue;
            }

            let wall_dist =
                point_to_segment_distance(probe_x, probe_z, wall.x1, wall.z1, wall.x2, wall.z2);
            if wall_dist < config.collision_distance {
                return dist;
            }
        }
    }

    step * max_steps as f32
}

/// Get the unit direction vector for a given direction.
fn direction_delta(dir: Direction) -> (f32, f32) {
    match dir {
        Direction::North => (0.0, -1.0),
        Direction::South => (0.0, 1.0),
        Direction::East => (1.0, 0.0),
        Direction::West => (-1.0, 0.0),
    }
}

/// Turn left from the current direction.
fn turn_left(dir: Direction) -> Direction {
    match dir {
        Direction::North => Direction::West,
        Direction::South => Direction::East,
        Direction::East => Direction::North,
        Direction::West => Direction::South,
    }
}

/// Turn right from the current direction.
fn turn_right(dir: Direction) -> Direction {
    match dir {
        Direction::North => Direction::East,
        Direction::South => Direction::West,
        Direction::East => Direction::South,
        Direction::West => Direction::North,
    }
}

/// Simple deterministic pseudo-random float [0, 1) from bot_id + timer.
fn pseudo_random(bot_id: PlayerId, timer: f32) -> f32 {
    let bits = (bot_id as u32).wrapping_mul(2654435761) ^ (timer * 1000.0) as u32;
    (bits % 1000) as f32 / 1000.0
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use breakpoint_core::game_trait::PlayerInputs;
    use breakpoint_core::test_helpers::{default_config, make_players};

    use super::*;
    use crate::TronCycles;
    use crate::win_zone::WinZone;

    fn make_test_state() -> TronState {
        TronState {
            players: HashMap::new(),
            wall_segments: Vec::new(),
            round_timer: 0.0,
            round_complete: false,
            round_number: 1,
            scores: HashMap::new(),
            win_zone: WinZone::default(),
            alive_count: 0,
            arena_width: 500.0,
            arena_depth: 500.0,
            time_since_last_death: 0.0,
            winner_id: None,
        }
    }

    #[test]
    fn bot_returns_default_for_missing_player() {
        let state = make_test_state();
        let config = TronConfig::default();
        let input = generate_bot_input(&state, 999, &config);
        assert_eq!(input.turn, TurnDirection::None);
        assert!(!input.brake);
    }

    #[test]
    fn bot_returns_default_for_dead_player() {
        let mut state = make_test_state();
        state.players.insert(
            1,
            CycleState {
                x: 250.0,
                z: 250.0,
                direction: Direction::East,
                speed: 50.0,
                rubber: 0.5,
                brake_fuel: 3.0,
                alive: false,
                trail_start_index: 0,
                turn_cooldown: 0.0,
                kills: 0,
                died: true,
                is_suicide: false,
            },
        );
        let config = TronConfig::default();
        let input = generate_bot_input(&state, 1, &config);
        assert_eq!(input.turn, TurnDirection::None);
    }

    #[test]
    fn bot_turns_when_approaching_wall() {
        let mut state = make_test_state();
        state.players.insert(
            1,
            CycleState {
                x: 498.0,
                z: 250.0,
                direction: Direction::East,
                speed: 50.0,
                rubber: 0.5,
                brake_fuel: 3.0,
                alive: true,
                trail_start_index: 0,
                turn_cooldown: 0.0,
                kills: 0,
                died: false,
                is_suicide: false,
            },
        );
        state.alive_count = 1;
        let config = TronConfig::default();
        let input = generate_bot_input(&state, 1, &config);
        assert_ne!(
            input.turn,
            TurnDirection::None,
            "Bot should turn to avoid wall"
        );
    }

    #[test]
    fn bot_survives_many_ticks() {
        use breakpoint_core::game_trait::BreakpointGame;

        let mut game = TronCycles::default();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        let config = game.config().clone();
        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };

        for _ in 0..200 {
            let state_bytes = game.serialize_state();
            let state: TronState = rmp_serde::from_slice(&state_bytes).unwrap();

            let bot_input = generate_bot_input(&state, 1, &config);
            let input_bytes = rmp_serde::to_vec(&bot_input).unwrap();
            game.apply_input(1, &input_bytes);

            let bot_input2 = generate_bot_input(&state, 2, &config);
            let input_bytes2 = rmp_serde::to_vec(&bot_input2).unwrap();
            game.apply_input(2, &input_bytes2);

            game.update(0.05, &empty);

            if game.state().round_complete {
                break;
            }
        }
    }

    #[test]
    fn turn_left_right_directions() {
        assert_eq!(turn_left(Direction::North), Direction::West);
        assert_eq!(turn_left(Direction::West), Direction::South);
        assert_eq!(turn_left(Direction::South), Direction::East);
        assert_eq!(turn_left(Direction::East), Direction::North);

        assert_eq!(turn_right(Direction::North), Direction::East);
        assert_eq!(turn_right(Direction::East), Direction::South);
        assert_eq!(turn_right(Direction::South), Direction::West);
        assert_eq!(turn_right(Direction::West), Direction::North);
    }

    #[test]
    fn pseudo_random_bounded() {
        for id in 1..=100 {
            for timer_int in 0..100 {
                let val = pseudo_random(id, timer_int as f32 * 0.1);
                assert!((0.0..1.0).contains(&val));
            }
        }
    }
}
