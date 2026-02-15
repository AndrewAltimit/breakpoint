use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

use breakpoint_core::game_trait::GameId;

use crate::app::AppState;
use crate::effects::screen_shake::ScreenShake;
use crate::game::ActiveGame;
use crate::theme::{Theme, rgb};

#[cfg(feature = "golf")]
use crate::game::golf_plugin::GolfCourseInfo;
// NetworkRole and read_game_state needed for golf ball-follow and platformer player-follow cameras
#[cfg(any(feature = "golf", feature = "platformer"))]
use crate::game::{NetworkRole, read_game_state};

pub struct GameCameraPlugin;

impl Plugin for GameCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_camera)
            .add_systems(
                Update,
                update_camera.run_if(
                    in_state(AppState::InGame)
                        .or(in_state(AppState::BetweenRounds))
                        .or(in_state(AppState::GameOver)),
                ),
            )
            .add_systems(OnExit(AppState::InGame), mark_camera_pending_cleanup)
            .add_systems(OnEnter(AppState::Lobby), cleanup_camera);
    }
}

/// Marker for the in-game 3D camera.
#[derive(Component)]
pub struct GameCamera;

/// Marker to distinguish the light entity from the camera entity.
/// Public so game plugins can exclude it from camera queries.
#[derive(Component)]
pub struct GameLight;

/// Marker to track cleanup needed when returning to lobby.
#[derive(Resource)]
struct CameraPendingCleanup;

fn setup_camera(mut commands: Commands, theme: Res<Theme>) {
    // Sky-blue clear color
    // Tonemapping::None is required for WebGL2 — the default TonyMcMapface
    // uses a 3D LUT texture that fails silently, causing a magenta screen.
    commands.spawn((
        GameCamera,
        Camera3d::default(),
        Msaa::Off,
        Tonemapping::None,
        Camera {
            clear_color: ClearColorConfig::Custom(rgb(&theme.camera.clear_color)),
            ..default()
        },
        // Default position; will be overridden by update_camera for golf
        Transform::from_xyz(10.0, 30.0, -5.0).looking_at(Vec3::new(10.0, 0.0, 15.0), Vec3::Y),
    ));

    // Ambient light for softer shadows
    commands.spawn((
        GameCamera,
        AmbientLight {
            brightness: theme.camera.ambient_brightness,
            ..default()
        },
    ));

    // Directional light (sun-like, better angle)
    commands.spawn((
        GameCamera,
        GameLight,
        DirectionalLight {
            illuminance: theme.camera.directional_illuminance,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.0, 0.4, 0.0)),
    ));
}

#[allow(clippy::too_many_arguments)]
fn update_camera(
    game: Option<Res<ActiveGame>>,
    mut camera_query: Query<&mut Transform, (With<GameCamera>, Without<GameLight>)>,
    screen_shake: Option<Res<ScreenShake>>,
    #[cfg(feature = "golf")] course_info: Option<Res<GolfCourseInfo>>,
    #[cfg(any(feature = "golf", feature = "platformer"))] network_role: Option<Res<NetworkRole>>,
    #[cfg(any(feature = "golf", feature = "platformer"))] time: Res<Time>,
) {
    let Some(game) = game else {
        return;
    };

    match game.game_id {
        #[cfg(feature = "platformer")]
        GameId::Platformer => {
            // Follow the local player (side-view: XY plane at Z=0, camera at Z<0).
            let player_pos = network_role.as_ref().and_then(|role| {
                let state: Option<breakpoint_platformer::PlatformerState> = read_game_state(&game);
                state.and_then(|s| {
                    s.players
                        .get(&role.local_player_id)
                        .map(|p| Vec2::new(p.x, p.y))
                })
            });

            if let Some(pos) = player_pos {
                let camera_z = -25.0;
                let look_y_offset = 3.0;
                let target = Vec3::new(pos.x, pos.y + look_y_offset, camera_z);
                let look_at = Vec3::new(pos.x, pos.y + look_y_offset, 0.0);

                let lerp_factor = (5.0 * time.delta_secs()).min(1.0);
                for mut transform in &mut camera_query {
                    transform.translation = transform.translation.lerp(target, lerp_factor);
                    *transform = transform.looking_at(look_at, Vec3::Y);
                }
            } else {
                // Fallback: fixed overview while state isn't ready
                for mut transform in &mut camera_query {
                    *transform = Transform::from_xyz(10.0, 10.0, -25.0)
                        .looking_at(Vec3::new(10.0, 10.0, 0.0), Vec3::Y);
                }
            }
        },
        #[cfg(not(feature = "platformer"))]
        GameId::Platformer => {},
        GameId::LaserTag => {
            // Height 62 gives a full view of the 50x50 arena with margin.
            // At 45° FOV from height 62: visible Z range = -0.7 to 50.7.
            for mut transform in &mut camera_query {
                *transform = Transform::from_xyz(25.0, 62.0, 25.0)
                    .looking_at(Vec3::new(25.0, 0.0, 25.0), Vec3::Z);
            }
        },
        #[cfg(feature = "golf")]
        GameId::Golf => {
            // Try to follow the local player's ball for a close-up view.
            // Falls back to course-center overview if ball position unavailable.
            let ball_pos = network_role.as_ref().and_then(|role| {
                let state: Option<breakpoint_golf::GolfState> = read_game_state(&game);
                state.and_then(|s| {
                    s.balls
                        .get(&role.local_player_id)
                        .map(|b| Vec3::new(b.position.x, 0.0, b.position.z))
                })
            });

            if let Some(ball_xz) = ball_pos {
                let camera_height = 15.0;
                let offset_z = -2.0; // Slight offset for perspective feel
                let target = Vec3::new(ball_xz.x, camera_height, ball_xz.z + offset_z);
                let look_target = Vec3::new(ball_xz.x, 0.0, ball_xz.z);

                let lerp_factor = (5.0 * time.delta_secs()).min(1.0);
                for mut transform in &mut camera_query {
                    transform.translation = transform.translation.lerp(target, lerp_factor);
                    *transform = transform.looking_at(look_target, Vec3::Y);
                }
            } else if let Some(ref info) = course_info {
                let cx = info.width / 2.0;
                let cz = info.depth / 2.0;
                let h = info.width.max(info.depth) * 1.1;
                let offset_z = -info.depth * 0.15;
                for mut transform in &mut camera_query {
                    *transform = Transform::from_xyz(cx, h, cz + offset_z)
                        .looking_at(Vec3::new(cx, 0.0, cz), Vec3::Y);
                }
            }
        },
        #[cfg(not(feature = "golf"))]
        GameId::Golf => {},
    }

    // Apply screen shake offset after all camera positioning
    if let Some(ref shake) = screen_shake
        && shake.timer > 0.0
    {
        for mut transform in &mut camera_query {
            transform.translation += shake.offset;
        }
    }
}

fn mark_camera_pending_cleanup(mut commands: Commands) {
    commands.insert_resource(CameraPendingCleanup);
}

fn cleanup_camera(
    mut commands: Commands,
    query: Query<Entity, With<GameCamera>>,
    pending: Option<Res<CameraPendingCleanup>>,
) {
    if pending.is_some() {
        for entity in &query {
            commands.entity(entity).despawn();
        }
        commands.remove_resource::<CameraPendingCleanup>();
    }
}
