use glam::{Vec3, Vec4};

use crate::app::ActiveGame;
use crate::game::read_game_state;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::theme::{Theme, rgb_vec4};

/// Sync the 3D scene with the current platformer game state.
pub fn sync_platformer_scene(scene: &mut Scene, active: &ActiveGame, theme: &Theme, _dt: f32) {
    let state: Option<breakpoint_platformer::PlatformerState> = read_game_state(active);
    let Some(state) = state else {
        return;
    };

    scene.clear();

    let tile_size = breakpoint_platformer::physics::TILE_SIZE;

    // Render course tiles
    for y in 0..state.course.height {
        for x in 0..state.course.width {
            let tile = state.course.get_tile(x as i32, y as i32);
            let color = match tile {
                breakpoint_platformer::course_gen::Tile::Empty => continue,
                breakpoint_platformer::course_gen::Tile::PowerUpSpawn => continue,
                breakpoint_platformer::course_gen::Tile::Solid => {
                    rgb_vec4(&theme.platformer.solid_tile)
                },
                breakpoint_platformer::course_gen::Tile::Platform => {
                    rgb_vec4(&theme.platformer.platform_tile)
                },
                breakpoint_platformer::course_gen::Tile::Hazard => {
                    rgb_vec4(&theme.platformer.hazard_tile)
                },
                breakpoint_platformer::course_gen::Tile::Checkpoint => {
                    Vec4::new(0.2, 0.8, 0.2, 1.0)
                },
                breakpoint_platformer::course_gen::Tile::Finish => {
                    rgb_vec4(&theme.platformer.finish_tile)
                },
            };
            let wx = x as f32 * tile_size + tile_size / 2.0;
            let wy = y as f32 * tile_size + tile_size / 2.0;
            scene.add(
                MeshType::Cuboid,
                MaterialType::Unlit { color },
                Transform::from_xyz(wx, wy, 0.0)
                    .with_scale(Vec3::new(tile_size, tile_size, tile_size)),
            );
        }
    }

    // Render hazard line (survival mode rising hazard)
    if state.hazard_y > 0.0 {
        let course_width = state.course.width as f32 * tile_size;
        scene.add(
            MeshType::Cuboid,
            MaterialType::Glow {
                color: Vec4::new(1.0, 0.15, 0.0, 0.6),
                intensity: 2.0,
            },
            Transform::from_xyz(course_width / 2.0, state.hazard_y, 0.0).with_scale(Vec3::new(
                course_width,
                0.1,
                1.0,
            )),
        );
    }

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
