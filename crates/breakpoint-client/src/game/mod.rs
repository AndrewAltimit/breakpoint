pub mod golf_plugin;
pub mod lasertag_plugin;
pub mod platformer_plugin;

use std::collections::HashMap;

use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::net::messages::{GameEndMsg, GameStateMsg, PlayerScoreEntry, RoundEndMsg};
use breakpoint_core::net::protocol::{
    decode_client_message, decode_message_type, decode_server_message, encode_server_message,
};

use crate::app::AppState;
use crate::lobby::LobbyState;
use crate::net_client::WsClient;

use golf_plugin::GolfPlugin;
use lasertag_plugin::LaserTagPlugin;
use platformer_plugin::PlatformerPlugin;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameRegistry::default())
            .add_plugins((GolfPlugin, PlatformerPlugin, LaserTagPlugin))
            .add_systems(OnEnter(AppState::InGame), setup_game)
            .add_systems(
                Update,
                (
                    game_tick_system,
                    host_broadcast_system,
                    client_receive_system,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_game);
    }
}

/// Factory function type: creates a new game instance.
type GameFactory = fn() -> Box<dyn BreakpointGame>;

/// Registry mapping game IDs to factory functions.
#[derive(Resource, Default)]
pub struct GameRegistry {
    factories: HashMap<String, GameFactory>,
}

impl GameRegistry {
    pub fn register(&mut self, game_id: &str, factory: GameFactory) {
        self.factories.insert(game_id.to_string(), factory);
    }

    pub fn create(&self, game_id: &str) -> Option<Box<dyn BreakpointGame>> {
        self.factories.get(game_id).map(|f| f())
    }
}

/// The active game instance (game-agnostic).
#[derive(Resource)]
pub struct ActiveGame {
    pub game: Box<dyn BreakpointGame>,
    pub game_id: String,
    pub tick: u32,
    pub tick_accumulator: f32,
}

/// Network role for this client.
#[derive(Resource)]
pub struct NetworkRole {
    pub is_host: bool,
    pub local_player_id: PlayerId,
    pub is_spectator: bool,
}

/// Marker for game entities to clean up on exit.
#[derive(Component)]
pub struct GameEntity;

/// Round tracking for multi-round games.
#[derive(Resource)]
pub struct RoundTracker {
    pub current_round: u8,
    pub total_rounds: u8,
    pub cumulative_scores: HashMap<PlayerId, i32>,
}

impl RoundTracker {
    pub fn new(total_rounds: u8) -> Self {
        Self {
            current_round: 1,
            total_rounds,
            cumulative_scores: HashMap::new(),
        }
    }

    pub fn record_round(&mut self, scores: &[PlayerScore]) {
        for s in scores {
            *self.cumulative_scores.entry(s.player_id).or_insert(0) += s.score;
        }
    }

    pub fn is_final_round(&self) -> bool {
        self.current_round >= self.total_rounds
    }
}

fn setup_game(mut commands: Commands, lobby: Res<LobbyState>, registry: Res<GameRegistry>) {
    let game_id = lobby.selected_game.clone();
    let mut game = registry
        .create(&game_id)
        .unwrap_or_else(|| registry.create("mini-golf").unwrap());

    let config = GameConfig {
        round_count: 1,
        round_duration: std::time::Duration::from_secs(90),
        custom: HashMap::new(),
    };
    game.init(&lobby.players, &config);

    let is_host = lobby.is_host;
    let local_player_id = lobby.local_player_id.unwrap_or(0);

    commands.insert_resource(ActiveGame {
        game,
        game_id,
        tick: 0,
        tick_accumulator: 0.0,
    });
    commands.insert_resource(NetworkRole {
        is_host,
        local_player_id,
        is_spectator: lobby.is_spectator,
    });
    commands.insert_resource(RoundTracker::new(config.round_count));
}

fn game_tick_system(
    time: Res<Time>,
    mut active_game: ResMut<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut next_state: ResMut<NextState<AppState>>,
    mut round_tracker: ResMut<RoundTracker>,
    ws_client: NonSend<WsClient>,
) {
    if !network_role.is_host || network_role.is_spectator {
        return;
    }

    let tick_rate = active_game.game.tick_rate();
    let tick_interval = 1.0 / tick_rate;

    active_game.tick_accumulator += time.delta_secs();
    while active_game.tick_accumulator >= tick_interval {
        active_game.tick_accumulator -= tick_interval;
        active_game.tick += 1;
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        active_game.game.update(tick_interval, &inputs);
    }

    // Check round completion
    if active_game.game.is_round_complete() {
        let results = active_game.game.round_results();
        round_tracker.record_round(&results);

        let scores: Vec<PlayerScoreEntry> = results
            .iter()
            .map(|s| PlayerScoreEntry {
                player_id: s.player_id,
                score: s.score,
            })
            .collect();

        if round_tracker.is_final_round() {
            // Final round: send GameEnd, go to GameOver
            let final_scores: Vec<PlayerScoreEntry> = round_tracker
                .cumulative_scores
                .iter()
                .map(|(&pid, &score)| PlayerScoreEntry {
                    player_id: pid,
                    score,
                })
                .collect();
            let msg =
                breakpoint_core::net::messages::ServerMessage::GameEnd(GameEndMsg { final_scores });
            if let Ok(data) = encode_server_message(&msg) {
                let _ = ws_client.send(&data);
            }
            next_state.set(AppState::GameOver);
        } else {
            // More rounds: send RoundEnd, go to BetweenRounds
            let msg = breakpoint_core::net::messages::ServerMessage::RoundEnd(RoundEndMsg {
                round: round_tracker.current_round,
                scores,
            });
            if let Ok(data) = encode_server_message(&msg) {
                let _ = ws_client.send(&data);
            }
            next_state.set(AppState::BetweenRounds);
        }
    }
}

fn host_broadcast_system(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    ws_client: NonSend<WsClient>,
) {
    if !network_role.is_host || !active_game.is_changed() {
        return;
    }

    let state_data = active_game.game.serialize_state();
    let msg = breakpoint_core::net::messages::ServerMessage::GameState(GameStateMsg {
        tick: active_game.tick,
        state_data,
    });
    if let Ok(data) = encode_server_message(&msg) {
        let _ = ws_client.send(&data);
    }
}

fn client_receive_system(
    ws_client: NonSend<WsClient>,
    mut active_game: ResMut<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut next_state: ResMut<NextState<AppState>>,
    mut overlay_queue: ResMut<crate::overlay::OverlayEventQueue>,
    mut round_tracker: ResMut<RoundTracker>,
) {
    use breakpoint_core::net::messages::MessageType;

    let messages = ws_client.drain_messages();
    for data in messages {
        let msg_type = match decode_message_type(&data) {
            Ok(t) => t,
            Err(_) => continue,
        };

        match msg_type {
            // Host receives relayed PlayerInput as ClientMessage
            MessageType::PlayerInput if network_role.is_host => {
                if let Ok(breakpoint_core::net::messages::ClientMessage::PlayerInput(pi)) =
                    decode_client_message(&data)
                {
                    active_game.game.apply_input(pi.player_id, &pi.input_data);
                }
            },
            // Non-host receives GameState
            MessageType::GameState if !network_role.is_host => {
                if let Ok(breakpoint_core::net::messages::ServerMessage::GameState(gs)) =
                    decode_server_message(&data)
                {
                    active_game.game.apply_state(&gs.state_data);
                    active_game.tick = gs.tick;
                }
            },
            MessageType::RoundEnd => {
                if let Ok(breakpoint_core::net::messages::ServerMessage::RoundEnd(re)) =
                    decode_server_message(&data)
                {
                    let scores: Vec<PlayerScore> = re
                        .scores
                        .iter()
                        .map(|s| PlayerScore {
                            player_id: s.player_id,
                            score: s.score,
                        })
                        .collect();
                    round_tracker.record_round(&scores);
                    next_state.set(AppState::BetweenRounds);
                }
            },
            MessageType::GameEnd => {
                if let Ok(breakpoint_core::net::messages::ServerMessage::GameEnd(ge)) =
                    decode_server_message(&data)
                {
                    let scores: Vec<PlayerScore> = ge
                        .final_scores
                        .iter()
                        .map(|s| PlayerScore {
                            player_id: s.player_id,
                            score: s.score,
                        })
                        .collect();
                    round_tracker.record_round(&scores);
                    next_state.set(AppState::GameOver);
                }
            },
            // Forward alert messages to the overlay
            MessageType::AlertEvent => {
                if let Ok(breakpoint_core::net::messages::ServerMessage::AlertEvent(ae)) =
                    decode_server_message(&data)
                {
                    overlay_queue.push(crate::overlay::OverlayNetEvent::AlertReceived(Box::new(
                        ae.event,
                    )));
                }
            },
            MessageType::AlertClaimed => {
                if let Ok(breakpoint_core::net::messages::ServerMessage::AlertClaimed(ac)) =
                    decode_server_message(&data)
                {
                    overlay_queue.push(crate::overlay::OverlayNetEvent::AlertClaimed {
                        event_id: ac.event_id,
                        claimed_by: ac.claimed_by.to_string(),
                    });
                }
            },
            MessageType::AlertDismissed => {
                if let Ok(breakpoint_core::net::messages::ServerMessage::AlertDismissed(ad)) =
                    decode_server_message(&data)
                {
                    overlay_queue.push(crate::overlay::OverlayNetEvent::AlertDismissed {
                        event_id: ad.event_id,
                    });
                }
            },
            _ => {},
        }
    }
}

fn cleanup_game(mut commands: Commands, query: Query<Entity, With<GameEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<ActiveGame>();
    commands.remove_resource::<NetworkRole>();
    commands.remove_resource::<RoundTracker>();
}
