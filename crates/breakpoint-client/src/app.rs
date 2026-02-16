use std::collections::HashMap;

use glam::{Vec2, Vec4};
use serde::{Deserialize, Serialize};

use breakpoint_core::game_trait::{BreakpointGame, GameConfig, GameId, PlayerId, PlayerScore};
use breakpoint_core::net::messages::MessageType;
use breakpoint_core::net::protocol::{decode_message_type, decode_server_message};
use breakpoint_core::player::Player;

use crate::audio::{AudioEventQueue, AudioManager, AudioSettings};
use crate::bridge;
use crate::camera_gl::{Camera, CameraMode};
use crate::effects::ScreenShake;
use crate::game::{GameRegistry, read_game_state};
use crate::input::InputState;
use crate::net_client::WsClient;
use crate::overlay::{OverlayEventQueue, OverlayNetEvent, OverlayState};
use crate::renderer::Renderer;
use crate::scene::Scene;
use crate::theme::Theme;

/// Application state machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AppState {
    #[default]
    Lobby,
    InGame,
    BetweenRounds,
    GameOver,
}

/// Lobby state.
#[derive(Default)]
pub struct LobbyState {
    pub player_name: String,
    pub color_index: usize,
    pub room_code: String,
    pub local_player_id: Option<PlayerId>,
    pub is_leader: bool,
    pub players: Vec<Player>,
    pub connected: bool,
    pub is_spectator: bool,
    pub error_message: Option<String>,
    pub ws_url: String,
    pub selected_game: GameId,
    pub join_code_input: String,
    pub status_message: Option<String>,
}

/// Active game instance.
pub struct ActiveGame {
    pub game: Box<dyn BreakpointGame>,
    pub game_id: GameId,
    pub tick: u32,
    pub tick_accumulator: f32,
    pub prev_state: Option<Vec<u8>>,
    pub interp_alpha: f32,
    pub cached_state_bytes: Option<Vec<u8>>,
}

/// Network role for this client.
pub struct NetworkRole {
    pub is_leader: bool,
    pub local_player_id: PlayerId,
    pub is_spectator: bool,
}

/// Round tracking for multi-round games.
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

/// Central application struct holding all state.
pub struct App {
    pub state: AppState,
    pub renderer: Renderer,
    pub camera: Camera,
    pub scene: Scene,
    pub input: InputState,
    pub ws: WsClient,
    pub audio_manager: AudioManager,
    pub audio_events: AudioEventQueue,
    pub audio_settings: AudioSettings,
    pub theme: Theme,
    pub lobby: LobbyState,
    pub game: Option<ActiveGame>,
    pub network_role: Option<NetworkRole>,
    pub overlay: OverlayState,
    pub overlay_queue: OverlayEventQueue,
    pub round_tracker: Option<RoundTracker>,
    pub registry: GameRegistry,
    pub screen_shake: ScreenShake,
    pub was_connected: bool,
    prev_timestamp: f64,
}

impl App {
    pub fn new(renderer: Renderer) -> Self {
        let theme = Theme::load();
        let mut lobby = LobbyState {
            player_name: format!("Player{}", fastrand::u16(..1000)),
            ..Default::default()
        };

        // Determine WebSocket URL
        #[cfg(target_family = "wasm")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(location) = window.location().host() {
                    let protocol = if window
                        .location()
                        .protocol()
                        .unwrap_or_default()
                        .contains("https")
                    {
                        "wss"
                    } else {
                        "ws"
                    };
                    lobby.ws_url = format!("{protocol}://{location}/ws");
                }
            }
        }
        #[cfg(not(target_family = "wasm"))]
        {
            lobby.ws_url = "ws://localhost:8080/ws".to_string();
        }

        // Read room code from URL ?room= parameter
        #[cfg(target_family = "wasm")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(search) = window.location().search() {
                    if let Some(room_param) = search
                        .trim_start_matches('?')
                        .split('&')
                        .find(|p| p.starts_with("room="))
                    {
                        let code = room_param.trim_start_matches("room=");
                        if !code.is_empty() {
                            lobby.join_code_input = code.to_uppercase();
                        }
                    }
                }
            }
        }

        // Load audio settings from localStorage
        let mut audio_settings = AudioSettings::default();
        crate::storage::with_local_storage(|storage| {
            if let Ok(Some(val)) = storage.get_item("audio_muted") {
                audio_settings.muted = val == "true";
            }
            if let Ok(Some(val)) = storage.get_item("audio_master_volume")
                && let Ok(v) = val.parse::<f32>()
            {
                audio_settings.muted = false;
                audio_settings.master_volume = v.clamp(0.0, 1.0);
            }
        });

        let registry = crate::game::create_registry();

        Self {
            state: AppState::Lobby,
            renderer,
            camera: Camera::new(),
            scene: Scene::new(),
            input: InputState::new(),
            ws: WsClient::new(),
            audio_manager: AudioManager::new(),
            audio_events: AudioEventQueue::default(),
            audio_settings,
            theme,
            lobby,
            game: None,
            network_role: None,
            overlay: OverlayState::new(),
            overlay_queue: OverlayEventQueue::default(),
            round_tracker: None,
            registry,
            screen_shake: ScreenShake::default(),
            was_connected: false,
            prev_timestamp: 0.0,
        }
    }

    /// Main frame update, called each requestAnimationFrame.
    pub fn frame(&mut self, timestamp: f64) {
        let dt = if self.prev_timestamp > 0.0 {
            ((timestamp - self.prev_timestamp) / 1000.0) as f32
        } else {
            1.0 / 60.0
        };
        let dt = dt.min(0.1); // Cap at 100ms to avoid spiral of death
        self.prev_timestamp = timestamp;

        // Resize canvas and update camera aspect
        self.renderer.resize();
        let (vw, vh) = self.renderer.viewport_size();
        if vh > 0.0 {
            self.camera.aspect = vw / vh;
        }

        // Process network messages
        self.process_network();

        // Process overlay events
        self.overlay
            .process_events(&mut self.overlay_queue, &mut self.audio_events);

        // State-specific update
        match self.state {
            AppState::Lobby => {},
            AppState::InGame => {
                self.update_game(dt);
            },
            AppState::BetweenRounds | AppState::GameOver => {},
        }

        // Update camera
        self.camera.update(dt);

        // Screen shake
        if self.screen_shake.timer > 0.0 {
            self.screen_shake.tick(dt);
            self.camera.apply_shake(self.screen_shake.offset);
        }

        // Process audio
        if !self.audio_settings.muted {
            self.audio_events
                .process(&self.audio_manager, &self.audio_settings);
        } else {
            self.audio_events.clear();
        }

        // Render 3D scene — Tron uses pure black background
        let clear_color = if self
            .game
            .as_ref()
            .is_some_and(|g| g.game_id == GameId::Tron)
        {
            Vec4::new(0.0, 0.0, 0.0, 1.0)
        } else {
            let clear = &self.theme.camera.clear_color;
            Vec4::new(clear[0], clear[1], clear[2], 1.0)
        };
        self.renderer
            .draw(&self.scene, &self.camera, dt, clear_color);

        // Push UI state to JS
        bridge::push_ui_state(self);

        // End frame
        self.input.end_frame();
    }

    fn process_network(&mut self) {
        // Connection monitoring
        let connected = self.ws.is_connected();
        if self.was_connected && !connected && self.ws.has_connection() {
            bridge::show_disconnect_banner();
        }
        if connected && !self.was_connected {
            bridge::hide_disconnect_banner();
        }
        self.was_connected = connected;

        let messages = self.ws.drain_messages();
        for data in messages {
            let msg_type = match decode_message_type(&data) {
                Ok(t) => t,
                Err(e) => {
                    crate::diag::console_warn!(
                        "Failed to decode message type ({} bytes): {e}",
                        data.len()
                    );
                    continue;
                },
            };

            match self.state {
                AppState::Lobby => self.process_lobby_message(&data, msg_type),
                AppState::InGame => self.process_game_message(&data, msg_type),
                AppState::BetweenRounds | AppState::GameOver => {
                    // Forward alerts
                    self.process_alert_message(&data, msg_type);
                },
            }
        }
    }

    fn process_lobby_message(&mut self, data: &[u8], msg_type: MessageType) {
        use breakpoint_core::net::messages::ServerMessage;

        let msg = match decode_server_message(data) {
            Ok(m) => m,
            Err(e) => {
                crate::diag::console_warn!(
                    "Failed to decode lobby message ({msg_type:?}, {} bytes): {e}",
                    data.len()
                );
                return;
            },
        };

        match msg {
            ServerMessage::JoinRoomResponse(resp) => {
                if resp.success {
                    self.lobby.local_player_id = resp.player_id;
                    if let Some(code) = &resp.room_code {
                        self.lobby.room_code = code.clone();
                    }
                    self.lobby.connected = true;
                    self.lobby.error_message = None;
                    self.overlay.local_player_id = resp.player_id;

                    if self.lobby.is_leader {
                        self.lobby.status_message = Some(
                            "Room created! Click Start Game, or share the code with friends."
                                .to_string(),
                        );
                    } else {
                        self.lobby.status_message =
                            Some("Joined! Waiting for leader to start...".to_string());
                    }

                    if let Some(room_state) = resp.room_state
                        && room_state != breakpoint_core::room::RoomState::Lobby
                    {
                        self.lobby.is_spectator = true;
                        self.transition_to(AppState::InGame);
                    }
                } else {
                    self.lobby.error_message = resp.error.clone();
                    self.lobby.status_message = resp.error;
                }
            },
            ServerMessage::PlayerList(pl) => {
                self.lobby.players = pl.players.clone();
                if let Some(my_id) = self.lobby.local_player_id {
                    self.lobby.is_leader = pl.leader_id == my_id;
                }
                self.lobby.connected = true;
            },
            ServerMessage::GameStart(gs) => {
                self.lobby.selected_game = GameId::from_str_opt(&gs.game_name).unwrap_or_default();
                self.transition_to(AppState::InGame);
            },
            ServerMessage::AlertEvent(ae) => {
                self.overlay_queue
                    .push(OverlayNetEvent::AlertReceived(Box::new(ae.event)));
            },
            ServerMessage::AlertClaimed(ac) => {
                self.overlay_queue.push(OverlayNetEvent::AlertClaimed {
                    event_id: ac.event_id,
                    claimed_by: ac.claimed_by.to_string(),
                });
            },
            ServerMessage::AlertDismissed(ad) => {
                self.overlay_queue.push(OverlayNetEvent::AlertDismissed {
                    event_id: ad.event_id,
                });
            },
            _ => {},
        }
    }

    fn process_game_message(&mut self, data: &[u8], msg_type: MessageType) {
        use breakpoint_core::net::messages::ServerMessage;

        match msg_type {
            MessageType::GameState => match decode_server_message(data) {
                Ok(ServerMessage::GameState(gs)) => {
                    if let Some(ref mut active) = self.game {
                        active.prev_state = Some(active.game.serialize_state());
                        active.game.apply_state(&gs.state_data);
                        active.cached_state_bytes = Some(gs.state_data);
                        active.tick = gs.tick;
                        active.interp_alpha = 0.0;
                    }
                },
                Err(e) => {
                    crate::diag::console_warn!(
                        "Failed to decode GameState ({} bytes): {e}",
                        data.len()
                    );
                },
                _ => {},
            },
            MessageType::RoundEnd => match decode_server_message(data) {
                Ok(ServerMessage::RoundEnd(re)) => {
                    let scores: Vec<PlayerScore> = re
                        .scores
                        .iter()
                        .map(|s| PlayerScore {
                            player_id: s.player_id,
                            score: s.score,
                        })
                        .collect();
                    if let Some(ref mut tracker) = self.round_tracker {
                        tracker.record_round(&scores);
                    }
                    self.transition_to(AppState::BetweenRounds);
                },
                Err(e) => {
                    crate::diag::console_warn!(
                        "Failed to decode RoundEnd ({} bytes): {e}",
                        data.len()
                    );
                },
                _ => {},
            },
            MessageType::GameEnd => match decode_server_message(data) {
                Ok(ServerMessage::GameEnd(ge)) => {
                    let scores: Vec<PlayerScore> = ge
                        .final_scores
                        .iter()
                        .map(|s| PlayerScore {
                            player_id: s.player_id,
                            score: s.score,
                        })
                        .collect();
                    if let Some(ref mut tracker) = self.round_tracker {
                        tracker.record_round(&scores);
                    }
                    self.transition_to(AppState::GameOver);
                },
                Err(e) => {
                    crate::diag::console_warn!(
                        "Failed to decode GameEnd ({} bytes): {e}",
                        data.len()
                    );
                },
                _ => {},
            },
            MessageType::AlertEvent | MessageType::AlertClaimed | MessageType::AlertDismissed => {
                self.process_alert_message(data, msg_type);
            },
            _ => {},
        }
    }

    fn process_alert_message(&mut self, data: &[u8], msg_type: MessageType) {
        use breakpoint_core::net::messages::ServerMessage;

        match msg_type {
            MessageType::AlertEvent => match decode_server_message(data) {
                Ok(ServerMessage::AlertEvent(ae)) => {
                    self.overlay_queue
                        .push(OverlayNetEvent::AlertReceived(Box::new(ae.event)));
                },
                Err(e) => {
                    crate::diag::console_warn!(
                        "Failed to decode AlertEvent ({} bytes): {e}",
                        data.len()
                    );
                },
                _ => {},
            },
            MessageType::AlertClaimed => match decode_server_message(data) {
                Ok(ServerMessage::AlertClaimed(ac)) => {
                    self.overlay_queue.push(OverlayNetEvent::AlertClaimed {
                        event_id: ac.event_id,
                        claimed_by: ac.claimed_by.to_string(),
                    });
                },
                Err(e) => {
                    crate::diag::console_warn!(
                        "Failed to decode AlertClaimed ({} bytes): {e}",
                        data.len()
                    );
                },
                _ => {},
            },
            MessageType::AlertDismissed => match decode_server_message(data) {
                Ok(ServerMessage::AlertDismissed(ad)) => {
                    self.overlay_queue.push(OverlayNetEvent::AlertDismissed {
                        event_id: ad.event_id,
                    });
                },
                Err(e) => {
                    crate::diag::console_warn!(
                        "Failed to decode AlertDismissed ({} bytes): {e}",
                        data.len()
                    );
                },
                _ => {},
            },
            _ => {},
        }
    }

    fn update_game(&mut self, dt: f32) {
        let Some(ref active) = self.game else {
            return;
        };

        // Update camera mode based on game type
        match active.game_id {
            #[cfg(feature = "golf")]
            GameId::Golf => {
                if let Some(ref role) = self.network_role
                    && let Some(s) = read_game_state::<breakpoint_golf::GolfState>(active)
                    && let Some(b) = s.balls.get(&role.local_player_id)
                {
                    self.camera.set_mode(CameraMode::GolfFollow {
                        ball_pos: glam::Vec3::new(b.position.x, 0.0, b.position.z),
                    });
                }
            },
            #[cfg(feature = "platformer")]
            GameId::Platformer => {
                if let Some(ref role) = self.network_role
                    && let Some(s) =
                        read_game_state::<breakpoint_platformer::PlatformerState>(active)
                    && let Some(p) = s.players.get(&role.local_player_id)
                {
                    self.camera.set_mode(CameraMode::PlatformerFollow {
                        player_pos: Vec2::new(p.x, p.y),
                    });
                }
            },
            #[cfg(feature = "lasertag")]
            GameId::LaserTag => {
                self.camera.set_mode(CameraMode::LaserTagFixed);
            },
            #[cfg(feature = "tron")]
            GameId::Tron => {
                if let Some(ref role) = self.network_role
                    && let Some(s) = read_game_state::<breakpoint_tron::TronState>(active)
                    && let Some(c) = s.players.get(&role.local_player_id)
                    && c.alive
                {
                    let dir = match c.direction {
                        breakpoint_tron::Direction::North => [0.0, -1.0],
                        breakpoint_tron::Direction::South => [0.0, 1.0],
                        breakpoint_tron::Direction::East => [1.0, 0.0],
                        breakpoint_tron::Direction::West => [-1.0, 0.0],
                    };
                    self.camera.set_mode(CameraMode::TronFollow {
                        cycle_pos: glam::Vec3::new(c.x, 0.0, c.z),
                        direction: dir,
                    });
                }
            },
            #[allow(unreachable_patterns)]
            _ => {},
        }

        // Game-specific input and rendering
        self.update_game_input();
        self.sync_game_scene(dt);
    }

    fn update_game_input(&mut self) {
        let Some(ref mut active) = self.game else {
            return;
        };
        let Some(ref role) = self.network_role else {
            return;
        };

        match active.game_id {
            #[cfg(feature = "golf")]
            GameId::Golf => {
                crate::game::golf_input::process_golf_input(
                    &self.input,
                    &self.camera,
                    &self.renderer,
                    active,
                    role,
                    &self.ws,
                );
            },
            #[cfg(feature = "platformer")]
            GameId::Platformer => {
                crate::game::platformer_input::process_platformer_input(
                    &self.input,
                    active,
                    role,
                    &self.ws,
                );
            },
            #[cfg(feature = "lasertag")]
            GameId::LaserTag => {
                crate::game::lasertag_input::process_lasertag_input(
                    &self.input,
                    &self.camera,
                    &self.renderer,
                    active,
                    role,
                    &self.ws,
                );
            },
            #[cfg(feature = "tron")]
            GameId::Tron => {
                crate::game::tron_input::process_tron_input(&self.input, active, role, &self.ws);
            },
            #[allow(unreachable_patterns)]
            _ => {},
        }
    }

    fn sync_game_scene(&mut self, dt: f32) {
        let Some(ref active) = self.game else {
            return;
        };

        match active.game_id {
            #[cfg(feature = "golf")]
            GameId::Golf => {
                crate::game::golf_render::sync_golf_scene(
                    &mut self.scene,
                    active,
                    &self.theme,
                    dt,
                    &self.input,
                    &self.camera,
                    &self.renderer,
                    self.network_role.as_ref(),
                );
            },
            #[cfg(feature = "platformer")]
            GameId::Platformer => {
                crate::game::platformer_render::sync_platformer_scene(
                    &mut self.scene,
                    active,
                    &self.theme,
                    dt,
                );
            },
            #[cfg(feature = "lasertag")]
            GameId::LaserTag => {
                crate::game::lasertag_render::sync_lasertag_scene(
                    &mut self.scene,
                    active,
                    &self.theme,
                    dt,
                );
            },
            #[cfg(feature = "tron")]
            GameId::Tron => {
                let local_id = self.network_role.as_ref().map(|r| r.local_player_id);
                crate::game::tron_render::sync_tron_scene(
                    &mut self.scene,
                    active,
                    &self.theme,
                    dt,
                    local_id,
                );
            },
            #[allow(unreachable_patterns)]
            _ => {},
        }
    }

    /// Transition to a new app state.
    pub fn transition_to(&mut self, new_state: AppState) {
        let old_state = self.state;
        self.state = new_state;

        match (old_state, new_state) {
            (AppState::Lobby, AppState::InGame) => {
                self.setup_game();
            },
            (AppState::InGame, AppState::Lobby) | (_, AppState::Lobby) => {
                self.scene.clear();
                self.game = None;
                self.network_role = None;
                self.round_tracker = None;
            },
            _ => {},
        }
    }

    fn setup_game(&mut self) {
        if self.game.is_some() {
            return;
        }

        let game_id = self.lobby.selected_game;
        let mut game = match self.registry.create(game_id) {
            Some(g) => g,
            None => return,
        };

        let round_count = game.round_count_hint();
        let config = GameConfig {
            round_count,
            round_duration: std::time::Duration::from_secs(90),
            custom: HashMap::new(),
        };
        game.init(&self.lobby.players, &config);

        let local_player_id = self.lobby.local_player_id.unwrap_or(0);

        self.game = Some(ActiveGame {
            game,
            game_id,
            tick: 0,
            tick_accumulator: 0.0,
            prev_state: None,
            interp_alpha: 0.0,
            cached_state_bytes: None,
        });
        self.network_role = Some(NetworkRole {
            is_leader: self.lobby.is_leader,
            local_player_id,
            is_spectator: self.lobby.is_spectator,
        });
        self.round_tracker = Some(RoundTracker::new(round_count));
        self.scene.clear();
    }
}

// ── requestAnimationFrame loop ─────────────────────────────────

#[cfg(target_family = "wasm")]
pub fn run() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use wasm_bindgen::JsCast;

    let renderer = match Renderer::new() {
        Ok(r) => r,
        Err(e) => {
            web_sys::console::error_1(&format!("Renderer init failed: {e}").into());
            return;
        },
    };

    let app = Rc::new(RefCell::new(App::new(renderer)));

    // Attach input listeners
    bridge::attach_input_listeners(&app);
    // Attach JS→Rust bridge callbacks
    bridge::attach_ui_callbacks(&app);

    // Start the rAF loop
    let f: Rc<RefCell<Option<wasm_bindgen::closure::Closure<dyn FnMut(f64)>>>> =
        Rc::new(RefCell::new(None));
    let g = Rc::clone(&f);
    let app_loop = Rc::clone(&app);

    *g.borrow_mut() = Some(wasm_bindgen::closure::Closure::new(
        move |timestamp: f64| {
            app_loop.borrow_mut().frame(timestamp);

            // Schedule next frame
            if let Some(window) = web_sys::window() {
                let _ = window
                    .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref());
            }
        },
    ));

    if let Some(window) = web_sys::window() {
        let _ =
            window.request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref());
    }

    // Prevent the closure from being dropped
    std::mem::forget(app);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        tracker.current_round = 3;
        assert!(tracker.is_final_round());
    }

    #[test]
    fn app_state_default_is_lobby() {
        assert_eq!(AppState::default(), AppState::Lobby);
    }
}
