#[cfg(feature = "golf")]
pub mod golf_plugin;
#[cfg(feature = "lasertag")]
pub mod lasertag_plugin;
#[cfg(feature = "platformer")]
pub mod platformer_plugin;

use std::collections::HashMap;

use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameId, PlayerId, PlayerInputs, PlayerScore,
};
use breakpoint_core::net::messages::{
    GameEndMsg, GameStateMsg, PlayerInputMsg, PlayerScoreEntry, RoundEndMsg,
};
use breakpoint_core::net::protocol::{
    decode_client_message, decode_message_type, decode_server_message, encode_client_message,
    encode_server_message,
};

use breakpoint_core::player::PlayerColor;

use crate::app::AppState;
use crate::lobby::LobbyState;
use crate::net_client::WsClient;

/// Convert a PlayerColor (u8 RGB) to a Bevy Color.
pub fn player_color_to_bevy(color: &PlayerColor) -> Color {
    Color::srgb(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
    )
}

/// Serialize and route player input: host applies directly, non-host sends via WebSocket.
pub fn send_player_input(
    input: &impl serde::Serialize,
    active_game: &mut ActiveGame,
    network_role: &NetworkRole,
    ws_client: &WsClient,
) {
    if let Ok(data) = rmp_serde::to_vec(input) {
        if network_role.is_host {
            active_game
                .game
                .apply_input(network_role.local_player_id, &data);
        } else {
            let msg = breakpoint_core::net::messages::ClientMessage::PlayerInput(PlayerInputMsg {
                player_id: network_role.local_player_id,
                tick: active_game.tick,
                input_data: data,
            });
            if let Ok(encoded) = encode_client_message(&msg) {
                let _ = ws_client.send(&encoded);
            }
        }
    }
}

/// Deserialize the current game state from the active game.
pub fn read_game_state<S: serde::de::DeserializeOwned>(active_game: &ActiveGame) -> Option<S> {
    rmp_serde::from_slice(&active_game.game.serialize_state()).ok()
}

/// HUD text anchor position.
pub enum HudPosition {
    TopLeft,
    TopRight,
    TopCenter,
}

/// Spawn an absolutely-positioned HUD text element with a marker component.
pub fn spawn_hud_text(
    commands: &mut Commands,
    marker: impl Bundle,
    text: impl Into<String>,
    font_size: f32,
    color: Color,
    position: HudPosition,
) -> Entity {
    let node = match position {
        HudPosition::TopLeft => Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        HudPosition::TopRight => Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(10.0),
            ..default()
        },
        HudPosition::TopCenter => Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Percent(50.0),
            ..default()
        },
    };

    commands
        .spawn((
            GameEntity,
            marker,
            Text::new(text),
            TextFont {
                font_size,
                ..default()
            },
            TextColor(color),
            node,
        ))
        .id()
}

#[cfg(feature = "golf")]
use golf_plugin::GolfPlugin;
#[cfg(feature = "lasertag")]
use lasertag_plugin::LaserTagPlugin;
#[cfg(feature = "platformer")]
use platformer_plugin::PlatformerPlugin;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameRegistry::default());

        #[cfg(feature = "golf")]
        app.add_plugins(GolfPlugin);
        #[cfg(feature = "platformer")]
        app.add_plugins(PlatformerPlugin);
        #[cfg(feature = "lasertag")]
        app.add_plugins(LaserTagPlugin);

        app.add_systems(OnEnter(AppState::InGame), setup_game)
            .add_systems(
                Update,
                (
                    game_tick_system,
                    host_broadcast_system,
                    client_receive_system,
                    controls_hint_spawn_log,
                    controls_hint_dismiss_system,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_game_entities)
            .add_systems(OnEnter(AppState::Lobby), full_cleanup);
    }
}

/// Factory function type: creates a new game instance.
type GameFactory = fn() -> Box<dyn BreakpointGame>;

/// Registry mapping game IDs to factory functions.
#[derive(Resource, Default)]
pub struct GameRegistry {
    factories: HashMap<GameId, GameFactory>,
}

impl GameRegistry {
    pub fn register(&mut self, game_id: GameId, factory: GameFactory) {
        self.factories.insert(game_id, factory);
    }

    pub fn create(&self, game_id: GameId) -> Option<Box<dyn BreakpointGame>> {
        self.factories.get(&game_id).map(|f| f())
    }
}

/// The active game instance (game-agnostic).
#[derive(Resource)]
pub struct ActiveGame {
    pub game: Box<dyn BreakpointGame>,
    pub game_id: GameId,
    pub tick: u32,
    pub tick_accumulator: f32,
    /// Previous serialized state (for client-side interpolation).
    /// Game plugins can use this to lerp positions between updates.
    pub prev_state: Option<Vec<u8>>,
    /// Fraction [0..1] of progress toward next tick (for smooth rendering).
    pub interp_alpha: f32,
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

/// Controls hint that auto-dismisses after a timeout.
#[derive(Component)]
pub struct ControlsHint {
    pub timer: f32,
}

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

fn setup_game(
    mut commands: Commands,
    lobby: Res<LobbyState>,
    registry: Res<GameRegistry>,
    existing_game: Option<Res<ActiveGame>>,
) {
    // If ActiveGame already exists (re-entry from BetweenRounds), skip creation.
    // The host already re-initialized the game in between_rounds_host_transition.
    if existing_game.is_some() {
        return;
    }

    // Validate we have enough state to start — spectators joining mid-game
    // may not have full lobby data yet.
    if lobby.players.is_empty() && lobby.is_spectator {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::warn_1(
            &"Spectator join: no player data yet, deferring game setup".into(),
        );
        return;
    }

    let game_id = lobby.selected_game;
    let mut game = match registry.create(game_id) {
        Some(g) => g,
        None => {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(
                &format!("Unknown game ID '{game_id}', falling back to mini-golf").into(),
            );
            match registry.create(GameId::Golf) {
                Some(g) => g,
                None => return,
            }
        },
    };

    let round_count = game.round_count_hint();

    let config = GameConfig {
        round_count,
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
        prev_state: None,
        interp_alpha: 0.0,
    });
    commands.insert_resource(NetworkRole {
        is_host,
        local_player_id,
        is_spectator: lobby.is_spectator,
    });
    commands.insert_resource(RoundTracker::new(round_count));
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
    active_game.interp_alpha = active_game.tick_accumulator / tick_interval;

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
                    // Store previous state for interpolation
                    active_game.prev_state = Some(active_game.game.serialize_state());
                    active_game.game.apply_state(&gs.state_data);
                    active_game.tick = gs.tick;
                    active_game.interp_alpha = 0.0;
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

fn controls_hint_spawn_log(query: Query<&ControlsHint, Added<ControlsHint>>) {
    if !query.is_empty() {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(
            "BREAKPOINT:CONTROLS_HINT:SPAWNED",
        ));
    }
}

fn controls_hint_dismiss_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut ControlsHint)>,
) {
    for (entity, mut hint) in &mut query {
        hint.timer -= time.delta_secs();
        if hint.timer <= 0.0 {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(
                "BREAKPOINT:CONTROLS_HINT:DISMISSED",
            ));
            commands.entity(entity).despawn();
        }
    }
}

/// Project a screen-space cursor position onto the Y=0 ground plane using
/// the camera's current Transform (no dependency on Camera.computed).
///
/// This avoids `Camera::viewport_to_world()` which silently returns `Err`
/// in WASM/WebGL2 when `Camera.computed` is unpopulated or stale.
pub fn cursor_to_ground(cursor_pos: Vec2, window: &Window, cam: &Transform) -> Option<Vec3> {
    let w = window.width();
    let h = window.height();
    if w < 1.0 || h < 1.0 {
        return None;
    }

    // Cursor to NDC: x in [-1,1] (left to right), y in [-1,1] (bottom to top)
    let ndc_x = (cursor_pos.x / w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (cursor_pos.y / h) * 2.0;

    // Camera3d default vertical FOV = pi/4 (45 degrees)
    let half_v = (std::f32::consts::FRAC_PI_4 * 0.5).tan();
    let half_h = half_v * (w / h);

    // Build world-space ray direction from camera axes.
    // Bevy's looking_at rotation places the local +X axis opposite to
    // screen-right in world space, so negate it to get the correct
    // screen-right direction for ray construction.
    let forward = *cam.forward();
    let right = -*cam.right();
    let up = *cam.up();
    let ray_dir = (forward + right * (ndc_x * half_h) + up * (ndc_y * half_v)).normalize();

    // Intersect with Y=0 plane
    if ray_dir.y.abs() < 1e-6 {
        return None;
    }
    let t = -cam.translation.y / ray_dir.y;
    if t <= 0.0 {
        return None;
    }
    Some(cam.translation + ray_dir * t)
}

/// Despawn 3D game entities only (meshes, UI spawned by game plugins).
/// Resources (ActiveGame, NetworkRole, RoundTracker) are preserved for BetweenRounds.
fn cleanup_game_entities(mut commands: Commands, query: Query<Entity, With<GameEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

/// Full cleanup when returning to Lobby — remove all game resources.
fn full_cleanup(mut commands: Commands) {
    commands.remove_resource::<ActiveGame>();
    commands.remove_resource::<NetworkRole>();
    commands.remove_resource::<RoundTracker>();
    #[cfg(feature = "golf")]
    commands.remove_resource::<golf_plugin::GolfCourseInfo>();
}
