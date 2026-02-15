use glam::{Vec3, Vec4};

use crate::app::ActiveGame;
use crate::game::read_game_state;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::theme::{Theme, rgb_vec4};

/// Sync the 3D scene with the current laser tag game state.
pub fn sync_lasertag_scene(scene: &mut Scene, active: &ActiveGame, theme: &Theme, _dt: f32) {
    let state: Option<breakpoint_lasertag::LaserTagState> = read_game_state(active);
    let Some(state) = state else {
        return;
    };

    scene.clear();

    // Use a default arena size for rendering (arena geometry is server-side)
    let arena_size = 50.0_f32;

    // Arena floor
    scene.add(
        MeshType::Plane,
        MaterialType::Unlit {
            color: rgb_vec4(&theme.lasertag.arena_floor),
        },
        Transform::from_xyz(arena_size / 2.0, 0.0, arena_size / 2.0)
            .with_scale(Vec3::new(arena_size, 1.0, arena_size)),
    );

    // Players as cylinders
    for player in state.players.values() {
        // Stunned players rendered dimmer
        let alpha = if player.is_stunned() { 0.4 } else { 1.0 };
        let color = Vec4::new(0.3, 0.7, 0.9, alpha);
        scene.add(
            MeshType::Cylinder { segments: 12 },
            MaterialType::Unlit { color },
            Transform::from_xyz(player.x, 0.75, player.z).with_scale(Vec3::new(0.5, 1.5, 0.5)),
        );
    }

    // Laser trails
    for trail in &state.laser_trails {
        if trail.age > 0.3 {
            continue;
        }
        let alpha = 1.0 - trail.age / 0.3;
        let color = Vec4::new(1.0, 0.2, 0.2, alpha);

        for &(start_x, start_z, end_x, end_z) in &trail.segments {
            let dx = end_x - start_x;
            let dz = end_z - start_z;
            let len = (dx * dx + dz * dz).sqrt();
            if len < 0.01 {
                continue;
            }
            let cx = (start_x + end_x) / 2.0;
            let cz = (start_z + end_z) / 2.0;
            let angle = dz.atan2(dx);
            scene.add(
                MeshType::Cuboid,
                MaterialType::Glow {
                    color,
                    intensity: 2.0,
                },
                Transform::from_xyz(cx, 0.75, cz)
                    .with_scale(Vec3::new(len, 0.05, 0.05))
                    .with_rotation(glam::Quat::from_rotation_y(-angle)),
            );
        }
    }
}
