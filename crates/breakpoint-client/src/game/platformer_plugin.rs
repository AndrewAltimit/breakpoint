use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::PlayerId;
use breakpoint_core::net::messages::PlayerInputMsg;
use breakpoint_core::net::protocol::encode_client_message;
use breakpoint_core::player::PlayerColor;

use breakpoint_platformer::course_gen::Tile;
use breakpoint_platformer::physics::{PLAYER_HEIGHT, PLAYER_WIDTH, PlatformerInput, TILE_SIZE};
use breakpoint_platformer::{PlatformRacer, PlatformerState};

use crate::app::AppState;
use crate::net_client::WsClient;

use super::{ActiveGame, GameEntity, GameRegistry, NetworkRole};

pub struct PlatformerPlugin;

impl Plugin for PlatformerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_platformer)
            .add_systems(
                OnEnter(AppState::InGame),
                setup_platformer.run_if(is_platformer_active),
            )
            .add_systems(
                Update,
                (
                    platformer_input_system,
                    platformer_render_sync,
                    platformer_hud_system,
                )
                    .run_if(in_state(AppState::InGame).and(is_platformer_active)),
            )
            .add_systems(
                OnExit(AppState::InGame),
                cleanup_platformer.run_if(resource_exists::<PlatformerLocalInput>),
            );
    }
}

fn register_platformer(mut registry: ResMut<GameRegistry>) {
    registry.register("platform-racer", || Box::new(PlatformRacer::new()));
}

fn is_platformer_active(game: Option<Res<ActiveGame>>) -> bool {
    game.is_some_and(|g| g.game_id == "platform-racer")
}

/// Local input state for platformer.
#[derive(Resource, Default)]
struct PlatformerLocalInput {
    move_dir: f32,
    jump: bool,
}

/// Marker for player mesh entities.
#[derive(Component)]
struct PlatformerPlayerEntity(PlayerId);

/// Marker for course tile entities.
#[derive(Component)]
struct CourseTileEntity;

/// Marker for HUD timer text.
#[derive(Component)]
struct PlatformerTimerText;

/// Marker for position indicator text.
#[derive(Component)]
struct PlatformerPositionText;

fn setup_platformer(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lobby: Res<crate::lobby::LobbyState>,
    network_role: Res<NetworkRole>,
    active_game: Res<ActiveGame>,
) {
    commands.insert_resource(PlatformerLocalInput::default());

    // Get course from game state
    let state: Option<PlatformerState> =
        rmp_serde::from_slice(&active_game.game.serialize_state()).ok();

    // We need to create a temporary game to access the course
    let temp_game = PlatformRacer::new();
    let course = temp_game.course();

    // Render course tiles as colored cubes
    let solid_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.4, 0.4, 0.5),
        ..default()
    });
    let platform_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.6, 0.3),
        ..default()
    });
    let hazard_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.2, 0.1),
        emissive: LinearRgba::new(2.0, 0.2, 0.1, 1.0),
        ..default()
    });
    let checkpoint_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.5, 0.9),
        emissive: LinearRgba::new(0.2, 0.5, 2.0, 1.0),
        ..default()
    });
    let finish_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.85, 0.1),
        emissive: LinearRgba::new(2.0, 1.7, 0.2, 1.0),
        ..default()
    });

    let tile_mesh = meshes.add(Cuboid::new(TILE_SIZE, TILE_SIZE, TILE_SIZE));

    for y in 0..course.height {
        for x in 0..course.width {
            let tile = course.get_tile(x as i32, y as i32);
            let mat = match tile {
                Tile::Solid => Some(solid_mat.clone()),
                Tile::Platform => Some(platform_mat.clone()),
                Tile::Hazard => Some(hazard_mat.clone()),
                Tile::Checkpoint => Some(checkpoint_mat.clone()),
                Tile::Finish => Some(finish_mat.clone()),
                _ => None,
            };

            if let Some(mat) = mat {
                // Platformer uses XY plane for rendering (side view),
                // map tile grid to 3D: tile_x -> X, tile_y -> Y, depth -> Z=0
                let wx = x as f32 * TILE_SIZE + TILE_SIZE / 2.0;
                let wy = y as f32 * TILE_SIZE + TILE_SIZE / 2.0;

                commands.spawn((
                    GameEntity,
                    CourseTileEntity,
                    Mesh3d(tile_mesh.clone()),
                    MeshMaterial3d(mat),
                    Transform::from_xyz(wx, wy, 0.0),
                ));
            }
        }
    }

    // Spawn player meshes
    let player_mesh = meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_WIDTH));
    let local_pid = network_role.local_player_id;
    for player in &lobby.players {
        if player.is_spectator {
            continue;
        }
        let color = player_color_to_bevy(&player.color);
        let alpha = if player.id == local_pid { 1.0 } else { 0.6 };
        let spawn_y = state
            .as_ref()
            .and_then(|s| s.players.get(&player.id))
            .map(|p| p.y)
            .unwrap_or(3.0);
        let spawn_x = state
            .as_ref()
            .and_then(|s| s.players.get(&player.id))
            .map(|p| p.x)
            .unwrap_or(2.0);

        commands.spawn((
            GameEntity,
            PlatformerPlayerEntity(player.id),
            Mesh3d(player_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color.with_alpha(alpha),
                alpha_mode: if alpha < 1.0 {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                },
                ..default()
            })),
            Transform::from_xyz(spawn_x, spawn_y, 0.0),
        ));
    }

    // HUD
    commands.spawn((
        GameEntity,
        PlatformerTimerText,
        Text::new("Time: 0.0s"),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));

    commands.spawn((
        GameEntity,
        PlatformerPositionText,
        Text::new(""),
        TextFont {
            font_size: 18.0,
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

fn platformer_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut local_input: ResMut<PlatformerLocalInput>,
    mut active_game: ResMut<ActiveGame>,
    network_role: Res<NetworkRole>,
    ws_client: NonSend<WsClient>,
    mut audio_queue: ResMut<crate::audio::AudioEventQueue>,
) {
    if network_role.is_spectator {
        return;
    }

    let mut move_dir = 0.0;
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        move_dir -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        move_dir += 1.0;
    }
    local_input.move_dir = move_dir;
    local_input.jump =
        keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::ArrowUp);

    if local_input.jump {
        audio_queue.push(crate::audio::AudioEvent::PlatformerJump);
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        audio_queue.push(crate::audio::AudioEvent::PlatformerPowerUp);
    }

    // Build and send/apply input
    let input = PlatformerInput {
        move_dir: local_input.move_dir,
        jump: local_input.jump,
        use_powerup: keyboard.just_pressed(KeyCode::KeyE),
    };

    if let Ok(data) = rmp_serde::to_vec(&input) {
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

fn platformer_render_sync(
    active_game: Res<ActiveGame>,
    mut player_query: Query<(&PlatformerPlayerEntity, &mut Transform, &mut Visibility)>,
) {
    let state: Option<PlatformerState> =
        rmp_serde::from_slice(&active_game.game.serialize_state()).ok();
    let Some(state) = state else {
        return;
    };

    for (entity, mut transform, mut visibility) in &mut player_query {
        if let Some(ps) = state.players.get(&entity.0) {
            if ps.eliminated {
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
                transform.translation.x = ps.x;
                transform.translation.y = ps.y;
                transform.translation.z = 0.0;
            }
        }
    }
}

fn platformer_hud_system(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut timer_text: Query<&mut Text, With<PlatformerTimerText>>,
    mut pos_text: Query<&mut Text, (With<PlatformerPositionText>, Without<PlatformerTimerText>)>,
) {
    let state: Option<PlatformerState> =
        rmp_serde::from_slice(&active_game.game.serialize_state()).ok();
    let Some(state) = state else {
        return;
    };

    if let Ok(mut text) = timer_text.single_mut() {
        **text = format!("Time: {:.1}s", state.round_timer);
    }

    if let Ok(mut text) = pos_text.single_mut()
        && let Some(player) = state.players.get(&network_role.local_player_id)
    {
        if player.finished {
            let pos = state
                .finish_order
                .iter()
                .position(|&id| id == network_role.local_player_id)
                .map(|i| i + 1)
                .unwrap_or(0);
            **text = format!("Finished #{pos}!");
        } else {
            **text = format!("X: {:.0}", player.x);
        }
    }
}

fn cleanup_platformer(mut commands: Commands) {
    commands.remove_resource::<PlatformerLocalInput>();
}

fn player_color_to_bevy(color: &PlayerColor) -> Color {
    Color::srgb(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
    )
}
