use bevy::prelude::*;

use crate::app::AppState;
use crate::game::ActiveGame;

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

/// Marker to track cleanup needed when returning to lobby.
#[derive(Resource)]
struct CameraPendingCleanup;

fn setup_camera(mut commands: Commands) {
    // Default: angled top-down view for golf.
    // Course is 20x30 on XZ, centered around (10, 0, 15).
    commands.spawn((
        GameCamera,
        Camera3d::default(),
        Transform::from_xyz(10.0, 30.0, -5.0).looking_at(Vec3::new(10.0, 0.0, 15.0), Vec3::Y),
    ));

    // Directional light (sun-like)
    commands.spawn((
        GameCamera, // reuse marker for cleanup
        DirectionalLight {
            illuminance: 10000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.3, 0.0)),
    ));
}

fn update_camera(
    game: Option<Res<ActiveGame>>,
    mut camera_query: Query<&mut Transform, With<GameCamera>>,
) {
    let Some(game) = game else {
        return;
    };

    // Per-game camera configuration
    match game.game_id.as_str() {
        "platform-racer" => {
            // Side-view orthographic-like: looking along Z at XY plane
            // Course extends right along X, camera follows center
            for mut transform in &mut camera_query {
                if transform.translation.y > 5.0 {
                    // Only adjust the main camera, not the light
                    *transform = Transform::from_xyz(50.0, 15.0, -30.0)
                        .looking_at(Vec3::new(50.0, 10.0, 0.0), Vec3::Y);
                }
            }
        },
        "laser-tag" => {
            // Top-down view: looking down Y at XZ plane
            for mut transform in &mut camera_query {
                if transform.translation.y > 5.0 {
                    *transform = Transform::from_xyz(25.0, 40.0, 25.0)
                        .looking_at(Vec3::new(25.0, 0.0, 25.0), Vec3::Z);
                }
            }
        },
        _ => {
            // Golf: keep default camera
        },
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
