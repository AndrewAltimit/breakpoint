pub mod combat;
pub mod course_gen;
pub mod enemies;
pub mod physics;
pub mod powerups;
pub mod rubber_band;
pub mod scoring;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

use breakpoint_core::breakpoint_game_boilerplate;
use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameEvent, GameMetadata, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::player::Player;

use combat::{CombatEvent, check_enemy_damage, check_player_attack};
use course_gen::{Course, Tile, generate_course};
use enemies::{Enemy, EnemyProjectile};
use physics::{
    PlatformerConfig, PlatformerInput, PlatformerPlayerState, SUBSTEPS, tick_player, try_break_wall,
};
use powerups::{ActivePowerUp, PowerUpKind, SpawnedPowerUp, select_powerup_for_position};
use rubber_band::{RubberBandFactor, compute_rubber_band};

/// Serializable game state for network broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformerState {
    pub players: HashMap<PlayerId, PlatformerPlayerState>,
    pub powerups: Vec<SpawnedPowerUp>,
    pub active_powerups: HashMap<PlayerId, Vec<ActivePowerUp>>,
    pub finish_order: Vec<PlayerId>,
    pub round_timer: f32,
    pub round_complete: bool,
    pub course: Course,
    pub enemies: Vec<Enemy>,
    pub projectiles: Vec<EnemyProjectile>,
    pub rubber_band: HashMap<PlayerId, RubberBandFactor>,
}

/// The Platform Racer game (Castlevania Rush).
pub struct PlatformRacer {
    course: Course,
    state: PlatformerState,
    player_ids: Vec<PlayerId>,
    pending_inputs: HashMap<PlayerId, PlatformerInput>,
    paused: bool,
    round_duration: f32,
    /// O(1) lookup companion for `state.finish_order`.
    finished_set: HashSet<PlayerId>,
    /// Data-driven game configuration (physics, timing).
    game_config: PlatformerConfig,
    /// Tick counter for periodic rubber-band recalculation.
    tick_counter: u32,
    /// RNG for power-up selection (seeded for determinism).
    rng: StdRng,
}

impl PlatformRacer {
    pub fn new() -> Self {
        Self::with_config(PlatformerConfig::load())
    }

    /// Create a PlatformRacer instance with explicit configuration.
    pub fn with_config(game_config: PlatformerConfig) -> Self {
        let round_duration = game_config.round_duration_secs;
        let initial_course = generate_course(42);
        Self {
            state: PlatformerState {
                players: HashMap::new(),
                powerups: Vec::new(),
                active_powerups: HashMap::new(),
                finish_order: Vec::new(),
                round_timer: 0.0,
                round_complete: false,
                course: initial_course.clone(),
                enemies: Vec::new(),
                projectiles: Vec::new(),
                rubber_band: HashMap::new(),
            },
            course: initial_course,
            player_ids: Vec::new(),
            pending_inputs: HashMap::new(),
            paused: false,
            round_duration,
            finished_set: HashSet::new(),
            game_config,
            tick_counter: 0,
            rng: StdRng::seed_from_u64(42),
        }
    }

    pub fn state(&self) -> &PlatformerState {
        &self.state
    }

    pub fn course(&self) -> &Course {
        &self.course
    }

    /// Accessor for the game configuration.
    pub fn config(&self) -> &PlatformerConfig {
        &self.game_config
    }

    // ---- Sub-update functions ----

    /// Process player movement and physics.
    fn process_player_movement(&mut self, dt: f32) {
        let sub_dt = dt / SUBSTEPS as f32;
        for i in 0..self.player_ids.len() {
            let pid = self.player_ids[i];
            let input = self.pending_inputs.remove(&pid).unwrap_or_default();

            if let Some(player) = self.state.players.get_mut(&pid) {
                // Apply speed boost from SpeedBoots power-up
                let speed_mult = if self
                    .state
                    .active_powerups
                    .get(&pid)
                    .is_some_and(|pus| pus.iter().any(|p| p.kind == PowerUpKind::SpeedBoots))
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
            }
        }
    }

    /// Process player whip attacks against enemies, plus breakable wall destruction.
    fn process_combat(&mut self) -> Vec<CombatEvent> {
        let mut events = Vec::new();

        for i in 0..self.player_ids.len() {
            let pid = self.player_ids[i];
            if let Some(player) = self.state.players.get(&pid) {
                if player.attack_timer <= 0.0 || player.death_respawn_timer > 0.0 {
                    continue;
                }

                let has_whip_extend = self
                    .state
                    .active_powerups
                    .get(&pid)
                    .is_some_and(|pus| pus.iter().any(|p| p.kind == PowerUpKind::WhipExtend));

                // Check whip vs enemies
                // We need to clone the player to avoid borrow issues
                let player_snapshot = player.clone();
                let attack_events =
                    check_player_attack(&player_snapshot, &mut self.state.enemies, has_whip_extend);
                events.extend(attack_events);

                // Check whip vs breakable walls
                let whip_dir = if player_snapshot.facing_right { 1 } else { -1 };
                let tx = (player_snapshot.x / physics::TILE_SIZE).floor() as i32 + whip_dir;
                let ty = (player_snapshot.y / physics::TILE_SIZE).floor() as i32;
                // Check the tile the whip is pointing at (and one above/below)
                try_break_wall(&mut self.course, tx, ty);
                try_break_wall(&mut self.course, tx, ty + 1);
                try_break_wall(&mut self.course, tx, ty - 1);

                // Sync course changes to state
                self.state.course = self.course.clone();
            }
        }

        events
    }

    /// Tick enemy AI and projectiles.
    fn process_enemies(&mut self, dt: f32) {
        let time = self.state.round_timer;
        enemies::tick_enemies(
            &mut self.state.enemies,
            dt,
            time,
            &mut self.state.projectiles,
        );
        enemies::tick_projectiles(&mut self.state.projectiles, dt);
    }

    /// Check enemy/projectile damage against players.
    fn process_damage(&mut self) -> Vec<CombatEvent> {
        let mut events = Vec::new();

        for i in 0..self.player_ids.len() {
            let pid = self.player_ids[i];
            if let Some(player) = self.state.players.get_mut(&pid) {
                let has_invincibility =
                    self.state.active_powerups.get(&pid).is_some_and(|pus| {
                        pus.iter().any(|p| p.kind == PowerUpKind::Invincibility)
                    });

                let damage_events = check_enemy_damage(
                    player,
                    pid,
                    &self.state.enemies,
                    &self.state.projectiles,
                    has_invincibility,
                );
                events.extend(damage_events);
            }
        }

        events
    }

    /// Process power-up collection and expiration.
    fn process_powerups(&mut self) {
        // Collect which powerups were picked up by which players
        let mut collected: Vec<(PlayerId, PowerUpKind)> = Vec::new();

        for pu in &mut self.state.powerups {
            if pu.collected {
                continue;
            }
            for &pid in &self.player_ids {
                if let Some(player) = self.state.players.get(&pid) {
                    if player.death_respawn_timer > 0.0 {
                        continue;
                    }
                    let dx = player.x - pu.x;
                    let dy = player.y - pu.y;
                    if dx * dx + dy * dy < 1.0 {
                        pu.collected = true;
                        collected.push((pid, pu.kind));
                        break;
                    }
                }
            }
        }

        // Apply collected power-ups (now that the borrow on self.state.powerups is released)
        for (pid, kind) in collected {
            self.apply_powerup(pid, kind);
        }
    }

    /// Apply a collected power-up to a player.
    fn apply_powerup(&mut self, pid: PlayerId, kind: PowerUpKind) {
        match kind {
            PowerUpKind::HolyWater => {
                // AOE: kill enemies within 5.0 units of player
                if let Some(player) = self.state.players.get(&pid) {
                    let px = player.x;
                    let py = player.y;
                    for enemy in &mut self.state.enemies {
                        if !enemy.alive {
                            continue;
                        }
                        let dx = enemy.x - px;
                        let dy = enemy.y - py;
                        if dx * dx + dy * dy < 25.0 {
                            enemies::kill_enemy(enemy);
                        }
                    }
                }
            },
            PowerUpKind::Crucifix => {
                // Screen clear: kill all alive enemies within 20.0 units
                if let Some(player) = self.state.players.get(&pid) {
                    let px = player.x;
                    let py = player.y;
                    for enemy in &mut self.state.enemies {
                        if !enemy.alive {
                            continue;
                        }
                        let dx = enemy.x - px;
                        let dy = enemy.y - py;
                        if dx * dx + dy * dy < 400.0 {
                            enemies::kill_enemy(enemy);
                        }
                    }
                }
                // Also clear nearby projectiles
                if let Some(player) = self.state.players.get(&pid) {
                    let px = player.x;
                    let py = player.y;
                    self.state.projectiles.retain(|proj| {
                        let dx = proj.x - px;
                        let dy = proj.y - py;
                        dx * dx + dy * dy >= 400.0
                    });
                }
            },
            PowerUpKind::DoubleJump => {
                if let Some(p) = self.state.players.get_mut(&pid) {
                    p.has_double_jump = true;
                }
                let active_pu = ActivePowerUp::new(kind);
                self.state
                    .active_powerups
                    .entry(pid)
                    .or_default()
                    .push(active_pu);
            },
            PowerUpKind::ArmorUp => {
                if let Some(p) = self.state.players.get_mut(&pid) {
                    p.max_hp += 1;
                    p.hp += 1;
                }
                let active_pu = ActivePowerUp::new(kind);
                self.state
                    .active_powerups
                    .entry(pid)
                    .or_default()
                    .push(active_pu);
            },
            PowerUpKind::SpeedBoots | PowerUpKind::Invincibility | PowerUpKind::WhipExtend => {
                let active_pu = ActivePowerUp::new(kind);
                self.state
                    .active_powerups
                    .entry(pid)
                    .or_default()
                    .push(active_pu);
            },
        }
    }

    /// Tick active power-ups (decrement timers, remove expired).
    fn tick_active_powerups(&mut self, dt: f32) {
        for pus in self.state.active_powerups.values_mut() {
            for pu in pus.iter_mut() {
                pu.tick(dt);
            }
            pus.retain(|p| !p.is_expired());
        }
    }

    /// Recalculate rubber-banding factors (every 30 ticks).
    fn update_rubber_banding(&mut self) {
        self.tick_counter += 1;
        if self.tick_counter.is_multiple_of(30) {
            self.state.rubber_band = compute_rubber_band(&self.state.players);
        }
    }

    /// Check for race finish and round completion.
    fn check_finish(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        for i in 0..self.player_ids.len() {
            let pid = self.player_ids[i];
            if let Some(player) = self.state.players.get_mut(&pid)
                && player.finished
                && !self.finished_set.contains(&pid)
            {
                player.finish_time = Some(scoring::finish_time_with_penalty(
                    self.state.round_timer,
                    player.deaths,
                ));
                self.state.finish_order.push(pid);
                self.finished_set.insert(pid);
                events.push(GameEvent::ScoreUpdate {
                    player_id: pid,
                    score: scoring::race_score(
                        Some(self.state.finish_order.len() - 1),
                        player.deaths,
                    ),
                });
            }
        }

        // Round completion: all finished or timer expired
        let timer_expired = self.state.round_timer >= self.round_duration;
        let all_finished = self.state.finish_order.len() == self.player_ids.len();

        if all_finished || timer_expired {
            self.state.round_complete = true;
            events.push(GameEvent::RoundComplete);
        }

        events
    }
}

impl Default for PlatformRacer {
    fn default() -> Self {
        Self::with_config(PlatformerConfig::default())
    }
}

impl BreakpointGame for PlatformRacer {
    fn metadata(&self) -> GameMetadata {
        GameMetadata {
            name: "Castlevania Rush".to_string(),
            description: "Race through a Castlevania-style castle, fighting monsters!".to_string(),
            min_players: 2,
            max_players: 6,
            estimated_round_duration: Duration::from_secs(180),
        }
    }

    fn tick_rate(&self) -> f32 {
        20.0
    }

    fn init(&mut self, players: &[Player], config: &GameConfig) {
        // Parse seed from config, or use default
        let seed = config
            .custom
            .get("seed")
            .and_then(|v| v.as_u64())
            .unwrap_or(42);

        self.course = generate_course(seed);
        self.rng = StdRng::seed_from_u64(seed.wrapping_add(12345));

        // Initialize enemies from course spawns
        let enemies: Vec<Enemy> = self
            .course
            .enemy_spawns
            .iter()
            .enumerate()
            .map(|(i, spawn)| Enemy::from_spawn(i as u16, spawn))
            .collect();

        self.state = PlatformerState {
            players: HashMap::new(),
            powerups: Vec::new(),
            active_powerups: HashMap::new(),
            finish_order: Vec::new(),
            round_timer: 0.0,
            round_complete: false,
            course: self.course.clone(),
            enemies,
            projectiles: Vec::new(),
            rubber_band: HashMap::new(),
        };
        self.player_ids.clear();
        self.pending_inputs.clear();
        self.paused = false;
        self.finished_set.clear();
        self.round_duration = config.round_duration.as_secs_f32();
        self.tick_counter = 0;

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
                if self.course.get_tile(x as i32, y as i32) == Tile::PowerUpSpawn {
                    // Use rubber-band quality for initial selection (middle tier)
                    let kind = select_powerup_for_position(0.5, &mut self.rng);
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

        // 1. Player movement and physics
        self.process_player_movement(dt);

        // 2. Player attacks vs enemies
        let combat_events = self.process_combat();
        // Convert combat events to game events if needed
        for ce in &combat_events {
            if let CombatEvent::PlayerDied { player_id } = ce {
                events.push(GameEvent::ScoreUpdate {
                    player_id: *player_id,
                    score: -1,
                });
            }
        }

        // 3. Enemy AI ticks
        self.process_enemies(dt);

        // 4. Enemy/projectile vs player damage
        let damage_events = self.process_damage();
        for ce in &damage_events {
            if let CombatEvent::PlayerDied { player_id } = ce {
                events.push(GameEvent::ScoreUpdate {
                    player_id: *player_id,
                    score: -1,
                });
            }
        }

        // 5. Power-up collection
        self.process_powerups();

        // 6. Tick active power-ups
        self.tick_active_powerups(dt);

        // 7. Rubber banding
        self.update_rubber_banding();

        // 8. Check finish / round completion
        let finish_events = self.check_finish();
        events.extend(finish_events);

        events
    }

    breakpoint_game_boilerplate!(state_type: PlatformerState);

    fn apply_input(&mut self, player_id: PlayerId, input: &[u8]) {
        match rmp_serde::from_slice::<PlatformerInput>(input) {
            Err(e) => {
                tracing::debug!(player_id, error = %e, "Dropped malformed platformer input");
            },
            Ok(pi) => {
                // Accumulate transient flags (jump, attack, use_powerup) across frames.
                if let Some(existing) = self.pending_inputs.get_mut(&player_id) {
                    existing.move_dir = pi.move_dir;
                    if pi.jump {
                        existing.jump = true;
                    }
                    if pi.use_powerup {
                        existing.use_powerup = true;
                    }
                    if pi.attack {
                        existing.attack = true;
                    }
                } else {
                    self.pending_inputs.insert(player_id, pi);
                }
            },
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
        self.player_ids
            .iter()
            .map(|&pid| {
                let pos = self.state.finish_order.iter().position(|&id| id == pid);
                let deaths = self.state.players.get(&pid).map(|p| p.deaths).unwrap_or(0);
                PlayerScore {
                    player_id: pid,
                    score: scoring::race_score(pos, deaths),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use breakpoint_core::test_helpers::{default_config, make_players};

    /// Helper: build empty PlayerInputs.
    fn empty_inputs() -> PlayerInputs {
        PlayerInputs {
            inputs: HashMap::new(),
        }
    }

    #[test]
    fn init_creates_player_states() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(180));
        assert_eq!(game.state.players.len(), 3);
    }

    #[test]
    fn state_roundtrip() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let data = game.serialize_state();
        let mut game2 = PlatformRacer::new();
        game2.init(&players, &default_config(180));
        game2.apply_state(&data);

        assert_eq!(game.state.players.len(), game2.state.players.len());
    }

    #[test]
    fn input_roundtrip() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            use_powerup: false,
            attack: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        assert!(game.pending_inputs.contains_key(&1));
    }

    #[test]
    fn tick_rate_is_20() {
        let game = PlatformRacer::new();
        assert_eq!(game.tick_rate(), 20.0);
    }

    #[test]
    fn metadata_is_castlevania_rush() {
        let game = PlatformRacer::new();
        let meta = game.metadata();
        assert_eq!(meta.name, "Castlevania Rush");
        assert_eq!(meta.max_players, 6);
        assert_eq!(meta.estimated_round_duration, Duration::from_secs(180));
    }

    #[test]
    fn enemies_initialized_from_course() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        assert!(
            !game.state.enemies.is_empty(),
            "Enemies should be initialized from course spawns"
        );
    }

    #[test]
    fn powerups_spawned_from_course() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        assert!(
            !game.state.powerups.is_empty(),
            "Power-ups should be spawned from course"
        );
    }

    #[test]
    fn race_round_completion() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(180));

        // Mark all players as finished
        for &pid in &game.player_ids.clone() {
            if let Some(player) = game.state.players.get_mut(&pid) {
                player.finished = true;
            }
        }

        let events = game.update(1.0 / 20.0, &empty_inputs());

        assert!(
            game.state.round_complete,
            "Round should be complete when all players finish"
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
        game.init(&players, &default_config(1));

        let events = game.update(2.0, &empty_inputs());

        assert!(
            game.state.round_complete,
            "Round should complete when timer exceeds duration"
        );
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::RoundComplete)),
            "RoundComplete event should be emitted on timer expiry"
        );
    }

    #[test]
    fn duplicate_finish_only_counted_once() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        game.state.players.get_mut(&1).unwrap().finished = true;

        game.update(1.0 / 20.0, &empty_inputs());
        game.update(1.0 / 20.0, &empty_inputs());

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
    fn speed_boots_multiplies_movement() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let initial_x = game.state.players[&1].x;

        // Give player SpeedBoots
        game.state
            .active_powerups
            .entry(1)
            .or_default()
            .push(ActivePowerUp::new(PowerUpKind::SpeedBoots));

        for _ in 0..20 {
            let input = PlatformerInput {
                move_dir: 1.0,
                jump: false,
                use_powerup: false,
                attack: false,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game.apply_input(1, &data);
            game.update(1.0 / 20.0, &empty_inputs());
        }
        let boosted_dx = game.state.players[&1].x - initial_x;

        // Now test without boost
        let mut game2 = PlatformRacer::new();
        let players2 = make_players(1);
        game2.init(&players2, &default_config(180));
        let initial_x2 = game2.state.players[&1].x;

        for _ in 0..20 {
            let input = PlatformerInput {
                move_dir: 1.0,
                jump: false,
                use_powerup: false,
                attack: false,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game2.apply_input(1, &data);
            game2.update(1.0 / 20.0, &empty_inputs());
        }
        let normal_dx = game2.state.players[&1].x - initial_x2;

        assert!(
            boosted_dx > normal_dx * 1.2,
            "Boosted movement ({boosted_dx}) should be notably more than normal ({normal_dx})"
        );
    }

    #[test]
    fn holy_water_kills_nearby_enemies() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let pid = 1u64;
        let player = &game.state.players[&pid];
        let px = player.x;
        let py = player.y;

        // Place an enemy near the player
        game.state.enemies.push(Enemy::from_spawn(
            100,
            &enemies::EnemySpawn {
                x: px + 2.0,
                y: py,
                enemy_type: enemies::EnemyType::Skeleton,
                patrol_min_x: px,
                patrol_max_x: px + 4.0,
            },
        ));
        // Place an enemy far from the player
        game.state.enemies.push(Enemy::from_spawn(
            101,
            &enemies::EnemySpawn {
                x: px + 50.0,
                y: py,
                enemy_type: enemies::EnemyType::Skeleton,
                patrol_min_x: px + 48.0,
                patrol_max_x: px + 52.0,
            },
        ));

        let near_idx = game.state.enemies.len() - 2;
        let far_idx = game.state.enemies.len() - 1;

        // Apply HolyWater
        game.apply_powerup(pid, PowerUpKind::HolyWater);

        assert!(
            !game.state.enemies[near_idx].alive,
            "Nearby enemy should be killed by HolyWater"
        );
        assert!(
            game.state.enemies[far_idx].alive,
            "Far enemy should NOT be killed by HolyWater"
        );
    }

    #[test]
    fn crucifix_clears_wide_area() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let pid = 1u64;
        let player = &game.state.players[&pid];
        let px = player.x;
        let py = player.y;

        // Place enemy within 20 units
        game.state.enemies.push(Enemy::from_spawn(
            200,
            &enemies::EnemySpawn {
                x: px + 15.0,
                y: py,
                enemy_type: enemies::EnemyType::Bat,
                patrol_min_x: px + 13.0,
                patrol_max_x: px + 17.0,
            },
        ));

        let idx = game.state.enemies.len() - 1;

        // Apply Crucifix
        game.apply_powerup(pid, PowerUpKind::Crucifix);

        assert!(
            !game.state.enemies[idx].alive,
            "Enemy within 20 units should be killed by Crucifix"
        );
    }

    #[test]
    fn armor_up_increases_max_hp() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let pid = 1u64;
        assert_eq!(game.state.players[&pid].max_hp, 3);

        game.apply_powerup(pid, PowerUpKind::ArmorUp);

        assert_eq!(
            game.state.players[&pid].max_hp, 4,
            "ArmorUp should increase max HP"
        );
        assert_eq!(
            game.state.players[&pid].hp, 4,
            "ArmorUp should also heal the new HP point"
        );
    }

    #[test]
    fn double_jump_powerup_grants_ability() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let pid = 1u64;
        assert!(!game.state.players[&pid].has_double_jump);

        game.apply_powerup(pid, PowerUpKind::DoubleJump);

        assert!(
            game.state.players[&pid].has_double_jump,
            "DoubleJump powerup should grant double jump"
        );
    }

    #[test]
    fn powerup_expiration() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let pid = 1u64;

        // SpeedBoots (5s) and DoubleJump (infinite)
        game.state
            .active_powerups
            .entry(pid)
            .or_default()
            .push(ActivePowerUp::new(PowerUpKind::SpeedBoots));
        game.state
            .active_powerups
            .entry(pid)
            .or_default()
            .push(ActivePowerUp::new(PowerUpKind::DoubleJump));

        assert_eq!(game.state.active_powerups[&pid].len(), 2);

        // Tick enough for SpeedBoots to expire (5s), at 20Hz = 100 ticks + extra
        for _ in 0..120 {
            game.update(1.0 / 20.0, &empty_inputs());
        }

        let pus = &game.state.active_powerups[&pid];
        assert_eq!(
            pus.len(),
            1,
            "SpeedBoots should have expired, leaving only DoubleJump"
        );
        assert_eq!(
            pus[0].kind,
            PowerUpKind::DoubleJump,
            "Remaining power-up should be DoubleJump"
        );
    }

    #[test]
    fn course_generation_reproducibility() {
        let seed = 12345u64;
        let course_a = generate_course(seed);
        let course_b = generate_course(seed);

        assert_eq!(course_a.width, course_b.width);
        assert_eq!(course_a.height, course_b.height);
        assert_eq!(course_a.tiles, course_b.tiles);
        assert_eq!(course_a.spawn_x, course_b.spawn_x);
        assert_eq!(course_a.spawn_y, course_b.spawn_y);

        let course_c = generate_course(seed + 1);
        assert_ne!(course_a.tiles, course_c.tiles);
    }

    #[test]
    fn round_complete_when_all_finished() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(180));

        for &pid in &game.player_ids.clone() {
            game.state.players.get_mut(&pid).unwrap().finished = true;
        }

        let events = game.update(1.0 / 20.0, &empty_inputs());
        assert!(game.state.round_complete);
        assert!(events.iter().any(|e| matches!(e, GameEvent::RoundComplete)));
    }

    #[test]
    fn platformer_jump_input_not_lost_across_overwrites() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        for _ in 0..20 {
            game.update(1.0 / 20.0, &empty_inputs());
        }

        // Frame N: jump=true
        let input_jump = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            use_powerup: false,
            attack: false,
        };
        let data_jump = rmp_serde::to_vec(&input_jump).unwrap();
        game.apply_input(1, &data_jump);

        // Frame N+1: jump=false (would overwrite in old code)
        let input_no_jump = PlatformerInput {
            move_dir: 1.0,
            jump: false,
            use_powerup: false,
            attack: false,
        };
        let data_no_jump = rmp_serde::to_vec(&input_no_jump).unwrap();
        game.apply_input(1, &data_no_jump);

        assert!(
            game.pending_inputs.get(&1).is_some_and(|i| i.jump),
            "Jump flag must be preserved across input overwrites"
        );
    }

    #[test]
    fn attack_input_not_lost_across_overwrites() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let input_attack = PlatformerInput {
            move_dir: 0.0,
            jump: false,
            use_powerup: false,
            attack: true,
        };
        let data = rmp_serde::to_vec(&input_attack).unwrap();
        game.apply_input(1, &data);

        let input_no_attack = PlatformerInput {
            move_dir: 0.0,
            jump: false,
            use_powerup: false,
            attack: false,
        };
        let data = rmp_serde::to_vec(&input_no_attack).unwrap();
        game.apply_input(1, &data);

        assert!(
            game.pending_inputs.get(&1).is_some_and(|i| i.attack),
            "Attack flag must be preserved across input overwrites"
        );
    }

    // ================================================================
    // NaN/Inf/Degenerate Input Fuzzing
    // ================================================================

    #[test]
    fn platformer_apply_input_nan_move_no_panic() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let input = PlatformerInput {
            move_dir: f32::NAN,
            jump: false,
            use_powerup: false,
            attack: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        game.update(1.0 / 20.0, &empty_inputs());
    }

    #[test]
    fn platformer_apply_input_inf_move_no_panic() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let input = PlatformerInput {
            move_dir: f32::INFINITY,
            jump: false,
            use_powerup: false,
            attack: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        game.update(1.0 / 20.0, &empty_inputs());
    }

    // ================================================================
    // Serialization Fuzzing
    // ================================================================

    #[test]
    fn platformer_apply_input_garbage_no_panic() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let garbage: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01, 0xAB, 0xCD];
        game.apply_input(1, &garbage);

        assert!(
            !game.state.players[&1].finished,
            "Garbage input should not finish the player"
        );
    }

    #[test]
    fn platformer_apply_state_truncated_no_panic() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let state = game.serialize_state();
        let truncated = &state[..state.len() / 2];
        game.apply_state(truncated);

        assert_eq!(game.state.players.len(), 1);
    }

    // ================================================================
    // State Machine Transition Tests
    // ================================================================

    #[test]
    fn platformer_double_pause_single_resume_works() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        game.pause();
        game.pause();
        game.resume();

        let timer_before = game.state.round_timer;
        game.update(1.0 / 20.0, &empty_inputs());

        assert!(
            game.state.round_timer > timer_before,
            "Timer should advance after resume"
        );
    }

    #[test]
    fn platformer_update_after_round_complete_is_noop() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        game.state.players.get_mut(&1).unwrap().finished = true;
        game.state.finish_order.push(1);
        game.finished_set.insert(1);
        game.update(1.0 / 20.0, &empty_inputs());
        assert!(game.is_round_complete());

        let timer = game.state.round_timer;
        let events = game.update(1.0 / 20.0, &empty_inputs());
        assert!(
            (game.state.round_timer - timer).abs() < 0.01,
            "Timer should not advance after round complete"
        );
        assert!(events.is_empty(), "No events after round complete");
    }

    // ================================================================
    // Platformer Edge Cases
    // ================================================================

    #[test]
    fn checkpoint_not_lost_on_backward_movement() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        game.state.players.get_mut(&1).unwrap().last_checkpoint_id = 3;
        game.state.players.get_mut(&1).unwrap().last_checkpoint_x = 50.0;
        game.state.players.get_mut(&1).unwrap().last_checkpoint_y = 2.0;
        let checkpoint_id = 3u16;

        game.state.players.get_mut(&1).unwrap().x = 30.0;
        game.state.players.get_mut(&1).unwrap().y = 2.0;

        let input = PlatformerInput {
            move_dir: -1.0,
            jump: false,
            use_powerup: false,
            attack: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        for _ in 0..10 {
            game.apply_input(1, &data);
            game.update(1.0 / 20.0, &empty_inputs());
        }

        assert!(
            game.state.players[&1].last_checkpoint_id >= checkpoint_id,
            "Checkpoint ID should not regress: expected >= {checkpoint_id}, got {}",
            game.state.players[&1].last_checkpoint_id
        );
    }

    #[test]
    fn simultaneous_finish_produces_valid_scores() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        game.state.players.get_mut(&1).unwrap().finished = true;
        game.state.players.get_mut(&2).unwrap().finished = true;
        game.state.finish_order.push(1);
        game.state.finish_order.push(2);
        game.finished_set.insert(1);
        game.finished_set.insert(2);

        game.update(1.0 / 20.0, &empty_inputs());
        assert!(game.is_round_complete());

        let results = game.round_results();
        assert_eq!(results.len(), 2);
        for result in &results {
            assert!(
                result.score >= 0,
                "Player {} should have non-negative score, got {}",
                result.player_id,
                result.score
            );
        }
    }

    #[test]
    fn checkpoint_advances_on_checkpoint_tile() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let pid = 1u64;

        // Find the first Checkpoint tile
        let mut checkpoint_tile: Option<(u32, u32)> = None;
        for y in 0..game.course.height {
            for x in 0..game.course.width {
                if game.course.get_tile(x as i32, y as i32) == Tile::Checkpoint {
                    checkpoint_tile = Some((x, y));
                    break;
                }
            }
            if checkpoint_tile.is_some() {
                break;
            }
        }
        let (cx, cy) = checkpoint_tile.expect("Course should have at least one Checkpoint tile");

        let initial_cp_id = game.state.players[&pid].last_checkpoint_id;
        let world_x = cx as f32 * physics::TILE_SIZE + physics::TILE_SIZE / 2.0;
        let world_y = cy as f32 * physics::TILE_SIZE + physics::TILE_SIZE / 2.0;

        let player = game.state.players.get_mut(&pid).unwrap();
        player.x = world_x;
        player.y = world_y;

        game.update(1.0 / 20.0, &empty_inputs());

        let player = &game.state.players[&pid];
        assert!(
            player.last_checkpoint_id > initial_cp_id,
            "Checkpoint ID should have advanced: initial={initial_cp_id}, current={}",
            player.last_checkpoint_id
        );
    }

    #[test]
    fn course_always_has_finish_tile() {
        for seed in 0..10 {
            let course = generate_course(seed);
            let has_finish = course.tiles.iter().any(|t| matches!(t, Tile::Finish));
            assert!(
                has_finish,
                "Course with seed {seed} should have at least one Finish tile"
            );
        }
    }

    #[test]
    fn platformer_move_right_increases_x() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let initial_x = game.state.players[&1].x;

        for _ in 0..30 {
            let input = PlatformerInput {
                move_dir: 1.0,
                jump: false,
                use_powerup: false,
                attack: false,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game.apply_input(1, &data);
            game.update(1.0 / 20.0, &empty_inputs());
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
        game.init(&players, &default_config(180));

        for _ in 0..20 {
            game.update(1.0 / 20.0, &empty_inputs());
        }

        let input = PlatformerInput {
            move_dir: 0.0,
            jump: true,
            use_powerup: false,
            attack: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        game.update(1.0 / 20.0, &empty_inputs());

        let player = &game.state.players[&1];
        assert!(
            player.vy > 0.0 || !player.grounded,
            "Player should have upward velocity or be airborne after jump: vy={}, grounded={}",
            player.vy,
            player.grounded
        );
    }

    // ================================================================
    // Input encoding/decoding roundtrip tests
    // ================================================================

    #[test]
    fn platformer_input_encode_decode_roundtrip() {
        let input = PlatformerInput {
            move_dir: -1.0,
            jump: true,
            use_powerup: true,
            attack: true,
        };
        let encoded = rmp_serde::to_vec(&input).unwrap();
        let decoded: PlatformerInput = rmp_serde::from_slice(&encoded).unwrap();
        assert!((decoded.move_dir - input.move_dir).abs() < 1e-5);
        assert_eq!(decoded.jump, input.jump);
        assert_eq!(decoded.use_powerup, input.use_powerup);
        assert_eq!(decoded.attack, input.attack);
    }

    #[test]
    fn platformer_input_through_protocol_roundtrip() {
        use breakpoint_core::net::messages::{ClientMessage, PlayerInputMsg};
        use breakpoint_core::net::protocol::{decode_client_message, encode_client_message};

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: true,
            use_powerup: false,
            attack: true,
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
                assert!(plat_input.attack);
            },
            other => panic!("Expected PlayerInput, got {:?}", other),
        }
    }

    #[test]
    fn platformer_input_apply_changes_game_state() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let before = game.serialize_state();

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: false,
            use_powerup: false,
            attack: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        game.update(1.0 / 20.0, &empty_inputs());

        breakpoint_core::test_helpers::assert_game_state_changed(&game, &before);
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
        game.init(&players, &default_config(180));

        let input = PlatformerInput {
            move_dir: 1.0,
            jump: false,
            use_powerup: false,
            attack: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        breakpoint_core::test_helpers::contract_apply_input_changes_state(&mut game, &data, 1);
    }

    #[test]
    fn contract_update_advances_time() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));
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
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_state_roundtrip_preserves(&mut game);
    }

    #[test]
    fn contract_pause_stops_updates() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_pause_stops_updates(&mut game);
    }

    #[test]
    fn contract_player_left_cleanup() {
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_player_left_cleanup(&mut game, 2, 2);
    }

    #[test]
    fn contract_round_results_complete() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_round_results_complete(&game, 3);
    }

    // ================================================================
    // Enemy interaction tests
    // ================================================================

    #[test]
    fn enemies_tick_during_update() {
        let mut game = PlatformRacer::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        // Record initial enemy positions
        let initial_positions: Vec<(f32, f32)> = game
            .state
            .enemies
            .iter()
            .filter(|e| e.alive)
            .map(|e| (e.x, e.y))
            .collect();

        // Tick several times
        for _ in 0..20 {
            game.update(1.0 / 20.0, &empty_inputs());
        }

        // Some enemies should have moved
        let moved = game
            .state
            .enemies
            .iter()
            .filter(|e| e.alive)
            .zip(initial_positions.iter())
            .any(|(e, &(ix, iy))| (e.x - ix).abs() > 0.01 || (e.y - iy).abs() > 0.01);

        assert!(
            moved,
            "At least some enemies should have moved during updates"
        );
    }

    #[test]
    fn rubber_banding_recalculates_periodically() {
        let mut game = PlatformRacer::new();
        let players = make_players(3);
        game.init(&players, &default_config(180));

        // Spread players out
        game.state.players.get_mut(&1).unwrap().x = 100.0;
        game.state.players.get_mut(&2).unwrap().x = 50.0;
        game.state.players.get_mut(&3).unwrap().x = 10.0;

        // Tick 30 times to trigger rubber band recalculation
        for _ in 0..30 {
            game.update(1.0 / 20.0, &empty_inputs());
        }

        assert!(
            !game.state.rubber_band.is_empty(),
            "Rubber band factors should be populated after 30 ticks"
        );
    }

    #[test]
    fn death_penalty_affects_score() {
        // Player with deaths should score lower than player without
        assert!(
            scoring::race_score(Some(0), 4) < scoring::race_score(Some(0), 0),
            "Deaths should reduce score"
        );
    }

    #[test]
    fn serialized_state_fits_protocol_limit() {
        // The protocol has a 64 KiB limit. Verify the initialized state fits.
        let mut game = PlatformRacer::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let state_bytes = game.serialize_state();
        eprintln!(
            "Serialized PlatformerState size: {} bytes",
            state_bytes.len()
        );

        // Protocol MAX_MESSAGE_SIZE is 64 KiB (65536 bytes).
        // The GameState wrapper adds tick (u32) + the state_data blob +
        // another MessagePack envelope + 1-byte type prefix.
        // Total overhead is small, so state_data itself should be well under 60 KiB.
        assert!(
            state_bytes.len() < 60_000,
            "Serialized state is {} bytes, exceeds 60 KiB safety margin for 64 KiB protocol limit",
            state_bytes.len()
        );
    }
}
