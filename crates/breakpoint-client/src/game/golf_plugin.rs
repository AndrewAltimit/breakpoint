use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::PlayerId;
use breakpoint_core::net::messages::PlayerInputMsg;
use breakpoint_core::net::protocol::encode_client_message;
use breakpoint_core::player::PlayerColor;

use breakpoint_golf::course;
use breakpoint_golf::physics::BALL_RADIUS;
use breakpoint_golf::{GolfInput, GolfState, MiniGolf};

use crate::app::AppState;
use crate::net_client::WsClient;

use super::{ActiveGame, GameEntity, GameRegistry, NetworkRole};

pub struct GolfPlugin;

impl Plugin for GolfPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_golf)
            .add_systems(OnEnter(AppState::InGame), setup_golf.run_if(is_golf_active))
            .add_systems(
                Update,
                (
                    golf_input_system,
                    golf_apply_input_system,
                    golf_render_sync,
                    aim_line_system,
                    power_bar_system,
                    stroke_counter_system,
                )
                    .run_if(in_state(AppState::InGame).and(is_golf_active)),
            )
            .add_systems(
                OnExit(AppState::InGame),
                cleanup_golf.run_if(resource_exists::<GolfLocalInput>),
            );
    }
}

fn register_golf(mut registry: ResMut<GameRegistry>) {
    registry.register("mini-golf", || Box::new(MiniGolf::new()));
}

fn is_golf_active(game: Option<Res<ActiveGame>>) -> bool {
    game.is_some_and(|g| g.game_id == "mini-golf")
}

/// Current local input state for golf (built from mouse/keyboard).
#[derive(Resource, Default)]
struct GolfLocalInput {
    aim_angle: f32,
    power: f32,
    stroke_requested: bool,
}

/// Marker for ball mesh entities, keyed by player id.
#[derive(Component)]
struct BallEntity(PlayerId);

/// Marker for the aim line mesh.
#[derive(Component)]
struct AimLine;

/// Marker for the power bar fill.
#[derive(Component)]
struct PowerBarFill;

/// Marker for the stroke counter text.
#[derive(Component)]
struct StrokeCounterText;

fn setup_golf(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lobby: Res<crate::lobby::LobbyState>,
    network_role: Res<NetworkRole>,
) {
    commands.insert_resource(GolfLocalInput::default());

    let course_data = course::default_course();

    // Spawn 3D course floor (green plane)
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Plane3d::new(
            Vec3::Y,
            Vec2::new(course_data.width / 2.0, course_data.depth / 2.0),
        ))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.6, 0.15),
            ..default()
        })),
        Transform::from_xyz(course_data.width / 2.0, 0.0, course_data.depth / 2.0),
    ));

    // Spawn walls as box meshes
    let wall_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.4, 0.25, 0.15),
        ..default()
    });
    for wall in &course_data.walls {
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
    for bumper in &course_data.bumpers {
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
        Transform::from_xyz(
            course_data.hole_position.x,
            0.01,
            course_data.hole_position.z,
        ),
    ));

    // Spawn ball meshes for each player
    let ball_mesh = meshes.add(Sphere::new(BALL_RADIUS));
    let local_player_id = network_role.local_player_id;
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
            Transform::from_xyz(
                course_data.spawn_point.x,
                BALL_RADIUS,
                course_data.spawn_point.z,
            ),
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

/// Gather mouse input and populate GolfLocalInput (no network or game mutation).
fn golf_input_system(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut local_input: ResMut<GolfLocalInput>,
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if network_role.is_spectator {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_transform)) = cameras.single() else {
        return;
    };

    // Deserialize current golf state to get ball position
    let state: Option<GolfState> = rmp_serde::from_slice(&active_game.game.serialize_state()).ok();
    let ball_pos = state.as_ref().and_then(|s| {
        s.balls
            .get(&network_role.local_player_id)
            .map(|b| Vec3::new(b.position.x, BALL_RADIUS, b.position.z))
    });

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

/// Apply golf input: host applies directly, non-host sends via WS.
fn golf_apply_input_system(
    mut local_input: ResMut<GolfLocalInput>,
    mut active_game: ResMut<ActiveGame>,
    network_role: Res<NetworkRole>,
    ws_client: NonSend<WsClient>,
    mut audio_queue: ResMut<crate::audio::AudioEventQueue>,
) {
    if !local_input.stroke_requested || network_role.is_spectator {
        return;
    }

    let input = GolfInput {
        aim_angle: local_input.aim_angle,
        power: local_input.power,
        stroke: true,
    };

    audio_queue.push(crate::audio::AudioEvent::GolfStroke);

    if network_role.is_host {
        // Host: apply input directly to the authoritative game
        if let Ok(data) = rmp_serde::to_vec(&input) {
            active_game
                .game
                .apply_input(network_role.local_player_id, &data);
        }
    } else {
        // Non-host: send input to server for relay to host
        if let Ok(data) = rmp_serde::to_vec(&input) {
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

    local_input.stroke_requested = false;
    local_input.power = 0.0;
}

fn golf_render_sync(
    active_game: Res<ActiveGame>,
    mut ball_query: Query<(&BallEntity, &mut Transform, &mut Visibility)>,
) {
    let state: Option<GolfState> = rmp_serde::from_slice(&active_game.game.serialize_state()).ok();
    let Some(state) = state else {
        return;
    };
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
    local_input: Res<GolfLocalInput>,
    mut aim_query: Query<(&mut Transform, &mut Visibility), With<AimLine>>,
) {
    let Ok((mut transform, mut visibility)) = aim_query.single_mut() else {
        return;
    };

    let state: Option<GolfState> = rmp_serde::from_slice(&active_game.game.serialize_state()).ok();
    let ball = state
        .as_ref()
        .and_then(|s| s.balls.get(&network_role.local_player_id));

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
    local_input: Res<GolfLocalInput>,
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
        let state: Option<GolfState> =
            rmp_serde::from_slice(&active_game.game.serialize_state()).ok();
        let strokes = state
            .and_then(|s| s.strokes.get(&network_role.local_player_id).copied())
            .unwrap_or(0);
        **text = format!("Strokes: {strokes}");
    }
}

fn cleanup_golf(mut commands: Commands) {
    commands.remove_resource::<GolfLocalInput>();
}

fn player_color_to_bevy(color: &PlayerColor) -> Color {
    Color::srgb(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
    )
}
