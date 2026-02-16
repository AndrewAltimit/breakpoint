use glam::{Vec3, Vec4};

use crate::app::ActiveGame;
use crate::game::read_game_state;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::theme::Theme;

/// Neon-on-dark tron colors.
const FLOOR_COLOR: Vec4 = Vec4::new(0.02, 0.02, 0.05, 1.0);
const GRID_COLOR: Vec4 = Vec4::new(0.05, 0.15, 0.1, 0.6);
const BOUNDARY_COLOR: Vec4 = Vec4::new(0.3, 0.5, 1.0, 1.0);
const WIN_ZONE_COLOR: Vec4 = Vec4::new(1.0, 0.85, 0.2, 0.7);

/// Player trail colors (neon palette).
const PLAYER_COLORS: [Vec4; 8] = [
    Vec4::new(0.0, 0.8, 1.0, 1.0), // cyan
    Vec4::new(1.0, 0.3, 0.1, 1.0), // orange-red
    Vec4::new(0.2, 1.0, 0.3, 1.0), // green
    Vec4::new(1.0, 0.1, 0.8, 1.0), // magenta
    Vec4::new(1.0, 1.0, 0.1, 1.0), // yellow
    Vec4::new(0.5, 0.2, 1.0, 1.0), // purple
    Vec4::new(1.0, 0.5, 0.5, 1.0), // pink
    Vec4::new(0.3, 0.8, 0.8, 1.0), // teal
];

/// Sync the 3D scene with the current tron game state.
pub fn sync_tron_scene(scene: &mut Scene, active: &ActiveGame, _theme: &Theme, _dt: f32) {
    let state: Option<breakpoint_tron::TronState> = read_game_state(active);
    let Some(state) = state else {
        return;
    };

    scene.clear();

    let arena_w = state.arena_width;
    let arena_d = state.arena_depth;

    // Arena floor (dark)
    scene.add(
        MeshType::Plane,
        MaterialType::Unlit { color: FLOOR_COLOR },
        Transform::from_xyz(arena_w / 2.0, 0.0, arena_d / 2.0)
            .with_scale(Vec3::new(arena_w, 1.0, arena_d)),
    );

    // Grid lines
    let grid_spacing = 25.0;
    let grid_height = 0.01;
    let grid_thickness = 0.15;

    // Vertical grid lines (along Z axis)
    let mut x = grid_spacing;
    while x < arena_w {
        scene.add(
            MeshType::Cuboid,
            MaterialType::Glow {
                color: GRID_COLOR,
                intensity: 0.3,
            },
            Transform::from_xyz(x, grid_height, arena_d / 2.0).with_scale(Vec3::new(
                grid_thickness,
                0.02,
                arena_d,
            )),
        );
        x += grid_spacing;
    }

    // Horizontal grid lines (along X axis)
    let mut z = grid_spacing;
    while z < arena_d {
        scene.add(
            MeshType::Cuboid,
            MaterialType::Glow {
                color: GRID_COLOR,
                intensity: 0.3,
            },
            Transform::from_xyz(arena_w / 2.0, grid_height, z).with_scale(Vec3::new(
                arena_w,
                0.02,
                grid_thickness,
            )),
        );
        z += grid_spacing;
    }

    // Arena boundary walls (glowing)
    let wall_height = 3.0;
    let wall_thickness = 0.5;
    let boundary_intensity = 1.5;

    // North wall (z=0)
    scene.add(
        MeshType::Cuboid,
        MaterialType::Glow {
            color: BOUNDARY_COLOR,
            intensity: boundary_intensity,
        },
        Transform::from_xyz(arena_w / 2.0, wall_height / 2.0, 0.0).with_scale(Vec3::new(
            arena_w,
            wall_height,
            wall_thickness,
        )),
    );
    // South wall (z=depth)
    scene.add(
        MeshType::Cuboid,
        MaterialType::Glow {
            color: BOUNDARY_COLOR,
            intensity: boundary_intensity,
        },
        Transform::from_xyz(arena_w / 2.0, wall_height / 2.0, arena_d).with_scale(Vec3::new(
            arena_w,
            wall_height,
            wall_thickness,
        )),
    );
    // West wall (x=0)
    scene.add(
        MeshType::Cuboid,
        MaterialType::Glow {
            color: BOUNDARY_COLOR,
            intensity: boundary_intensity,
        },
        Transform::from_xyz(0.0, wall_height / 2.0, arena_d / 2.0).with_scale(Vec3::new(
            wall_thickness,
            wall_height,
            arena_d,
        )),
    );
    // East wall (x=width)
    scene.add(
        MeshType::Cuboid,
        MaterialType::Glow {
            color: BOUNDARY_COLOR,
            intensity: boundary_intensity,
        },
        Transform::from_xyz(arena_w, wall_height / 2.0, arena_d / 2.0).with_scale(Vec3::new(
            wall_thickness,
            wall_height,
            arena_d,
        )),
    );

    // Build a player index for color mapping
    let mut player_index: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for (i, (&pid, _)) in state.players.iter().enumerate() {
        player_index.insert(pid, i);
    }

    // Wall trail segments
    let trail_height = 2.0;
    let trail_thickness = 0.3;
    for wall in &state.wall_segments {
        let dx = wall.x2 - wall.x1;
        let dz = wall.z2 - wall.z1;
        let len = (dx * dx + dz * dz).sqrt();
        if len < 0.1 {
            continue;
        }

        let cx = (wall.x1 + wall.x2) / 2.0;
        let cz = (wall.z1 + wall.z2) / 2.0;

        let color_idx =
            player_index.get(&wall.owner_id).copied().unwrap_or(0) % PLAYER_COLORS.len();
        let color = PLAYER_COLORS[color_idx];

        // Determine if horizontal or vertical
        let is_horizontal = dz.abs() < 0.1;
        let scale = if is_horizontal {
            Vec3::new(len, trail_height, trail_thickness)
        } else {
            Vec3::new(trail_thickness, trail_height, len)
        };

        scene.add(
            MeshType::Cuboid,
            MaterialType::Glow {
                color,
                intensity: 2.0,
            },
            Transform::from_xyz(cx, trail_height / 2.0, cz).with_scale(scale),
        );
    }

    // Cycle heads (bright cubes at the head of each trail)
    for (&pid, cycle) in &state.players {
        if !cycle.alive {
            continue;
        }
        let color_idx = player_index.get(&pid).copied().unwrap_or(0) % PLAYER_COLORS.len();
        let color = PLAYER_COLORS[color_idx];

        scene.add(
            MeshType::Cuboid,
            MaterialType::Glow {
                color,
                intensity: 3.0,
            },
            Transform::from_xyz(cycle.x, 1.0, cycle.z).with_scale(Vec3::new(1.2, 2.0, 1.2)),
        );
    }

    // Win zone (expanding golden circle)
    if state.win_zone.active {
        scene.add(
            MeshType::Cylinder { segments: 24 },
            MaterialType::Ripple {
                color: WIN_ZONE_COLOR,
                ring_count: 3.0,
                speed: 2.0,
            },
            Transform::from_xyz(state.win_zone.x, 0.05, state.win_zone.z).with_scale(Vec3::new(
                state.win_zone.radius * 2.0,
                0.1,
                state.win_zone.radius * 2.0,
            )),
        );
    }
}
