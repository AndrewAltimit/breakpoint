use glam::{Vec2, Vec3, Vec4};

use crate::app::{ActiveGame, NetworkRole};
use crate::camera_gl::Camera;
use crate::game::read_game_state;
use crate::input::InputState;
use crate::renderer::Renderer;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::theme::{Theme, rgb_vec4};

/// Sync the 3D scene with the current golf game state.
#[allow(clippy::too_many_arguments)]
pub fn sync_golf_scene(
    scene: &mut Scene,
    active: &ActiveGame,
    theme: &Theme,
    _dt: f32,
    input: &InputState,
    camera: &Camera,
    renderer: &Renderer,
    role: Option<&NetworkRole>,
) {
    let state: Option<breakpoint_golf::GolfState> = read_game_state(active);
    let Some(state) = state else {
        return;
    };

    scene.clear();

    // Look up the course by index
    let courses = breakpoint_golf::course::all_courses();
    let course_idx = state.course_index as usize;
    let Some(course) = courses.get(course_idx) else {
        return;
    };

    // Ground plane
    let ground_w = course.width;
    let ground_d = course.depth;
    scene.add(
        MeshType::Plane,
        MaterialType::Gradient {
            start: rgb_vec4(&theme.golf.ground_color),
            end: rgb_vec4(&theme.golf.dirt_color),
        },
        Transform::from_xyz(ground_w / 2.0, 0.0, ground_d / 2.0)
            .with_scale(Vec3::new(ground_w, 1.0, ground_d)),
    );

    // Walls
    for wall in &course.walls {
        let ax = wall.a.x;
        let az = wall.a.z;
        let bx = wall.b.x;
        let bz = wall.b.z;
        let cx = (ax + bx) / 2.0;
        let cz = (az + bz) / 2.0;
        let sx = (bx - ax).abs().max(0.3);
        let sz = (bz - az).abs().max(0.3);
        scene.add(
            MeshType::Cuboid,
            MaterialType::Unlit {
                color: rgb_vec4(&theme.golf.wall_color),
            },
            Transform::from_xyz(cx, wall.height / 2.0, cz).with_scale(Vec3::new(
                sx,
                wall.height,
                sz,
            )),
        );
    }

    // Bumpers
    for bumper in &course.bumpers {
        scene.add(
            MeshType::Cylinder { segments: 16 },
            MaterialType::Unlit {
                color: rgb_vec4(&theme.golf.bumper_color),
            },
            Transform::from_xyz(bumper.position.x, 0.5, bumper.position.z).with_scale(Vec3::new(
                bumper.radius * 2.0,
                1.0,
                bumper.radius * 2.0,
            )),
        );
    }

    // Hole
    scene.add(
        MeshType::Plane,
        MaterialType::Ripple {
            color: Vec4::new(0.03, 0.03, 0.03, 1.0),
            ring_count: 8.0,
            speed: 2.0,
        },
        Transform::from_xyz(course.hole_position.x, 0.01, course.hole_position.z)
            .with_scale(Vec3::splat(1.0)),
    );

    // Flag
    scene.add(
        MeshType::Cylinder { segments: 16 },
        MaterialType::Unlit {
            color: rgb_vec4(&theme.golf.flag_color),
        },
        Transform::from_xyz(course.hole_position.x, 0.75, course.hole_position.z)
            .with_scale(Vec3::new(0.05, 1.5, 0.05)),
    );

    // Balls — use theme ball color since BallState doesn't have a color field
    for ball in state.balls.values() {
        if ball.is_sunk {
            continue;
        }
        let color = rgb_vec4(&theme.golf.ball_color);
        scene.add(
            MeshType::Sphere { segments: 16 },
            MaterialType::Unlit { color },
            Transform::from_xyz(ball.position.x, ball.position.y.max(0.15), ball.position.z)
                .with_scale(Vec3::splat(0.3)),
        );
    }

    // Aim indicator: draw dots from local player's ball toward cursor ground position
    if let Some(role) = role
        && let Some(ball) = state.balls.get(&role.local_player_id)
        && !ball.is_sunk
    {
        let vel_sq = ball.velocity.x * ball.velocity.x
            + ball.velocity.y * ball.velocity.y
            + ball.velocity.z * ball.velocity.z;
        if vel_sq <= 0.01 {
            let (vw, vh) = renderer.viewport_size();
            let viewport = Vec2::new(vw, vh);
            if let Some(ground_pos) = camera.screen_to_ground(input.cursor_position, viewport) {
                let ball_pos = Vec3::new(ball.position.x, 0.15, ball.position.z);
                let dx = ground_pos.x - ball_pos.x;
                let dz = ground_pos.z - ball_pos.z;
                let dist = (dx * dx + dz * dz).sqrt();
                if dist > 0.5 {
                    let dir_x = dx / dist;
                    let dir_z = dz / dist;
                    let aim_color = Vec4::new(
                        theme.golf.aim_line_color[0],
                        theme.golf.aim_line_color[1],
                        theme.golf.aim_line_color[2],
                        theme.golf.aim_line_color[3],
                    );
                    let dot_count = 8;
                    let max_dist = dist.min(15.0);
                    let spacing = max_dist / dot_count as f32;
                    for i in 1..=dot_count {
                        let t = i as f32 * spacing;
                        let alpha_fade = 1.0 - (i as f32 / dot_count as f32) * 0.6;
                        let dot_color = Vec4::new(
                            aim_color.x,
                            aim_color.y,
                            aim_color.z,
                            aim_color.w * alpha_fade,
                        );
                        scene.add(
                            MeshType::Sphere { segments: 16 },
                            MaterialType::Glow {
                                color: dot_color,
                                intensity: 1.2,
                            },
                            Transform::from_xyz(
                                ball_pos.x + dir_x * t,
                                0.15,
                                ball_pos.z + dir_z * t,
                            )
                            .with_scale(Vec3::splat(0.12)),
                        );
                    }
                }
            }
        }
    }
}
