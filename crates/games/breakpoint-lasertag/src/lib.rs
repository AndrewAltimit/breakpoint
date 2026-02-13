pub mod arena;
pub mod powerups;
pub mod projectile;
pub mod scoring;

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use breakpoint_core::breakpoint_game_boilerplate;
use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameEvent, GameMetadata, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::player::Player;

use arena::{Arena, ArenaSize, generate_arena};
use powerups::{ActiveLaserPowerUp, LaserPowerUpKind, SpawnedLaserPowerUp};
use projectile::{FIRE_COOLDOWN, PLAYER_RADIUS, STUN_DURATION, raycast_laser};

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
}

impl LaserTagArena {
    pub fn new() -> Self {
        Self {
            arena: generate_arena(ArenaSize::Default),
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
            },
            player_ids: Vec::new(),
            pending_inputs: HashMap::new(),
            paused: false,
            round_duration: 180.0,
        }
    }

    pub fn state(&self) -> &LaserTagState {
        &self.state
    }

    pub fn arena(&self) -> &Arena {
        &self.arena
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
        Self::new()
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

        self.arena = generate_arena(arena_size);
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

        // Process player movement and firing
        let player_ids = self.player_ids.clone();
        for &pid in &player_ids {
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

                // Build player list for hit detection
                let player_positions: Vec<(u64, f32, f32)> = self
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
                        FIRE_COOLDOWN * 0.4
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
        if let Ok(li) = rmp_serde::from_slice::<LaserTagInput>(input) {
            self.pending_inputs.insert(player_id, li);
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
}
