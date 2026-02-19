use glam::Vec2;

use breakpoint_golf::GolfInput;

use crate::app::{ActiveGame, NetworkRole};
use crate::camera_gl::Camera;
use crate::game::{read_game_state, send_player_input};
use crate::input::{InputState, MouseButton};
use crate::net_client::WsClient;
use crate::renderer::Renderer;

/// Process golf input: mouse hold for power, aim via cursor_to_ground, release to fire.
/// Returns `true` if a stroke was sent this frame.
pub fn process_golf_input(
    input: &InputState,
    camera: &Camera,
    renderer: &Renderer,
    active: &mut ActiveGame,
    role: &NetworkRole,
    ws: &WsClient,
) -> bool {
    let state: Option<breakpoint_golf::GolfState> = read_game_state(active);
    let Some(state) = state else {
        return false;
    };

    // Don't allow input if round is complete
    if state.round_complete {
        return false;
    }

    let Some(ball) = state.balls.get(&role.local_player_id) else {
        return false;
    };

    // Don't allow input if ball is still moving
    let vel_sq = ball.velocity.x * ball.velocity.x
        + ball.velocity.y * ball.velocity.y
        + ball.velocity.z * ball.velocity.z;
    if vel_sq > 0.01 || ball.is_sunk {
        return false;
    }

    let (vw, vh) = renderer.viewport_size();
    let viewport = Vec2::new(vw, vh);

    if input.is_mouse_just_released(MouseButton::Left) {
        // Calculate aim direction from ball to cursor ground position
        if let Some(ground_pos) = camera.screen_to_ground(input.cursor_position, viewport) {
            let dx = ground_pos.x - ball.position.x;
            let dz = ground_pos.z - ball.position.z;
            let len = (dx * dx + dz * dz).sqrt();
            if len > 0.1 {
                let aim_angle = dz.atan2(dx);
                // Power based on distance (clamped 0..1)
                let power = (len / 15.0).min(1.0);
                let golf_input = GolfInput {
                    aim_angle,
                    power,
                    stroke: true,
                };
                send_player_input(&golf_input, active, role, ws);
                return true;
            }
        }
    }
    false
}
