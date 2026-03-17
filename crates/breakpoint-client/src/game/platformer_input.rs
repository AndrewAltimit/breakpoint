use breakpoint_platformer::physics::PlatformerInput;

use crate::app::{ActiveGame, NetworkRole};
use crate::game::send_player_input;
use crate::input::InputState;
use crate::net_client::WsClient;

/// Process platformer input: WASD/arrows for movement, Space for jump,
/// Shift for run, S/Down+Space for backdash, Ctrl/C for slide, E for powerup.
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

    let jump = input.is_key_just_pressed("Space")
        || input.is_key_just_pressed("ArrowUp")
        || input.is_key_just_pressed("KeyW");
    let jump_held =
        input.is_key_down("Space") || input.is_key_down("ArrowUp") || input.is_key_down("KeyW");

    let use_powerup = input.is_key_just_pressed("KeyE");
    let attack = input.is_key_just_pressed("KeyF") || input.is_key_just_pressed("KeyX");

    let run = input.is_key_down("ShiftLeft") || input.is_key_down("ShiftRight");

    // Backdash: dedicated key (KeyS or ArrowDown) when pressed as a tap
    let backdash = input.is_key_just_pressed("KeyQ");

    // Slide: Ctrl or KeyC while grounded (handled server-side)
    let slide = input.is_key_just_pressed("ControlLeft")
        || input.is_key_just_pressed("ControlRight")
        || input.is_key_just_pressed("KeyC");

    let plat_input = PlatformerInput {
        move_dir,
        jump,
        jump_held,
        use_powerup,
        attack,
        backdash,
        slide,
        run,
    };
    send_player_input(&plat_input, active, role, ws);
}
