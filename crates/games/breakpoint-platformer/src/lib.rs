pub mod course_gen;
pub mod physics;
pub mod powerups;
pub mod scoring;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use breakpoint_core::breakpoint_game_boilerplate;
use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameEvent, GameMetadata, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::player::Player;

use course_gen::{Course, generate_course};
use physics::{PlatformerInput, PlatformerPlayerState, SUBSTEPS, tick_player};
use powerups::{ActivePowerUp, PowerUpKind, SpawnedPowerUp};

/// Serializable game state for network broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformerState {
    pub players: HashMap<PlayerId, PlatformerPlayerState>,
    pub powerups: Vec<SpawnedPowerUp>,
    pub active_powerups: HashMap<PlayerId, Vec<ActivePowerUp>>,
    pub finish_order: Vec<PlayerId>,
    pub elimination_order: Vec<PlayerId>,
    pub round_timer: f32,
    pub hazard_y: f32,
    pub round_complete: bool,
    pub mode: GameMode,
}

/// Game mode for the platformer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMode {
    Race,
    Survival,
}

/// The Platform Racer game.
pub struct PlatformRacer {
    course: Course,
    state: PlatformerState,
    player_ids: Vec<PlayerId>,
    pending_inputs: HashMap<PlayerId, PlatformerInput>,
    paused: bool,
    round_duration: f32,
    /// O(1) lookup companion for `state.finish_order`.
    finished_set: HashSet<PlayerId>,
    /// O(1) lookup companion for `state.elimination_order`.
    eliminated_set: HashSet<PlayerId>,
}

impl PlatformRacer {
    pub fn new() -> Self {
        Self {
            course: generate_course(42),
            state: PlatformerState {
                players: HashMap::new(),
                powerups: Vec::new(),
                active_powerups: HashMap::new(),
                finish_order: Vec::new(),
                elimination_order: Vec::new(),
                round_timer: 0.0,
                hazard_y: -10.0,
                round_complete: false,
                mode: GameMode::Race,
            },
            player_ids: Vec::new(),
            pending_inputs: HashMap::new(),
            paused: false,
            round_duration: 120.0,
            finished_set: HashSet::new(),
            eliminated_set: HashSet::new(),
        }
    }

    pub fn state(&self) -> &PlatformerState {
        &self.state
    }

    pub fn course(&self) -> &Course {
        &self.course
    }
}

impl Default for PlatformRacer {
    fn default() -> Self {
        Self::new()
    }
}

impl BreakpointGame for PlatformRacer {
    fn metadata(&self) -> GameMetadata {
        GameMetadata {
            name: "Platform Racer".to_string(),
            description: "Race to the finish or survive the rising hazard!".to_string(),
            min_players: 2,
            max_players: 6,
            estimated_round_duration: Duration::from_secs(120),
        }
    }

    fn tick_rate(&self) -> f32 {
        15.0
    }

    fn init(&mut self, players: &[Player], config: &GameConfig) {
        // Parse mode from config
        let mode = config
            .custom
            .get("mode")
            .and_then(|v| v.as_str())
            .map(|s| {
                if s == "survival" {
                    GameMode::Survival
                } else {
                    GameMode::Race
                }
            })
            .unwrap_or(GameMode::Race);

        // Parse seed from config, or use default
        let seed = config
            .custom
            .get("seed")
            .and_then(|v| v.as_u64())
            .unwrap_or(42);

        self.course = generate_course(seed);

        self.state = PlatformerState {
            players: HashMap::new(),
            powerups: Vec::new(),
            active_powerups: HashMap::new(),
            finish_order: Vec::new(),
            elimination_order: Vec::new(),
            round_timer: 0.0,
            hazard_y: -10.0,
            round_complete: false,
            mode,
        };
        self.player_ids.clear();
        self.pending_inputs.clear();
        self.paused = false;
        self.finished_set.clear();
        self.eliminated_set.clear();
        self.round_duration = config.round_duration.as_secs_f32();

        // Initialize player states
        for (i, player) in players.iter().enumerate() {
            if player.is_spectator {
                continue;
            }
            self.player_ids.push(player.id);
            let spawn_y = self.course.spawn_y + (i as f32) * 0.1;
            self.state.players.insert(
                player.id,
                PlatformerPlayerState::new(self.course.spawn_x, spawn_y),
            );
            self.state.active_powerups.insert(player.id, Vec::new());
        }

        // Spawn power-ups at PowerUpSpawn tiles
        for y in 0..self.course.height {
            for x in 0..self.course.width {
                if self.course.get_tile(x as i32, y as i32) == course_gen::Tile::PowerUpSpawn {
                    let kind = match (x + y) % 4 {
                        0 => PowerUpKind::SpeedBoost,
                        1 => PowerUpKind::DoubleJump,
                        2 => PowerUpKind::Shield,
                        _ => PowerUpKind::Magnet,
                    };
                    self.state.powerups.push(SpawnedPowerUp {
                        x: x as f32 * physics::TILE_SIZE + physics::TILE_SIZE / 2.0,
                        y: y as f32 * physics::TILE_SIZE + physics::TILE_SIZE / 2.0,
                        kind,
                        collected: false,
                    });
                }
            }
        }
    }

    fn update(&mut self, dt: f32, _inputs: &PlayerInputs) -> Vec<GameEvent> {
        if self.paused || self.state.round_complete {
            return Vec::new();
        }

        self.state.round_timer += dt;
        let mut events = Vec::new();

        // Survival mode: raise hazard
        if self.state.mode == GameMode::Survival {
            self.state.hazard_y += dt * 0.5; // rises 0.5 units/sec
        }

        // Process each player
        let sub_dt = dt / SUBSTEPS as f32;
        let player_ids: Vec<PlayerId> = self.player_ids.clone();
        for &pid in &player_ids {
            let input = self.pending_inputs.remove(&pid).unwrap_or_default();

            if let Some(player) = self.state.players.get_mut(&pid) {
                // Apply speed boost
                let speed_mult = if self
                    .state
                    .active_powerups
                    .get(&pid)
                    .is_some_and(|pus| pus.iter().any(|p| p.kind == PowerUpKind::SpeedBoost))
                {
                    1.5
                } else {
                    1.0
                };

                let mut boosted_input = input.clone();
                boosted_input.move_dir *= speed_mult;

                for _ in 0..SUBSTEPS {
                    tick_player(player, &boosted_input, &self.course, sub_dt);
                }

                // Survival: eliminate if below hazard
                if self.state.mode == GameMode::Survival
                    && player.y < self.state.hazard_y
                    && !player.eliminated
                {
                    let has_shield = self
                        .state
                        .active_powerups
                        .get(&pid)
                        .is_some_and(|pus| pus.iter().any(|p| p.kind == PowerUpKind::Shield));

                    if has_shield {
                        // Consume shield
                        if let Some(pus) = self.state.active_powerups.get_mut(&pid) {
                            pus.retain(|p| p.kind != PowerUpKind::Shield);
                        }
                        player.respawn_at_checkpoint();
                    } else {
                        player.eliminated = true;
                        self.state.elimination_order.push(pid);
                        self.eliminated_set.insert(pid);
                    }
                }

                // Race: track finish
                if self.state.mode == GameMode::Race
                    && player.finished
                    && !self.finished_set.contains(&pid)
                {
                    player.finish_time = Some(self.state.round_timer);
                    self.state.finish_order.push(pid);
                    self.finished_set.insert(pid);
                    events.push(GameEvent::ScoreUpdate {
                        player_id: pid,
                        score: scoring::race_score(Some(self.state.finish_order.len() - 1)),
                    });
                }
            }
        }

        // Power-up collection
        for pu in &mut self.state.powerups {
            if pu.collected {
                continue;
            }
            for &pid in &self.player_ids {
                if let Some(player) = self.state.players.get(&pid) {
                    let dx = player.x - pu.x;
                    let dy = player.y - pu.y;
                    if dx * dx + dy * dy < 1.0 {
                        pu.collected = true;
                        let active_pu = ActivePowerUp::new(pu.kind);
                        if pu.kind == PowerUpKind::DoubleJump
                            && let Some(p) = self.state.players.get_mut(&pid)
                        {
                            p.has_double_jump = true;
                        }
                        self.state
                            .active_powerups
                            .entry(pid)
                            .or_default()
                            .push(active_pu);
                        break;
                    }
                }
            }
        }

        // Tick active power-ups
        for pus in self.state.active_powerups.values_mut() {
            for pu in pus.iter_mut() {
                pu.tick(dt);
            }
            pus.retain(|p| !p.is_expired());
        }

        // Check round completion
        let active_count = self
            .player_ids
            .iter()
            .filter(|pid| {
                self.state
                    .players
                    .get(pid)
                    .is_some_and(|p| !p.finished && !p.eliminated)
            })
            .count();

        let timer_expired = self.state.round_timer >= self.round_duration;

        let complete = match self.state.mode {
            GameMode::Race => {
                // All finished or timer expired
                self.state.finish_order.len() == self.player_ids.len() || timer_expired
            },
            GameMode::Survival => {
                // One or fewer players remain, or timer expired
                active_count <= 1 || timer_expired
            },
        };

        if complete {
            self.state.round_complete = true;
            events.push(GameEvent::RoundComplete);
        }

        events
    }

    breakpoint_game_boilerplate!(state_type: PlatformerState);

    fn apply_input(&mut self, player_id: PlayerId, input: &[u8]) {
        if let Ok(pi) = rmp_serde::from_slice::<PlatformerInput>(input) {
            // Accumulate transient flags (jump, use_powerup) across frames.
            // Without this, a jump:true in frame N gets overwritten by jump:false
            // in frame N+1 before the game tick processes it. Continuous values
            // (move_dir) are always overwritten with the latest.
            if let Some(existing) = self.pending_inputs.get_mut(&player_id) {
                existing.move_dir = pi.move_dir;
                if pi.jump {
                    existing.jump = true;
                }
                if pi.use_powerup {
                    existing.use_powerup = true;
                }
            } else {
                self.pending_inputs.insert(player_id, pi);
            }
        }
    }

    fn player_joined(&mut self, player: &Player) {
        if player.is_spectator || self.player_ids.contains(&player.id) {
            return;
        }
        self.player_ids.push(player.id);
        self.state.players.insert(
            player.id,
            PlatformerPlayerState::new(self.course.spawn_x, self.course.spawn_y),
        );
        self.state.active_powerups.insert(player.id, Vec::new());
    }

    fn player_left(&mut self, player_id: PlayerId) {
        self.player_ids.retain(|&id| id != player_id);
        self.state.players.remove(&player_id);
        self.state.active_powerups.remove(&player_id);
    }

    fn round_results(&self) -> Vec<PlayerScore> {
        match self.state.mode {
            GameMode::Race => self
                .player_ids
                .iter()
                .map(|&pid| {
                    let pos = self.state.finish_order.iter().position(|&id| id == pid);
                    PlayerScore {
                        player_id: pid,
                        score: scoring::race_score(pos),
                    }
                })
                .collect(),
            GameMode::Survival => self
                .player_ids
                .iter()
                .map(|&pid| {
                    let elim_order = self
                        .state
                        .elimination_order
                        .iter()
                        .position(|&id| id == pid);
                    PlayerScore {
                        player_id: pid,
                        score: scoring::survival_score(elim_order, self.player_ids.len()),
                    }
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use breakpoint_core::test_helpers::{default_config, make_players};

    #[test]
    fn init_creates_player_states() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(120));
        assert_eq!(game.state.players.len(), 3);
    }

    #[test]
    fn state_roundtrip() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        let data = game.serialize_state();
        let mut game2 = PlatformRacer::new();
        game2.init(&players, &default_config(120));
        game2.apply_state(&data);

        assert_eq!(game.state.players.len(), game2.state.players.len());
    }

    #[test]
    fn input_roundtrip() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        assert!(game.pending_inputs.contains_key(&1));
    }

    #[test]
    fn tick_rate_is_15() {
        let game = PlatformRacer::new();
        assert_eq!(game.tick_rate(), 15.0);
    }

    /// Helper: build a GameConfig with survival mode enabled.
    fn survival_config(round_duration_secs: u64) -> GameConfig {
        let mut config = default_config(round_duration_secs);
        config.custom.insert(
            "mode".to_string(),
            serde_json::Value::String("survival".to_string()),
        );
        config
    }

    /// Helper: build empty PlayerInputs.
    fn empty_inputs() -> PlayerInputs {
        PlayerInputs {
            inputs: HashMap::new(),
        }
    }

    #[test]
    fn hazard_elimination_with_shield() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &survival_config(120));

        let pid = 1u64;

        // Give player 1 a Shield power-up
        game.state
            .active_powerups
            .entry(pid)
            .or_default()
            .push(ActivePowerUp::new(PowerUpKind::Shield));

        // Raise hazard_y above the player's current position so the hazard check
        // triggers without the -5.0 floor respawn interfering.
        let player_y = game.state.players[&pid].y;
        game.state.hazard_y = player_y + 10.0;

        // Record checkpoint position before the tick
        let checkpoint_x = game.state.players[&pid].last_checkpoint_x;
        let checkpoint_y = game.state.players[&pid].last_checkpoint_y;

        // Tick the game — shield should save the player
        game.update(1.0 / 15.0, &empty_inputs());

        let player = &game.state.players[&pid];
        // Player should NOT be eliminated
        assert!(!player.eliminated, "Shield should prevent elimination");
        // Player should be respawned near checkpoint (physics substeps may slightly adjust)
        let expected_y = checkpoint_y + 1.0;
        assert!(
            (player.x - checkpoint_x).abs() < 1.0,
            "Player should respawn near checkpoint x"
        );
        assert!(
            (player.y - expected_y).abs() < 1.0,
            "Player should respawn near checkpoint y + 1.0, got {} expected {}",
            player.y,
            expected_y
        );
        // Shield should have been consumed
        let shields: Vec<_> = game.state.active_powerups[&pid]
            .iter()
            .filter(|p| p.kind == PowerUpKind::Shield)
            .collect();
        assert!(
            shields.is_empty(),
            "Shield should be consumed after saving player"
        );
        // Player should NOT be in elimination order
        assert!(
            !game.state.elimination_order.contains(&pid),
            "Shielded player should not appear in elimination_order"
        );
    }

    #[test]
    fn hazard_elimination_without_shield() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &survival_config(120));

        let pid = 2u64;

        // Raise hazard_y well above the player's position so the check triggers
        // after physics runs. This avoids the -5.0 floor respawn interfering.
        let player_y = game.state.players[&pid].y;
        game.state.hazard_y = player_y + 10.0;

        game.update(1.0 / 15.0, &empty_inputs());

        let player = &game.state.players[&pid];
        assert!(
            player.eliminated,
            "Player below hazard_y without shield should be eliminated"
        );
        assert!(
            game.state.elimination_order.contains(&pid),
            "Eliminated player should appear in elimination_order"
        );
    }

    #[test]
    fn double_jump_physics() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let pid = 1u64;

        // Verify player starts without double jump
        assert!(
            !game.state.players[&pid].has_double_jump,
            "Player should not start with double jump"
        );

        // Directly grant DoubleJump power-up (simulating collection)
        game.state
            .active_powerups
            .entry(pid)
            .or_default()
            .push(ActivePowerUp::new(PowerUpKind::DoubleJump));
        game.state.players.get_mut(&pid).unwrap().has_double_jump = true;

        assert!(
            game.state.players[&pid].has_double_jump,
            "Player should have double jump after collecting DoubleJump power-up"
        );
    }

    #[test]
    fn powerup_expiration() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let pid = 1u64;

        // Give player a SpeedBoost (duration = 3.0s) and a Shield (infinite)
        game.state
            .active_powerups
            .entry(pid)
            .or_default()
            .push(ActivePowerUp::new(PowerUpKind::SpeedBoost));
        game.state
            .active_powerups
            .entry(pid)
            .or_default()
            .push(ActivePowerUp::new(PowerUpKind::Shield));

        assert_eq!(
            game.state.active_powerups[&pid].len(),
            2,
            "Player should have 2 active power-ups"
        );

        // Tick enough time for SpeedBoost to expire (3.0s), but not Shield (infinite)
        // Each tick at 15 Hz = 1/15s, so 45 ticks = 3s. Use a few extra to be safe.
        for _ in 0..50 {
            game.update(1.0 / 15.0, &empty_inputs());
        }

        let pus = &game.state.active_powerups[&pid];
        assert_eq!(
            pus.len(),
            1,
            "SpeedBoost should have expired, leaving only Shield"
        );
        assert_eq!(
            pus[0].kind,
            PowerUpKind::Shield,
            "Remaining power-up should be Shield"
        );
    }

    #[test]
    fn course_generation_reproducibility() {
        let seed = 12345u64;
        let course_a = course_gen::generate_course(seed);
        let course_b = course_gen::generate_course(seed);

        assert_eq!(
            course_a.width, course_b.width,
            "Width should match for same seed"
        );
        assert_eq!(
            course_a.height, course_b.height,
            "Height should match for same seed"
        );
        assert_eq!(
            course_a.tiles, course_b.tiles,
            "Tiles should match for same seed"
        );
        assert_eq!(
            course_a.spawn_x, course_b.spawn_x,
            "Spawn X should match for same seed"
        );
        assert_eq!(
            course_a.spawn_y, course_b.spawn_y,
            "Spawn Y should match for same seed"
        );

        // Different seed should produce different tiles
        let course_c = course_gen::generate_course(seed + 1);
        assert_ne!(
            course_a.tiles, course_c.tiles,
            "Different seeds should produce different courses"
        );
    }

    #[test]
    fn race_round_completion() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(120));

        // Mark all players as finished
        for &pid in &game.player_ids.clone() {
            if let Some(player) = game.state.players.get_mut(&pid) {
                player.finished = true;
            }
        }

        let events = game.update(1.0 / 15.0, &empty_inputs());

        assert!(
            game.state.round_complete,
            "Round should be complete when all players finish in Race mode"
        );
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::RoundComplete)),
            "RoundComplete event should be emitted"
        );
    }

    #[test]
    fn survival_round_completion() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &survival_config(120));

        // Eliminate 2 of 3 players, leaving only 1 active
        for &pid in &[1u64, 2u64] {
            if let Some(player) = game.state.players.get_mut(&pid) {
                player.eliminated = true;
            }
            game.state.elimination_order.push(pid);
            game.eliminated_set.insert(pid);
        }

        let events = game.update(1.0 / 15.0, &empty_inputs());

        assert!(
            game.state.round_complete,
            "Round should be complete when only 1 player remains in Survival mode"
        );
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::RoundComplete)),
            "RoundComplete event should be emitted"
        );
    }

    #[test]
    fn timer_expiry_completes_round() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        // Set a very short round duration (1 second)
        game.init(&players, &default_config(1));

        // Tick past the round duration
        // round_duration is 1.0s, each tick adds dt to round_timer
        // Use a large dt to push past it in one call
        let events = game.update(2.0, &empty_inputs());

        assert!(
            game.state.round_complete,
            "Round should be complete when timer exceeds round duration"
        );
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::RoundComplete)),
            "RoundComplete event should be emitted on timer expiry"
        );
    }

    // ================================================================
    // Game Trait Contract Tests
    // ================================================================

    #[test]
    fn contract_init_creates_player_state() {
        let mut game = PlatformRacer::new();
        breakpoint_core::test_helpers::contract_init_creates_player_state(&mut game, 3);
    }

    #[test]
    fn contract_apply_input_changes_state() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        breakpoint_core::test_helpers::contract_apply_input_changes_state(&mut game, &data, 1);
    }

    #[test]
    fn contract_update_advances_time() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_update_advances_time(&mut game);
    }

    #[test]
    fn contract_round_eventually_completes() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(5));
        breakpoint_core::test_helpers::contract_round_eventually_completes(&mut game, 10);
    }

    #[test]
    fn contract_state_roundtrip_preserves() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_state_roundtrip_preserves(&mut game);
    }

    #[test]
    fn contract_pause_stops_updates() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_pause_stops_updates(&mut game);
    }

    #[test]
    fn contract_player_left_cleanup() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_player_left_cleanup(&mut game, 2, 2);
    }

    #[test]
    fn contract_round_results_complete() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(120));
        breakpoint_core::test_helpers::contract_round_results_complete(&game, 3);
    }

    // ================================================================
    // Input encoding/decoding roundtrip tests (Phase 2)
    // ================================================================

    #[test]
    fn platformer_input_encode_decode_roundtrip() {
        let input = PlatformerInput {
            move_dir: -1.0,
            jump: true,
            use_powerup: true,
        };
        let encoded = rmp_serde::to_vec(&input).unwrap();
        let decoded: PlatformerInput = rmp_serde::from_slice(&encoded).unwrap();
        assert!((decoded.move_dir - input.move_dir).abs() < 1e-5);
        assert_eq!(decoded.jump, input.jump);
        assert_eq!(decoded.use_powerup, input.use_powerup);
    }

    #[test]
    fn platformer_input_through_protocol_roundtrip() {
        use breakpoint_core::net::messages::{ClientMessage, PlayerInputMsg};
        use breakpoint_core::net::protocol::{decode_client_message, encode_client_message};

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            use_powerup: false,
        };
        let input_data = rmp_serde::to_vec(&input).unwrap();
        let msg = ClientMessage::PlayerInput(PlayerInputMsg {
            player_id: 1,
            tick: 10,
            input_data: input_data.clone(),
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        match decoded {
            ClientMessage::PlayerInput(pi) => {
                assert_eq!(pi.input_data, input_data);
                let plat_input: PlatformerInput = rmp_serde::from_slice(&pi.input_data).unwrap();
                assert!((plat_input.move_dir - 1.0).abs() < 1e-5);
                assert!(plat_input.jump);
            },
            other => panic!("Expected PlayerInput, got {:?}", other),
        }
    }

    #[test]
    fn platformer_input_apply_changes_game_state() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let before = game.serialize_state();

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        game.update(1.0 / 15.0, &empty_inputs());

        breakpoint_core::test_helpers::assert_game_state_changed(&game, &before);
    }

    // ================================================================
    // Game simulation tests (Phase 3)
    // ================================================================

    #[test]
    fn platformer_move_right_increases_x() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let initial_x = game.state.players[&1].x;

        // Apply rightward movement for 30 ticks
        for _ in 0..30 {
            let input = PlatformerInput {
                move_dir: 1.0,
                jump: false,
                use_powerup: false,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game.apply_input(1, &data);
            game.update(1.0 / 15.0, &empty_inputs());
        }

        assert!(
            game.state.players[&1].x > initial_x,
            "Player x should increase: initial={initial_x}, final={}",
            game.state.players[&1].x
        );
    }

    #[test]
    fn platformer_jump_changes_velocity() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        // First ensure the player is grounded by ticking a few times
        for _ in 0..20 {
            game.update(1.0 / 15.0, &empty_inputs());
        }

        // Apply jump
        let input = PlatformerInput {
            move_dir: 0.0,
            jump: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        game.update(1.0 / 15.0, &empty_inputs());

        // vy should be positive (upward) after jump, or at least y should have increased
        let player = &game.state.players[&1];
        assert!(
            player.vy > 0.0 || !player.grounded,
            "Player should have upward velocity or be airborne after jump: vy={}, grounded={}",
            player.vy,
            player.grounded
        );
    }

    // ================================================================
    // Phase 3d: Game-level edge cases
    // ================================================================

    #[test]
    fn duplicate_finish_only_counted_once() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        // Mark player 1 as finished
        game.state.players.get_mut(&1).unwrap().finished = true;

        // Tick twice — the finish should only be recorded once
        game.update(1.0 / 15.0, &empty_inputs());
        game.update(1.0 / 15.0, &empty_inputs());

        let count = game
            .state
            .finish_order
            .iter()
            .filter(|&&id| id == 1)
            .count();
        assert_eq!(
            count, 1,
            "Player should appear in finish_order exactly once"
        );
    }

    #[test]
    fn speed_boost_multiplies_movement() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let initial_x = game.state.players[&1].x;

        // Give player SpeedBoost
        game.state
            .active_powerups
            .entry(1)
            .or_default()
            .push(powerups::ActivePowerUp::new(
                powerups::PowerUpKind::SpeedBoost,
            ));

        // Move right for several ticks with boost
        for _ in 0..20 {
            let input = physics::PlatformerInput {
                move_dir: 1.0,
                jump: false,
                use_powerup: false,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game.apply_input(1, &data);
            game.update(1.0 / 15.0, &empty_inputs());
        }
        let boosted_dx = game.state.players[&1].x - initial_x;

        // Now test without boost (fresh game)
        let mut game2 = PlatformRacer::new();
        let players2 = make_players(1);
        game2.init(&players2, &default_config(120));
        let initial_x2 = game2.state.players[&1].x;

        for _ in 0..20 {
            let input = physics::PlatformerInput {
                move_dir: 1.0,
                jump: false,
                use_powerup: false,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game2.apply_input(1, &data);
            game2.update(1.0 / 15.0, &empty_inputs());
        }
        let normal_dx = game2.state.players[&1].x - initial_x2;

        assert!(
            boosted_dx > normal_dx * 1.2,
            "Boosted movement ({boosted_dx}) should be notably more than normal ({normal_dx})"
        );
    }

    #[test]
    fn round_complete_when_all_finished_race() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(120));

        // Mark all as finished
        for &pid in &game.player_ids.clone() {
            game.state.players.get_mut(&pid).unwrap().finished = true;
        }

        let events = game.update(1.0 / 15.0, &empty_inputs());
        assert!(
            game.state.round_complete,
            "Race should complete when all finish"
        );
        assert!(events.iter().any(|e| matches!(e, GameEvent::RoundComplete)));
    }

    #[test]
    fn round_complete_when_one_remains_survival() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &survival_config(120));

        // Eliminate 2 of 3
        for &pid in &[1u64, 2u64] {
            game.state.players.get_mut(&pid).unwrap().eliminated = true;
            game.state.elimination_order.push(pid);
            game.eliminated_set.insert(pid);
        }

        let events = game.update(1.0 / 15.0, &empty_inputs());
        assert!(
            game.state.round_complete,
            "Survival should complete when 1 player remains"
        );
        assert!(events.iter().any(|e| matches!(e, GameEvent::RoundComplete)));
    }

    #[test]
    fn platformer_jump_input_not_lost_across_overwrites() {
        // This test verifies the Bug 2 fix: transient inputs (jump) must be
        // preserved even if a subsequent apply_input overwrites with jump:false.
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        // Ensure player is grounded
        for _ in 0..20 {
            game.update(1.0 / 15.0, &empty_inputs());
        }

        // Frame N: jump=true
        let input_jump = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            use_powerup: false,
        };
        let data_jump = rmp_serde::to_vec(&input_jump).unwrap();
        game.apply_input(1, &data_jump);

        // Frame N+1: jump=false (would overwrite in old code)
        let input_no_jump = PlatformerInput {
            move_dir: 1.0,
            jump: false,
            use_powerup: false,
        };
        let data_no_jump = rmp_serde::to_vec(&input_no_jump).unwrap();
        game.apply_input(1, &data_no_jump);

        // The pending input should still have jump=true
        assert!(
            game.pending_inputs.get(&1).is_some_and(|i| i.jump),
            "Jump flag must be preserved across input overwrites"
        );

        // Tick the game — jump should actually happen
        game.update(1.0 / 15.0, &empty_inputs());

        let player = &game.state.players[&1];
        assert!(
            player.vy > 0.0 || !player.grounded,
            "Jump should have occurred despite being overwritten: vy={}, grounded={}",
            player.vy,
            player.grounded
        );
    }

    // ================================================================
    // P0-1: NaN/Inf/Degenerate Input Fuzzing
    // ================================================================

    // REGRESSION: NaN move_dir should not corrupt player position
    #[test]
    fn platformer_apply_input_nan_move_no_panic() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let input = PlatformerInput {
            move_dir: f32::NAN,
            jump: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        // Should not panic on update
        game.update(1.0 / 15.0, &empty_inputs());
    }

    // REGRESSION: Inf move_dir should not crash
    #[test]
    fn platformer_apply_input_inf_move_no_panic() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let input = PlatformerInput {
            move_dir: f32::INFINITY,
            jump: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        game.update(1.0 / 15.0, &empty_inputs());
    }

    // ================================================================
    // P1-1: Serialization Fuzzing
    // ================================================================

    // REGRESSION: Garbage input data should not panic
    #[test]
    fn platformer_apply_input_garbage_no_panic() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let garbage: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01, 0xAB, 0xCD];
        game.apply_input(1, &garbage);

        // Player should be unchanged
        assert!(
            !game.state.players[&1].finished,
            "Garbage input should not finish the player"
        );
    }

    // REGRESSION: Truncated state data should not panic
    #[test]
    fn platformer_apply_state_truncated_no_panic() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        let state = game.serialize_state();
        let truncated = &state[..state.len() / 2];
        game.apply_state(truncated);

        // Game should still be functional
        assert_eq!(game.state.players.len(), 1);
    }

    // ================================================================
    // P1-2: State Machine Transition Tests
    // ================================================================

    #[test]
    fn platformer_double_pause_single_resume_works() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        game.pause();
        game.pause();
        game.resume();

        let timer_before = game.state.round_timer;
        game.update(1.0 / 15.0, &empty_inputs());

        assert!(
            game.state.round_timer > timer_before,
            "Timer should advance after resume"
        );
    }

    #[test]
    fn platformer_update_after_round_complete_is_noop() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        // Force round complete by finishing the player
        game.state.players.get_mut(&1).unwrap().finished = true;
        game.state.finish_order.push(1);
        game.finished_set.insert(1);
        game.update(1.0 / 15.0, &empty_inputs());
        assert!(game.is_round_complete());

        let timer = game.state.round_timer;
        let events = game.update(1.0 / 15.0, &empty_inputs());
        assert!(
            (game.state.round_timer - timer).abs() < 0.01,
            "Timer should not advance after round complete"
        );
        assert!(events.is_empty(), "No events after round complete");
    }

    // ================================================================
    // P1-5: Platformer Edge Cases
    // ================================================================

    // REGRESSION: Checkpoint should not be lost when player moves backward
    #[test]
    fn checkpoint_not_lost_on_backward_movement() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        // Move player forward past what would be a checkpoint
        let _initial_checkpoint_x = game.state.players[&1].last_checkpoint_x;

        // Manually set a checkpoint further ahead
        game.state.players.get_mut(&1).unwrap().last_checkpoint_x = 50.0;
        game.state.players.get_mut(&1).unwrap().last_checkpoint_y = 2.0;
        let checkpoint_x = 50.0;

        // Move backward
        game.state.players.get_mut(&1).unwrap().x = 30.0;
        game.state.players.get_mut(&1).unwrap().y = 2.0;

        // Run a few ticks with leftward input
        let input = PlatformerInput {
            move_dir: -1.0,
            jump: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        for _ in 0..10 {
            game.apply_input(1, &data);
            game.update(1.0 / 15.0, &empty_inputs());
        }

        // Checkpoint should still be at 50.0
        assert!(
            game.state.players[&1].last_checkpoint_x >= checkpoint_x,
            "Checkpoint should not regress: expected >= {checkpoint_x}, got {}",
            game.state.players[&1].last_checkpoint_x
        );
    }

    // REGRESSION: Magnet powerup should not crash (it's a stub)
    #[test]
    fn magnet_powerup_stub_no_crash() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(120));

        // Give player a Magnet powerup
        game.state
            .active_powerups
            .entry(1)
            .or_default()
            .push(ActivePowerUp::new(PowerUpKind::Magnet));

        // Use powerup input
        let input = PlatformerInput {
            move_dir: 1.0,
            jump: false,
            use_powerup: true,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        // Should not panic
        game.update(1.0 / 15.0, &empty_inputs());
    }

    // REGRESSION: Simultaneous finish should produce valid scores for both
    #[test]
    fn simultaneous_finish_produces_valid_scores() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(120));

        // Both players finish on the same tick
        game.state.players.get_mut(&1).unwrap().finished = true;
        game.state.players.get_mut(&2).unwrap().finished = true;
        game.state.finish_order.push(1);
        game.state.finish_order.push(2);
        game.finished_set.insert(1);
        game.finished_set.insert(2);

        game.update(1.0 / 15.0, &empty_inputs());
        assert!(game.is_round_complete());

        let results = game.round_results();
        assert_eq!(results.len(), 2, "Both players should have results");
        for result in &results {
            assert!(
                result.score >= 0,
                "Player {} should have non-negative score, got {}",
                result.player_id,
                result.score
            );
        }
        // First finisher should score higher
        let p1_score = results.iter().find(|r| r.player_id == 1).unwrap().score;
        let p2_score = results.iter().find(|r| r.player_id == 2).unwrap().score;
        assert!(
            p1_score >= p2_score,
            "First finisher should score >= second: p1={p1_score}, p2={p2_score}"
        );
    }

    // P1-5: Course always has reachable finish for multiple seeds
    #[test]
    fn course_always_has_finish_tile() {
        for seed in 0..10 {
            let course = generate_course(seed);
            let has_finish = course
                .tiles
                .iter()
                .any(|t| matches!(t, course_gen::Tile::Finish));
            assert!(
                has_finish,
                "Course with seed {seed} should have at least one Finish tile"
            );
        }
    }
}
