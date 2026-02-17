use glam::{Quat, Vec3, Vec4};

use crate::app::ActiveGame;
use crate::game::read_game_state;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::theme::Theme;

// --- Armagetron-style color palette ---

/// Pure black floor.
const FLOOR_COLOR: Vec4 = Vec4::new(0.0, 0.0, 0.0, 1.0);
/// Subtle dark grey grid lines (Armagetron style).
const GRID_COLOR: Vec4 = Vec4::new(0.18, 0.18, 0.25, 1.0);
/// Dark boundary walls — visible but not distracting.
const BOUNDARY_COLOR: Vec4 = Vec4::new(0.12, 0.12, 0.2, 1.0);
/// Win zone ring.
const WIN_ZONE_COLOR: Vec4 = Vec4::new(1.0, 0.85, 0.2, 0.7);

/// Base speed threshold — cycles above this are grinding.
const BASE_SPEED: f32 = 50.0;

/// Player trail colors — vivid neon on black, inspired by Armagetron.
const PLAYER_COLORS: [Vec4; 8] = [
    Vec4::new(0.0, 0.85, 1.0, 1.0), // cyan (classic Tron blue)
    Vec4::new(1.0, 0.8, 0.0, 1.0),  // gold/yellow (Armagetron default)
    Vec4::new(0.1, 1.0, 0.2, 1.0),  // neon green
    Vec4::new(1.0, 0.0, 0.6, 1.0),  // hot pink / magenta
    Vec4::new(0.6, 0.3, 1.0, 1.0),  // electric purple
    Vec4::new(1.0, 0.35, 0.0, 1.0), // bright orange
    Vec4::new(0.0, 1.0, 0.7, 1.0),  // aquamarine
    Vec4::new(1.0, 0.1, 0.1, 1.0),  // red
];

/// Sync the 3D scene with the current tron game state.
pub fn sync_tron_scene(
    scene: &mut Scene,
    active: &ActiveGame,
    _theme: &Theme,
    _dt: f32,
    local_player_id: Option<u64>,
) {
    let state: Option<breakpoint_tron::TronState> = read_game_state(active);
    let Some(state) = state else {
        return;
    };

    scene.clear();

    let arena_w = state.arena_width;
    let arena_d = state.arena_depth;

    // Arena floor (pure black)
    scene.add(
        MeshType::Plane,
        MaterialType::Unlit { color: FLOOR_COLOR },
        Transform::from_xyz(arena_w / 2.0, 0.0, arena_d / 2.0)
            .with_scale(Vec3::new(arena_w, 1.0, arena_d)),
    );

    // Grid lines — hair-thin, dim, semi-transparent (fog handles horizon fade)
    // Use wider spacing to reduce draw call count (important for Firefox/Tegra).
    let grid_spacing = 50.0;
    let grid_height = 0.005;
    let grid_thickness = 0.08;
    let grid_color = Vec4::new(GRID_COLOR.x, GRID_COLOR.y, GRID_COLOR.z, 0.4);

    // Vertical grid lines (along Z axis)
    let mut x = grid_spacing;
    while x < arena_w {
        scene.add(
            MeshType::Cuboid,
            MaterialType::Unlit { color: grid_color },
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
            MaterialType::Unlit { color: grid_color },
            Transform::from_xyz(arena_w / 2.0, grid_height, z).with_scale(Vec3::new(
                arena_w,
                0.02,
                grid_thickness,
            )),
        );
        z += grid_spacing;
    }

    // Arena boundary walls — Glow shader for broad GPU compatibility
    let bwall_height = 8.0;
    let bwall_thickness = 0.5;
    let bwall_color = Vec4::new(BOUNDARY_COLOR.x, BOUNDARY_COLOR.y, BOUNDARY_COLOR.z, 0.8);

    // North wall (z=0)
    scene.add(
        MeshType::Cuboid,
        MaterialType::Glow {
            color: bwall_color,
            intensity: 0.7,
        },
        Transform::from_xyz(arena_w / 2.0, bwall_height / 2.0, 0.0).with_scale(Vec3::new(
            arena_w + bwall_thickness,
            bwall_height,
            bwall_thickness,
        )),
    );
    // South wall (z=depth)
    scene.add(
        MeshType::Cuboid,
        MaterialType::Glow {
            color: bwall_color,
            intensity: 0.7,
        },
        Transform::from_xyz(arena_w / 2.0, bwall_height / 2.0, arena_d).with_scale(Vec3::new(
            arena_w + bwall_thickness,
            bwall_height,
            bwall_thickness,
        )),
    );
    // West wall (x=0)
    scene.add(
        MeshType::Cuboid,
        MaterialType::Glow {
            color: bwall_color,
            intensity: 0.7,
        },
        Transform::from_xyz(0.0, bwall_height / 2.0, arena_d / 2.0).with_scale(Vec3::new(
            bwall_thickness,
            bwall_height,
            arena_d + bwall_thickness,
        )),
    );
    // East wall (x=width)
    scene.add(
        MeshType::Cuboid,
        MaterialType::Glow {
            color: bwall_color,
            intensity: 0.7,
        },
        Transform::from_xyz(arena_w, bwall_height / 2.0, arena_d / 2.0).with_scale(Vec3::new(
            bwall_thickness,
            bwall_height,
            arena_d + bwall_thickness,
        )),
    );

    // Build a player index for color mapping
    let mut player_index: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for (i, (&pid, _)) in state.players.iter().enumerate() {
        player_index.insert(pid, i);
    }

    // Wall trail segments — TronWall shader (dim body + bright top edge).
    // Own walls: short, high intensity. Enemy walls: tall, dimmer.
    // Cap total wall segments rendered to avoid GPU overload on weaker drivers.
    let max_wall_segments = 512;
    let trail_thickness = 0.3;
    let walls_to_render = if state.wall_segments.len() > max_wall_segments {
        // Render most recent segments (end of the vec)
        &state.wall_segments[state.wall_segments.len() - max_wall_segments..]
    } else {
        &state.wall_segments[..]
    };
    for wall in walls_to_render {
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

        let is_own = local_player_id == Some(wall.owner_id);

        // Own walls: cycle-height, bright. Enemy walls: tall, slightly dimmer.
        let (trail_height, intensity) = if is_own { (3.0, 2.0) } else { (5.0, 1.5) };
        let wall_color = Vec4::new(color.x, color.y, color.z, 1.0);

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
                color: wall_color,
                intensity,
            },
            Transform::from_xyz(cx, trail_height / 2.0, cz).with_scale(scale),
        );
    }

    // Cycle heads — oriented arrow-like shapes at the head of each trail
    for (&pid, cycle) in &state.players {
        if !cycle.alive {
            continue;
        }
        let color_idx = player_index.get(&pid).copied().unwrap_or(0) % PLAYER_COLORS.len();
        let color = PLAYER_COLORS[color_idx];

        // Rotate the cycle body to face the direction of travel
        let rotation = match cycle.direction {
            breakpoint_tron::Direction::North => Quat::from_rotation_y(std::f32::consts::PI),
            breakpoint_tron::Direction::South => Quat::IDENTITY,
            breakpoint_tron::Direction::East => Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
            breakpoint_tron::Direction::West => Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        };

        // Elongated cycle body (sleeker, smaller)
        scene.add(
            MeshType::Cuboid,
            MaterialType::Glow {
                color,
                intensity: 5.0,
            },
            Transform::from_xyz(cycle.x, 1.0, cycle.z)
                .with_rotation(rotation)
                .with_scale(Vec3::new(0.8, 1.5, 2.0)),
        );

        // Small bright "nose" at the front
        let (front_dx, front_dz) = match cycle.direction {
            breakpoint_tron::Direction::North => (0.0, -1.2),
            breakpoint_tron::Direction::South => (0.0, 1.2),
            breakpoint_tron::Direction::East => (1.2, 0.0),
            breakpoint_tron::Direction::West => (-1.2, 0.0),
        };
        scene.add(
            MeshType::Cuboid,
            MaterialType::Glow {
                color,
                intensity: 6.0,
            },
            Transform::from_xyz(cycle.x + front_dx, 1.0, cycle.z + front_dz)
                .with_rotation(rotation)
                .with_scale(Vec3::new(0.4, 1.0, 0.8)),
        );

        // Grinding spark effect — bright flash near the cycle when speed > base
        if cycle.speed > BASE_SPEED + 2.0 {
            let spark_intensity = ((cycle.speed - BASE_SPEED) / 10.0).min(3.0) + 2.0;
            let spark_color = Vec4::new(
                color.x * 0.5 + 0.5,
                color.y * 0.5 + 0.5,
                color.z * 0.5 + 0.5,
                0.9,
            );

            // Ground-level spark glow behind the cycle
            let (back_dx, back_dz) = match cycle.direction {
                breakpoint_tron::Direction::North => (0.0, 1.5),
                breakpoint_tron::Direction::South => (0.0, -1.5),
                breakpoint_tron::Direction::East => (-1.5, 0.0),
                breakpoint_tron::Direction::West => (1.5, 0.0),
            };
            scene.add(
                MeshType::Cuboid,
                MaterialType::Glow {
                    color: spark_color,
                    intensity: spark_intensity,
                },
                Transform::from_xyz(cycle.x + back_dx, 0.4, cycle.z + back_dz)
                    .with_scale(Vec3::new(1.5, 0.8, 1.5)),
            );
        }
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
