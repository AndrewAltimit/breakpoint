use bevy::ecs::system::NonSend;
use bevy::ecs::system::NonSendMut;
use bevy::input::ButtonInput;
use bevy::prelude::*;

use breakpoint_core::game_trait::{GameId, PlayerId};
use breakpoint_core::net::messages::{
    ClientMessage, GameStartMsg, JoinRoomMsg, JoinRoomResponseMsg, PlayerListMsg,
};
use breakpoint_core::net::protocol::{
    PROTOCOL_VERSION, decode_server_message, encode_client_message,
};
use breakpoint_core::player::{Player, PlayerColor};

use crate::app::AppState;
use crate::net_client::WsClient;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LobbyState>()
            .add_systems(OnEnter(AppState::Lobby), setup_lobby)
            .add_systems(
                Update,
                (
                    lobby_input_system,
                    lobby_keyboard_system,
                    lobby_network_system,
                    lobby_status_display_system,
                )
                    .run_if(in_state(AppState::Lobby)),
            )
            .add_systems(OnExit(AppState::Lobby), cleanup_lobby);
    }
}

/// Lobby state resource.
#[derive(Resource, Default)]
pub struct LobbyState {
    pub player_name: String,
    pub color_index: usize,
    pub room_code: String,
    pub local_player_id: Option<PlayerId>,
    pub is_host: bool,
    pub players: Vec<Player>,
    pub connected: bool,
    pub is_spectator: bool,
    pub error_message: Option<String>,
    pub ws_url: String,
    pub selected_game: GameId,
    /// Room code being typed by the user for joining.
    pub join_code_input: String,
    /// Status message to display (set by various systems, rendered by status_display).
    pub status_message: Option<String>,
}

#[derive(Component)]
struct LobbyUi;

#[derive(Component)]
struct LobbyCamera;

#[derive(Component)]
struct PlayerListText;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct RoomCodeText;

#[derive(Component)]
struct JoinCodeText;

type RoomCodeFilter = (
    With<RoomCodeText>,
    Without<PlayerListText>,
    Without<StatusText>,
    Without<JoinCodeText>,
);
type PlayerListFilter = (
    With<PlayerListText>,
    Without<RoomCodeText>,
    Without<StatusText>,
    Without<JoinCodeText>,
);
type StatusFilter = (
    With<StatusText>,
    Without<RoomCodeText>,
    Without<PlayerListText>,
    Without<JoinCodeText>,
);

#[derive(Component)]
enum LobbyButton {
    Create,
    Join,
    StartGame,
}

/// Marker for the StartGame button specifically (for targeted visibility changes).
#[derive(Component)]
struct StartGameButton;

/// Marker for the join row (code input + join button) to hide when connected.
#[derive(Component)]
struct JoinRow;

#[derive(Component)]
struct GameSelectButton(GameId);

#[derive(Component)]
struct GameSelectionText;

fn setup_lobby(mut commands: Commands, mut lobby: ResMut<LobbyState>) {
    // Spawn a 2D camera for UI rendering.
    // Msaa::Off is required for WebGL2 compatibility.
    commands.spawn((LobbyCamera, Camera2d, Msaa::Off));

    if lobby.player_name.is_empty() {
        lobby.player_name = format!("Player{}", fastrand::u16(..1000));
    }
    // Determine WebSocket URL
    if lobby.ws_url.is_empty() {
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
    }

    // Read room code from URL ?room= parameter (for join links)
    #[cfg(target_family = "wasm")]
    {
        if lobby.room_code.is_empty() && lobby.join_code_input.is_empty() {
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
    }

    let bg_color = Color::srgba(0.1, 0.1, 0.18, 0.95);
    let btn_color = Color::srgb(0.2, 0.4, 0.8);
    let text_color = Color::srgb(0.9, 0.9, 0.9);

    commands
        .spawn((
            LobbyUi,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(bg_color),
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new("BREAKPOINT"),
                TextFont {
                    font_size: 48.0,
                    ..default()
                },
                TextColor(Color::srgb(0.3, 0.7, 1.0)),
            ));

            parent.spawn((
                Text::new("Multiplayer Game Arena"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(text_color),
            ));

            // Game selection buttons
            parent.spawn((
                Text::new("Select Game:"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(text_color),
            ));

            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_game_button(row, "Mini-Golf", GameId::Golf, btn_color);
                    spawn_game_button(row, "Platform Racer", GameId::Platformer, btn_color);
                    spawn_game_button(row, "Laser Tag", GameId::LaserTag, btn_color);
                });

            // Selected game indicator
            parent.spawn((
                GameSelectionText,
                Text::new(format!("Game: {}", lobby.selected_game)),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.3, 0.9, 0.3)),
            ));

            parent.spawn((
                Text::new(format!("Name: {}", lobby.player_name)),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(text_color),
            ));

            let color = PlayerColor::PALETTE[lobby.color_index % PlayerColor::PALETTE.len()];
            parent.spawn((
                Text::new("Your Color"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                )),
            ));

            // Create Room button
            spawn_button(parent, "Create Room", LobbyButton::Create, btn_color);

            // Join code input display + Join button (hidden when connected)
            parent
                .spawn((
                    JoinRow,
                    Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                ))
                .with_children(|row| {
                    // Join code display (shows typed characters)
                    let initial_code = if lobby.join_code_input.is_empty() {
                        "Type room code...".to_string()
                    } else {
                        lobby.join_code_input.clone()
                    };
                    row.spawn((
                        JoinCodeText,
                        Text::new(initial_code),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.7, 0.7, 0.5)),
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                            min_width: Val::Px(200.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.15, 0.15, 0.25)),
                    ));
                    spawn_button(row, "Join Room", LobbyButton::Join, btn_color);
                });

            // Start Game button (hidden initially, shown when host is in room)
            parent
                .spawn((
                    LobbyButton::StartGame,
                    StartGameButton,
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(24.0), Val::Px(12.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.1, 0.6, 0.2)),
                    Visibility::Hidden,
                ))
                .with_child((
                    Text::new("Start Game"),
                    TextFont {
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));

            // Room code display
            parent.spawn((
                RoomCodeText,
                Text::new(""),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.3)),
            ));

            // Player list
            parent.spawn((
                PlayerListText,
                Text::new(""),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(text_color),
            ));

            // Status (yellow for info, errors override to red)
            parent.spawn((
                StatusText,
                Text::new(""),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.85, 0.3)),
            ));
        });
}

fn spawn_game_button(parent: &mut ChildSpawnerCommands, label: &str, game_id: GameId, color: Color) {
    parent
        .spawn((
            GameSelectButton(game_id),
            Button,
            Node {
                padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(color),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

fn spawn_button(parent: &mut ChildSpawnerCommands, label: &str, action: LobbyButton, color: Color) {
    parent
        .spawn((
            action,
            Button,
            Node {
                padding: UiRect::axes(Val::Px(24.0), Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(color),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: 20.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

/// Handle keyboard input for typing room codes.
fn lobby_keyboard_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut lobby: ResMut<LobbyState>,
    mut join_code_text: Query<&mut Text, With<JoinCodeText>>,
) {
    // Don't accept keyboard input if already in a room
    if lobby.connected {
        return;
    }

    let mut changed = false;

    for key in keyboard.get_just_pressed() {
        let ch = match key {
            KeyCode::KeyA => Some('A'),
            KeyCode::KeyB => Some('B'),
            KeyCode::KeyC => Some('C'),
            KeyCode::KeyD => Some('D'),
            KeyCode::KeyE => Some('E'),
            KeyCode::KeyF => Some('F'),
            KeyCode::KeyG => Some('G'),
            KeyCode::KeyH => Some('H'),
            KeyCode::KeyI => Some('I'),
            KeyCode::KeyJ => Some('J'),
            KeyCode::KeyK => Some('K'),
            KeyCode::KeyL => Some('L'),
            KeyCode::KeyM => Some('M'),
            KeyCode::KeyN => Some('N'),
            KeyCode::KeyO => Some('O'),
            KeyCode::KeyP => Some('P'),
            KeyCode::KeyQ => Some('Q'),
            KeyCode::KeyR => Some('R'),
            KeyCode::KeyS => Some('S'),
            KeyCode::KeyT => Some('T'),
            KeyCode::KeyU => Some('U'),
            KeyCode::KeyV => Some('V'),
            KeyCode::KeyW => Some('W'),
            KeyCode::KeyX => Some('X'),
            KeyCode::KeyY => Some('Y'),
            KeyCode::KeyZ => Some('Z'),
            KeyCode::Digit0 | KeyCode::Numpad0 => Some('0'),
            KeyCode::Digit1 | KeyCode::Numpad1 => Some('1'),
            KeyCode::Digit2 | KeyCode::Numpad2 => Some('2'),
            KeyCode::Digit3 | KeyCode::Numpad3 => Some('3'),
            KeyCode::Digit4 | KeyCode::Numpad4 => Some('4'),
            KeyCode::Digit5 | KeyCode::Numpad5 => Some('5'),
            KeyCode::Digit6 | KeyCode::Numpad6 => Some('6'),
            KeyCode::Digit7 | KeyCode::Numpad7 => Some('7'),
            KeyCode::Digit8 | KeyCode::Numpad8 => Some('8'),
            KeyCode::Digit9 | KeyCode::Numpad9 => Some('9'),
            KeyCode::Minus => Some('-'),
            _ => None,
        };

        if let Some(c) = ch {
            lobby.join_code_input.push(c);
            changed = true;
        } else if *key == KeyCode::Backspace {
            lobby.join_code_input.pop();
            changed = true;
        }
    }

    // Auto-insert dash after 4 chars if missing (ABCD -> ABCD-)
    if changed && lobby.join_code_input.len() == 5 && !lobby.join_code_input.contains('-') {
        lobby.join_code_input.insert(4, '-');
    }

    // Cap at 9 chars (ABCD-1234)
    lobby.join_code_input.truncate(9);

    if changed && let Ok(mut text) = join_code_text.single_mut() {
        if lobby.join_code_input.is_empty() {
            **text = "Type room code...".to_string();
        } else {
            **text = lobby.join_code_input.clone();
        }
    }
}

/// Handle button clicks. No Text queries here to avoid query conflicts.
fn lobby_input_system(
    interaction_query: Query<(&Interaction, &LobbyButton), Changed<Interaction>>,
    game_select_query: Query<(&Interaction, &GameSelectButton), Changed<Interaction>>,
    mut lobby: ResMut<LobbyState>,
    mut ws_client: NonSendMut<WsClient>,
    mut game_text_query: Query<&mut Text, With<GameSelectionText>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    // Handle game selection buttons
    for (interaction, btn) in &game_select_query {
        if *interaction == Interaction::Pressed {
            lobby.selected_game = btn.0;
            if let Ok(mut text) = game_text_query.single_mut() {
                **text = format!("Game: {}", lobby.selected_game);
            }
        }
    }

    for (interaction, button) in &interaction_query {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match button {
            LobbyButton::Create => {
                if lobby.connected {
                    lobby.status_message =
                        Some("Already in a room. Refresh page to create a new one.".to_string());
                    continue;
                }
                if !ws_client.has_connection() {
                    let url = lobby.ws_url.clone();
                    if let Err(e) = ws_client.connect(&url) {
                        lobby.status_message = Some(format!("Connection failed: {e}"));
                        continue;
                    }
                }
                lobby.is_host = true;
                let color = PlayerColor::PALETTE[lobby.color_index % PlayerColor::PALETTE.len()];
                let msg = ClientMessage::JoinRoom(JoinRoomMsg {
                    room_code: String::new(),
                    player_name: lobby.player_name.clone(),
                    player_color: color,
                    protocol_version: PROTOCOL_VERSION,
                });
                if let Ok(data) = encode_client_message(&msg) {
                    let _ = ws_client.send(&data);
                }
                lobby.status_message = Some("Creating room...".to_string());
            },
            LobbyButton::Join => {
                if lobby.connected {
                    lobby.status_message =
                        Some("Already in a room. Refresh page to join a new one.".to_string());
                    continue;
                }
                let code = lobby.join_code_input.trim().to_uppercase();
                if code.is_empty() {
                    lobby.status_message =
                        Some("Type a room code first (e.g. ABCD-1234)".to_string());
                    continue;
                }
                if !ws_client.has_connection() {
                    let url = lobby.ws_url.clone();
                    if let Err(e) = ws_client.connect(&url) {
                        lobby.status_message = Some(format!("Connection failed: {e}"));
                        continue;
                    }
                }
                lobby.is_host = false;
                let color = PlayerColor::PALETTE[lobby.color_index % PlayerColor::PALETTE.len()];
                let msg = ClientMessage::JoinRoom(JoinRoomMsg {
                    room_code: code.clone(),
                    player_name: lobby.player_name.clone(),
                    player_color: color,
                    protocol_version: PROTOCOL_VERSION,
                });
                if let Ok(data) = encode_client_message(&msg) {
                    let _ = ws_client.send(&data);
                }
                lobby.status_message = Some(format!("Joining room {code}..."));
            },
            LobbyButton::StartGame => {
                if lobby.is_host {
                    let msg =
                        breakpoint_core::net::messages::ServerMessage::GameStart(GameStartMsg {
                            game_name: lobby.selected_game.to_string(),
                            players: lobby.players.clone(),
                            host_id: lobby.local_player_id.unwrap_or(0),
                        });
                    if let Ok(data) = breakpoint_core::net::protocol::encode_server_message(&msg) {
                        let _ = ws_client.send(&data);
                    }
                    // The server relays GameStart to other players but NOT back to
                    // the sender (broadcast_to_room_except). Transition locally.
                    next_state.set(AppState::InGame);
                }
            },
        }
    }
}

/// Display status/error messages from LobbyState. Runs separately to avoid query conflicts.
fn lobby_status_display_system(
    lobby: Res<LobbyState>,
    mut status_query: Query<(&mut Text, &mut TextColor), StatusFilter>,
) {
    if !lobby.is_changed() {
        return;
    }
    if let Ok((mut text, mut color)) = status_query.single_mut() {
        if let Some(ref err) = lobby.error_message {
            **text = err.clone();
            *color = TextColor(Color::srgb(1.0, 0.4, 0.4)); // Red for errors
        } else if let Some(ref msg) = lobby.status_message {
            **text = msg.clone();
            *color = TextColor(Color::srgb(0.9, 0.85, 0.3)); // Yellow for info
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lobby_network_system(
    ws_client: NonSend<WsClient>,
    mut lobby: ResMut<LobbyState>,
    mut room_code_text: Query<&mut Text, RoomCodeFilter>,
    mut player_list_text: Query<&mut Text, PlayerListFilter>,
    mut start_btn_vis: Query<&mut Visibility, With<StartGameButton>>,
    mut join_row_node: Query<&mut Node, With<JoinRow>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut overlay_queue: ResMut<crate::overlay::OverlayEventQueue>,
    mut overlay_state: ResMut<crate::overlay::OverlayState>,
) {
    let messages = ws_client.drain_messages();
    for data in messages {
        let msg = match decode_server_message(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };

        match msg {
            breakpoint_core::net::messages::ServerMessage::JoinRoomResponse(resp) => {
                handle_join_response(&resp, &mut lobby);
                if resp.success {
                    overlay_state.local_player_id = resp.player_id;
                    if lobby.is_host {
                        lobby.status_message = Some(
                            "Room created! Click Start Game, or share the code with friends."
                                .to_string(),
                        );
                    } else {
                        lobby.status_message =
                            Some("Joined! Waiting for host to start...".to_string());
                    }

                    if let Some(room_state) = resp.room_state
                        && room_state != breakpoint_core::room::RoomState::Lobby
                    {
                        lobby.is_spectator = true;
                        next_state.set(AppState::InGame);
                    }
                }
                if let Ok(mut text) = room_code_text.single_mut()
                    && let Some(code) = &resp.room_code
                {
                    **text = format!("Room: {code}");
                }
                if !resp.success {
                    lobby.status_message = resp.error.clone();
                }
            },
            breakpoint_core::net::messages::ServerMessage::PlayerList(pl) => {
                handle_player_list(&pl, &mut lobby);
                if let Ok(mut text) = player_list_text.single_mut() {
                    let names: Vec<String> = lobby
                        .players
                        .iter()
                        .map(|p| {
                            if p.is_host {
                                format!("{} (host)", p.display_name)
                            } else {
                                p.display_name.clone()
                            }
                        })
                        .collect();
                    **text = format!("Players: {}", names.join(", "));
                }
                if lobby.connected {
                    // Collapse the join row entirely (Display::None removes from layout)
                    for mut node in &mut join_row_node {
                        node.display = Display::None;
                    }
                    // Show Start Game for the host
                    if lobby.is_host {
                        for mut vis in &mut start_btn_vis {
                            *vis = Visibility::Visible;
                        }
                    }
                }
            },
            breakpoint_core::net::messages::ServerMessage::GameStart(gs) => {
                lobby.selected_game =
                    GameId::from_str_opt(&gs.game_name).unwrap_or_default();
                next_state.set(AppState::InGame);
            },
            breakpoint_core::net::messages::ServerMessage::AlertEvent(ae) => {
                overlay_queue.push(crate::overlay::OverlayNetEvent::AlertReceived(Box::new(
                    ae.event,
                )));
            },
            breakpoint_core::net::messages::ServerMessage::AlertClaimed(ac) => {
                overlay_queue.push(crate::overlay::OverlayNetEvent::AlertClaimed {
                    event_id: ac.event_id,
                    claimed_by: ac.claimed_by.to_string(),
                });
            },
            breakpoint_core::net::messages::ServerMessage::AlertDismissed(ad) => {
                overlay_queue.push(crate::overlay::OverlayNetEvent::AlertDismissed {
                    event_id: ad.event_id,
                });
            },
            _ => {},
        }
    }
}

fn handle_join_response(resp: &JoinRoomResponseMsg, lobby: &mut LobbyState) {
    if resp.success {
        lobby.local_player_id = resp.player_id;
        if let Some(code) = &resp.room_code {
            lobby.room_code = code.clone();
        }
        lobby.connected = true;
        lobby.error_message = None;
    } else {
        lobby.error_message = resp.error.clone();
    }
}

fn handle_player_list(pl: &PlayerListMsg, lobby: &mut LobbyState) {
    lobby.players = pl.players.clone();
    if let Some(my_id) = lobby.local_player_id {
        lobby.is_host = pl.host_id == my_id;
    }
}

fn cleanup_lobby(
    mut commands: Commands,
    ui_query: Query<Entity, With<LobbyUi>>,
    camera_query: Query<Entity, With<LobbyCamera>>,
) {
    for entity in &ui_query {
        commands.entity(entity).despawn();
    }
    for entity in &camera_query {
        commands.entity(entity).despawn();
    }
}
