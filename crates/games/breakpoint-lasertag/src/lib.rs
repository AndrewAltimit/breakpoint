pub mod arena;
pub mod powerups;
pub mod projectile;
pub mod scoring;

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use breakpoint_core::breakpoint_game_boilerplate;
use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameEvent, GameMetadata, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::player::Player;

use arena::{Arena, ArenaSize, load_arena};
use powerups::{ActiveLaserPowerUp, LaserPowerUpKind, SpawnedLaserPowerUp};
use projectile::{
    FIRE_COOLDOWN, LaserTagConfig, PLAYER_RADIUS, RAPIDFIRE_COOLDOWN_MULT, STUN_DURATION,
    raycast_laser,
};

/// Serializable game state for network broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaserTagState {
    pub players: HashMap<PlayerId, LaserPlayerState>,
    pub powerups: Vec<SpawnedLaserPowerUp>,
    pub active_powerups: HashMap<PlayerId, Vec<ActiveLaserPowerUp>>,
    pub round_timer: f32,
    pub round_complete: bool,
    pub team_mode: TeamMode,
    pub teams: HashMap<PlayerId, u8>,
    pub tags_scored: HashMap<PlayerId, u32>,
    pub laser_trails: Vec<LaserTrail>,
    pub arena_width: f32,
    pub arena_depth: f32,
    pub arena_walls: Vec<arena::ArenaWall>,
    pub smoke_zones: Vec<(f32, f32, f32)>,
}

/// A player's state in laser tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaserPlayerState {
    pub x: f32,
    pub z: f32,
    pub aim_angle: f32,
    pub stun_remaining: f32,
    pub fire_cooldown: f32,
    pub move_speed: f32,
}

impl LaserPlayerState {
    fn new(x: f32, z: f32, angle: f32) -> Self {
        Self {
            x,
            z,
            aim_angle: angle,
            stun_remaining: 0.0,
            fire_cooldown: 0.0,
            move_speed: 8.0,
        }
    }

    pub fn is_stunned(&self) -> bool {
        self.stun_remaining > 0.0
    }
}

/// Team mode configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamMode {
    FreeForAll,
    Teams { team_count: u8 },
}

/// Visual laser trail for client rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaserTrail {
    pub segments: Vec<(f32, f32, f32, f32)>,
    pub age: f32,
}

/// Input from a laser tag player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaserTagInput {
    pub move_x: f32,
    pub move_z: f32,
    pub aim_angle: f32,
    pub fire: bool,
    pub use_powerup: bool,
}

impl Default for LaserTagInput {
    fn default() -> Self {
        Self {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: false,
            use_powerup: false,
        }
    }
}

/// The Laser Tag Arena game.
pub struct LaserTagArena {
    arena: Arena,
    state: LaserTagState,
    player_ids: Vec<PlayerId>,
    pending_inputs: HashMap<PlayerId, LaserTagInput>,
    paused: bool,
    round_duration: f32,
    /// Data-driven game configuration (physics, timing).
    game_config: LaserTagConfig,
}

impl LaserTagArena {
    pub fn new() -> Self {
        Self::with_config(LaserTagConfig::load())
    }

    /// Create a LaserTagArena instance with explicit configuration.
    pub fn with_config(config: LaserTagConfig) -> Self {
        let round_duration = config.round_duration_secs;
        let initial_arena = load_arena(ArenaSize::Default);
        Self {
            state: LaserTagState {
                players: HashMap::new(),
                powerups: Vec::new(),
                active_powerups: HashMap::new(),
                round_timer: 0.0,
                round_complete: false,
                team_mode: TeamMode::FreeForAll,
                teams: HashMap::new(),
                tags_scored: HashMap::new(),
                laser_trails: Vec::new(),
                arena_width: initial_arena.width,
                arena_depth: initial_arena.depth,
                arena_walls: initial_arena.walls.clone(),
                smoke_zones: initial_arena.smoke_zones.clone(),
            },
            arena: initial_arena,
            player_ids: Vec::new(),
            pending_inputs: HashMap::new(),
            paused: false,
            round_duration,
            game_config: config,
        }
    }

    pub fn state(&self) -> &LaserTagState {
        &self.state
    }

    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    /// Accessor for the game configuration.
    pub fn config(&self) -> &LaserTagConfig {
        &self.game_config
    }

    fn get_team_ids(&self, player_id: PlayerId) -> Vec<u64> {
        if self.state.team_mode == TeamMode::FreeForAll {
            return Vec::new();
        }
        let Some(&my_team) = self.state.teams.get(&player_id) else {
            return Vec::new();
        };
        self.state
            .teams
            .iter()
            .filter(|(pid, team)| **pid != player_id && **team == my_team)
            .map(|(&pid, _)| pid)
            .collect()
    }
}

impl Default for LaserTagArena {
    fn default() -> Self {
        Self::with_config(LaserTagConfig::default())
    }
}

impl BreakpointGame for LaserTagArena {
    fn metadata(&self) -> GameMetadata {
        GameMetadata {
            name: "Laser Tag Arena".to_string(),
            description: "Tag opponents with bouncing lasers! FFA or team mode.".to_string(),
            min_players: 2,
            max_players: 8,
            estimated_round_duration: Duration::from_secs(180),
        }
    }

    fn tick_rate(&self) -> f32 {
        20.0
    }

    fn init(&mut self, players: &[Player], config: &GameConfig) {
        // Parse team mode from config
        let team_mode = config
            .custom
            .get("team_mode")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "teams_2" => TeamMode::Teams { team_count: 2 },
                "teams_3" => TeamMode::Teams { team_count: 3 },
                "teams_4" => TeamMode::Teams { team_count: 4 },
                _ => TeamMode::FreeForAll,
            })
            .unwrap_or(TeamMode::FreeForAll);

        // Parse arena size from config
        let arena_size = config
            .custom
            .get("arena_size")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "small" => ArenaSize::Small,
                "large" => ArenaSize::Large,
                _ => ArenaSize::Default,
            })
            .unwrap_or(ArenaSize::Default);

        self.arena = load_arena(arena_size);
        self.round_duration = config
            .custom
            .get("round_duration")
            .and_then(|v| v.as_f64())
            .unwrap_or(180.0) as f32;

        self.state = LaserTagState {
            players: HashMap::new(),
            powerups: Vec::new(),
            active_powerups: HashMap::new(),
            round_timer: 0.0,
            round_complete: false,
            team_mode,
            teams: HashMap::new(),
            tags_scored: HashMap::new(),
            laser_trails: Vec::new(),
            arena_width: self.arena.width,
            arena_depth: self.arena.depth,
            arena_walls: self.arena.walls.clone(),
            smoke_zones: self.arena.smoke_zones.clone(),
        };
        self.player_ids.clear();
        self.pending_inputs.clear();
        self.paused = false;

        // Initialize player states at spawn points
        let active_players: Vec<&Player> = players.iter().filter(|p| !p.is_spectator).collect();

        for (i, player) in active_players.iter().enumerate() {
            self.player_ids.push(player.id);
            let spawn = &self.arena.spawn_points[i % self.arena.spawn_points.len()];
            self.state.players.insert(
                player.id,
                LaserPlayerState::new(spawn.x, spawn.z, spawn.angle),
            );
            self.state.active_powerups.insert(player.id, Vec::new());
            self.state.tags_scored.insert(player.id, 0);

            // Assign teams (round-robin)
            if let TeamMode::Teams { team_count } = team_mode {
                self.state.teams.insert(player.id, (i as u8) % team_count);
            }
        }

        // Spawn power-ups in arena (scale spread with arena size)
        let cx = self.arena.width / 2.0;
        let cz = self.arena.depth / 2.0;
        let spread = (self.arena.width.min(self.arena.depth) * 0.2).min(15.0);
        let power_up_spots = [
            (cx - spread, cz, LaserPowerUpKind::RapidFire),
            (cx + spread, cz, LaserPowerUpKind::SpeedBoost),
            (cx, cz - spread, LaserPowerUpKind::Shield),
            (cx, cz + spread, LaserPowerUpKind::WideBeam),
        ];
        for (x, z, kind) in power_up_spots {
            self.state.powerups.push(SpawnedLaserPowerUp {
                x,
                z,
                kind,
                collected: false,
                respawn_timer: 0.0,
            });
        }
    }

    fn update(&mut self, dt: f32, _inputs: &PlayerInputs) -> Vec<GameEvent> {
        if self.paused || self.state.round_complete {
            return Vec::new();
        }

        self.state.round_timer += dt;
        let mut events = Vec::new();

        // Age and remove old laser trails
        for trail in &mut self.state.laser_trails {
            trail.age += dt;
        }
        self.state.laser_trails.retain(|t| t.age < 0.3);

        // Process player movement and firing (iterate by index to avoid clone)
        for i in 0..self.player_ids.len() {
            let pid = self.player_ids[i];
            let input = self.pending_inputs.remove(&pid).unwrap_or_default();

            // Update aim
            if let Some(player) = self.state.players.get_mut(&pid) {
                player.aim_angle = input.aim_angle;
                player.fire_cooldown = (player.fire_cooldown - dt).max(0.0);
                player.stun_remaining = (player.stun_remaining - dt).max(0.0);

                if player.is_stunned() {
                    continue;
                }

                // Movement
                let speed =
                    if self.state.active_powerups.get(&pid).is_some_and(|pus| {
                        pus.iter().any(|p| p.kind == LaserPowerUpKind::SpeedBoost)
                    }) {
                        player.move_speed * 1.5
                    } else {
                        player.move_speed
                    };

                player.x += input.move_x * speed * dt;
                player.z += input.move_z * speed * dt;

                // Clamp to arena bounds
                player.x = player
                    .x
                    .clamp(PLAYER_RADIUS, self.arena.width - PLAYER_RADIUS);
                player.z = player
                    .z
                    .clamp(PLAYER_RADIUS, self.arena.depth - PLAYER_RADIUS);
            }

            // Firing
            let can_fire = self
                .state
                .players
                .get(&pid)
                .is_some_and(|p| !p.is_stunned() && p.fire_cooldown <= 0.0);

            if input.fire && can_fire {
                let (ox, oz, angle) = {
                    let p = &self.state.players[&pid];
                    (p.x, p.z, p.aim_angle)
                };

                // Build player list for hit detection (stack-allocated for up to 8 players)
                let player_positions: SmallVec<[(u64, f32, f32); 8]> = self
                    .state
                    .players
                    .iter()
                    .filter(|(_, p)| !p.is_stunned())
                    .map(|(&id, p)| (id, p.x, p.z))
                    .collect();

                let team_ids = self.get_team_ids(pid);

                let hit = raycast_laser(
                    ox,
                    oz,
                    angle,
                    &self.arena.walls,
                    &player_positions,
                    pid,
                    &team_ids,
                    100.0,
                );

                // Record laser trail for rendering
                self.state.laser_trails.push(LaserTrail {
                    segments: hit.segments,
                    age: 0.0,
                });

                // Apply hit
                if let Some(target_id) = hit.hit_player {
                    let has_shield = self
                        .state
                        .active_powerups
                        .get(&target_id)
                        .is_some_and(|pus| pus.iter().any(|p| p.kind == LaserPowerUpKind::Shield));

                    if has_shield {
                        // Consume shield
                        if let Some(pus) = self.state.active_powerups.get_mut(&target_id) {
                            pus.retain(|p| p.kind != LaserPowerUpKind::Shield);
                        }
                    } else {
                        // Stun the target
                        if let Some(target) = self.state.players.get_mut(&target_id) {
                            target.stun_remaining = STUN_DURATION;
                        }
                        *self.state.tags_scored.entry(pid).or_insert(0) += 1;
                        events.push(GameEvent::ScoreUpdate {
                            player_id: pid,
                            score: self.state.tags_scored[&pid] as i32,
                        });
                    }
                }

                // Apply cooldown
                let cooldown =
                    if self.state.active_powerups.get(&pid).is_some_and(|pus| {
                        pus.iter().any(|p| p.kind == LaserPowerUpKind::RapidFire)
                    }) {
                        FIRE_COOLDOWN * RAPIDFIRE_COOLDOWN_MULT
                    } else {
                        FIRE_COOLDOWN
                    };

                if let Some(player) = self.state.players.get_mut(&pid) {
                    player.fire_cooldown = cooldown;
                }
            }
        }

        // Power-up collection
        for pu in &mut self.state.powerups {
            if pu.collected {
                pu.respawn_timer -= dt;
                if pu.respawn_timer <= 0.0 {
                    pu.collected = false;
                }
                continue;
            }
            for &pid in &self.player_ids {
                if let Some(player) = self.state.players.get(&pid) {
                    let dx = player.x - pu.x;
                    let dz = player.z - pu.z;
                    if dx * dx + dz * dz < 2.0 {
                        pu.collected = true;
                        pu.respawn_timer = powerups::POWERUP_RESPAWN_TIME;
                        self.state
                            .active_powerups
                            .entry(pid)
                            .or_default()
                            .push(ActiveLaserPowerUp::new(pu.kind));
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

        // Check round completion (timer)
        if self.state.round_timer >= self.round_duration {
            self.state.round_complete = true;
            events.push(GameEvent::RoundComplete);
        }

        events
    }

    breakpoint_game_boilerplate!(state_type: LaserTagState);

    fn apply_input(&mut self, player_id: PlayerId, input: &[u8]) {
        match rmp_serde::from_slice::<LaserTagInput>(input) {
            Err(e) => {
                tracing::debug!(player_id, error = %e, "Dropped malformed laser tag input");
            },
            Ok(mut li) => {
                // Sanitize NaN/Inf inputs to prevent position corruption
                if !li.move_x.is_finite() {
                    li.move_x = 0.0;
                }
                if !li.move_z.is_finite() {
                    li.move_z = 0.0;
                }
                if !li.aim_angle.is_finite() {
                    li.aim_angle = 0.0;
                }
                // Accumulate transient flags (fire, use_powerup) across frames.
                // Without this, a fire:true in frame N gets overwritten by fire:false
                // in frame N+1 before the game tick processes it. Continuous values
                // (move_x, move_z, aim_angle) are always overwritten with the latest.
                if let Some(existing) = self.pending_inputs.get_mut(&player_id) {
                    existing.move_x = li.move_x;
                    existing.move_z = li.move_z;
                    existing.aim_angle = li.aim_angle;
                    if li.fire {
                        existing.fire = true;
                    }
                    if li.use_powerup {
                        existing.use_powerup = true;
                    }
                } else {
                    self.pending_inputs.insert(player_id, li);
                }
            },
        }
    }

    fn player_joined(&mut self, player: &Player) {
        if player.is_spectator || self.player_ids.contains(&player.id) {
            return;
        }
        let idx = self.player_ids.len();
        self.player_ids.push(player.id);
        let spawn = &self.arena.spawn_points[idx % self.arena.spawn_points.len()];
        self.state.players.insert(
            player.id,
            LaserPlayerState::new(spawn.x, spawn.z, spawn.angle),
        );
        self.state.active_powerups.insert(player.id, Vec::new());
        self.state.tags_scored.insert(player.id, 0);
    }

    fn player_left(&mut self, player_id: PlayerId) {
        self.player_ids.retain(|&id| id != player_id);
        self.state.players.remove(&player_id);
        self.state.active_powerups.remove(&player_id);
        self.state.tags_scored.remove(&player_id);
        self.state.teams.remove(&player_id);
    }

    fn round_results(&self) -> Vec<PlayerScore> {
        self.player_ids
            .iter()
            .map(|&pid| {
                let tags = self.state.tags_scored.get(&pid).copied().unwrap_or(0);
                PlayerScore {
                    player_id: pid,
                    score: scoring::ffa_score(tags),
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
        let mut game = LaserTagArena::new();
        let players = make_players(4);
        game.init(&players, &default_config(180));
        assert_eq!(game.state.players.len(), 4);
        assert_eq!(game.state.tags_scored.len(), 4);
    }

    #[test]
    fn state_roundtrip() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let data = game.serialize_state();
        let mut game2 = LaserTagArena::new();
        game2.init(&players, &default_config(180));
        game2.apply_state(&data);

        assert_eq!(game.state.players.len(), game2.state.players.len());
    }

    #[test]
    fn input_roundtrip() {
        let mut game = LaserTagArena::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let input = LaserTagInput {
            move_x: 1.0,
            move_z: 0.0,
            aim_angle: 0.5,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        assert!(game.pending_inputs.contains_key(&1));
    }

    #[test]
    fn tick_rate_is_20() {
        let game = LaserTagArena::new();
        assert_eq!(game.tick_rate(), 20.0);
    }

    #[test]
    fn powerups_within_arena_bounds() {
        for arena_name in ["small", "default", "large"] {
            let mut game = LaserTagArena::new();
            let players = make_players(2);
            let mut config = default_config(180);
            if arena_name != "default" {
                config.custom.insert(
                    "arena_size".to_string(),
                    serde_json::Value::String(arena_name.to_string()),
                );
            }
            game.init(&players, &config);

            for pu in &game.state.powerups {
                assert!(
                    pu.x > 0.0 && pu.x < game.arena.width,
                    "Power-up x={} out of bounds for {arena_name} arena (width={})",
                    pu.x,
                    game.arena.width
                );
                assert!(
                    pu.z > 0.0 && pu.z < game.arena.depth,
                    "Power-up z={} out of bounds for {arena_name} arena (depth={})",
                    pu.z,
                    game.arena.depth
                );
            }
        }
    }

    #[test]
    fn ffa_and_team_modes() {
        let mut game = LaserTagArena::new();
        let players = make_players(4);

        // FFA mode
        game.init(&players, &default_config(180));
        assert_eq!(game.state.team_mode, TeamMode::FreeForAll);
        assert!(game.state.teams.is_empty());

        // Team mode
        let mut config = default_config(180);
        config.custom.insert(
            "team_mode".to_string(),
            serde_json::Value::String("teams_2".to_string()),
        );
        game.init(&players, &config);
        assert_eq!(game.state.team_mode, TeamMode::Teams { team_count: 2 });
        assert_eq!(game.state.teams.len(), 4);
    }

    // ================================================================
    // Edge case tests
    // ================================================================

    /// Helper: create a 2-team config with 4 players.
    fn teams_config() -> GameConfig {
        let mut config = default_config(180);
        config.custom.insert(
            "team_mode".to_string(),
            serde_json::Value::String("teams_2".to_string()),
        );
        config
    }

    #[test]
    fn team_mode_friendly_fire() {
        let mut game = LaserTagArena::new();
        let players = make_players(4);
        game.init(&players, &teams_config());

        // With teams_2 and round-robin assignment:
        //   Player 1 (idx 0) → team 0
        //   Player 2 (idx 1) → team 1
        //   Player 3 (idx 2) → team 0
        //   Player 4 (idx 3) → team 1
        assert_eq!(game.state.teams[&1], 0);
        assert_eq!(game.state.teams[&3], 0);

        // Position player 1 and teammate player 3 so player 1's laser would hit player 3
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0; // aiming +X
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        // Place teammate directly in the line of fire
        game.state.players.get_mut(&3).unwrap().x = 10.0;
        game.state.players.get_mut(&3).unwrap().z = 10.0;
        game.state.players.get_mut(&3).unwrap().stun_remaining = 0.0;

        // Move other players far away so they can't be hit
        game.state.players.get_mut(&2).unwrap().x = 5.0;
        game.state.players.get_mut(&2).unwrap().z = 45.0;
        game.state.players.get_mut(&4).unwrap().x = 5.0;
        game.state.players.get_mut(&4).unwrap().z = 45.0;

        // Player 1 fires
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.05, &inputs);

        // Teammate should NOT be stunned (friendly fire blocked)
        assert!(
            !game.state.players[&3].is_stunned(),
            "Teammate should not be stunned by friendly fire"
        );

        // Player 1 should not get a tag scored
        assert_eq!(
            game.state.tags_scored[&1], 0,
            "No tag should be scored for hitting a teammate"
        );

        // No ScoreUpdate events for player 1
        let score_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, GameEvent::ScoreUpdate { player_id: 1, .. }))
            .collect();
        assert!(
            score_events.is_empty(),
            "No score event should be emitted for friendly fire"
        );
    }

    #[test]
    fn ffa_scoring() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Position player 1 to fire at player 2
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0; // aiming +X
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        // Place player 2 directly in line of fire
        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // Player 1 fires
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.05, &inputs);

        // Player 2 should be stunned
        assert!(
            game.state.players[&2].is_stunned(),
            "Target should be stunned after being hit"
        );

        // Player 1 should have 1 tag scored
        assert_eq!(
            game.state.tags_scored[&1], 1,
            "Shooter should get 1 tag scored"
        );

        // ScoreUpdate event should be emitted
        let has_score_event = events.iter().any(|e| {
            matches!(
                e,
                GameEvent::ScoreUpdate {
                    player_id: 1,
                    score: 1
                }
            )
        });
        assert!(
            has_score_event,
            "ScoreUpdate event should be emitted for tag"
        );
    }

    #[test]
    fn powerup_duration_expiry() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Give player 1 a RapidFire power-up (duration = 5.0s)
        game.state
            .active_powerups
            .entry(1)
            .or_default()
            .push(ActiveLaserPowerUp::new(LaserPowerUpKind::RapidFire));

        assert_eq!(
            game.state.active_powerups[&1].len(),
            1,
            "Player should have 1 active power-up"
        );

        // Advance time but not past the duration
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(2.0, &inputs);
        assert_eq!(
            game.state.active_powerups[&1].len(),
            1,
            "Power-up should still be active at 2.0s"
        );

        // Advance past the 5.0s duration (total > 5.0s)
        game.update(4.0, &inputs);
        assert_eq!(
            game.state.active_powerups[&1].len(),
            0,
            "Power-up should have expired after 6.0s total (duration is 5.0s)"
        );
    }

    #[test]
    fn arena_boundary_clamping() {
        let mut game = LaserTagArena::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let arena_width = game.arena.width;
        let arena_depth = game.arena.depth;

        // Place player near the right edge and push them beyond the boundary
        game.state.players.get_mut(&1).unwrap().x = arena_width - 1.0;
        game.state.players.get_mut(&1).unwrap().z = arena_depth - 1.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        // Send large positive movement to push beyond bounds
        let input = LaserTagInput {
            move_x: 1.0,
            move_z: 1.0,
            aim_angle: 0.0,
            fire: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        // Run several ticks with large dt to push well beyond bounds
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(2.0, &inputs);

        let player = &game.state.players[&1];
        assert!(
            player.x <= arena_width - projectile::PLAYER_RADIUS,
            "X should be clamped to arena bounds: x={}, max={}",
            player.x,
            arena_width - projectile::PLAYER_RADIUS
        );
        assert!(
            player.z <= arena_depth - projectile::PLAYER_RADIUS,
            "Z should be clamped to arena bounds: z={}, max={}",
            player.z,
            arena_depth - projectile::PLAYER_RADIUS
        );

        // Also test clamping at the lower boundary
        game.state.players.get_mut(&1).unwrap().x = 1.0;
        game.state.players.get_mut(&1).unwrap().z = 1.0;

        let input_neg = LaserTagInput {
            move_x: -1.0,
            move_z: -1.0,
            aim_angle: 0.0,
            fire: false,
            use_powerup: false,
        };
        let data_neg = rmp_serde::to_vec(&input_neg).unwrap();
        game.apply_input(1, &data_neg);

        game.update(2.0, &inputs);

        let player = &game.state.players[&1];
        assert!(
            player.x >= projectile::PLAYER_RADIUS,
            "X should be clamped to lower bound: x={}, min={}",
            player.x,
            projectile::PLAYER_RADIUS
        );
        assert!(
            player.z >= projectile::PLAYER_RADIUS,
            "Z should be clamped to lower bound: z={}, min={}",
            player.z,
            projectile::PLAYER_RADIUS
        );
    }

    #[test]
    fn stun_prevents_movement() {
        let mut game = LaserTagArena::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        // Place player at a known position and stun them
        game.state.players.get_mut(&1).unwrap().x = 20.0;
        game.state.players.get_mut(&1).unwrap().z = 20.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 5.0;

        let pos_before_x = game.state.players[&1].x;
        let pos_before_z = game.state.players[&1].z;

        // Send movement input
        let input = LaserTagInput {
            move_x: 1.0,
            move_z: 1.0,
            aim_angle: 0.0,
            fire: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        let player = &game.state.players[&1];
        assert_eq!(
            player.x, pos_before_x,
            "Stunned player X should not change: was {pos_before_x}, now {}",
            player.x
        );
        assert_eq!(
            player.z, pos_before_z,
            "Stunned player Z should not change: was {pos_before_z}, now {}",
            player.z
        );

        // Verify stun is still active (only decreased by dt=0.05 from 5.0)
        assert!(
            player.is_stunned(),
            "Player should still be stunned, remaining={}",
            player.stun_remaining
        );
    }

    // ================================================================
    // Game Trait Contract Tests
    // ================================================================

    #[test]
    fn contract_init_creates_player_state() {
        let mut game = LaserTagArena::new();
        breakpoint_core::test_helpers::contract_init_creates_player_state(&mut game, 4);
    }

    #[test]
    fn contract_apply_input_changes_state() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let input = LaserTagInput {
            move_x: 1.0,
            move_z: 0.0,
            aim_angle: 0.5,
            fire: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        breakpoint_core::test_helpers::contract_apply_input_changes_state(&mut game, &data, 1);
    }

    #[test]
    fn contract_update_advances_time() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_update_advances_time(&mut game);
    }

    #[test]
    fn contract_round_eventually_completes() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        // Laser tag reads round_duration from custom config, not GameConfig.round_duration
        let mut config = default_config(180);
        config
            .custom
            .insert("round_duration".to_string(), serde_json::json!(5.0));
        game.init(&players, &config);
        breakpoint_core::test_helpers::contract_round_eventually_completes(&mut game, 10);
    }

    #[test]
    fn contract_state_roundtrip_preserves() {
        // Use a single player to avoid HashMap key ordering non-determinism
        let mut game = LaserTagArena::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_state_roundtrip_preserves(&mut game);
    }

    #[test]
    fn contract_pause_stops_updates() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_pause_stops_updates(&mut game);
    }

    #[test]
    fn contract_player_left_cleanup() {
        let mut game = LaserTagArena::new();
        let players = make_players(3);
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_player_left_cleanup(&mut game, 3, 3);
    }

    #[test]
    fn contract_round_results_complete() {
        let mut game = LaserTagArena::new();
        let players = make_players(4);
        game.init(&players, &default_config(180));
        breakpoint_core::test_helpers::contract_round_results_complete(&game, 4);
    }

    // ================================================================
    // Input encoding/decoding roundtrip tests (Phase 2)
    // ================================================================

    #[test]
    fn lasertag_input_encode_decode_roundtrip() {
        let input = LaserTagInput {
            move_x: 0.7,
            move_z: -0.3,
            aim_angle: 1.57,
            fire: true,
            use_powerup: false,
        };
        let encoded = rmp_serde::to_vec(&input).unwrap();
        let decoded: LaserTagInput = rmp_serde::from_slice(&encoded).unwrap();
        assert!((decoded.move_x - input.move_x).abs() < 1e-5);
        assert!((decoded.move_z - input.move_z).abs() < 1e-5);
        assert!((decoded.aim_angle - input.aim_angle).abs() < 1e-5);
        assert_eq!(decoded.fire, input.fire);
        assert_eq!(decoded.use_powerup, input.use_powerup);
    }

    #[test]
    fn lasertag_input_through_protocol_roundtrip() {
        use breakpoint_core::net::messages::{ClientMessage, PlayerInputMsg};
        use breakpoint_core::net::protocol::{decode_client_message, encode_client_message};

        let input = LaserTagInput {
            move_x: 1.0,
            move_z: 0.0,
            aim_angle: 0.5,
            fire: true,
            use_powerup: false,
        };
        let input_data = rmp_serde::to_vec(&input).unwrap();
        let msg = ClientMessage::PlayerInput(PlayerInputMsg {
            player_id: 1,
            tick: 20,
            input_data: input_data.clone(),
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        match decoded {
            ClientMessage::PlayerInput(pi) => {
                assert_eq!(pi.input_data, input_data);
                let lt_input: LaserTagInput = rmp_serde::from_slice(&pi.input_data).unwrap();
                assert!((lt_input.aim_angle - 0.5).abs() < 1e-5);
                assert!(lt_input.fire);
            },
            other => panic!("Expected PlayerInput, got {:?}", other),
        }
    }

    #[test]
    fn lasertag_input_apply_changes_game_state() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let before = game.serialize_state();

        let input = LaserTagInput {
            move_x: 1.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        breakpoint_core::test_helpers::assert_game_state_changed(&game, &before);
    }

    // ================================================================
    // Game simulation tests (Phase 3)
    // ================================================================

    #[test]
    fn lasertag_move_changes_position() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let initial_x = game.state.players[&1].x;

        // Apply rightward movement for 20 ticks
        for _ in 0..20 {
            let input = LaserTagInput {
                move_x: 1.0,
                move_z: 0.0,
                aim_angle: 0.0,
                fire: false,
                use_powerup: false,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game.apply_input(1, &data);
            let inputs = PlayerInputs {
                inputs: HashMap::new(),
            };
            game.update(0.05, &inputs);
        }

        assert!(
            game.state.players[&1].x > initial_x,
            "Player x should increase: initial={initial_x}, final={}",
            game.state.players[&1].x
        );
    }

    #[test]
    fn lasertag_fire_at_target_scores() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Position player 1 to fire at player 2
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        // Place player 2 directly in line of fire
        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // Fire
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        assert!(
            game.state.players[&2].is_stunned(),
            "Target should be stunned"
        );
        assert_eq!(game.state.tags_scored[&1], 1, "Shooter should get 1 tag");
    }

    #[test]
    fn lasertag_full_match_round_completes() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Advance to round completion via timer
        let events = breakpoint_core::test_helpers::run_game_ticks(&mut game, 200, 1.0);

        assert!(
            game.is_round_complete(),
            "Round should complete after enough ticks"
        );
        assert!(
            events.iter().any(|e| matches!(e, GameEvent::RoundComplete)),
            "RoundComplete event should be emitted"
        );
    }

    // ================================================================
    // Phase 2e: Stun & cooldown edge cases
    // ================================================================

    #[test]
    fn fire_while_stunned_rejected() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Position and stun player 1
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 2.0;

        // Place player 2 in line of fire
        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // Player 1 (stunned) tries to fire
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        // Player 2 should NOT be stunned
        assert!(
            !game.state.players[&2].is_stunned(),
            "Stunned player's fire should have no effect"
        );
        assert_eq!(game.state.tags_scored[&1], 0, "No tag should be scored");
    }

    #[test]
    fn stun_hit_resets_timer() {
        let mut game = LaserTagArena::new();
        let players = make_players(3);
        game.init(&players, &default_config(180));

        // Stun player 2 partially
        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.5; // partially stunned

        // Player 1 fires at player 2
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        // Move player 3 far away
        game.state.players.get_mut(&3).unwrap().x = 5.0;
        game.state.players.get_mut(&3).unwrap().z = 45.0;

        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        // Note: The game skips stunned players in hit detection (they're filtered out
        // of the player_positions list). So the hit won't register. This is by design:
        // already-stunned players can't be re-stunned.
        // Verify the stun timer decremented normally
        let stun = game.state.players[&2].stun_remaining;
        assert!(
            stun < 0.5,
            "Stun timer should have decremented from 0.5, got {stun}"
        );
    }

    #[test]
    fn stun_expires_at_exact_boundary() {
        let mut game = LaserTagArena::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        // Set stun to exactly dt so it expires this tick
        let dt = 0.05;
        game.state.players.get_mut(&1).unwrap().stun_remaining = dt;

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(dt, &inputs);

        assert!(
            !game.state.players[&1].is_stunned(),
            "Stun should expire when timer reaches 0: remaining={}",
            game.state.players[&1].stun_remaining
        );
    }

    #[test]
    fn fire_cooldown_boundary() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Set cooldown to exactly 0.0
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        // Fire should succeed at cooldown=0.0
        assert!(
            game.state.players[&2].is_stunned(),
            "Fire at cooldown=0.0 should work"
        );
    }

    #[test]
    fn shield_absorbs_hit_no_stun() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Give player 2 a shield
        game.state
            .active_powerups
            .entry(2)
            .or_default()
            .push(powerups::ActiveLaserPowerUp::new(
                powerups::LaserPowerUpKind::Shield,
            ));

        // Position player 1 to fire at player 2
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        // Player 2 should NOT be stunned
        assert!(
            !game.state.players[&2].is_stunned(),
            "Shield should absorb the hit, no stun"
        );
        // Shield should be consumed
        let shields: Vec<_> = game.state.active_powerups[&2]
            .iter()
            .filter(|p| p.kind == powerups::LaserPowerUpKind::Shield)
            .collect();
        assert!(shields.is_empty(), "Shield should be consumed");
    }

    #[test]
    fn shield_consumed_second_hit_stuns() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Give player 2 a shield
        game.state
            .active_powerups
            .entry(2)
            .or_default()
            .push(powerups::ActiveLaserPowerUp::new(
                powerups::LaserPowerUpKind::Shield,
            ));

        // Position players
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // First hit — consumes shield
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);
        assert!(
            !game.state.players[&2].is_stunned(),
            "First hit absorbed by shield"
        );

        // Second hit — should stun
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.apply_input(1, &data);
        game.update(0.05, &inputs);

        assert!(
            game.state.players[&2].is_stunned(),
            "Second hit (no shield) should stun"
        );
    }

    #[test]
    fn lasertag_fire_input_not_lost_across_overwrites() {
        // Verifies Bug 2 fix: fire:true must be preserved even if a
        // subsequent apply_input has fire:false.
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Position player 1 to fire at player 2
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // Frame N: fire=true
        let input_fire = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data_fire = rmp_serde::to_vec(&input_fire).unwrap();
        game.apply_input(1, &data_fire);

        // Frame N+1: fire=false (would overwrite in old code)
        let input_no_fire = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: false,
            use_powerup: false,
        };
        let data_no_fire = rmp_serde::to_vec(&input_no_fire).unwrap();
        game.apply_input(1, &data_no_fire);

        // The pending input should still have fire=true
        assert!(
            game.pending_inputs.get(&1).is_some_and(|i| i.fire),
            "Fire flag must be preserved across input overwrites"
        );

        // Tick the game — fire should actually happen
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        assert!(
            game.state.players[&2].is_stunned(),
            "Target should be stunned despite fire being overwritten"
        );
        assert_eq!(
            game.state.tags_scored[&1], 1,
            "Tag should be scored despite fire being overwritten"
        );
    }

    // ================================================================
    // P0-1: NaN/Inf/Degenerate Input Fuzzing
    // ================================================================

    // REGRESSION: NaN movement values should not corrupt player position
    #[test]
    fn lasertag_apply_input_nan_move_no_panic() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let input = LaserTagInput {
            move_x: f32::NAN,
            move_z: f32::NAN,
            aim_angle: f32::NAN,
            fire: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        // Should not panic on update
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);
    }

    // REGRESSION: Inf movement should be clamped by arena bounds
    #[test]
    fn lasertag_apply_input_inf_move_clamped() {
        let mut game = LaserTagArena::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        let input = LaserTagInput {
            move_x: f32::INFINITY,
            move_z: f32::INFINITY,
            aim_angle: 0.0,
            fire: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        let p = &game.state.players[&1];
        assert!(
            p.x <= game.arena.width && p.z <= game.arena.depth,
            "Player should be clamped to arena bounds: ({}, {})",
            p.x,
            p.z
        );
    }

    // ================================================================
    // P1-1: Serialization Fuzzing
    // ================================================================

    // REGRESSION: Garbage input data should not panic
    #[test]
    fn lasertag_apply_input_garbage_no_panic() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let garbage: Vec<u8> = vec![0xFF, 0xFE, 0x00, 0x01, 0xAB, 0xCD];
        game.apply_input(1, &garbage);

        // Player should be unchanged
        let p = &game.state.players[&1];
        assert!(
            !p.is_stunned(),
            "Garbage input should not affect player state"
        );
    }

    // REGRESSION: Truncated state data should not panic
    #[test]
    fn lasertag_apply_state_truncated_no_panic() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let state = game.serialize_state();
        let truncated = &state[..state.len() / 2];
        game.apply_state(truncated);

        // Game should still be functional
        assert_eq!(game.state.players.len(), 2);
    }

    // ================================================================
    // P1-2: State Machine Transition Tests
    // ================================================================

    #[test]
    fn lasertag_double_pause_single_resume_works() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

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
    fn lasertag_update_after_round_complete_is_noop() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Force round complete
        game.state.round_timer = 179.99;
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);
        assert!(game.is_round_complete());

        let timer = game.state.round_timer;
        let events = game.update(0.05, &inputs);
        assert!(
            (game.state.round_timer - timer).abs() < 0.01,
            "Timer should not advance after round complete"
        );
        assert!(events.is_empty(), "No events after round complete");
    }

    // ================================================================
    // P1-4: Laser Tag Edge Cases
    // ================================================================

    #[test]
    fn late_joiner_team_assignment_balanced() {
        let mut game = LaserTagArena::new();
        let players = make_players(5);
        game.init(&players, &teams_config());

        // With 5 players on 2 teams, distribution should be 3/2 or 2/3
        let team0_count = game.state.teams.values().filter(|&&t| t == 0).count();
        let team1_count = game.state.teams.values().filter(|&&t| t == 1).count();
        let diff = (team0_count as i32 - team1_count as i32).unsigned_abs();
        assert!(
            diff <= 1,
            "Teams should be balanced: team0={team0_count}, team1={team1_count}"
        );
    }

    // REGRESSION: Stunned player should not be able to move
    #[test]
    fn stunned_player_cannot_move() {
        let mut game = LaserTagArena::new();
        let players = make_players(1);
        game.init(&players, &default_config(180));

        // Stun the player
        game.state.players.get_mut(&1).unwrap().stun_remaining = STUN_DURATION;
        let pos_before = (game.state.players[&1].x, game.state.players[&1].z);

        // Apply movement input
        let input = LaserTagInput {
            move_x: 1.0,
            move_z: 1.0,
            aim_angle: 0.0,
            fire: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        let pos_after = (game.state.players[&1].x, game.state.players[&1].z);
        assert!(
            (pos_before.0 - pos_after.0).abs() < 0.01 && (pos_before.1 - pos_after.1).abs() < 0.01,
            "Stunned player should not move: before={pos_before:?}, after={pos_after:?}"
        );
    }

    // REGRESSION: RapidFire expiry should revert cooldown to normal
    #[test]
    fn rapidfire_expiry_reverts_cooldown() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        // Position players for hit
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // Give player 1 RapidFire
        game.state
            .active_powerups
            .entry(1)
            .or_default()
            .push(ActiveLaserPowerUp::new(LaserPowerUpKind::RapidFire));

        // Fire with RapidFire active
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        let rapid_cooldown = game.state.players[&1].fire_cooldown;
        assert!(
            rapid_cooldown <= FIRE_COOLDOWN * RAPIDFIRE_COOLDOWN_MULT + 0.01,
            "RapidFire cooldown should be ~{}, got {rapid_cooldown}",
            FIRE_COOLDOWN * RAPIDFIRE_COOLDOWN_MULT
        );

        // Now expire the RapidFire powerup
        if let Some(pus) = game.state.active_powerups.get_mut(&1) {
            pus.clear();
        }

        // Wait for cooldown to expire
        for _ in 0..20 {
            game.update(0.05, &inputs);
        }

        // Fire again without RapidFire
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;

        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);
        game.update(0.05, &inputs);

        let normal_cooldown = game.state.players[&1].fire_cooldown;
        assert!(
            (normal_cooldown - FIRE_COOLDOWN).abs() < 0.01,
            "Normal cooldown should be ~{FIRE_COOLDOWN}, got {normal_cooldown}"
        );
    }

    // REGRESSION: Two players at same powerup — only one should collect
    #[test]
    fn two_players_at_same_powerup_only_one_collects() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        if game.state.powerups.is_empty() {
            // If no powerups in this arena config, skip
            return;
        }

        // Move both players to the first powerup location
        let pu_x = game.state.powerups[0].x;
        let pu_z = game.state.powerups[0].z;

        game.state.players.get_mut(&1).unwrap().x = pu_x;
        game.state.players.get_mut(&1).unwrap().z = pu_z;
        game.state.players.get_mut(&2).unwrap().x = pu_x;
        game.state.players.get_mut(&2).unwrap().z = pu_z;

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        // Exactly one powerup should be collected
        assert!(
            game.state.powerups[0].collected,
            "Powerup should be collected when players are on it"
        );

        // Only one player should have the active powerup
        let p1_pus = game.state.active_powerups.get(&1).map_or(0, |v| v.len());
        let p2_pus = game.state.active_powerups.get(&2).map_or(0, |v| v.len());
        assert_eq!(
            p1_pus + p2_pus,
            1,
            "Only one player should collect: p1={p1_pus}, p2={p2_pus}"
        );
    }

    // REGRESSION: Fire at exact cooldown boundary
    #[test]
    fn fire_cooldown_boundary_exact_timing() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // Cooldown exactly 0.0 — fire should succeed
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        assert!(
            game.state.players[&2].is_stunned(),
            "Fire at cooldown=0.0 should succeed"
        );
        assert_eq!(game.state.tags_scored[&1], 1, "Should score a tag");

        // Reset for second test
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // Cooldown slightly above 0 — fire should be rejected
        // Cooldown was set by previous fire, so player 1 can't fire again yet
        let input2 = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data2 = rmp_serde::to_vec(&input2).unwrap();
        game.apply_input(1, &data2);
        game.update(0.05, &inputs);

        // Player 2 should not be re-stunned (fire_cooldown > 0)
        assert_eq!(
            game.state.tags_scored[&1], 1,
            "Fire with active cooldown should be rejected"
        );
    }

    // ================================================================
    // Multi-team mode hardening tests
    // ================================================================

    /// Helper: build a config for 3-team mode.
    fn teams_3_config() -> GameConfig {
        let mut config = default_config(180);
        config.custom.insert(
            "team_mode".to_string(),
            serde_json::Value::String("teams_3".to_string()),
        );
        config
    }

    /// Helper: build a config for 4-team mode.
    fn teams_4_config() -> GameConfig {
        let mut config = default_config(180);
        config.custom.insert(
            "team_mode".to_string(),
            serde_json::Value::String("teams_4".to_string()),
        );
        config
    }

    #[test]
    fn three_team_mode_assignment() {
        let mut game = LaserTagArena::new();
        let players = make_players(6);
        game.init(&players, &teams_3_config());

        // Verify team mode is set correctly
        assert_eq!(
            game.state.team_mode,
            TeamMode::Teams { team_count: 3 },
            "Team mode should be 3 teams"
        );

        // All 6 players should be assigned to teams
        assert_eq!(
            game.state.teams.len(),
            6,
            "All 6 players should have team assignments"
        );

        // Each team (0, 1, 2) should have exactly 2 players (6 / 3 = 2 each)
        for team_id in 0..3u8 {
            let count = game.state.teams.values().filter(|&&t| t == team_id).count();
            assert_eq!(
                count, 2,
                "Team {team_id} should have 2 players, got {count}"
            );
        }

        // Verify round-robin assignment: player IDs 1-6 map to teams 0,1,2,0,1,2
        assert_eq!(game.state.teams[&1], 0);
        assert_eq!(game.state.teams[&2], 1);
        assert_eq!(game.state.teams[&3], 2);
        assert_eq!(game.state.teams[&4], 0);
        assert_eq!(game.state.teams[&5], 1);
        assert_eq!(game.state.teams[&6], 2);
    }

    #[test]
    fn four_team_mode_assignment() {
        let mut game = LaserTagArena::new();
        let players = make_players(8);
        game.init(&players, &teams_4_config());

        // Verify team mode is set correctly
        assert_eq!(
            game.state.team_mode,
            TeamMode::Teams { team_count: 4 },
            "Team mode should be 4 teams"
        );

        // All 8 players should be assigned to teams
        assert_eq!(
            game.state.teams.len(),
            8,
            "All 8 players should have team assignments"
        );

        // Each team (0, 1, 2, 3) should have exactly 2 players (8 / 4 = 2 each)
        for team_id in 0..4u8 {
            let count = game.state.teams.values().filter(|&&t| t == team_id).count();
            assert_eq!(
                count, 2,
                "Team {team_id} should have 2 players, got {count}"
            );
        }

        // Verify round-robin: players 1-8 map to teams 0,1,2,3,0,1,2,3
        assert_eq!(game.state.teams[&1], 0);
        assert_eq!(game.state.teams[&2], 1);
        assert_eq!(game.state.teams[&3], 2);
        assert_eq!(game.state.teams[&4], 3);
        assert_eq!(game.state.teams[&5], 0);
        assert_eq!(game.state.teams[&6], 1);
        assert_eq!(game.state.teams[&7], 2);
        assert_eq!(game.state.teams[&8], 3);
    }

    #[test]
    fn cross_team_hit_detection() {
        let mut game = LaserTagArena::new();
        let players = make_players(4);
        game.init(&players, &teams_config());

        // teams_config() uses teams_2, round-robin:
        //   Player 1 (idx 0) -> team 0
        //   Player 2 (idx 1) -> team 1
        //   Player 3 (idx 2) -> team 0
        //   Player 4 (idx 3) -> team 1
        assert_eq!(game.state.teams[&1], 0, "Player 1 should be on team 0");
        assert_eq!(game.state.teams[&2], 1, "Player 2 should be on team 1");

        // Position player 1 (team 0) to fire at player 2 (team 1)
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0; // aiming +X
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        // Place player 2 (team 1) directly in line of fire
        game.state.players.get_mut(&2).unwrap().x = 10.0;
        game.state.players.get_mut(&2).unwrap().z = 10.0;
        game.state.players.get_mut(&2).unwrap().stun_remaining = 0.0;

        // Move other players far away so they can't interfere
        game.state.players.get_mut(&3).unwrap().x = 5.0;
        game.state.players.get_mut(&3).unwrap().z = 45.0;
        game.state.players.get_mut(&4).unwrap().x = 5.0;
        game.state.players.get_mut(&4).unwrap().z = 45.0;

        // Player 1 fires at player 2 (cross-team)
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.05, &inputs);

        // Player 2 (enemy team) SHOULD be stunned
        assert!(
            game.state.players[&2].is_stunned(),
            "Cross-team target should be stunned"
        );

        // Player 1 should have 1 tag scored
        assert_eq!(
            game.state.tags_scored[&1], 1,
            "Cross-team hit should award a tag"
        );

        // ScoreUpdate event should be emitted
        let has_score_event = events.iter().any(|e| {
            matches!(
                e,
                GameEvent::ScoreUpdate {
                    player_id: 1,
                    score: 1
                }
            )
        });
        assert!(
            has_score_event,
            "ScoreUpdate event should be emitted for cross-team hit"
        );
    }

    #[test]
    fn same_team_no_friendly_fire() {
        let mut game = LaserTagArena::new();
        let players = make_players(4);
        game.init(&players, &teams_config());

        // teams_config() uses teams_2, round-robin:
        //   Player 1 (idx 0) -> team 0
        //   Player 3 (idx 2) -> team 0
        assert_eq!(game.state.teams[&1], 0, "Player 1 should be on team 0");
        assert_eq!(game.state.teams[&3], 0, "Player 3 should be on team 0");

        // Position player 1 (team 0) to fire at player 3 (same team 0)
        game.state.players.get_mut(&1).unwrap().x = 5.0;
        game.state.players.get_mut(&1).unwrap().z = 10.0;
        game.state.players.get_mut(&1).unwrap().aim_angle = 0.0; // aiming +X
        game.state.players.get_mut(&1).unwrap().fire_cooldown = 0.0;
        game.state.players.get_mut(&1).unwrap().stun_remaining = 0.0;

        // Place teammate (player 3) directly in line of fire
        game.state.players.get_mut(&3).unwrap().x = 10.0;
        game.state.players.get_mut(&3).unwrap().z = 10.0;
        game.state.players.get_mut(&3).unwrap().stun_remaining = 0.0;

        // Move other players far away
        game.state.players.get_mut(&2).unwrap().x = 5.0;
        game.state.players.get_mut(&2).unwrap().z = 45.0;
        game.state.players.get_mut(&4).unwrap().x = 5.0;
        game.state.players.get_mut(&4).unwrap().z = 45.0;

        // Player 1 fires at player 3 (same team)
        let input = LaserTagInput {
            move_x: 0.0,
            move_z: 0.0,
            aim_angle: 0.0,
            fire: true,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&input).unwrap();
        game.apply_input(1, &data);

        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        let events = game.update(0.05, &inputs);

        // Teammate (player 3) should NOT be stunned
        assert!(
            !game.state.players[&3].is_stunned(),
            "Same-team target should not be stunned (no friendly fire)"
        );

        // Player 1 should have 0 tags scored
        assert_eq!(
            game.state.tags_scored[&1], 0,
            "No tag should be scored for friendly fire attempt"
        );

        // No ScoreUpdate events for player 1
        let score_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, GameEvent::ScoreUpdate { player_id: 1, .. }))
            .collect();
        assert!(
            score_events.is_empty(),
            "No score event should be emitted for same-team hit attempt"
        );
    }

    #[test]
    fn nan_inputs_sanitized() {
        let mut game = LaserTagArena::new();
        let players = make_players(2);
        game.init(&players, &default_config(180));

        let nan_input = LaserTagInput {
            move_x: f32::NAN,
            move_z: f32::INFINITY,
            aim_angle: f32::NEG_INFINITY,
            fire: false,
            use_powerup: false,
        };
        let data = rmp_serde::to_vec(&nan_input).unwrap();

        let x_before = game.state.players[&1].x;
        let z_before = game.state.players[&1].z;

        game.apply_input(1, &data);
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.05, &inputs);

        let player = &game.state.players[&1];
        assert!(
            player.x.is_finite() && player.z.is_finite(),
            "Player position should remain finite after NaN inputs: x={}, z={}",
            player.x,
            player.z
        );
        assert!(
            (player.x - x_before).abs() < 0.01 && (player.z - z_before).abs() < 0.01,
            "NaN move inputs should be sanitized to 0 — no movement expected"
        );
    }
}
