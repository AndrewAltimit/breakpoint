use glam::Vec2;

use breakpoint_lasertag::LaserTagInput;

use crate::app::{ActiveGame, NetworkRole};
use crate::camera_gl::Camera;
use crate::game::{read_game_state, send_player_input};
use crate::input::{InputState, MouseButton};
use crate::net_client::WsClient;
use crate::renderer::Renderer;

/// Process laser tag input: WASD for movement, mouse aim + click to fire.
pub fn process_lasertag_input(
    input: &InputState,
    camera: &Camera,
    renderer: &Renderer,
    active: &mut ActiveGame,
    role: &NetworkRole,
    ws: &WsClient,
) {
    let mut move_x: f32 = 0.0;
    let mut move_z: f32 = 0.0;
    if input.is_key_down("KeyD") || input.is_key_down("ArrowRight") {
        move_x += 1.0;
    }
    if input.is_key_down("KeyA") || input.is_key_down("ArrowLeft") {
        move_x -= 1.0;
    }
    if input.is_key_down("KeyW") || input.is_key_down("ArrowUp") {
        move_z += 1.0;
    }
    if input.is_key_down("KeyS") || input.is_key_down("ArrowDown") {
        move_z -= 1.0;
    }

    // Aim direction from cursor
    let (vw, vh) = renderer.viewport_size();
    let viewport = Vec2::new(vw, vh);
    let aim_angle = camera
        .screen_to_ground(input.cursor_position, viewport)
        .and_then(|ground| {
            let state: Option<breakpoint_lasertag::LaserTagState> = read_game_state(active);
            state.and_then(|s| {
                s.players.get(&role.local_player_id).map(|p| {
                    let dx = ground.x - p.x;
                    let dz = ground.z - p.z;
                    dz.atan2(dx)
                })
            })
        })
        .unwrap_or(0.0);

    let fire = input.is_mouse_just_pressed(MouseButton::Left);
    let use_powerup = input.is_key_just_pressed("KeyE");

    let lt_input = LaserTagInput {
        move_x,
        move_z,
        aim_angle,
        fire,
        use_powerup,
    };
    send_player_input(&lt_input, active, role, ws);
}
