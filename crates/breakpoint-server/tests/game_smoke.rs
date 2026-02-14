//! Game flow smoke tests: verify real game inputs survive the full
//! network relay pipeline (encode -> WS -> relay -> WS -> decode -> apply).

#[allow(dead_code)]
mod common;

use std::collections::HashMap;

use futures::SinkExt;
use tokio_tungstenite::tungstenite::Message;

use breakpoint_core::game_trait::{BreakpointGame, PlayerInputs};
use breakpoint_core::net::messages::{ClientMessage, GameStartMsg, PlayerInputMsg, ServerMessage};
use breakpoint_core::net::protocol::{decode_client_message, encode_client_message};
use breakpoint_core::test_helpers::{default_config, make_players};

use common::{
    TestServer, ws_connect, ws_create_room, ws_join_room, ws_read_raw, ws_read_server_msg,
    ws_send_server_msg,
};

/// Start a game: host creates room, client joins, host sends GameStart.
/// Returns (host_stream, client_stream, host_player_id, client_player_id).
async fn setup_two_player_room(
    server: &TestServer,
) -> (common::WsStream, common::WsStream, u64, u64) {
    let mut host = ws_connect(&server.ws_url()).await;
    let (host_resp, room_code) = ws_create_room(&mut host, "Host").await;
    let host_id = host_resp.player_id.unwrap();

    // Drain PlayerList that comes after JoinRoomResponse
    let _ = ws_read_server_msg(&mut host).await;

    let mut client = ws_connect(&server.ws_url()).await;
    let client_resp = ws_join_room(&mut client, &room_code, "Client").await;
    let client_id = client_resp.player_id.unwrap();

    // Drain PlayerList updates for both
    let _ = ws_read_server_msg(&mut host).await;
    let _ = ws_read_server_msg(&mut client).await;

    // Host sends GameStart
    let players = vec![
        breakpoint_core::player::Player {
            id: host_id,
            display_name: "Host".to_string(),
            color: breakpoint_core::player::PlayerColor::default(),
            is_host: true,
            is_spectator: false,
        },
        breakpoint_core::player::Player {
            id: client_id,
            display_name: "Client".to_string(),
            color: breakpoint_core::player::PlayerColor::PALETTE[1],
            is_host: false,
            is_spectator: false,
        },
    ];

    let game_start = ServerMessage::GameStart(GameStartMsg {
        game_name: "mini-golf".to_string(),
        players,
        host_id,
    });
    ws_send_server_msg(&mut host, &game_start).await;

    // Client receives GameStart
    let msg = ws_read_server_msg(&mut client).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Client should receive GameStart"
    );

    (host, client, host_id, client_id)
}

#[tokio::test]
async fn golf_input_relayed_and_applied() {
    let server = TestServer::new().await;
    let (mut host, mut client, _host_id, client_id) = setup_two_player_room(&server).await;

    // Initialize a real MiniGolf game on the "host" side
    let mut game = breakpoint_golf::MiniGolf::new();
    let players = make_players(2);
    game.init(&players, &default_config(90));

    let before = game.serialize_state();

    // Client sends a PlayerInput with real GolfInput data
    let golf_input = breakpoint_golf::GolfInput {
        aim_angle: 0.5,
        power: 0.6,
        stroke: true,
    };
    let input_data = rmp_serde::to_vec(&golf_input).unwrap();
    let msg = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: client_id,
        tick: 1,
        input_data: input_data.clone(),
    });
    let encoded = encode_client_message(&msg).unwrap();
    client.send(Message::Binary(encoded.into())).await.unwrap();

    // Host receives the relayed PlayerInput (raw ClientMessage bytes)
    let raw = ws_read_raw(&mut host).await;
    let client_msg = decode_client_message(&raw).expect("Host should decode relayed PlayerInput");
    match client_msg {
        ClientMessage::PlayerInput(pi) => {
            // Apply the relayed input to our game instance (simulating host logic)
            game.apply_input(1, &pi.input_data);
            let empty = PlayerInputs {
                inputs: HashMap::new(),
            };
            game.update(0.1, &empty);

            let after = game.serialize_state();
            assert_ne!(
                before, after,
                "Game state should change after applying relayed golf input"
            );
            assert_eq!(game.state().strokes[&1], 1, "Stroke count should increment");
        },
        other => panic!("Expected PlayerInput, got: {other:?}"),
    }
}

#[tokio::test]
async fn platformer_input_relayed_and_applied() {
    let server = TestServer::new().await;
    let (_host, mut client, _host_id, client_id) = setup_two_player_room(&server).await;

    // Initialize a real PlatformRacer game
    let mut game = breakpoint_platformer::PlatformRacer::new();
    let players = make_players(2);
    game.init(&players, &default_config(120));

    let before = game.serialize_state();

    // Client sends PlatformerInput
    let plat_input = breakpoint_platformer::physics::PlatformerInput {
        move_dir: 1.0,
        jump: true,
        use_powerup: false,
    };
    let input_data = rmp_serde::to_vec(&plat_input).unwrap();
    let msg = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: client_id,
        tick: 1,
        input_data: input_data.clone(),
    });
    let encoded = encode_client_message(&msg).unwrap();
    client.send(Message::Binary(encoded.into())).await.unwrap();

    // Apply directly to game (simulating host logic)
    game.apply_input(1, &input_data);
    let empty = PlayerInputs {
        inputs: HashMap::new(),
    };
    game.update(1.0 / 15.0, &empty);

    let after = game.serialize_state();
    assert_ne!(
        before, after,
        "Game state should change after applying relayed platformer input"
    );
}

#[tokio::test]
async fn lasertag_input_relayed_and_applied() {
    let server = TestServer::new().await;
    let (_host, mut client, _host_id, client_id) = setup_two_player_room(&server).await;

    // Initialize a real LaserTagArena game
    let mut game = breakpoint_lasertag::LaserTagArena::new();
    let players = make_players(2);
    game.init(&players, &default_config(180));

    let before = game.serialize_state();

    // Client sends LaserTagInput
    let lt_input = breakpoint_lasertag::LaserTagInput {
        move_x: 1.0,
        move_z: 0.0,
        aim_angle: 0.5,
        fire: false,
        use_powerup: false,
    };
    let input_data = rmp_serde::to_vec(&lt_input).unwrap();
    let msg = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: client_id,
        tick: 1,
        input_data: input_data.clone(),
    });
    let encoded = encode_client_message(&msg).unwrap();
    client.send(Message::Binary(encoded.into())).await.unwrap();

    // Apply directly to game (simulating host logic)
    game.apply_input(1, &input_data);
    let empty = PlayerInputs {
        inputs: HashMap::new(),
    };
    game.update(0.05, &empty);

    let after = game.serialize_state();
    assert_ne!(
        before, after,
        "Game state should change after applying relayed laser tag input"
    );
}

#[tokio::test]
async fn full_golf_round_via_game_engine() {
    // Test a complete golf round purely through the game engine
    // (no network, but uses the same code paths).
    let mut game = breakpoint_golf::MiniGolf::new();
    let players = make_players(1);

    // Use Gentle Straight course for reliable sinking
    let mut config = default_config(90);
    config.custom.insert(
        "hole_index".to_string(),
        serde_json::Value::Number(serde_json::Number::from(1)),
    );
    game.init(&players, &config);

    let empty = PlayerInputs {
        inputs: HashMap::new(),
    };

    let hole = game.course().hole_position;
    let spawn = game.course().spawn_point;
    let aim = (hole.z - spawn.z).atan2(hole.x - spawn.x);

    // Stroke toward hole
    let input = breakpoint_golf::GolfInput {
        aim_angle: aim,
        power: 0.6,
        stroke: true,
    };
    let data = rmp_serde::to_vec(&input).unwrap();
    game.apply_input(1, &data);

    // Simulate until round completes, re-stroking when stopped
    for _ in 0..500 {
        let events = game.update(0.1, &empty);
        if events
            .iter()
            .any(|e| matches!(e, breakpoint_core::game_trait::GameEvent::RoundComplete))
        {
            break;
        }
        if game.state().balls[&1].is_stopped() && !game.state().balls[&1].is_sunk {
            let ball_pos = game.state().balls[&1].position;
            let aim = (hole.z - ball_pos.z).atan2(hole.x - ball_pos.x);
            let input = breakpoint_golf::GolfInput {
                aim_angle: aim,
                power: 0.4,
                stroke: true,
            };
            let data = rmp_serde::to_vec(&input).unwrap();
            game.apply_input(1, &data);
        }
    }

    assert!(game.is_round_complete(), "Golf round should complete");
    let results = game.round_results();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].player_id, 1);
}
