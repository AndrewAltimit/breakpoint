pub mod course_gen;
pub mod physics;
pub mod powerups;
pub mod scoring;

use std::collections::HashMap;
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
        for &pid in &self.player_ids.clone() {
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
                    }
                }

                // Race: track finish
                if self.state.mode == GameMode::Race
                    && player.finished
                    && !self.state.finish_order.contains(&pid)
                {
                    player.finish_time = Some(self.state.round_timer);
                    self.state.finish_order.push(pid);
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
            self.pending_inputs.insert(player_id, pi);
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
}
