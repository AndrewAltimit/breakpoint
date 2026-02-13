pub mod course;
pub mod physics;
pub mod scoring;

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use breakpoint_core::breakpoint_game_boilerplate;
use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameEvent, GameMetadata, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::player::Player;

use course::{Course, all_courses};
use physics::BallState;
use scoring::calculate_score;

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
}

impl MiniGolf {
    pub fn new() -> Self {
        let courses = all_courses();
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

    /// Round time limit in seconds.
    const ROUND_DURATION: f32 = 90.0;
}

impl Default for MiniGolf {
    fn default() -> Self {
        Self::new()
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
        for &pid in &self.player_ids {
            if let Some(ball) = self.state.balls.get(&pid)
                && ball.is_sunk
                && !self.state.sunk_order.contains(&pid)
            {
                self.state.sunk_order.push(pid);
                let was_first = self.state.sunk_order.len() == 1;
                let strokes = self.state.strokes.get(&pid).copied().unwrap_or(0);
                let score = calculate_score(strokes, course.par, was_first, true);
                events.push(GameEvent::ScoreUpdate {
                    player_id: pid,
                    score,
                });
            }
        }

        // Check round completion: all sunk or timer expired
        let all_sunk = self
            .player_ids
            .iter()
            .all(|id| self.state.sunk_order.contains(id));
        let timer_expired = self.state.round_timer >= Self::ROUND_DURATION;

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
            Err(_) => return,
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
        self.player_ids
            .iter()
            .map(|&pid| {
                let strokes = self.state.strokes.get(&pid).copied().unwrap_or(0);
                let finished = self.state.sunk_order.contains(&pid);
                let was_first = self.state.sunk_order.first() == Some(&pid);
                let score = calculate_score(strokes, par, was_first, finished);
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
        game.state.round_timer = MiniGolf::ROUND_DURATION - 0.01;
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
}
