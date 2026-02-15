#[cfg(feature = "golf")]
pub mod golf_plugin;
#[cfg(feature = "lasertag")]
pub mod lasertag_plugin;
#[cfg(feature = "platformer")]
pub mod platformer_plugin;

use std::collections::HashMap;

use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::{BreakpointGame, GameConfig, GameId, PlayerId, PlayerScore};
use breakpoint_core::net::messages::PlayerInputMsg;
use breakpoint_core::net::protocol::{
    decode_message_type, decode_server_message, encode_client_message,
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

/// Serialize and send player input to the server via WebSocket.
/// In the server-authoritative model, all clients send inputs the same way.
pub fn send_player_input(
    input: &impl serde::Serialize,
    active_game: &mut ActiveGame,
    network_role: &NetworkRole,
    ws_client: &WsClient,
) {
    if let Ok(data) = rmp_serde::to_vec(input) {
        #[cfg(target_arch = "wasm32")]
        if active_game.tick <= 10 {
            web_sys::console::log_1(
                &format!(
                    "BREAKPOINT:INPUT tick={} bytes={}",
                    active_game.tick,
                    data.len()
                )
                .into(),
            );
        }

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

/// Deserialize the current game state from the active game.
/// Uses cached state bytes from the last server update when available,
/// avoiding redundant `serialize_state()` calls (called 6+ times/frame).
pub fn read_game_state<S: serde::de::DeserializeOwned>(active_game: &ActiveGame) -> Option<S> {
    let bytes = if let Some(ref cached) = active_game.cached_state_bytes {
        cached.as_slice()
    } else {
        // Fallback for first frame before any server state arrives (setup phase)
        return match rmp_serde::from_slice(&active_game.game.serialize_state()) {
            Ok(state) => Some(state),
            Err(e) => {
                #[cfg(target_arch = "wasm32")]
                web_sys::console::warn_1(&format!("Failed to deserialize game state: {e}").into());
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("Failed to deserialize game state: {e}");
                None
            },
        };
    };
    match rmp_serde::from_slice(bytes) {
        Ok(state) => Some(state),
        Err(e) => {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(&format!("Failed to deserialize game state: {e}").into());
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!("Failed to deserialize game state: {e}");
            None
        },
    }
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
    /// Cached raw state bytes from the last server update. Avoids redundant
    /// `serialize_state()` calls in `read_game_state()` (called 6+ times/frame).
    pub cached_state_bytes: Option<Vec<u8>>,
}

/// Network role for this client.
#[derive(Resource)]
pub struct NetworkRole {
    pub is_leader: bool,
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

    let is_leader = lobby.is_leader;
    let local_player_id = lobby.local_player_id.unwrap_or(0);

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(
        &format!(
            "BREAKPOINT:SETUP_GAME game={game_id} is_leader={is_leader} local_pid={local_player_id} \
             players={} spectator={}",
            lobby.players.len(),
            lobby.is_spectator
        )
        .into(),
    );

    commands.insert_resource(ActiveGame {
        game,
        game_id,
        tick: 0,
        tick_accumulator: 0.0,
        prev_state: None,
        interp_alpha: 0.0,
        cached_state_bytes: None,
    });
    commands.insert_resource(NetworkRole {
        is_leader,
        local_player_id,
        is_spectator: lobby.is_spectator,
    });
    commands.insert_resource(RoundTracker::new(round_count));
}

fn client_receive_system(
    ws_client: NonSend<WsClient>,
    mut active_game: ResMut<ActiveGame>,
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
            // All clients receive GameState from the server
            MessageType::GameState => {
                if let Ok(breakpoint_core::net::messages::ServerMessage::GameState(gs)) =
                    decode_server_message(&data)
                {
                    // Store previous state for interpolation
                    active_game.prev_state = Some(active_game.game.serialize_state());
                    active_game.game.apply_state(&gs.state_data);
                    // Cache raw state bytes so read_game_state() avoids re-serializing
                    active_game.cached_state_bytes = Some(gs.state_data);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_color_to_bevy_black() {
        let color = PlayerColor { r: 0, g: 0, b: 0 };
        let bevy_color = player_color_to_bevy(&color);
        assert_eq!(bevy_color, Color::srgb(0.0, 0.0, 0.0));
    }

    #[test]
    fn player_color_to_bevy_white() {
        let color = PlayerColor {
            r: 255,
            g: 255,
            b: 255,
        };
        let bevy_color = player_color_to_bevy(&color);
        assert_eq!(bevy_color, Color::srgb(1.0, 1.0, 1.0));
    }

    #[test]
    fn player_color_to_bevy_midvalue() {
        let color = PlayerColor {
            r: 128,
            g: 64,
            b: 32,
        };
        let bevy_color = player_color_to_bevy(&color);
        let expected = Color::srgb(128.0 / 255.0, 64.0 / 255.0, 32.0 / 255.0);
        assert_eq!(bevy_color, expected);
    }

    #[test]
    fn round_tracker_new() {
        let tracker = RoundTracker::new(9);
        assert_eq!(tracker.current_round, 1);
        assert_eq!(tracker.total_rounds, 9);
        assert!(tracker.cumulative_scores.is_empty());
    }

    #[test]
    fn round_tracker_record_round() {
        let mut tracker = RoundTracker::new(3);
        tracker.record_round(&[
            PlayerScore {
                player_id: 1,
                score: 5,
            },
            PlayerScore {
                player_id: 2,
                score: 3,
            },
        ]);
        assert_eq!(tracker.cumulative_scores[&1], 5);
        assert_eq!(tracker.cumulative_scores[&2], 3);

        // Second round scores accumulate
        tracker.record_round(&[
            PlayerScore {
                player_id: 1,
                score: 2,
            },
            PlayerScore {
                player_id: 2,
                score: 7,
            },
        ]);
        assert_eq!(tracker.cumulative_scores[&1], 7);
        assert_eq!(tracker.cumulative_scores[&2], 10);
    }

    #[test]
    fn round_tracker_is_final_round() {
        let mut tracker = RoundTracker::new(3);
        assert!(!tracker.is_final_round());

        tracker.current_round = 2;
        assert!(!tracker.is_final_round());

        tracker.current_round = 3;
        assert!(tracker.is_final_round());

        tracker.current_round = 4;
        assert!(tracker.is_final_round());
    }

    #[test]
    fn round_tracker_single_round_game() {
        let tracker = RoundTracker::new(1);
        assert!(tracker.is_final_round());
    }

    #[test]
    fn game_registry_register_and_create() {
        let mut registry = GameRegistry::default();
        assert!(registry.create(GameId::Golf).is_none());

        registry.register(GameId::Golf, || Box::new(breakpoint_golf::MiniGolf::new()));
        assert!(registry.create(GameId::Golf).is_some());
        // Other game IDs still missing
        assert!(registry.create(GameId::Platformer).is_none());
    }

    #[test]
    fn game_registry_multiple_games() {
        let mut registry = GameRegistry::default();
        registry.register(GameId::Golf, || Box::new(breakpoint_golf::MiniGolf::new()));
        registry.register(GameId::Platformer, || {
            Box::new(breakpoint_platformer::PlatformRacer::new())
        });
        assert!(registry.create(GameId::Golf).is_some());
        assert!(registry.create(GameId::Platformer).is_some());
    }
}
