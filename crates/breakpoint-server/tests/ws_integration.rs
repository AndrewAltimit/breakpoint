#[allow(dead_code)]
mod common;

use breakpoint_core::net::messages::{
    ChatMessageMsg, ClientMessage, GameEndMsg, GameStartMsg, GameStateMsg, PlayerInputMsg,
    PlayerScoreEntry, RoundEndMsg, ServerMessage,
};
use breakpoint_core::net::protocol::{decode_client_message, encode_client_message};
use breakpoint_core::player::PlayerColor;
use common::{
    TestServer, ws_connect, ws_join_room, ws_join_room_expect_error, ws_join_room_with_name,
    ws_read_raw, ws_read_server_msg, ws_send_client_msg, ws_send_server_msg, ws_try_read_raw,
};
use futures::SinkExt;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn create_room() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    let (join_resp, room_code) = common::ws_create_room(&mut stream, "Alice").await;

    assert!(join_resp.success);
    assert_eq!(join_resp.player_id, Some(1));
    assert!(!room_code.is_empty());
    // Room code format: ABCD-1234
    assert_eq!(room_code.len(), 9);
    assert_eq!(&room_code[4..5], "-");

    // Should also receive a PlayerList
    let msg = ws_read_server_msg(&mut stream).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 1);
            assert_eq!(pl.players[0].display_name, "Alice");
            assert!(pl.players[0].is_host);
        },
        other => panic!("Expected PlayerList, got: {other:?}"),
    }
}

#[tokio::test]
async fn join_existing_room() {
    let server = TestServer::new().await;

    // Host creates room
    let mut host = ws_connect(&server.ws_url()).await;
    let (_join_resp, room_code) = common::ws_create_room(&mut host, "Alice").await;
    // Consume host's PlayerList (1 player)
    let _ = ws_read_server_msg(&mut host).await;

    // Client joins room
    let mut client = ws_connect(&server.ws_url()).await;
    let join_resp = ws_join_room(&mut client, &room_code, "Bob").await;
    assert!(join_resp.success);
    assert_eq!(join_resp.player_id, Some(2));

    // Client should receive PlayerList with 2 players
    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 2);
        },
        other => panic!("Expected PlayerList, got: {other:?}"),
    }

    // Host should also receive the updated PlayerList
    let msg = ws_read_server_msg(&mut host).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 2);
            assert_eq!(pl.host_id, 1);
        },
        other => panic!("Expected PlayerList, got: {other:?}"),
    }
}

#[tokio::test]
async fn join_nonexistent_room() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    let resp = ws_join_room_expect_error(&mut stream, "ZZZZ-9999", "Bob").await;
    assert!(!resp.success);
    assert!(resp.error.is_some());
}

#[tokio::test]
async fn chat_broadcast() {
    let server = TestServer::new().await;

    // Host creates room
    let mut host = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut host, "Alice").await;
    let _ = ws_read_server_msg(&mut host).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let join_resp = ws_join_room(&mut client, &room_code, "Bob").await;
    let bob_id = join_resp.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut host).await; // PlayerList update

    // Client sends chat
    let chat_msg = ClientMessage::ChatMessage(ChatMessageMsg {
        player_id: bob_id,
        content: "Hello!".to_string(),
    });
    let encoded = encode_client_message(&chat_msg).unwrap();
    client.send(Message::Binary(encoded.into())).await.unwrap();

    // Host receives chat — relayed as raw client message bytes
    let data = ws_read_raw(&mut host).await;
    let decoded = decode_client_message(&data).unwrap();
    match decoded {
        ClientMessage::ChatMessage(cm) => {
            assert_eq!(cm.player_id, bob_id);
            assert_eq!(cm.content, "Hello!");
        },
        other => panic!("Expected ChatMessage, got: {other:?}"),
    }
}

#[tokio::test]
async fn game_state_relay() {
    let server = TestServer::new().await;

    // Host creates room
    let mut host = ws_connect(&server.ws_url()).await;
    let (host_join, room_code) = common::ws_create_room(&mut host, "Alice").await;
    let host_id = host_join.player_id.unwrap();
    let _ = ws_read_server_msg(&mut host).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let join_resp = ws_join_room(&mut client, &room_code, "Bob").await;
    let bob_id = join_resp.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut host).await; // PlayerList update

    // Host sends GameState → client receives it
    // GameState is a ServerMessage type (0x10), relayed as-is to non-host players
    let gs_msg = ServerMessage::GameState(GameStateMsg {
        tick: 42,
        state_data: vec![1, 2, 3],
    });
    let encoded = breakpoint_core::net::protocol::encode_server_message(&gs_msg).unwrap();
    host.send(Message::Binary(encoded.into())).await.unwrap();

    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::GameState(gs) => {
            assert_eq!(gs.tick, 42);
            assert_eq!(gs.state_data, vec![1, 2, 3]);
        },
        other => panic!("Expected GameState, got: {other:?}"),
    }

    // Client sends PlayerInput → host receives it (relayed as client message bytes)
    let input_msg = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: bob_id,
        tick: 42,
        input_data: vec![0xAA, 0xBB],
    });
    let encoded = encode_client_message(&input_msg).unwrap();
    client.send(Message::Binary(encoded.into())).await.unwrap();

    let data = ws_read_raw(&mut host).await;
    let decoded = decode_client_message(&data).unwrap();
    match decoded {
        ClientMessage::PlayerInput(pi) => {
            assert_eq!(pi.player_id, bob_id);
            assert_eq!(pi.tick, 42);
            assert_eq!(pi.input_data, vec![0xAA, 0xBB]);
        },
        other => panic!("Expected PlayerInput, got: {other:?}"),
    }

    // Verify host sending GameState doesn't echo back to host
    let gs_msg2 = ServerMessage::GameState(GameStateMsg {
        tick: 43,
        state_data: vec![4, 5, 6],
    });
    let encoded = breakpoint_core::net::protocol::encode_server_message(&gs_msg2).unwrap();
    host.send(Message::Binary(encoded.into())).await.unwrap();

    // Host should NOT receive its own GameState back
    let maybe = ws_try_read_raw(&mut host, 200).await;
    assert!(maybe.is_none(), "Host should not receive its own GameState");

    // But client should
    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::GameState(gs) => assert_eq!(gs.tick, 43),
        other => panic!("Expected GameState, got: {other:?}"),
    }

    // Verify host doesn't relay its own PlayerInput to itself
    let host_input = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: host_id,
        tick: 50,
        input_data: vec![0xFF],
    });
    let encoded = encode_client_message(&host_input).unwrap();
    host.send(Message::Binary(encoded.into())).await.unwrap();

    // Host's PlayerInput should NOT be relayed (host sends to itself = no-op)
    let maybe = ws_try_read_raw(&mut host, 200).await;
    assert!(
        maybe.is_none(),
        "Host's own PlayerInput should not be relayed back"
    );
}

#[tokio::test]
async fn disconnect_updates_player_list() {
    let server = TestServer::new().await;

    // Host creates room
    let mut host = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut host, "Alice").await;
    let _ = ws_read_server_msg(&mut host).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let _ = ws_join_room(&mut client, &room_code, "Bob").await;
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut host).await; // PlayerList update

    // Client disconnects
    drop(client);

    // Host should get updated PlayerList with 1 player
    let msg = ws_read_server_msg(&mut host).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 1);
            assert_eq!(pl.players[0].display_name, "Alice");
        },
        other => panic!("Expected PlayerList, got: {other:?}"),
    }
}

#[tokio::test]
async fn host_migration_on_disconnect() {
    let server = TestServer::new().await;

    // Host creates room
    let mut host = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut host, "Alice").await;
    let _ = ws_read_server_msg(&mut host).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let join_resp = ws_join_room(&mut client, &room_code, "Bob").await;
    let bob_id = join_resp.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut host).await; // PlayerList update

    // Host disconnects
    drop(host);

    // Client should get PlayerList showing them as host
    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 1);
            assert_eq!(pl.host_id, bob_id);
            assert!(pl.players[0].is_host);
            assert_eq!(pl.players[0].display_name, "Bob");
        },
        other => panic!("Expected PlayerList, got: {other:?}"),
    }
}

#[tokio::test]
async fn join_with_empty_name_rejected() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    let resp = ws_join_room_with_name(&mut stream, "").await;
    assert!(!resp.success);
    assert!(
        resp.error
            .as_deref()
            .unwrap()
            .contains("Invalid player name")
    );
}

#[tokio::test]
async fn join_with_long_name_rejected() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    let long_name = "A".repeat(33);
    let resp = ws_join_room_with_name(&mut stream, &long_name).await;
    assert!(!resp.success);
    assert!(
        resp.error
            .as_deref()
            .unwrap()
            .contains("Invalid player name")
    );
}

// ============================================================================
// Game lifecycle relay tests
// ============================================================================

/// Helper: set up a 2-player room and consume all PlayerList messages.
/// Returns (host_stream, client_stream, host_id, client_id, room_code).
async fn setup_two_player_room(
    server: &TestServer,
) -> (common::WsStream, common::WsStream, u64, u64, String) {
    let mut host = ws_connect(&server.ws_url()).await;
    let (host_join, room_code) = common::ws_create_room(&mut host, "Alice").await;
    let host_id = host_join.player_id.unwrap();
    let _ = ws_read_server_msg(&mut host).await; // PlayerList (1 player)

    let mut client = ws_connect(&server.ws_url()).await;
    let client_join = ws_join_room(&mut client, &room_code, "Bob").await;
    let client_id = client_join.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList (2 players)
    let _ = ws_read_server_msg(&mut host).await; // PlayerList update

    (host, client, host_id, client_id, room_code)
}

#[tokio::test]
async fn game_lifecycle_start_state_input_end() {
    let server = TestServer::new().await;
    let (mut host, mut client, host_id, client_id, _room_code) =
        setup_two_player_room(&server).await;

    // 1. Host sends GameStart → client receives it
    let game_start = ServerMessage::GameStart(GameStartMsg {
        game_name: "mini-golf".to_string(),
        players: vec![
            breakpoint_core::player::Player {
                id: host_id,
                display_name: "Alice".to_string(),
                color: PlayerColor::default(),
                is_host: true,
                is_spectator: false,
            },
            breakpoint_core::player::Player {
                id: client_id,
                display_name: "Bob".to_string(),
                color: PlayerColor::PALETTE[1],
                is_host: false,
                is_spectator: false,
            },
        ],
        host_id,
    });
    ws_send_server_msg(&mut host, &game_start).await;

    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::GameStart(gs) => {
            assert_eq!(gs.game_name, "mini-golf");
            assert_eq!(gs.players.len(), 2);
            assert_eq!(gs.host_id, host_id);
        },
        other => panic!("Expected GameStart, got: {other:?}"),
    }

    // 2. Host sends 5 GameState messages → client receives all 5
    for tick in 1..=5 {
        let gs = ServerMessage::GameState(GameStateMsg {
            tick,
            state_data: vec![tick as u8; 4],
        });
        ws_send_server_msg(&mut host, &gs).await;
    }

    for tick in 1..=5_u32 {
        let msg = ws_read_server_msg(&mut client).await;
        match msg {
            ServerMessage::GameState(gs) => {
                assert_eq!(gs.tick, tick);
                assert_eq!(gs.state_data, vec![tick as u8; 4]);
            },
            other => panic!("Expected GameState tick {tick}, got: {other:?}"),
        }
    }

    // Host should NOT receive its own GameState
    let maybe = ws_try_read_raw(&mut host, 200).await;
    assert!(maybe.is_none(), "Host should not echo its own GameState");

    // 3. Client sends PlayerInput → host receives it
    let input = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: client_id,
        tick: 3,
        input_data: vec![0xCA, 0xFE],
    });
    ws_send_client_msg(&mut client, &input).await;

    let data = ws_read_raw(&mut host).await;
    let decoded = decode_client_message(&data).unwrap();
    match decoded {
        ClientMessage::PlayerInput(pi) => {
            assert_eq!(pi.player_id, client_id);
            assert_eq!(pi.tick, 3);
            assert_eq!(pi.input_data, vec![0xCA, 0xFE]);
        },
        other => panic!("Expected PlayerInput, got: {other:?}"),
    }

    // 4. Host sends RoundEnd → client receives it
    let round_end = ServerMessage::RoundEnd(RoundEndMsg {
        round: 1,
        scores: vec![
            PlayerScoreEntry {
                player_id: host_id,
                score: 5,
            },
            PlayerScoreEntry {
                player_id: client_id,
                score: 3,
            },
        ],
    });
    ws_send_server_msg(&mut host, &round_end).await;

    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::RoundEnd(re) => {
            assert_eq!(re.round, 1);
            assert_eq!(re.scores.len(), 2);
        },
        other => panic!("Expected RoundEnd, got: {other:?}"),
    }

    // 5. Host sends GameEnd → client receives it
    let game_end = ServerMessage::GameEnd(GameEndMsg {
        final_scores: vec![
            PlayerScoreEntry {
                player_id: host_id,
                score: 5,
            },
            PlayerScoreEntry {
                player_id: client_id,
                score: 3,
            },
        ],
    });
    ws_send_server_msg(&mut host, &game_end).await;

    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::GameEnd(ge) => {
            assert_eq!(ge.final_scores.len(), 2);
        },
        other => panic!("Expected GameEnd, got: {other:?}"),
    }
}

#[tokio::test]
async fn non_host_cannot_send_game_start() {
    let server = TestServer::new().await;
    let (mut host, mut client, host_id, client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Client (non-host) sends GameStart — should be silently dropped
    let game_start = ServerMessage::GameStart(GameStartMsg {
        game_name: "mini-golf".to_string(),
        players: vec![],
        host_id: client_id,
    });
    ws_send_server_msg(&mut client, &game_start).await;

    // Host should NOT receive it
    let maybe = ws_try_read_raw(&mut host, 500).await;
    assert!(
        maybe.is_none(),
        "Non-host GameStart should be silently dropped"
    );

    // Also verify the legitimate host can still send it after
    let real_start = ServerMessage::GameStart(GameStartMsg {
        game_name: "mini-golf".to_string(),
        players: vec![],
        host_id,
    });
    ws_send_server_msg(&mut host, &real_start).await;
    let msg = ws_read_server_msg(&mut client).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Host GameStart should still work"
    );
}

#[tokio::test]
async fn game_state_relayed_regardless_of_room_state() {
    let server = TestServer::new().await;
    let (mut host, mut client, _host_id, _client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Room is in Lobby state — host sends GameState, relay should still forward it
    let gs = ServerMessage::GameState(GameStateMsg {
        tick: 1,
        state_data: vec![42],
    });
    ws_send_server_msg(&mut host, &gs).await;

    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::GameState(gs) => {
            assert_eq!(gs.tick, 1);
            assert_eq!(gs.state_data, vec![42]);
        },
        other => panic!("Expected GameState in Lobby state, got: {other:?}"),
    }
}

#[tokio::test]
async fn player_input_with_real_golf_data() {
    let server = TestServer::new().await;
    let (mut host, mut client, _host_id, client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Host sends GameStart to transition room state
    let game_start = ServerMessage::GameStart(GameStartMsg {
        game_name: "mini-golf".to_string(),
        players: vec![],
        host_id: _host_id,
    });
    ws_send_server_msg(&mut host, &game_start).await;
    let _ = ws_read_server_msg(&mut client).await; // consume GameStart

    // Client sends PlayerInput with real msgpack-encoded GolfInput
    let golf_input = breakpoint_golf::GolfInput {
        aim_angle: 1.57,
        power: 0.8,
        stroke: true,
    };
    let input_data = rmp_serde::to_vec(&golf_input).unwrap();

    let input_msg = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: client_id,
        tick: 10,
        input_data: input_data.clone(),
    });
    ws_send_client_msg(&mut client, &input_msg).await;

    // Host receives it and can decode the GolfInput
    let data = ws_read_raw(&mut host).await;
    let decoded = decode_client_message(&data).unwrap();
    match decoded {
        ClientMessage::PlayerInput(pi) => {
            assert_eq!(pi.player_id, client_id);
            assert_eq!(pi.tick, 10);

            let recovered: breakpoint_golf::GolfInput =
                rmp_serde::from_slice(&pi.input_data).unwrap();
            assert!((recovered.aim_angle - 1.57).abs() < 0.01);
            assert!((recovered.power - 0.8).abs() < 0.01);
            assert!(recovered.stroke);
        },
        other => panic!("Expected PlayerInput, got: {other:?}"),
    }
}
