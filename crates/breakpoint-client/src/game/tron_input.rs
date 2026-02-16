use breakpoint_tron::{TronInput, TurnDirection};

use crate::app::{ActiveGame, NetworkRole};
use crate::game::send_player_input;
use crate::input::InputState;
use crate::net_client::WsClient;

/// Process tron input: A/D or Left/Right for turning, Space for brake.
pub fn process_tron_input(
    input: &InputState,
    active: &mut ActiveGame,
    role: &NetworkRole,
    ws: &WsClient,
) {
    let turn = if input.is_key_just_pressed("KeyA") || input.is_key_just_pressed("ArrowLeft") {
        TurnDirection::Left
    } else if input.is_key_just_pressed("KeyD") || input.is_key_just_pressed("ArrowRight") {
        TurnDirection::Right
    } else {
        TurnDirection::None
    };

    let brake =
        input.is_key_down("Space") || input.is_key_down("KeyS") || input.is_key_down("ArrowDown");

    let tron_input = TronInput { turn, brake };
    send_player_input(&tron_input, active, role, ws);
}
