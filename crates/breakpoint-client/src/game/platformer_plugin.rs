use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::{GameId, PlayerId};
use breakpoint_platformer::course_gen::Tile;
use breakpoint_platformer::physics::{PLAYER_HEIGHT, PLAYER_WIDTH, PlatformerInput, TILE_SIZE};
use breakpoint_platformer::{PlatformRacer, PlatformerState};

use crate::app::AppState;
use crate::net_client::WsClient;
use crate::theme::{Theme, rgb, rgba};

use super::{
    ActiveGame, ControlsHint, GameEntity, GameRegistry, HudPosition, NetworkRole,
    player_color_to_bevy, read_game_state, send_player_input, spawn_hud_text,
};

pub struct PlatformerPlugin;

impl Plugin for PlatformerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_platformer)
            .add_systems(
                Update,
                (
                    setup_platformer.run_if(platformer_needs_setup),
                    ApplyDeferred,
                    (
                        platformer_input_system,
                        platformer_render_sync,
                        platformer_hud_system,
                    ),
                )
                    .chain()
                    .run_if(in_state(AppState::InGame).and(is_platformer_active)),
            )
            .add_systems(
                OnExit(AppState::InGame),
                cleanup_platformer.run_if(resource_exists::<PlatformerLocalInput>),
            );
    }
}

fn register_platformer(mut registry: ResMut<GameRegistry>) {
    registry.register(GameId::Platformer, || Box::new(PlatformRacer::new()));
}

fn is_platformer_active(game: Option<Res<ActiveGame>>) -> bool {
    game.is_some_and(|g| g.game_id == GameId::Platformer)
}

fn platformer_needs_setup(input: Option<Res<PlatformerLocalInput>>) -> bool {
    input.is_none()
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
    theme: Res<Theme>,
) {
    commands.insert_resource(PlatformerLocalInput::default());

    // Get course from game state
    let state: Option<PlatformerState> = read_game_state(&active_game);

    // We need to create a temporary game to access the course
    let temp_game = PlatformRacer::new();
    let course = temp_game.course();

    // Render course tiles as colored cubes
    let solid_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.solid_tile),
        ..default()
    });
    let platform_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.grass_tile),
        ..default()
    });
    let hazard_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.hazard_tile),
        emissive: LinearRgba::new(2.0, 0.2, 0.1, 1.0),
        ..default()
    });
    let checkpoint_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.platform_tile),
        emissive: LinearRgba::new(0.2, 0.5, 2.0, 1.0),
        ..default()
    });
    let finish_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.finish_tile),
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
    spawn_hud_text(
        &mut commands,
        PlatformerTimerText,
        "Time: 0.0s",
        18.0,
        Color::WHITE,
        HudPosition::TopLeft,
    );
    spawn_hud_text(
        &mut commands,
        PlatformerPositionText,
        "",
        18.0,
        Color::WHITE,
        HudPosition::TopRight,
    );

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(
        &format!(
            "BREAKPOINT:SETUP_PLATFORMER course={}x{} local_pid={}",
            course.width, course.height, local_pid
        )
        .into(),
    );

    // Controls hint (bottom-left, auto-dismiss)
    commands.spawn((
        GameEntity,
        ControlsHint { timer: 8.0 },
        Text::new("A/D or Arrows to move\nSpace to jump\nE for power-up"),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(rgba(&theme.platformer.hud_text)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(60.0),
            left: Val::Px(10.0),
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

    send_player_input(&input, &mut active_game, &network_role, &ws_client);
}

fn platformer_render_sync(
    active_game: Res<ActiveGame>,
    time: Res<Time>,
    mut player_query: Query<(&PlatformerPlayerEntity, &mut Transform, &mut Visibility)>,
) {
    let state: Option<PlatformerState> = read_game_state(&active_game);
    let Some(state) = state else {
        return;
    };

    let lerp_factor = (15.0 * time.delta_secs()).min(1.0);
    for (entity, mut transform, mut visibility) in &mut player_query {
        if let Some(ps) = state.players.get(&entity.0) {
            if ps.eliminated {
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
                let target = Vec3::new(ps.x, ps.y, 0.0);
                transform.translation = transform.translation.lerp(target, lerp_factor);
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
    let state: Option<PlatformerState> = read_game_state(&active_game);
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
