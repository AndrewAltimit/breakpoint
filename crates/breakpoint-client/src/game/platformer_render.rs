use glam::{Vec3, Vec4};

use crate::app::ActiveGame;
use crate::game::read_game_state;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::theme::Theme;

/// Sync the 3D scene with the current platformer game state.
pub fn sync_platformer_scene(scene: &mut Scene, active: &ActiveGame, _theme: &Theme, _dt: f32) {
    let state: Option<breakpoint_platformer::PlatformerState> = read_game_state(active);
    let Some(state) = state else {
        return;
    };

    scene.clear();

    // Render players as colored boxes
    for (pid, player) in &state.players {
        if player.eliminated {
            continue;
        }
        let color = Vec4::new(
            ((*pid * 37) % 255) as f32 / 255.0,
            ((*pid * 73) % 255) as f32 / 255.0,
            ((*pid * 113) % 255) as f32 / 255.0,
            1.0,
        );
        scene.add(
            MeshType::Cuboid,
            MaterialType::Unlit { color },
            Transform::from_xyz(player.x, player.y, 0.0).with_scale(Vec3::new(0.8, 0.8, 0.8)),
        );
    }

    // Render uncollected powerups
    for pu in &state.powerups {
        if pu.collected {
            continue;
        }
        let color = match pu.kind {
            breakpoint_platformer::powerups::PowerUpKind::SpeedBoost => {
                Vec4::new(1.0, 0.8, 0.0, 1.0)
            },
            breakpoint_platformer::powerups::PowerUpKind::DoubleJump => {
                Vec4::new(0.0, 0.8, 1.0, 1.0)
            },
            breakpoint_platformer::powerups::PowerUpKind::Shield => Vec4::new(0.5, 0.5, 1.0, 1.0),
            breakpoint_platformer::powerups::PowerUpKind::Magnet => Vec4::new(1.0, 0.3, 0.3, 1.0),
        };
        scene.add(
            MeshType::Sphere { segments: 8 },
            MaterialType::Glow {
                color,
                intensity: 1.5,
            },
            Transform::from_xyz(pu.x, pu.y, 0.0).with_scale(Vec3::splat(0.4)),
        );
    }
}
