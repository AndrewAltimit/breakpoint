pub mod arena;
pub mod collision;
pub mod config;
pub mod physics;
pub mod scoring;
pub mod win_zone;

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use breakpoint_core::breakpoint_game_boilerplate;
use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameEvent, GameMetadata, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::player::Player;

use config::TronConfig;
use win_zone::WinZone;

/// Cardinal direction on the 2D grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

/// Turn direction input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnDirection {
    None,
    Left,
    Right,
}

/// A wall segment left behind by a cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallSegment {
    pub x1: f32,
    pub z1: f32,
    pub x2: f32,
    pub z2: f32,
    pub owner_id: PlayerId,
    /// Whether this is the actively-extending segment (the cycle's current tail).
    pub is_active: bool,
}

/// State of a single cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleState {
    pub x: f32,
    pub z: f32,
    pub direction: Direction,
    pub speed: f32,
    pub rubber: f32,
    pub brake_fuel: f32,
    pub alive: bool,
    /// Index into the wall_segments vec where this cycle's trail starts.
    pub trail_start_index: usize,
    pub turn_cooldown: f32,
    /// Tracking: how many opponents died to this cycle's walls.
    pub kills: u32,
    pub died: bool,
    pub is_suicide: bool,
}

/// Input from a tron player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TronInput {
    pub turn: TurnDirection,
    pub brake: bool,
}

impl Default for TronInput {
    fn default() -> Self {
        Self {
            turn: TurnDirection::None,
            brake: false,
        }
    }
}

/// Serializable game state for network broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TronState {
    pub players: HashMap<PlayerId, CycleState>,
    pub wall_segments: Vec<WallSegment>,
    pub round_timer: f32,
    pub round_complete: bool,
    pub round_number: u8,
    pub scores: HashMap<PlayerId, i32>,
    pub win_zone: WinZone,
    pub alive_count: u32,
    pub arena_width: f32,
    pub arena_depth: f32,
    pub time_since_last_death: f32,
    pub winner_id: Option<PlayerId>,
}

/// The Tron Light Cycles game.
pub struct TronCycles {
    state: TronState,
    player_ids: Vec<PlayerId>,
    pending_inputs: HashMap<PlayerId, TronInput>,
    paused: bool,
    game_config: TronConfig,
}

impl TronCycles {
    pub fn new() -> Self {
        Self::with_config(TronConfig::load())
    }

    pub fn with_config(config: TronConfig) -> Self {
        Self {
            state: TronState {
                players: HashMap::new(),
                wall_segments: Vec::new(),
                round_timer: 0.0,
                round_complete: false,
                round_number: 1,
                scores: HashMap::new(),
                win_zone: WinZone::default(),
                alive_count: 0,
                arena_width: config.arena_width,
                arena_depth: config.arena_depth,
                time_since_last_death: 0.0,
                winner_id: None,
            },
            player_ids: Vec::new(),
            pending_inputs: HashMap::new(),
            paused: false,
            game_config: config,
        }
    }

    pub fn state(&self) -> &TronState {
        &self.state
    }

    pub fn config(&self) -> &TronConfig {
        &self.game_config
    }

    /// Kill a cycle and record who killed it.
    fn kill_cycle(&mut self, player_id: PlayerId, killer_id: Option<PlayerId>, is_suicide: bool) {
        if let Some(cycle) = self.state.players.get_mut(&player_id) {
            if !cycle.alive {
                return;
            }
            cycle.alive = false;
            cycle.died = true;
            cycle.is_suicide = is_suicide;
            self.state.alive_count = self.state.alive_count.saturating_sub(1);
            self.state.time_since_last_death = 0.0;

            // Credit the kill to the wall owner
            if let Some(kid) = killer_id
                && let Some(killer_cycle) = self.state.players.get_mut(&kid)
            {
                killer_cycle.kills += 1;
            }
        }

        // Finalize the dead cycle's active wall segment
        for wall in &mut self.state.wall_segments {
            if wall.owner_id == player_id && wall.is_active {
                wall.is_active = false;
            }
        }
    }

    /// Start a new wall segment at the turn point, extending to the cycle's current position.
    fn start_new_segment_at(
        &mut self,
        player_id: PlayerId,
        turn_x: f32,
        turn_z: f32,
        current_x: f32,
        current_z: f32,
    ) {
        // Close the current active segment at the turn point
        for wall in &mut self.state.wall_segments {
            if wall.owner_id == player_id && wall.is_active {
                wall.x2 = turn_x;
                wall.z2 = turn_z;
                wall.is_active = false;
            }
        }

        // Start a new active segment from turn point to current position
        self.state.wall_segments.push(WallSegment {
            x1: turn_x,
            z1: turn_z,
            x2: current_x,
            z2: current_z,
            owner_id: player_id,
            is_active: true,
        });
    }
}

impl Default for TronCycles {
    fn default() -> Self {
        Self::with_config(TronConfig::default())
    }
}

impl BreakpointGame for TronCycles {
    fn metadata(&self) -> GameMetadata {
        GameMetadata {
            name: "Tron Light Cycles".to_string(),
            description: "Drive fast, leave walls, don't crash! Grind walls for speed boosts."
                .to_string(),
            min_players: 2,
            max_players: 8,
            estimated_round_duration: Duration::from_secs(120),
        }
    }

    fn tick_rate(&self) -> f32 {
        20.0
    }

    fn round_count_hint(&self) -> u8 {
        self.game_config.round_count
    }

    fn init(&mut self, players: &[Player], _config: &GameConfig) {
        let active_players: Vec<&Player> = players.iter().filter(|p| !p.is_spectator).collect();

        let arena = arena::create_arena(
            self.game_config.arena_width,
            self.game_config.arena_depth,
            active_players.len(),
        );

        self.state = TronState {
            players: HashMap::new(),
            wall_segments: Vec::new(),
            round_timer: 0.0,
            round_complete: false,
            round_number: 1,
            scores: HashMap::new(),
            win_zone: WinZone::default(),
            alive_count: active_players.len() as u32,
            arena_width: arena.width,
            arena_depth: arena.depth,
            time_since_last_death: 0.0,
            winner_id: None,
        };
        self.player_ids.clear();
        self.pending_inputs.clear();
        self.paused = false;

        for (i, player) in active_players.iter().enumerate() {
            self.player_ids.push(player.id);
            let spawn = &arena.spawn_points[i % arena.spawn_points.len()];

            let cycle = CycleState {
                x: spawn.x,
                z: spawn.z,
                direction: spawn.direction,
                speed: self.game_config.base_speed,
                rubber: self.game_config.rubber_max,
                brake_fuel: self.game_config.brake_fuel_max,
                alive: true,
                trail_start_index: self.state.wall_segments.len(),
                turn_cooldown: 0.0,
                kills: 0,
                died: false,
                is_suicide: false,
            };

            // Start the initial wall segment for this cycle
            self.state.wall_segments.push(WallSegment {
                x1: spawn.x,
                z1: spawn.z,
                x2: spawn.x,
                z2: spawn.z,
                owner_id: player.id,
                is_active: true,
            });

            self.state.players.insert(player.id, cycle);
            self.state.scores.insert(player.id, 0);
        }
    }

    fn update(&mut self, dt: f32, _inputs: &PlayerInputs) -> Vec<GameEvent> {
        if self.paused || self.state.round_complete {
            return Vec::new();
        }

        self.state.round_timer += dt;
        self.state.time_since_last_death += dt;
        let mut events = Vec::new();

        // Process each cycle
        let player_ids: Vec<PlayerId> = self.player_ids.clone();
        for &pid in &player_ids {
            let input = self.pending_inputs.remove(&pid).unwrap_or_default();

            // Save pre-movement position as the potential turn point
            let turn_point = self
                .state
                .players
                .get(&pid)
                .map(|c| (c.x, c.z, c.direction));

            // Update cycle physics (applies turn + movement)
            physics::update_cycle(
                match self.state.players.get_mut(&pid) {
                    Some(c) => c,
                    None => continue,
                },
                pid,
                &input,
                &self.state.wall_segments,
                self.state.arena_width,
                self.state.arena_depth,
                dt,
                &self.game_config,
            );

            let cycle = match self.state.players.get(&pid) {
                Some(c) => c,
                None => continue,
            };

            if !cycle.alive {
                continue;
            }

            // If direction changed, split segment at the PRE-movement turn point
            let direction_changed = turn_point
                .map(|(_, _, old_dir)| old_dir != cycle.direction)
                .unwrap_or(false);

            if direction_changed {
                let (tx, tz, _) = turn_point.unwrap();
                self.start_new_segment_at(pid, tx, tz, cycle.x, cycle.z);
            } else {
                // Update the active segment endpoint
                let cx = cycle.x;
                let cz = cycle.z;
                for wall in &mut self.state.wall_segments {
                    if wall.owner_id == pid && wall.is_active {
                        wall.x2 = cx;
                        wall.z2 = cz;
                    }
                }
            }
        }

        // Collision detection (separate pass to avoid borrow issues)
        let mut kills: Vec<(PlayerId, Option<PlayerId>, bool)> = Vec::new();

        for &pid in &player_ids {
            let cycle = match self.state.players.get(&pid) {
                Some(c) if c.alive => c,
                _ => continue,
            };

            // Check arena boundary
            if collision::check_arena_boundary(
                cycle,
                self.state.arena_width,
                self.state.arena_depth,
            ) {
                kills.push((pid, None, true));
                continue;
            }

            // Check wall collisions
            let result = collision::check_wall_collision(
                cycle,
                pid,
                &self.state.wall_segments,
                &self.game_config,
            );
            if !result.alive {
                kills.push((pid, result.killer_id, result.is_suicide));
            }
        }

        // Apply kills
        for (pid, killer_id, is_suicide) in kills {
            self.kill_cycle(pid, killer_id, is_suicide);
        }

        // Win zone logic
        if !self.state.win_zone.active
            && win_zone::should_spawn_win_zone(
                self.state.round_timer,
                self.state.time_since_last_death,
                &self.game_config,
            )
        {
            self.state
                .win_zone
                .spawn(self.state.arena_width, self.state.arena_depth);
        }

        if self.state.win_zone.active {
            self.state.win_zone.update(dt, &self.game_config);

            // Check if any alive player entered the win zone
            for &pid in &player_ids {
                let cycle = match self.state.players.get(&pid) {
                    Some(c) if c.alive => c,
                    _ => continue,
                };
                if self.state.win_zone.contains(cycle.x, cycle.z) {
                    // This player wins the round
                    self.state.winner_id = Some(pid);
                    self.state.round_complete = true;
                    events.push(GameEvent::RoundComplete);
                    return events;
                }
            }
        }

        // Check round completion: last player alive wins
        if self.state.alive_count <= 1 && self.player_ids.len() >= 2 {
            self.state.round_complete = true;
            // Find the winner
            for &pid in &player_ids {
                if let Some(cycle) = self.state.players.get(&pid)
                    && cycle.alive
                {
                    self.state.winner_id = Some(pid);
                    break;
                }
            }
            events.push(GameEvent::RoundComplete);
        }

        events
    }

    breakpoint_game_boilerplate!(state_type: TronState);

    fn apply_input(&mut self, player_id: PlayerId, input: &[u8]) {
        match rmp_serde::from_slice::<TronInput>(input) {
            Err(e) => {
                tracing::debug!(player_id, error = %e, "Dropped malformed tron input");
            },
            Ok(ti) => {
                // Accumulate transient turn flags across frames
                if let Some(existing) = self.pending_inputs.get_mut(&player_id) {
                    // Preserve turn if a turn was requested
                    if ti.turn != TurnDirection::None {
                        existing.turn = ti.turn;
                    }
                    // Preserve brake (OR logic — once pressed, keep until tick)
                    if ti.brake {
                        existing.brake = true;
                    }
                } else {
                    self.pending_inputs.insert(player_id, ti);
                }
            },
        }
    }

    fn player_joined(&mut self, player: &Player) {
        if player.is_spectator || self.player_ids.contains(&player.id) {
            return;
        }
        // Late joiners start as dead spectators for this round
        self.player_ids.push(player.id);
        let cycle = CycleState {
            x: self.state.arena_width / 2.0,
            z: self.state.arena_depth / 2.0,
            direction: Direction::East,
            speed: 0.0,
            rubber: 0.0,
            brake_fuel: 0.0,
            alive: false,
            trail_start_index: self.state.wall_segments.len(),
            turn_cooldown: 0.0,
            kills: 0,
            died: true,
            is_suicide: false,
        };
        self.state.players.insert(player.id, cycle);
        self.state.scores.insert(player.id, 0);
    }

    fn player_left(&mut self, player_id: PlayerId) {
        self.player_ids.retain(|&id| id != player_id);
        if let Some(cycle) = self.state.players.get(&player_id)
            && cycle.alive
        {
            self.state.alive_count = self.state.alive_count.saturating_sub(1);
        }
        self.state.players.remove(&player_id);
        self.state.scores.remove(&player_id);
        self.pending_inputs.remove(&player_id);

        // Finalize any active wall segments for this player
        for wall in &mut self.state.wall_segments {
            if wall.owner_id == player_id && wall.is_active {
                wall.is_active = false;
            }
        }
    }

    fn round_results(&self) -> Vec<PlayerScore> {
        self.player_ids
            .iter()
            .map(|&pid| {
                let cycle = self.state.players.get(&pid);
                let survived = cycle.is_some_and(|c| c.alive);
                let died = cycle.is_some_and(|c| c.died);
                let is_suicide = cycle.is_some_and(|c| c.is_suicide);
                let kills = cycle.map_or(0, |c| c.kills);

                PlayerScore {
                    player_id: pid,
                    score: scoring::calculate_score(survived, kills, died, is_suicide),
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
    fn init_creates_player_states() {
        let mut game = TronCycles::new();
        let players = make_players(4);
        game.init(&players, &default_config(120));
        assert_eq!(game.state.players.len(), 4);
        assert_eq!(game.state.alive_count, 4);
    }

    #[test]
    fn state_roundtrip() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        let data = game.serialize_state();
        let mut game2 = TronCycles::new();
        game2.init(&players, &default_config(120));
        game2.apply_state(&data);

        assert_eq!(game.state.players.len(), game2.state.players.len());
    }

    #[test]
    fn input_roundtrip() {
        let mut game = TronCycles::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let input = TronInput {
            turn: TurnDirection::Left,
            brake: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        assert!(game.pending_inputs.contains_key(&1));
    }

    #[test]
    fn tick_rate_is_20() {
        let game = TronCycles::new();
        assert_eq!(game.tick_rate(), 20.0);
    }

    #[test]
    fn cycles_move_forward_on_update() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        // Verify the round timer advanced (cycles move in their spawn direction)
        assert!(game.state.round_timer > 0.0, "Round timer should advance");
    }

    #[test]
    fn wall_segments_created_on_init() {
        let mut game = TronCycles::new();
        let players = make_players(3);
        game.init(&players, &default_config(120));

        // Each player should have one initial wall segment
        assert_eq!(game.state.wall_segments.len(), 3);
        for wall in &game.state.wall_segments {
            assert!(wall.is_active, "Initial segments should be active");
        }
    }

    #[test]
    fn turn_creates_new_wall_segment() {
        let mut game = TronCycles::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        // Move forward a bit first
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        for _ in 0..10 {
            game.update(0.05, &inputs);
        }

        let segments_before = game.state.wall_segments.len();

        // Apply a turn
        let input = TronInput {
            turn: TurnDirection::Left,
            brake: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        game.update(0.05, &inputs);

        assert!(
            game.state.wall_segments.len() > segments_before,
            "Turn should create a new wall segment"
        );
    }

    #[test]
    fn arena_boundary_kills_cycle() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        // Place a cycle right at the boundary
        game.state.players.get_mut(&1).unwrap().x = 0.05;
        game.state.players.get_mut(&1).unwrap().z = 250.0;
        game.state.players.get_mut(&1).unwrap().direction = Direction::West;

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        assert!(
            !game.state.players[&1].alive,
            "Cycle at arena boundary should be killed"
        );
    }

    #[test]
    fn last_player_wins_round() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        // Kill player 1
        game.kill_cycle(1, None, true);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.05, &inputs);

        assert!(game.state.round_complete, "Round should be complete");
        assert_eq!(
            game.state.winner_id,
            Some(2),
            "Player 2 should be the winner"
        );
        assert!(events.iter().any(|e| matches!(e, GameEvent::RoundComplete)));
    }

    #[test]
    fn scoring_correct() {
        let mut game = TronCycles::new();
        let players = make_players(3);
        game.init(&players, &default_config(120));

        // Player 2 hits player 3's wall → player 3 gets a kill, player 2 dies
        game.kill_cycle(2, Some(3), false);
        // Player 1 hits own wall → suicide
        game.kill_cycle(1, None, true);

        let results = game.round_results();
        let p1_score = results.iter().find(|r| r.player_id == 1).unwrap().score;
        let p2_score = results.iter().find(|r| r.player_id == 2).unwrap().score;
        let p3_score = results.iter().find(|r| r.player_id == 3).unwrap().score;

        // Player 1: died (suicide) = -4
        assert_eq!(p1_score, scoring::SUICIDE_POINTS);
        // Player 2: died (not suicide) = -2
        assert_eq!(p2_score, scoring::DEATH_POINTS);
        // Player 3: survived (+10) + 1 kill (+3) = 13
        assert_eq!(p3_score, scoring::SURVIVE_POINTS + scoring::KILL_POINTS);
    }

    #[test]
    fn brake_reduces_speed_during_game() {
        let mut game = TronCycles::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let speed_before = game.state.players[&1].speed;

        let input = TronInput {
            turn: TurnDirection::None,
            brake: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        assert!(
            game.state.players[&1].speed < speed_before,
            "Speed should decrease while braking"
        );
    }

    #[test]
    fn player_left_cleanup() {
        let mut game = TronCycles::new();
        let players = make_players(3);
        game.init(&players, &default_config(120));

        game.player_left(3);
        assert_eq!(game.state.players.len(), 2);
        assert!(!game.state.players.contains_key(&3));
    }

    // ================================================================
    // Game Trait Contract Tests
    // ================================================================

    #[test]
    fn contract_init_creates_player_state() {
        let mut game = TronCycles::new();
        breakpoint_core::test_helpers::contract_init_creates_player_state(&mut game, 4);
    }

    #[test]
    fn contract_apply_input_changes_state() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        let input = TronInput {
            turn: TurnDirection::Left,
            brake: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        breakpoint_core::test_helpers::contract_apply_input_changes_state(&mut game, &data, 1);
    }

    #[test]
    fn contract_update_advances_time() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_update_advances_time(&mut game);
    }

    #[test]
    fn contract_round_eventually_completes() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        // Use small arena so cycles hit walls quickly
        game.game_config.arena_width = 50.0;
        game.game_config.arena_depth = 50.0;
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_round_eventually_completes(&mut game, 500);
    }

    #[test]
    fn contract_state_roundtrip_preserves() {
        let mut game = TronCycles::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_state_roundtrip_preserves(&mut game);
    }

    #[test]
    fn contract_pause_stops_updates() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_pause_stops_updates(&mut game);
    }

    #[test]
    fn contract_player_left_cleanup() {
        let mut game = TronCycles::new();
        let players = make_players(3);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_player_left_cleanup(&mut game, 3, 3);
    }

    #[test]
    fn contract_round_results_complete() {
        let mut game = TronCycles::new();
        let players = make_players(4);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_round_results_complete(&game, 4);
    }

    // ================================================================
    // Input edge cases
    // ================================================================

    #[test]
    fn tron_input_encode_decode_roundtrip() {
        let input = TronInput {
            turn: TurnDirection::Right,
            brake: true,
        };
        let encoded = rmp_serde::to_vec(&input).unwrap();
        let decoded: TronInput = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(decoded.turn, input.turn);
        assert_eq!(decoded.brake, input.brake);
    }

    #[test]
    fn tron_apply_input_garbage_no_panic() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        let garbage: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01, 0xAB, 0xCD];
        game.apply_input(1, &garbage);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);
        // Should not panic
    }

    #[test]
    fn tron_apply_state_truncated_no_panic() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        let state = game.serialize_state();
        let truncated = &state[..state.len() / 2];
        game.apply_state(truncated);

        // Game should still be functional
        assert_eq!(game.state.players.len(), 2);
    }

    #[test]
    fn tron_turn_input_not_lost_across_overwrites() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        // Frame N: turn left
        let input1 = TronInput {
            turn: TurnDirection::Left,
            brake: false,
        };
        let data1 = rmp_serde::to_vec(&input1).unwrap();
        game.apply_input(1, &data1);

        // Frame N+1: no turn (would overwrite in naive impl)
        let input2 = TronInput {
            turn: TurnDirection::None,
            brake: false,
        };
        let data2 = rmp_serde::to_vec(&input2).unwrap();
        game.apply_input(1, &data2);

        // Turn should be preserved
        assert_eq!(
            game.pending_inputs.get(&1).unwrap().turn,
            TurnDirection::Left,
            "Turn flag must be preserved across input overwrites"
        );
    }

    #[test]
    fn tron_double_pause_single_resume() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        game.pause();
        game.pause();
        game.resume();

        let timer_before = game.state.round_timer;
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        assert!(
            game.state.round_timer > timer_before,
            "Timer should advance after resume"
        );
    }

    #[test]
    fn tron_update_after_round_complete_is_noop() {
        let mut game = TronCycles::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        // Force round complete
        game.state.round_complete = true;

        let timer = game.state.round_timer;
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.05, &inputs);

        assert!(
            (game.state.round_timer - timer).abs() < 0.001,
            "Timer should not advance after round complete"
        );
        assert!(events.is_empty(), "No events after round complete");
    }
}
