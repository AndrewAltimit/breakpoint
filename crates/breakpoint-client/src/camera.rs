use bevy::prelude::*;

use crate::app::AppState;

pub struct GameCameraPlugin;

impl Plugin for GameCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_camera)
            .add_systems(OnExit(AppState::InGame), cleanup_camera);
    }
}

/// Marker for the in-game 3D camera.
#[derive(Component)]
struct GameCamera;

fn setup_camera(mut commands: Commands) {
    // Angled top-down view: looking at course center from above and slightly south.
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

fn cleanup_camera(mut commands: Commands, query: Query<Entity, With<GameCamera>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
