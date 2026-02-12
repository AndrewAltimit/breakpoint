use std::collections::HashMap;

use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::{BreakpointGame, GameConfig, PlayerId, PlayerInputs};
use breakpoint_core::net::messages::{GameStateMsg, PlayerInputMsg};
use breakpoint_core::net::protocol::{
    decode_client_message, decode_message_type, decode_server_message, encode_client_message,
    encode_server_message,
};
use breakpoint_core::player::PlayerColor;

use breakpoint_golf::course;
use breakpoint_golf::physics::BALL_RADIUS;
use breakpoint_golf::{GolfInput, MiniGolf};

use crate::app::AppState;
use crate::lobby::LobbyState;
use crate::net_client::WsClient;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_game)
            .add_systems(
                Update,
                (
                    game_input_system,
                    game_tick_system,
                    host_broadcast_system,
                    client_receive_system,
                    game_render_sync,
                    aim_line_system,
                    power_bar_system,
                    stroke_counter_system,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_game);
    }
}

/// The active game instance.
#[derive(Resource)]
struct ActiveGame {
    golf: MiniGolf,
    tick: u32,
    tick_accumulator: f32,
}

/// Network role for this client.
#[derive(Resource)]
struct NetworkRole {
    is_host: bool,
    local_player_id: PlayerId,
}

/// Current local input state (built from mouse/keyboard).
#[derive(Resource, Default)]
struct LocalInput {
    aim_angle: f32,
    power: f32,
    stroke_requested: bool,
}

/// Marker for game entities to clean up on exit.
#[derive(Component)]
struct GameEntity;

/// Marker for ball mesh entities, keyed by player id.
#[derive(Component)]
struct BallEntity(PlayerId);

/// Marker for the aim line mesh.
#[derive(Component)]
struct AimLine;

/// Marker for the power bar UI.
#[derive(Component)]
struct PowerBarUi;

/// Marker for the power bar fill.
#[derive(Component)]
struct PowerBarFill;

/// Marker for the stroke counter text.
#[derive(Component)]
struct StrokeCounterText;

/// Tick rate for game simulation (Hz).
const TICK_RATE: f32 = 10.0;
const TICK_INTERVAL: f32 = 1.0 / TICK_RATE;

fn setup_game(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lobby: Res<LobbyState>,
) {
    let mut golf = MiniGolf::new();
    let config = GameConfig {
        round_count: 1,
        round_duration: std::time::Duration::from_secs(90),
        custom: HashMap::new(),
    };
    golf.init(&lobby.players, &config);

    let is_host = lobby.is_host;
    let local_player_id = lobby.local_player_id.unwrap_or(0);

    commands.insert_resource(ActiveGame {
        golf,
        tick: 0,
        tick_accumulator: 0.0,
    });
    commands.insert_resource(NetworkRole {
        is_host,
        local_player_id,
    });
    commands.insert_resource(LocalInput::default());

    let course = course::default_course();

    // Spawn 3D course floor (green plane)
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Plane3d::new(
            Vec3::Y,
            Vec2::new(course.width / 2.0, course.depth / 2.0),
        ))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.6, 0.15),
            ..default()
        })),
        Transform::from_xyz(course.width / 2.0, 0.0, course.depth / 2.0),
    ));

    // Spawn walls as box meshes
    let wall_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.4, 0.25, 0.15),
        ..default()
    });
    for wall in &course.walls {
        let dx = wall.b.x - wall.a.x;
        let dz = wall.b.z - wall.a.z;
        let length = (dx * dx + dz * dz).sqrt();
        let cx = (wall.a.x + wall.b.x) / 2.0;
        let cz = (wall.a.z + wall.b.z) / 2.0;
        let angle = dz.atan2(dx);
        let thickness = 0.3;

        commands.spawn((
            GameEntity,
            Mesh3d(meshes.add(Cuboid::new(length, wall.height, thickness))),
            MeshMaterial3d(wall_mat.clone()),
            Transform::from_xyz(cx, wall.height / 2.0, cz)
                .with_rotation(Quat::from_rotation_y(-angle)),
        ));
    }

    // Spawn bumpers as spheres
    for bumper in &course.bumpers {
        commands.spawn((
            GameEntity,
            Mesh3d(meshes.add(Sphere::new(bumper.radius))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.2, 0.2),
                ..default()
            })),
            Transform::from_xyz(bumper.position.x, bumper.radius, bumper.position.z),
        ));
    }

    // Spawn hole (dark cylinder)
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Cylinder::new(breakpoint_golf::physics::HOLE_RADIUS, 0.05))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.05, 0.05, 0.05),
            ..default()
        })),
        Transform::from_xyz(course.hole_position.x, 0.01, course.hole_position.z),
    ));

    // Spawn ball meshes for each player
    let ball_mesh = meshes.add(Sphere::new(BALL_RADIUS));
    for player in &lobby.players {
        if player.is_spectator {
            continue;
        }
        let color = player_color_to_bevy(&player.color);
        let alpha = if player.id == local_player_id {
            1.0
        } else {
            0.6
        };
        commands.spawn((
            GameEntity,
            BallEntity(player.id),
            Mesh3d(ball_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color.with_alpha(alpha),
                alpha_mode: if alpha < 1.0 {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                },
                ..default()
            })),
            Transform::from_xyz(course.spawn_point.x, BALL_RADIUS, course.spawn_point.z),
        ));
    }

    // Aim line (thin cylinder pointing from ball in aim direction)
    commands.spawn((
        GameEntity,
        AimLine,
        Mesh3d(meshes.add(Cylinder::new(0.05, 3.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 0.3, 0.7),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.15, 0.0),
        Visibility::Hidden,
    ));

    // Power bar UI overlay
    commands
        .spawn((
            GameEntity,
            PowerBarUi,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(20.0),
                left: Val::Percent(50.0),
                width: Val::Px(200.0),
                height: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 0.8)),
        ))
        .with_children(|parent| {
            parent.spawn((
                PowerBarFill,
                Node {
                    width: Val::Percent(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(1.0, 0.5, 0.0)),
            ));
        });

    // Stroke counter
    commands.spawn((
        GameEntity,
        StrokeCounterText,
        Text::new("Strokes: 0"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(10.0),
            ..default()
        },
    ));
}

fn game_input_system(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut local_input: ResMut<LocalInput>,
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_transform)) = cameras.single() else {
        return;
    };

    // Get the local player's ball position
    let ball_pos = active_game
        .golf
        .state()
        .balls
        .get(&network_role.local_player_id)
        .map(|b| Vec3::new(b.position.x, BALL_RADIUS, b.position.z));

    let Some(ball_pos) = ball_pos else {
        return;
    };

    // Raycast from cursor to the ground plane (Y=0)
    if let Ok(ray) = camera.viewport_to_world(cam_transform, cursor_pos)
        && ray.direction.y.abs() > 1e-6
    {
        let t = -ray.origin.y / ray.direction.y;
        let ground_point = ray.origin + ray.direction * t;

        let dx = ground_point.x - ball_pos.x;
        let dz = ground_point.z - ball_pos.z;
        local_input.aim_angle = dz.atan2(dx);
    }

    // Power: hold left mouse button to charge, release to stroke
    if mouse.pressed(MouseButton::Left) {
        local_input.power = (local_input.power + 0.02).min(1.0);
    }
    if mouse.just_released(MouseButton::Left) && local_input.power > 0.01 {
        local_input.stroke_requested = true;
    }
    if !mouse.pressed(MouseButton::Left) && !local_input.stroke_requested {
        local_input.power = 0.0;
    }
}

fn game_tick_system(
    time: Res<Time>,
    mut active_game: ResMut<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut local_input: ResMut<LocalInput>,
    ws_client: NonSend<WsClient>,
) {
    if !network_role.is_host {
        // Non-host: send input if stroke requested
        if local_input.stroke_requested {
            let input = GolfInput {
                aim_angle: local_input.aim_angle,
                power: local_input.power,
                stroke: true,
            };
            if let Ok(input_data) = rmp_serde::to_vec(&input) {
                let msg =
                    breakpoint_core::net::messages::ClientMessage::PlayerInput(PlayerInputMsg {
                        player_id: network_role.local_player_id,
                        tick: active_game.tick,
                        input_data,
                    });
                if let Ok(data) = encode_client_message(&msg) {
                    let _ = ws_client.send(&data);
                }
            }
            local_input.stroke_requested = false;
            local_input.power = 0.0;
        }
        return;
    }

    // Host: apply local input
    if local_input.stroke_requested {
        let input = GolfInput {
            aim_angle: local_input.aim_angle,
            power: local_input.power,
            stroke: true,
        };
        if let Ok(data) = rmp_serde::to_vec(&input) {
            active_game
                .golf
                .apply_input(network_role.local_player_id, &data);
        }
        local_input.stroke_requested = false;
        local_input.power = 0.0;
    }

    // Host: run simulation at tick rate
    active_game.tick_accumulator += time.delta_secs();
    while active_game.tick_accumulator >= TICK_INTERVAL {
        active_game.tick_accumulator -= TICK_INTERVAL;
        active_game.tick += 1;
        let inputs = PlayerInputs {
            inputs: HashMap::new(),
        };
        active_game.golf.update(TICK_INTERVAL, &inputs);
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

    let state_data = active_game.golf.serialize_state();
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
                    active_game.golf.apply_input(pi.player_id, &pi.input_data);
                }
            },
            // Non-host receives GameState
            MessageType::GameState if !network_role.is_host => {
                if let Ok(breakpoint_core::net::messages::ServerMessage::GameState(gs)) =
                    decode_server_message(&data)
                {
                    active_game.golf.apply_state(&gs.state_data);
                    active_game.tick = gs.tick;
                }
            },
            MessageType::RoundEnd | MessageType::GameEnd => {
                next_state.set(AppState::Lobby);
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

fn game_render_sync(
    active_game: Res<ActiveGame>,
    mut ball_query: Query<(&BallEntity, &mut Transform, &mut Visibility)>,
) {
    let state = active_game.golf.state();
    for (ball_entity, mut transform, mut visibility) in &mut ball_query {
        if let Some(ball) = state.balls.get(&ball_entity.0) {
            if ball.is_sunk {
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
                transform.translation.x = ball.position.x;
                transform.translation.y = BALL_RADIUS;
                transform.translation.z = ball.position.z;
            }
        }
    }
}

fn aim_line_system(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    local_input: Res<LocalInput>,
    mut aim_query: Query<(&mut Transform, &mut Visibility), With<AimLine>>,
) {
    let Ok((mut transform, mut visibility)) = aim_query.single_mut() else {
        return;
    };

    let ball = active_game
        .golf
        .state()
        .balls
        .get(&network_role.local_player_id);

    if let Some(ball) = ball
        && !ball.is_sunk
        && ball.is_stopped()
    {
        *visibility = Visibility::Visible;
        let aim_len = 1.5;
        let offset_x = local_input.aim_angle.cos() * aim_len;
        let offset_z = local_input.aim_angle.sin() * aim_len;
        transform.translation =
            Vec3::new(ball.position.x + offset_x, 0.15, ball.position.z + offset_z);
        transform.rotation = Quat::from_rotation_y(-local_input.aim_angle);
    } else {
        *visibility = Visibility::Hidden;
    }
}

fn power_bar_system(
    local_input: Res<LocalInput>,
    mut fill_query: Query<&mut Node, With<PowerBarFill>>,
) {
    if let Ok(mut node) = fill_query.single_mut() {
        node.width = Val::Percent(local_input.power * 100.0);
    }
}

fn stroke_counter_system(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut text_query: Query<&mut Text, With<StrokeCounterText>>,
) {
    if let Ok(mut text) = text_query.single_mut() {
        let strokes = active_game
            .golf
            .state()
            .strokes
            .get(&network_role.local_player_id)
            .copied()
            .unwrap_or(0);
        **text = format!("Strokes: {strokes}");
    }
}

fn player_color_to_bevy(color: &PlayerColor) -> Color {
    Color::srgb(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
    )
}

fn cleanup_game(mut commands: Commands, query: Query<Entity, With<GameEntity>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<ActiveGame>();
    commands.remove_resource::<NetworkRole>();
    commands.remove_resource::<LocalInput>();
}
