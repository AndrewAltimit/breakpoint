pub mod course;
pub mod physics;
pub mod scoring;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use breakpoint_core::breakpoint_game_boilerplate;
use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameEvent, GameMetadata, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::player::Player;

use course::{Course, all_courses, load_courses_from_dir};
use physics::{BallState, GolfConfig};
use scoring::calculate_score_with_config;

/// Serializable game state broadcast from host to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GolfState {
    pub balls: HashMap<PlayerId, BallState>,
    pub strokes: HashMap<PlayerId, u32>,
    pub sunk_order: Vec<PlayerId>,
    pub round_timer: f32,
    pub round_complete: bool,
    /// Which course (0-indexed) is currently being played.
    pub course_index: u8,
}

/// Input from a single player for a stroke.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GolfInput {
    /// Aim angle in radians (0 = +X direction).
    pub aim_angle: f32,
    /// Stroke power (0.0 to 1.0, scaled to MAX_POWER internally).
    pub power: f32,
    /// Whether the player is actually taking a stroke this tick.
    pub stroke: bool,
}

/// The MiniGolf game, implementing `BreakpointGame`.
pub struct MiniGolf {
    courses: Vec<Course>,
    course_index: usize,
    state: GolfState,
    player_ids: Vec<PlayerId>,
    paused: bool,
    /// O(1) lookup companion for `state.sunk_order`.
    sunk_set: HashSet<PlayerId>,
    /// Data-driven game configuration (physics, scoring, timing).
    game_config: GolfConfig,
}

impl MiniGolf {
    pub fn new() -> Self {
        let config = GolfConfig::load();
        let courses_dir = std::env::var("BREAKPOINT_COURSES_DIR")
            .unwrap_or_else(|_| "config/courses".to_string());
        let courses = load_courses_from_dir(&courses_dir);
        Self::with_config_and_courses(config, courses)
    }

    /// Create a MiniGolf instance with explicit configuration (uses hardcoded courses).
    pub fn with_config(game_config: GolfConfig) -> Self {
        Self::with_config_and_courses(game_config, all_courses())
    }

    /// Create a MiniGolf instance with explicit configuration and courses.
    pub fn with_config_and_courses(game_config: GolfConfig, courses: Vec<Course>) -> Self {
        Self {
            course_index: 0,
            state: GolfState {
                balls: HashMap::new(),
                strokes: HashMap::new(),
                sunk_order: Vec::new(),
                round_timer: 0.0,
                round_complete: false,
                course_index: 0,
            },
            courses,
            player_ids: Vec::new(),
            paused: false,
            sunk_set: HashSet::new(),
            game_config,
        }
    }

    /// Accessor for the current course.
    pub fn course(&self) -> &Course {
        &self.courses[self.course_index]
    }

    /// Accessor for the current game state.
    pub fn state(&self) -> &GolfState {
        &self.state
    }

    /// Current course index (0-based).
    pub fn course_index(&self) -> usize {
        self.course_index
    }

    /// Total number of holes available.
    pub fn total_holes(&self) -> usize {
        self.courses.len()
    }

    /// Accessor for the game configuration.
    pub fn config(&self) -> &GolfConfig {
        &self.game_config
    }

    /// Round time limit in seconds (from config).
    fn round_duration(&self) -> f32 {
        self.game_config.round_duration_secs
    }
}

impl Default for MiniGolf {
    fn default() -> Self {
        Self::with_config(GolfConfig::default())
    }
}

impl BreakpointGame for MiniGolf {
    fn metadata(&self) -> GameMetadata {
        GameMetadata {
            name: "Mini-Golf".to_string(),
            description: "3D mini-golf! Aim, set power, and stroke. First to sink earns a bonus."
                .to_string(),
            min_players: 1,
            max_players: 8,
            estimated_round_duration: Duration::from_secs(90),
        }
    }

    fn init(&mut self, players: &[Player], config: &GameConfig) {
        // Select course from config (default to 0)
        let hole_index = config
            .custom
            .get("hole_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        self.course_index = hole_index.min(self.courses.len().saturating_sub(1));

        self.state.balls.clear();
        self.state.strokes.clear();
        self.state.sunk_order.clear();
        self.sunk_set.clear();
        self.state.round_timer = 0.0;
        self.state.round_complete = false;
        self.state.course_index = self.course_index as u8;
        self.player_ids.clear();

        let spawn = self.courses[self.course_index].spawn_point;
        for player in players {
            if player.is_spectator {
                continue;
            }
            self.player_ids.push(player.id);
            self.state.balls.insert(player.id, BallState::new(spawn));
            self.state.strokes.insert(player.id, 0);
        }
    }

    fn update(&mut self, dt: f32, _inputs: &PlayerInputs) -> Vec<GameEvent> {
        if self.paused || self.state.round_complete {
            return Vec::new();
        }

        self.state.round_timer += dt;

        let course = &self.courses[self.course_index];

        // Tick all balls
        for ball in self.state.balls.values_mut() {
            ball.tick(course);
        }

        // Check for newly sunk balls
        let mut events = Vec::new();
        let scoring = &self.game_config.scoring;
        for &pid in &self.player_ids {
            if let Some(ball) = self.state.balls.get(&pid)
                && ball.is_sunk
                && !self.sunk_set.contains(&pid)
            {
                self.state.sunk_order.push(pid);
                self.sunk_set.insert(pid);
                let was_first = self.state.sunk_order.len() == 1;
                let strokes = self.state.strokes.get(&pid).copied().unwrap_or(0);
                let score =
                    calculate_score_with_config(strokes, course.par, was_first, true, scoring);
                events.push(GameEvent::ScoreUpdate {
                    player_id: pid,
                    score,
                });
            }
        }

        // Check round completion: all sunk or timer expired
        let all_sunk = self.player_ids.iter().all(|id| self.sunk_set.contains(id));
        let timer_expired = self.state.round_timer >= self.round_duration();

        if all_sunk || timer_expired {
            self.state.round_complete = true;
            events.push(GameEvent::RoundComplete);
        }

        events
    }

    breakpoint_game_boilerplate!(state_type: GolfState);

    fn apply_input(&mut self, player_id: PlayerId, input: &[u8]) {
        let golf_input: GolfInput = match rmp_serde::from_slice(input) {
            Ok(i) => i,
            Err(e) => {
                tracing::debug!(player_id, error = %e, "Dropped malformed golf input");
                return;
            },
        };

        if golf_input.stroke
            && let Some(ball) = self.state.balls.get_mut(&player_id)
            && ball.is_stopped()
            && !ball.is_sunk
        {
            ball.stroke(golf_input.aim_angle, golf_input.power * physics::MAX_POWER);
            *self.state.strokes.entry(player_id).or_insert(0) += 1;
        }
    }

    fn player_joined(&mut self, player: &Player) {
        if player.is_spectator {
            return;
        }
        if !self.player_ids.contains(&player.id) {
            self.player_ids.push(player.id);
            let spawn = self.courses[self.course_index].spawn_point;
            self.state.balls.insert(player.id, BallState::new(spawn));
            self.state.strokes.insert(player.id, 0);
        }
    }

    fn player_left(&mut self, player_id: PlayerId) {
        self.player_ids.retain(|&id| id != player_id);
        self.state.balls.remove(&player_id);
        self.state.strokes.remove(&player_id);
    }

    fn round_count_hint(&self) -> u8 {
        self.courses.len() as u8
    }

    fn round_results(&self) -> Vec<PlayerScore> {
        let par = self.courses[self.course_index].par;
        let scoring = &self.game_config.scoring;
        self.player_ids
            .iter()
            .map(|&pid| {
                let strokes = self.state.strokes.get(&pid).copied().unwrap_or(0);
                let finished = self.sunk_set.contains(&pid);
                let was_first = self.state.sunk_order.first() == Some(&pid);
                let score =
                    calculate_score_with_config(strokes, par, was_first, finished, scoring);
                PlayerScore {
                    player_id: pid,
                    score,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use breakpoint_core::test_helpers::{default_config, make_players};

    #[test]
    fn init_creates_balls_for_all_players() {
        let mut game = MiniGolf::new();
        let players = make_players(3);
        game.init(&players, &default_config(90));

        assert_eq!(game.state.balls.len(), 3);
        assert_eq!(game.state.strokes.len(), 3);
        for p in &players {
            assert!(game.state.balls.contains_key(&p.id));
            assert_eq!(game.state.strokes[&p.id], 0);
        }
    }

    #[test]
    fn spectators_not_added() {
        let mut game = MiniGolf::new();
        let mut players = make_players(2);
        players[1].is_spectator = true;
        game.init(&players, &default_config(90));

        assert_eq!(game.state.balls.len(), 1);
    }

    #[test]
    fn apply_input_increments_strokes() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        let input = GolfInput {
            aim_angle: 0.0,
            power: 0.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        assert_eq!(game.state.strokes[&1], 1);
        assert!(
            !game.state.balls[&1].is_stopped(),
            "Ball should be moving after stroke"
        );
    }

    #[test]
    fn stroke_rejected_while_moving() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        // First stroke
        let input = GolfInput {
            aim_angle: 0.0,
            power: 0.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        assert_eq!(game.state.strokes[&1], 1);

        // Second stroke while moving — should be rejected
        game.apply_input(1, &data);
        assert_eq!(game.state.strokes[&1], 1);
    }

    #[test]
    fn round_complete_when_all_sunk() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        // Manually sink both balls
        let hole_pos = game.course().hole_position;
        for ball in game.state.balls.values_mut() {
            ball.position = hole_pos;
            ball.velocity = course::Vec3::new(0.01, 0.0, 0.0);
            ball.is_sunk = false;
        }
        for (_, strokes) in game.state.strokes.iter_mut() {
            *strokes = 1;
        }

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.1, &inputs);

        assert!(game.is_round_complete());
        assert!(events.iter().any(|e| matches!(e, GameEvent::RoundComplete)));
    }

    #[test]
    fn round_complete_on_timer() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };

        // Advance past the round timer
        game.state.round_timer = game.round_duration() - 0.01;
        let events = game.update(0.1, &inputs);

        assert!(game.is_round_complete());
        assert!(events.iter().any(|e| matches!(e, GameEvent::RoundComplete)));
    }

    #[test]
    fn serialize_deserialize_state_roundtrip() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        let data = game.serialize_state();
        let mut game2 = MiniGolf::new();
        game2.init(&players, &default_config(90));
        game2.apply_state(&data);

        assert_eq!(game.state.balls.len(), game2.state.balls.len());
        for (&pid, ball) in &game.state.balls {
            let ball2 = &game2.state.balls[&pid];
            assert_eq!(ball.position, ball2.position);
            assert_eq!(ball.is_sunk, ball2.is_sunk);
        }
    }

    #[test]
    fn round_results_scoring() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        // Player 1 sinks in 2 strokes (under par 3, first)
        game.state.sunk_order.push(1);
        game.sunk_set.insert(1);
        game.state.strokes.insert(1, 2);

        // Player 2 didn't finish
        game.state.round_complete = true;

        let results = game.round_results();
        assert_eq!(results.len(), 2);

        let p1 = results.iter().find(|r| r.player_id == 1).unwrap();
        // Under par by 1: 1*2 = 2, first sink +3 = 5
        assert_eq!(p1.score, 5);

        let p2 = results.iter().find(|r| r.player_id == 2).unwrap();
        // DNF: -1
        assert_eq!(p2.score, -1);
    }

    #[test]
    fn pause_stops_updates() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        game.pause();
        let timer_before = game.state.round_timer;
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(1.0, &inputs);
        assert_eq!(game.state.round_timer, timer_before);

        game.resume();
        game.update(1.0, &inputs);
        assert!(game.state.round_timer > timer_before);
    }

    #[test]
    fn player_left_removes_state() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        game.player_left(2);
        assert!(!game.state.balls.contains_key(&2));
        assert!(!game.state.strokes.contains_key(&2));
        assert_eq!(game.player_ids.len(), 1);
    }

    // ================================================================
    // Full game session / simulation tests
    // ================================================================

    /// Use course index 1 ("Gentle Straight") — no obstacles, straight shot.
    fn gentle_straight_config() -> GameConfig {
        let mut config = default_config(90);
        config.custom.insert(
            "hole_index".to_string(),
            serde_json::Value::Number(serde_json::Number::from(1)),
        );
        config
    }

    #[test]
    fn full_game_session_stroke_to_sink() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &gentle_straight_config());

        let empty_inputs = PlayerInputs {
            inputs: HashMap::new(),
        };

        // Aim directly at hole: atan2(hole.z - spawn.z, hole.x - spawn.x)
        // Gentle Straight: spawn (6,0,3), hole (6,0,21) → dx=0, dz=18 → angle = pi/2
        let hole = game.course().hole_position;
        let spawn = game.course().spawn_point;
        let aim = (hole.z - spawn.z).atan2(hole.x - spawn.x);

        // Stroke toward hole
        let input = GolfInput {
            aim_angle: aim,
            power: 0.6,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        assert!(
            !game.state.balls[&1].is_stopped(),
            "Ball should be moving after stroke"
        );

        // Simulate up to 500 ticks, re-stroking when ball stops
        let mut sunk = false;
        for _ in 0..500 {
            let events = game.update(0.1, &empty_inputs);
            if events.iter().any(|e| matches!(e, GameEvent::RoundComplete)) {
                sunk = true;
                break;
            }

            // If ball stopped but hasn't sunk, stroke again toward hole
            if game.state.balls[&1].is_stopped() && !game.state.balls[&1].is_sunk {
                let ball_pos = game.state.balls[&1].position;
                let aim = (hole.z - ball_pos.z).atan2(hole.x - ball_pos.x);
                let input = GolfInput {
                    aim_angle: aim,
                    power: 0.4,
                    stroke: true,
                };
                let data = rmp_serde::to_vec(&input).unwrap();
                game.apply_input(1, &data);
            }
        }

        assert!(sunk, "Ball should eventually sink on Gentle Straight");
        assert!(game.is_round_complete());

        let results = game.round_results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].player_id, 1);
    }

    #[test]
    fn serialize_apply_state_preserves_ball_motion() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        // Stroke the ball
        let input = GolfInput {
            aim_angle: 0.5,
            power: 0.6,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        // Tick 5 times so ball is in motion
        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };
        for _ in 0..5 {
            game.update(0.1, &empty);
        }

        let pos1 = game.state.balls[&1].position;
        let vel1 = game.state.balls[&1].velocity;
        let strokes1 = game.state.strokes[&1];
        assert!(
            !game.state.balls[&1].is_stopped(),
            "Ball should still be moving after 5 ticks"
        );

        // Serialize and apply to a fresh game
        let state_bytes = game.serialize_state();
        let mut game2 = MiniGolf::new();
        game2.init(&players, &default_config(90));
        game2.apply_state(&state_bytes);

        let pos2 = game2.state.balls[&1].position;
        let vel2 = game2.state.balls[&1].velocity;
        let strokes2 = game2.state.strokes[&1];

        assert_eq!(pos1, pos2, "Position should match after state apply");
        assert_eq!(vel1, vel2, "Velocity should match after state apply");
        assert_eq!(strokes1, strokes2, "Strokes should match after state apply");
    }

    // ================================================================
    // apply_input direction tests
    // ================================================================

    #[test]
    fn apply_input_direction_positive_x() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &gentle_straight_config());

        let input = GolfInput {
            aim_angle: 0.0,
            power: 0.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let ball = &game.state.balls[&1];
        assert!(
            ball.velocity.x > 0.0,
            "vx should be positive, got {}",
            ball.velocity.x
        );
        assert!(
            ball.velocity.z.abs() < 0.1,
            "vz should be ~0, got {}",
            ball.velocity.z
        );
    }

    #[test]
    fn apply_input_direction_positive_z() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &gentle_straight_config());

        let input = GolfInput {
            aim_angle: std::f32::consts::FRAC_PI_2,
            power: 0.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let ball = &game.state.balls[&1];
        assert!(
            ball.velocity.x.abs() < 0.1,
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
    fn apply_input_aim_at_hole_moves_toward_hole() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &gentle_straight_config());

        let spawn = game.course().spawn_point;
        let hole = game.course().hole_position;
        let aim = (hole.z - spawn.z).atan2(hole.x - spawn.x);

        let input = GolfInput {
            aim_angle: aim,
            power: 0.4,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };
        // Tick until ball stops
        for _ in 0..300 {
            game.update(0.1, &empty);
            if game.state.balls[&1].is_stopped() {
                break;
            }
        }

        let final_pos = game.state.balls[&1].position;
        let initial_dist = ((hole.x - spawn.x).powi(2) + (hole.z - spawn.z).powi(2)).sqrt();
        let final_dist = ((hole.x - final_pos.x).powi(2) + (hole.z - final_pos.z).powi(2)).sqrt();
        assert!(
            final_dist < initial_dist,
            "Ball should be closer to hole: initial_dist={initial_dist}, final_dist={final_dist}"
        );
    }

    #[test]
    fn apply_input_direction_parametric() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

        // (angle, expected_vx_sign, expected_vz_sign)
        // sign: 1 = positive, -1 = negative
        let cases: &[(f32, f32, f32)] = &[
            (0.0, 1.0, 0.0),                // +X
            (FRAC_PI_4, 1.0, 1.0),          // +X +Z
            (FRAC_PI_2, 0.0, 1.0),          // +Z
            (3.0 * FRAC_PI_4, -1.0, 1.0),   // -X +Z
            (PI, -1.0, 0.0),                // -X
            (-3.0 * FRAC_PI_4, -1.0, -1.0), // -X -Z
            (-FRAC_PI_2, 0.0, -1.0),        // -Z
            (-FRAC_PI_4, 1.0, -1.0),        // +X -Z
        ];

        for &(angle, expect_vx_sign, expect_vz_sign) in cases {
            let mut game = MiniGolf::new();
            let players = make_players(1);
            game.init(&players, &gentle_straight_config());

            let input = GolfInput {
                aim_angle: angle,
                power: 0.5,
                stroke: true,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game.apply_input(1, &data);

            let ball = &game.state.balls[&1];
            if expect_vx_sign > 0.0 {
                assert!(
                    ball.velocity.x > 0.1,
                    "angle={angle:.2}: vx should be positive, got {}",
                    ball.velocity.x
                );
            } else if expect_vx_sign < 0.0 {
                assert!(
                    ball.velocity.x < -0.1,
                    "angle={angle:.2}: vx should be negative, got {}",
                    ball.velocity.x
                );
            } else {
                assert!(
                    ball.velocity.x.abs() < 0.1,
                    "angle={angle:.2}: vx should be ~0, got {}",
                    ball.velocity.x
                );
            }

            if expect_vz_sign > 0.0 {
                assert!(
                    ball.velocity.z > 0.1,
                    "angle={angle:.2}: vz should be positive, got {}",
                    ball.velocity.z
                );
            } else if expect_vz_sign < 0.0 {
                assert!(
                    ball.velocity.z < -0.1,
                    "angle={angle:.2}: vz should be negative, got {}",
                    ball.velocity.z
                );
            } else {
                assert!(
                    ball.velocity.z.abs() < 0.1,
                    "angle={angle:.2}: vz should be ~0, got {}",
                    ball.velocity.z
                );
            }
        }
    }

    #[test]
    fn multi_player_independent_strokes() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        let spawn = game.course().spawn_point;
        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };

        // Player 1 strokes, Player 2 does not
        let input = GolfInput {
            aim_angle: 0.0,
            power: 0.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        // Tick a few times
        for _ in 0..3 {
            game.update(0.1, &empty);
        }

        // Player 1's ball should have moved from spawn
        let p1_pos = game.state.balls[&1].position;
        assert!(
            (p1_pos.x - spawn.x).abs() > 0.1 || (p1_pos.z - spawn.z).abs() > 0.1,
            "Player 1's ball should have moved"
        );

        // Player 2's ball should still be at spawn
        let p2_pos = game.state.balls[&2].position;
        assert_eq!(p2_pos, spawn, "Player 2's ball should still be at spawn");
        assert_eq!(game.state.strokes[&2], 0);

        // Wait for Player 1's ball to stop
        for _ in 0..500 {
            game.update(0.1, &empty);
            if game.state.balls[&1].is_stopped() {
                break;
            }
        }
        assert!(game.state.balls[&1].is_stopped());

        // Now Player 2 strokes
        let input2 = GolfInput {
            aim_angle: 1.0,
            power: 0.4,
            stroke: true,
        };
        let data2 = rmp_serde::to_vec(&input2).unwrap();
        game.apply_input(2, &data2);

        assert_eq!(game.state.strokes[&2], 1);
        assert!(
            !game.state.balls[&2].is_stopped(),
            "Player 2's ball should now be moving"
        );
    }

    // ================================================================
    // Edge case tests
    // ================================================================

    #[test]
    fn multi_player_simultaneous_sunk() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        // Place both balls at the hole position with tiny velocity so they sink on the
        // same update tick.
        let hole_pos = game.course().hole_position;
        for ball in game.state.balls.values_mut() {
            ball.position = hole_pos;
            ball.velocity = course::Vec3::new(0.01, 0.0, 0.0);
            ball.is_sunk = false;
        }
        // Give each player 1 stroke so scoring is meaningful
        for strokes in game.state.strokes.values_mut() {
            *strokes = 1;
        }

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.1, &inputs);

        // Both players should be in the sunk_order
        assert_eq!(
            game.state.sunk_order.len(),
            2,
            "Both players should be recorded in sunk_order"
        );

        // The first player in sunk_order gets the first-sink bonus (+3)
        let first_sunk_id = game.state.sunk_order[0];
        let second_sunk_id = game.state.sunk_order[1];
        assert_ne!(first_sunk_id, second_sunk_id);

        // Both should have ScoreUpdate events
        let score_events: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                GameEvent::ScoreUpdate { player_id, score } => Some((*player_id, *score)),
                _ => None,
            })
            .collect();
        assert_eq!(score_events.len(), 2, "Should have 2 ScoreUpdate events");

        // First-sunk player gets the first-sink bonus; second does not
        let first_score = score_events
            .iter()
            .find(|(pid, _)| *pid == first_sunk_id)
            .unwrap()
            .1;
        let second_score = score_events
            .iter()
            .find(|(pid, _)| *pid == second_sunk_id)
            .unwrap()
            .1;
        assert!(
            first_score > second_score,
            "First to sink should get bonus: first={first_score}, second={second_score}"
        );

        // Round should be complete (all sunk)
        assert!(game.is_round_complete());
        assert!(events.iter().any(|e| matches!(e, GameEvent::RoundComplete)));
    }

    #[test]
    fn dnf_timeout_scoring() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        // Player 1 sinks manually
        game.state.sunk_order.push(1);
        game.sunk_set.insert(1);
        game.state.strokes.insert(1, 2);
        game.state.balls.get_mut(&1).unwrap().is_sunk = true;

        // Player 2 never sinks — advance the timer past the round duration
        game.state.round_timer = game.round_duration() - 0.01;

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.1, &inputs);

        // Round should complete due to timer expiry
        assert!(game.is_round_complete());
        assert!(events.iter().any(|e| matches!(e, GameEvent::RoundComplete)));

        // Check round results: player 2 (DNF) should score -1
        let results = game.round_results();
        let p2_result = results.iter().find(|r| r.player_id == 2).unwrap();
        assert_eq!(p2_result.score, -1, "DNF player should score -1");

        // Player 1 (finished, first sink, under par) should have a positive score
        let p1_result = results.iter().find(|r| r.player_id == 1).unwrap();
        assert!(
            p1_result.score > 0,
            "Finished player should have positive score, got {}",
            p1_result.score
        );
    }

    #[test]
    fn power_clamping() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        // Send a stroke with power > 1.0 (which gets multiplied by MAX_POWER in apply_input).
        // Power 1.5 * MAX_POWER = 37.5, but stroke() internally clamps to MAX_POWER.
        let input = GolfInput {
            aim_angle: 0.0,
            power: 1.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        // Ball should be moving (stroke accepted)
        assert!(
            !game.state.balls[&1].is_stopped(),
            "Ball should be moving after stroke with clamped power"
        );
        assert_eq!(game.state.strokes[&1], 1, "Stroke should be counted");

        // Velocity magnitude should be clamped to MAX_POWER
        let vel = &game.state.balls[&1].velocity;
        let speed = (vel.x * vel.x + vel.z * vel.z).sqrt();
        assert!(
            speed <= physics::MAX_POWER + 0.01,
            "Speed should be clamped to MAX_POWER ({:.2}), got {speed:.2}",
            physics::MAX_POWER
        );
    }

    // ================================================================
    // Game Trait Contract Tests
    // ================================================================

    #[test]
    fn contract_init_creates_player_state() {
        let mut game = MiniGolf::new();
        breakpoint_core::test_helpers::contract_init_creates_player_state(&mut game, 3);
    }

    #[test]
    fn contract_apply_input_changes_state() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        let input = GolfInput {
            aim_angle: 0.5,
            power: 0.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        breakpoint_core::test_helpers::contract_apply_input_changes_state(&mut game, &data, 1);
    }

    #[test]
    fn contract_update_advances_time() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));
        breakpoint_core::test_helpers::contract_update_advances_time(&mut game);
    }

    #[test]
    fn contract_round_eventually_completes() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));
        breakpoint_core::test_helpers::contract_round_eventually_completes(&mut game, 100);
    }

    #[test]
    fn contract_state_roundtrip_preserves() {
        // Use a single player to avoid HashMap key ordering non-determinism
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));
        breakpoint_core::test_helpers::contract_state_roundtrip_preserves(&mut game);
    }

    #[test]
    fn contract_pause_stops_updates() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));
        breakpoint_core::test_helpers::contract_pause_stops_updates(&mut game);
    }

    #[test]
    fn contract_player_left_cleanup() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));
        breakpoint_core::test_helpers::contract_player_left_cleanup(&mut game, 2, 2);
    }

    #[test]
    fn contract_round_results_complete() {
        let mut game = MiniGolf::new();
        let players = make_players(3);
        game.init(&players, &default_config(90));
        breakpoint_core::test_helpers::contract_round_results_complete(&game, 3);
    }

    // ================================================================
    // Input encoding/decoding roundtrip tests (Phase 2)
    // ================================================================

    #[test]
    fn golf_input_encode_decode_roundtrip() {
        let input = GolfInput {
            aim_angle: 1.23,
            power: 0.75,
            stroke: true,
        };
        let encoded = rmp_serde::to_vec(&input).unwrap();
        let decoded: GolfInput = rmp_serde::from_slice(&encoded).unwrap();
        assert!((decoded.aim_angle - input.aim_angle).abs() < 1e-5);
        assert!((decoded.power - input.power).abs() < 1e-5);
        assert_eq!(decoded.stroke, input.stroke);
    }

    #[test]
    fn golf_input_through_protocol_roundtrip() {
        use breakpoint_core::net::messages::{ClientMessage, PlayerInputMsg};
        use breakpoint_core::net::protocol::{decode_client_message, encode_client_message};

        let input = GolfInput {
            aim_angle: 0.5,
            power: 0.8,
            stroke: true,
        };
        let input_data = rmp_serde::to_vec(&input).unwrap();
        let msg = ClientMessage::PlayerInput(PlayerInputMsg {
            player_id: 1,
            tick: 42,
            input_data: input_data.clone(),
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        match decoded {
            ClientMessage::PlayerInput(pi) => {
                assert_eq!(pi.player_id, 1);
                assert_eq!(pi.tick, 42);
                assert_eq!(pi.input_data, input_data);
                let golf_input: GolfInput = rmp_serde::from_slice(&pi.input_data).unwrap();
                assert!((golf_input.aim_angle - 0.5).abs() < 1e-5);
                assert!(golf_input.stroke);
            },
            other => panic!("Expected PlayerInput, got {:?}", other),
        }
    }

    #[test]
    fn golf_input_apply_changes_game_state() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        let before = game.serialize_state();

        let input = GolfInput {
            aim_angle: 0.0,
            power: 0.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.1, &empty);

        breakpoint_core::test_helpers::assert_game_state_changed(&game, &before);
    }

    // ================================================================
    // P0-1: NaN/Inf/Degenerate Input Fuzzing
    // ================================================================

    // REGRESSION: NaN aim_angle via apply_input should not corrupt game state
    #[test]
    fn golf_apply_input_nan_angle_no_panic() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        let input = GolfInput {
            aim_angle: f32::NAN,
            power: 0.5,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        // The stroke should have been applied (ball not stopped)
        // but position must remain finite after update
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        // Should not panic
        game.update(0.1, &inputs);
    }

    // REGRESSION: Inf power via apply_input should be clamped
    #[test]
    fn golf_apply_input_inf_power_no_panic() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        let input = GolfInput {
            aim_angle: 0.0,
            power: f32::INFINITY,
            stroke: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let vel = &game.state.balls[&1].velocity;
        let speed = (vel.x * vel.x + vel.z * vel.z).sqrt();
        assert!(
            speed <= physics::MAX_POWER + 0.01,
            "Inf power should be clamped to MAX_POWER, got {speed}"
        );
    }

    // ================================================================
    // P0-3: All-Course Aim-at-Hole Regression Tests
    // ================================================================

    // REGRESSION: Verify aim-at-hole works on every course, not just Gentle Straight
    #[test]
    fn aim_at_hole_moves_toward_hole_all_courses() {
        let courses = course::all_courses();
        for (idx, c) in courses.iter().enumerate() {
            let spawn = c.spawn_point;
            let hole = c.hole_position;
            let dx = hole.x - spawn.x;
            let dz = hole.z - spawn.z;
            let aim_angle = dz.atan2(dx);
            let initial_dist = ((hole.x - spawn.x).powi(2) + (hole.z - spawn.z).powi(2)).sqrt();

            let mut ball = physics::BallState::new(spawn);
            ball.stroke(aim_angle, physics::MAX_POWER * 0.8);

            for _ in 0..200 {
                ball.tick(c);
                if ball.is_stopped() || ball.is_sunk {
                    break;
                }
            }

            let final_dist =
                ((hole.x - ball.position.x).powi(2) + (hole.z - ball.position.z).powi(2)).sqrt();
            assert!(
                final_dist < initial_dist || ball.is_sunk,
                "Course {idx} ({}): ball should be closer to hole after aimed stroke. \
                 initial_dist={initial_dist:.2}, final_dist={final_dist:.2}",
                c.name
            );
        }
    }

    // REGRESSION: Ball position resets correctly between rounds
    #[test]
    fn ball_position_resets_between_rounds() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        let course0_spawn = game.course().spawn_point;
        // Verify ball starts at course 0's spawn
        let ball_pos = game.state.balls[&1].position;
        assert!(
            (ball_pos.x - course0_spawn.x).abs() < 0.01
                && (ball_pos.z - course0_spawn.z).abs() < 0.01,
            "Ball should start at course 0 spawn"
        );

        // Force sink the ball to complete round
        game.state.balls.get_mut(&1).unwrap().is_sunk = true;
        game.state.sunk_order.push(1);
        game.sunk_set.insert(1);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.1, &inputs);
        assert!(game.is_round_complete());

        // Re-init for next round (simulating what the host does)
        let mut game2 = MiniGolf::new();
        game2.init(&players, &default_config(90));
        // Advance to course 1
        game2.course_index = 1;
        let course1_spawn = game2.courses[1].spawn_point;
        game2
            .state
            .balls
            .insert(1, physics::BallState::new(course1_spawn));
        game2.state.course_index = 1;

        let ball_pos2 = game2.state.balls[&1].position;
        assert!(
            (ball_pos2.x - course1_spawn.x).abs() < 0.01
                && (ball_pos2.z - course1_spawn.z).abs() < 0.01,
            "Ball should spawn at course 1 spawn, not course 0 hole"
        );
    }

    // REGRESSION: Straight shot on Gentle Straight should eventually sink
    #[test]
    fn stroke_from_spawn_toward_hole_sinks_on_gentle_straight() {
        let courses = course::all_courses();
        let gentle = &courses[1]; // Course 1: Gentle Straight, no obstacles
        let spawn = gentle.spawn_point;
        let hole = gentle.hole_position;
        let dx = hole.x - spawn.x;
        let dz = hole.z - spawn.z;
        let aim_angle = dz.atan2(dx);

        let mut ball = physics::BallState::new(spawn);
        ball.stroke(aim_angle, physics::MAX_POWER);

        for _ in 0..500 {
            ball.tick(gentle);
            if ball.is_sunk {
                break;
            }
        }

        assert!(
            ball.is_sunk,
            "Full-power straight shot on Gentle Straight should sink. \
             Final pos: ({:.2}, {:.2}), hole: ({:.2}, {:.2})",
            ball.position.x, ball.position.z, hole.x, hole.z
        );
    }

    // ================================================================
    // P1-1: Serialization Fuzzing
    // ================================================================

    // REGRESSION: Garbage input data should not panic
    #[test]
    fn apply_input_with_garbage_data_no_panic() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        let garbage: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01, 0xAB, 0xCD];
        game.apply_input(1, &garbage);

        // Ball should still be at spawn, unmoved
        assert!(
            game.state.balls[&1].is_stopped(),
            "Garbage input should not move the ball"
        );
    }

    // REGRESSION: Truncated state data should not panic
    #[test]
    fn apply_state_with_truncated_data_no_panic() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        let original_state = game.serialize_state();
        // Truncate to half length
        let truncated = &original_state[..original_state.len() / 2];
        game.apply_state(truncated);

        // Game should still be functional (state unchanged from failed apply)
        assert_eq!(
            game.state.balls.len(),
            1,
            "State should be unchanged after truncated apply_state"
        );
    }

    // ================================================================
    // P1-2: State Machine Transition Tests
    // ================================================================

    #[test]
    fn double_pause_single_resume_works() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        game.pause();
        game.pause(); // double pause
        game.resume(); // single resume

        let timer_before = game.state.round_timer;
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(1.0, &inputs);

        assert!(
            game.state.round_timer > timer_before,
            "Timer should advance after resume: before={timer_before}, after={}",
            game.state.round_timer
        );
    }

    #[test]
    fn update_after_round_complete_is_noop() {
        let mut game = MiniGolf::new();
        let players = make_players(1);
        game.init(&players, &default_config(90));

        // Force round complete
        game.state.balls.get_mut(&1).unwrap().is_sunk = true;
        game.state.sunk_order.push(1);
        game.sunk_set.insert(1);
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.1, &inputs);
        assert!(game.is_round_complete());

        let timer = game.state.round_timer;
        let events = game.update(1.0, &inputs);
        assert!(
            (game.state.round_timer - timer).abs() < 0.01,
            "Timer should not advance after round complete"
        );
        assert!(
            events.is_empty(),
            "No events should be emitted after round complete"
        );
    }

    // ================================================================
    // P1-3: Golf Multi-Hole Session Tests
    // ================================================================

    #[test]
    fn multi_round_scoring_accumulation() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        // Complete round 1 by sinking both players
        game.state.strokes.insert(1, 2);
        game.state.strokes.insert(2, 3);
        game.state.balls.get_mut(&1).unwrap().is_sunk = true;
        game.state.balls.get_mut(&2).unwrap().is_sunk = true;
        game.state.sunk_order.push(1);
        game.state.sunk_order.push(2);
        game.sunk_set.insert(1);
        game.sunk_set.insert(2);
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.1, &inputs);
        assert!(game.is_round_complete());

        let results = game.round_results();
        assert_eq!(results.len(), 2, "Both players should have results");

        // Re-init for round 2 with hole_index=1
        let mut config2 = default_config(90);
        config2.custom.insert(
            "hole_index".to_string(),
            serde_json::Value::Number(serde_json::Number::from(1)),
        );
        let mut game2 = MiniGolf::new();
        game2.init(&players, &config2);

        assert_eq!(
            game2.state.strokes[&1], 0,
            "Strokes should reset for new round"
        );
        assert!(
            game2.state.sunk_order.is_empty(),
            "Sunk order should be empty for new round"
        );
        assert_eq!(game2.course_index, 1, "Course should be 1 for round 2");
    }

    #[test]
    fn scoring_across_all_par_values() {
        // Verify scoring formula handles par 2, 3, and 4 (all used by courses)
        let courses = course::all_courses();
        let pars: Vec<u8> = courses.iter().map(|c| c.par).collect();

        for &par in &pars {
            // Under par
            let score = scoring::calculate_score(1, par, true, true);
            assert!(
                score > 0,
                "1 stroke on par {par} should give positive score, got {score}"
            );

            // At par
            let score = scoring::calculate_score(par as u32, par, false, true);
            assert_eq!(score, 1, "At par ({par}) without first-sink should score 1");

            // Over par
            let score = scoring::calculate_score(par as u32 + 2, par, false, true);
            assert_eq!(score, 0, "Over par ({par}) should score 0, got {score}");
        }
    }

    #[test]
    fn dnf_all_players_still_produces_results() {
        let mut game = MiniGolf::new();
        let players = make_players(3);
        game.init(&players, &default_config(90));

        // Nobody sinks — advance timer past round duration
        game.state.round_timer = game.round_duration() - 0.01;
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.1, &inputs);
        assert!(game.is_round_complete());

        let results = game.round_results();
        assert_eq!(results.len(), 3, "All players should have results");
        for result in &results {
            assert_eq!(
                result.score, -1,
                "DNF player {} should score -1, got {}",
                result.player_id, result.score
            );
        }
    }

    // REGRESSION: Verify JS msgpackr encoding is correctly decoded by rmp_serde.
    // msgpackr encodes 0.0 as integer 0 (not float32) and 0.8 as float64 (not float32).
    #[test]
    fn js_msgpackr_golf_input_decodes_correctly() {
        // Exact bytes from: msgpackr.pack([0.0, 0.8, true])
        // 93 = fixarray(3)
        // 00 = fixint(0)  <-- aim_angle=0.0 encoded as INTEGER
        // cb 3f e9 99 99 99 99 99 9a = float64(0.8)  <-- power as f64
        // c3 = true
        let js_bytes: Vec<u8> = vec![
            0x93, 0x00, 0xcb, 0x3f, 0xe9, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9a, 0xc3,
        ];

        let result: Result<GolfInput, _> = rmp_serde::from_slice(&js_bytes);
        match result {
            Ok(input) => {
                assert_eq!(input.aim_angle, 0.0, "aim_angle should be 0.0");
                assert!(
                    (input.power - 0.8).abs() < 0.001,
                    "power should be ~0.8, got {}",
                    input.power
                );
                assert!(input.stroke, "stroke should be true");

                // Verify stroke direction: aim_angle=0 should produce +X velocity
                let mut ball = BallState::new(course::Vec3::new(10.0, 0.0, 3.0));
                ball.stroke(input.aim_angle, input.power * physics::MAX_POWER);
                assert!(
                    ball.velocity.x > 0.0,
                    "aim_angle=0 should produce +X velocity, got vx={}",
                    ball.velocity.x
                );
                assert!(
                    ball.velocity.z.abs() < 0.01,
                    "aim_angle=0 should produce ~0 Z velocity, got vz={}",
                    ball.velocity.z
                );
            },
            Err(e) => {
                panic!(
                    "rmp_serde CANNOT decode JS msgpackr encoding: {e}\n\
                     This means aim_angle=0 (integer) or power=0.8 (float64) is incompatible!\n\
                     The apply_input Err branch silently drops the input."
                );
            },
        }
    }

    // REGRESSION: Test that apply_input with JS-encoded bytes actually moves the ball
    #[test]
    fn apply_input_with_js_encoded_bytes_moves_ball() {
        let mut game = MiniGolf::new();
        let players = make_players(2);
        game.init(&players, &default_config(90));

        // Exact bytes from JS: msgpackr.pack([0.0, 0.8, true])
        let js_golf_input: Vec<u8> = vec![
            0x93, 0x00, 0xcb, 0x3f, 0xe9, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9a, 0xc3,
        ];

        let initial_x = game.state.balls[&2].position.x;
        game.apply_input(2, &js_golf_input);

        // Ball should now have velocity
        let ball = &game.state.balls[&2];
        assert!(
            ball.velocity.x > 0.0,
            "Ball vx should be positive after aim_angle=0 stroke, got {}",
            ball.velocity.x
        );

        // Tick physics to see movement
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        for _ in 0..10 {
            game.update(0.1, &inputs);
        }

        let after_x = game.state.balls[&2].position.x;
        let dx = after_x - initial_x;
        assert!(
            dx > 0.0,
            "Ball should move +X with aim_angle=0, got dx={dx} (initial={initial_x}, after={after_x})"
        );
    }
}
