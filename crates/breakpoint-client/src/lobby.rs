use bevy::ecs::system::NonSend;
use bevy::ecs::system::NonSendMut;
use bevy::prelude::*;

use breakpoint_core::game_trait::PlayerId;
use breakpoint_core::net::messages::{
    ClientMessage, GameStartMsg, JoinRoomMsg, JoinRoomResponseMsg, PlayerListMsg,
};
use breakpoint_core::net::protocol::{decode_server_message, encode_client_message};
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
                (lobby_input_system, lobby_network_system).run_if(in_state(AppState::Lobby)),
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
    pub selected_game: String,
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

type RoomCodeFilter = (
    With<RoomCodeText>,
    Without<PlayerListText>,
    Without<StatusText>,
);
type PlayerListFilter = (
    With<PlayerListText>,
    Without<RoomCodeText>,
    Without<StatusText>,
);
type StatusFilter = (
    With<StatusText>,
    Without<RoomCodeText>,
    Without<PlayerListText>,
);

#[derive(Component)]
enum LobbyButton {
    Create,
    Join,
    StartGame,
    OpenEditor,
}

#[derive(Component)]
struct GameSelectButton(String);

#[derive(Component)]
struct GameSelectionText;

fn setup_lobby(mut commands: Commands, mut lobby: ResMut<LobbyState>) {
    // Spawn a 2D camera for UI rendering.
    // Msaa::Off is required for WebGL2 compatibility.
    commands.spawn((LobbyCamera, Camera2d, Msaa::Off));

    if lobby.player_name.is_empty() {
        lobby.player_name = format!("Player{}", fastrand::u16(..1000));
    }
    if lobby.selected_game.is_empty() {
        lobby.selected_game = "mini-golf".to_string();
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
                    spawn_game_button(row, "Mini-Golf", "mini-golf", btn_color);
                    spawn_game_button(row, "Platform Racer", "platform-racer", btn_color);
                    spawn_game_button(row, "Laser Tag", "laser-tag", btn_color);
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

            // Buttons
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(16.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_button(row, "Create Room", LobbyButton::Create, btn_color);
                    spawn_button(row, "Join Room", LobbyButton::Join, btn_color);
                    spawn_button(
                        row,
                        "Editor",
                        LobbyButton::OpenEditor,
                        Color::srgb(0.5, 0.3, 0.6),
                    );
                });

            // Start Game button (hidden initially)
            parent
                .spawn((
                    LobbyButton::StartGame,
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

            // Status
            parent.spawn((
                StatusText,
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.4, 0.4)),
            ));
        });
}

fn spawn_game_button(parent: &mut ChildSpawnerCommands, label: &str, game_id: &str, color: Color) {
    parent
        .spawn((
            GameSelectButton(game_id.to_string()),
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

#[allow(clippy::too_many_arguments)]
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
            lobby.selected_game = btn.0.clone();
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
                if !ws_client.is_connected() {
                    let url = lobby.ws_url.clone();
                    if let Err(e) = ws_client.connect(&url) {
                        lobby.error_message = Some(format!("Connection failed: {e}"));
                        continue;
                    }
                }
                lobby.is_host = true;
                let color = PlayerColor::PALETTE[lobby.color_index % PlayerColor::PALETTE.len()];
                let msg = ClientMessage::JoinRoom(JoinRoomMsg {
                    room_code: String::new(),
                    player_name: lobby.player_name.clone(),
                    player_color: color,
                });
                if let Ok(data) = encode_client_message(&msg) {
                    let _ = ws_client.send(&data);
                }
            },
            LobbyButton::Join => {
                if lobby.room_code.is_empty() {
                    lobby.error_message = Some("Enter a room code first".to_string());
                    continue;
                }
                if !ws_client.is_connected() {
                    let url = lobby.ws_url.clone();
                    if let Err(e) = ws_client.connect(&url) {
                        lobby.error_message = Some(format!("Connection failed: {e}"));
                        continue;
                    }
                }
                lobby.is_host = false;
                let color = PlayerColor::PALETTE[lobby.color_index % PlayerColor::PALETTE.len()];
                let msg = ClientMessage::JoinRoom(JoinRoomMsg {
                    room_code: lobby.room_code.clone(),
                    player_name: lobby.player_name.clone(),
                    player_color: color,
                });
                if let Ok(data) = encode_client_message(&msg) {
                    let _ = ws_client.send(&data);
                }
            },
            LobbyButton::StartGame => {
                if lobby.is_host {
                    let msg =
                        breakpoint_core::net::messages::ServerMessage::GameStart(GameStartMsg {
                            game_name: lobby.selected_game.clone(),
                            players: lobby.players.clone(),
                            host_id: lobby.local_player_id.unwrap_or(0),
                        });
                    if let Ok(data) = breakpoint_core::net::protocol::encode_server_message(&msg) {
                        let _ = ws_client.send(&data);
                    }
                }
            },
            LobbyButton::OpenEditor => {
                next_state.set(AppState::Editor);
            },
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lobby_network_system(
    ws_client: NonSend<WsClient>,
    mut lobby: ResMut<LobbyState>,
    mut room_code_text: Query<&mut Text, RoomCodeFilter>,
    mut player_list_text: Query<&mut Text, PlayerListFilter>,
    mut status_text: Query<&mut Text, StatusFilter>,
    mut start_btn_vis: Query<&mut Visibility, With<LobbyButton>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut overlay_queue: ResMut<crate::overlay::OverlayEventQueue>,
    mut overlay_state: ResMut<crate::overlay::OverlayState>,
) {
    let messages = ws_client.drain_messages();
    for data in messages {
        // Try decoding as server message first
        let msg = match decode_server_message(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };

        match msg {
            breakpoint_core::net::messages::ServerMessage::JoinRoomResponse(resp) => {
                handle_join_response(&resp, &mut lobby);
                if resp.success {
                    overlay_state.local_player_id = resp.player_id;

                    // Late-join: if room is already in-game, mark as spectator
                    // and transition directly to InGame
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
                if !resp.success
                    && let Ok(mut text) = status_text.single_mut()
                {
                    **text = resp.error.unwrap_or_default();
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
                if lobby.is_host && lobby.players.len() >= 2 {
                    for mut vis in &mut start_btn_vis {
                        *vis = Visibility::Visible;
                    }
                }
            },
            breakpoint_core::net::messages::ServerMessage::GameStart(gs) => {
                lobby.selected_game = gs.game_name;
                next_state.set(AppState::InGame);
            },
            // Forward alert messages to the overlay
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
