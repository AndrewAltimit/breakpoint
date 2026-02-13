use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

use crate::app::AppState;
use crate::game::ActiveGame;

#[cfg(feature = "golf")]
use crate::game::golf_plugin::GolfCourseInfo;

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
#[derive(Component)]
struct GameLight;

/// Marker to track cleanup needed when returning to lobby.
#[derive(Resource)]
struct CameraPendingCleanup;

fn setup_camera(mut commands: Commands) {
    // Sky-blue clear color
    // Tonemapping::None is required for WebGL2 — the default TonyMcMapface
    // uses a 3D LUT texture that fails silently, causing a magenta screen.
    commands.spawn((
        GameCamera,
        Camera3d::default(),
        Msaa::Off,
        Tonemapping::None,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.53, 0.81, 0.98)),
            ..default()
        },
        // Default position; will be overridden by update_camera for golf
        Transform::from_xyz(10.0, 30.0, -5.0).looking_at(Vec3::new(10.0, 0.0, 15.0), Vec3::Y),
    ));

    // Ambient light for softer shadows
    commands.spawn((
        GameCamera,
        AmbientLight {
            brightness: 300.0,
            ..default()
        },
    ));

    // Directional light (sun-like, better angle)
    commands.spawn((
        GameCamera,
        GameLight,
        DirectionalLight {
            illuminance: 8000.0,
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
    #[cfg(feature = "golf")] course_info: Option<Res<GolfCourseInfo>>,
) {
    let Some(game) = game else {
        return;
    };

    match game.game_id.as_str() {
        "platform-racer" => {
            for mut transform in &mut camera_query {
                *transform = Transform::from_xyz(50.0, 15.0, -30.0)
                    .looking_at(Vec3::new(50.0, 10.0, 0.0), Vec3::Y);
            }
        },
        "laser-tag" => {
            for mut transform in &mut camera_query {
                *transform = Transform::from_xyz(25.0, 40.0, 25.0)
                    .looking_at(Vec3::new(25.0, 0.0, 25.0), Vec3::Z);
            }
        },
        #[cfg(feature = "golf")]
        "mini-golf" => {
            if let Some(ref info) = course_info {
                let cx = info.width / 2.0;
                let cz = info.depth / 2.0;
                // Camera height proportional to course size
                let h = info.width.max(info.depth) * 1.1;
                // Slight angle offset so it's not perfectly top-down
                let offset_z = -info.depth * 0.15;
                for mut transform in &mut camera_query {
                    *transform = Transform::from_xyz(cx, h, cz + offset_z)
                        .looking_at(Vec3::new(cx, 0.0, cz), Vec3::Y);
                }
            }
        },
        _ => {},
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
