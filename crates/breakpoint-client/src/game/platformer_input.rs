use breakpoint_platformer::physics::PlatformerInput;

use crate::app::{ActiveGame, NetworkRole};
use crate::game::send_player_input;
use crate::input::InputState;
use crate::net_client::WsClient;

/// Process platformer input: WASD/arrows for movement, Space for jump, E for powerup.
pub fn process_platformer_input(
    input: &InputState,
    active: &mut ActiveGame,
    role: &NetworkRole,
    ws: &WsClient,
) {
    let mut move_dir: f32 = 0.0;
    if input.is_key_down("KeyD") || input.is_key_down("ArrowRight") {
        move_dir += 1.0;
    }
    if input.is_key_down("KeyA") || input.is_key_down("ArrowLeft") {
        move_dir -= 1.0;
    }

    let jump =
        input.is_key_down("Space") || input.is_key_down("ArrowUp") || input.is_key_down("KeyW");
    let use_powerup = input.is_key_just_pressed("KeyE");

    let plat_input = PlatformerInput {
        move_dir,
        jump,
        use_powerup,
    };
    send_player_input(&plat_input, active, role, ws);
}
