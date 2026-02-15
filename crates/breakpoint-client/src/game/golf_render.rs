use glam::{Vec3, Vec4};

use crate::app::ActiveGame;
use crate::game::read_game_state;
use crate::scene::{MaterialType, MeshType, Scene, Transform};
use crate::theme::{Theme, rgb_vec4};

/// Sync the 3D scene with the current golf game state.
pub fn sync_golf_scene(scene: &mut Scene, active: &ActiveGame, theme: &Theme, _dt: f32) {
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
        MeshType::Cylinder { segments: 8 },
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
}
