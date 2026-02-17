#[allow(dead_code)]
mod common;

use breakpoint_core::net::messages::{
    ChatMessageMsg, ClientMessage, GameEndMsg, GameStateMsg, JoinRoomMsg, PlayerInputMsg,
    RoundEndMsg, ServerMessage,
};
use breakpoint_core::net::protocol::{decode_client_message, encode_client_message};
use breakpoint_core::player::PlayerColor;
use common::{
    TestServer, ws_connect, ws_join_room, ws_join_room_expect_error, ws_join_room_with_name,
    ws_read_raw, ws_read_server_msg, ws_request_game_start, ws_send_client_msg, ws_send_server_msg,
    ws_try_read_raw,
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
            assert!(pl.players[0].is_leader);
        },
        other => panic!("Expected PlayerList, got: {other:?}"),
    }
}

#[tokio::test]
async fn join_existing_room() {
    let server = TestServer::new().await;

    // Leader creates room
    let mut leader = ws_connect(&server.ws_url()).await;
    let (_join_resp, room_code) = common::ws_create_room(&mut leader, "Alice").await;
    // Consume leader's PlayerList (1 player)
    let _ = ws_read_server_msg(&mut leader).await;

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

    // Leader should also receive the updated PlayerList
    let msg = ws_read_server_msg(&mut leader).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 2);
            assert_eq!(pl.leader_id, 1);
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

    // Leader creates room
    let mut leader = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut leader, "Alice").await;
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let join_resp = ws_join_room(&mut client, &room_code, "Bob").await;
    let bob_id = join_resp.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList update

    // Client sends chat
    let chat_msg = ClientMessage::ChatMessage(ChatMessageMsg {
        player_id: bob_id,
        content: "Hello!".to_string(),
    });
    let encoded = encode_client_message(&chat_msg).unwrap();
    client.send(Message::Binary(encoded.into())).await.unwrap();

    // Leader receives chat — relayed as raw client message bytes
    let data = ws_read_raw(&mut leader).await;
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
async fn server_broadcasts_game_state() {
    let server = TestServer::new().await;

    // Leader creates room
    let mut leader = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut leader, "Alice").await;
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let _ = ws_join_room(&mut client, &room_code, "Bob").await;
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList update

    // Leader requests game start
    ws_request_game_start(&mut leader, "mini-golf").await;

    // Both should receive GameStart from server
    let msg = ws_read_server_msg(&mut leader).await;
    assert!(matches!(msg, ServerMessage::GameStart(_)));
    let msg = ws_read_server_msg(&mut client).await;
    assert!(matches!(msg, ServerMessage::GameStart(_)));

    // Both should start receiving GameState from server's game loop
    let leader_msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(leader_msg, ServerMessage::GameState(_)),
        "Leader should receive GameState from server"
    );
    let client_msg = ws_read_server_msg(&mut client).await;
    assert!(
        matches!(client_msg, ServerMessage::GameState(_)),
        "Client should receive GameState from server"
    );
}

#[tokio::test]
async fn disconnect_updates_player_list() {
    let server = TestServer::new().await;

    // Leader creates room
    let mut leader = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut leader, "Alice").await;
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let _ = ws_join_room(&mut client, &room_code, "Bob").await;
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList update

    // Client disconnects
    drop(client);

    // Leader should get updated PlayerList with 1 player
    let msg = ws_read_server_msg(&mut leader).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 1);
            assert_eq!(pl.players[0].display_name, "Alice");
        },
        other => panic!("Expected PlayerList, got: {other:?}"),
    }
}

#[tokio::test]
async fn leader_migration_on_disconnect() {
    let server = TestServer::new().await;

    // Leader creates room
    let mut leader = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut leader, "Alice").await;
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let join_resp = ws_join_room(&mut client, &room_code, "Bob").await;
    let bob_id = join_resp.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList update

    // Leader disconnects
    drop(leader);

    // Client should get PlayerList showing them as leader
    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 1);
            assert_eq!(pl.leader_id, bob_id);
            assert!(pl.players[0].is_leader);
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
// Server-authoritative game lifecycle tests
// ============================================================================

/// Helper: set up a 2-player room and consume all PlayerList messages.
/// Returns (leader_stream, client_stream, leader_id, client_id, room_code).
async fn setup_two_player_room(
    server: &TestServer,
) -> (common::WsStream, common::WsStream, u64, u64, String) {
    let mut leader = ws_connect(&server.ws_url()).await;
    let (leader_join, room_code) = common::ws_create_room(&mut leader, "Alice").await;
    let leader_id = leader_join.player_id.unwrap();
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList (1 player)

    let mut client = ws_connect(&server.ws_url()).await;
    let client_join = ws_join_room(&mut client, &room_code, "Bob").await;
    let client_id = client_join.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList (2 players)
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList update

    (leader, client, leader_id, client_id, room_code)
}

#[tokio::test]
async fn server_game_lifecycle() {
    let server = TestServer::new().await;
    let (mut leader, mut client, leader_id, client_id, _room_code) =
        setup_two_player_room(&server).await;

    // 1. Leader sends RequestGameStart → both receive GameStart from server
    ws_request_game_start(&mut leader, "mini-golf").await;

    let msg = ws_read_server_msg(&mut leader).await;
    match msg {
        ServerMessage::GameStart(gs) => {
            assert_eq!(gs.game_name, "mini-golf");
            assert_eq!(gs.players.len(), 2);
            assert_eq!(gs.leader_id, leader_id);
        },
        other => panic!("Expected GameStart for leader, got: {other:?}"),
    }

    let msg = ws_read_server_msg(&mut client).await;
    match msg {
        ServerMessage::GameStart(gs) => {
            assert_eq!(gs.game_name, "mini-golf");
            assert_eq!(gs.players.len(), 2);
        },
        other => panic!("Expected GameStart for client, got: {other:?}"),
    }

    // 2. Both receive GameState from server's game loop
    let leader_msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(leader_msg, ServerMessage::GameState(_)),
        "Leader should receive GameState"
    );
    let client_msg = ws_read_server_msg(&mut client).await;
    let initial_state = match client_msg {
        ServerMessage::GameState(ref gs) => gs.state_data.clone(),
        ref other => panic!("Expected GameState for client, got: {other:?}"),
    };

    // 3. Client sends PlayerInput → verify server processes it
    let golf_input = breakpoint_golf::GolfInput {
        aim_angle: 0.5,
        power: 0.6,
        stroke: true,
    };
    let input_data = rmp_serde::to_vec(&golf_input).unwrap();
    let input = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: client_id,
        tick: 1,
        input_data,
    });
    ws_send_client_msg(&mut client, &input).await;

    // Verify state changes (input was processed by server game loop)
    for _ in 0..50 {
        let msg = ws_read_server_msg(&mut client).await;
        if let ServerMessage::GameState(gs) = msg
            && gs.state_data != initial_state
        {
            return; // Input was processed, state changed
        }
    }
    panic!("GameState never reflected player input");
}

#[tokio::test]
async fn non_leader_cannot_request_game_start() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _leader_id, _client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Client (non-leader) sends RequestGameStart — should be rejected
    ws_request_game_start(&mut client, "mini-golf").await;

    // Neither player should receive GameStart
    let maybe = ws_try_read_raw(&mut leader, 500).await;
    assert!(
        maybe.is_none(),
        "Non-leader RequestGameStart should not produce GameStart"
    );

    // Verify the leader can still start the game
    ws_request_game_start(&mut leader, "mini-golf").await;
    let msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Leader RequestGameStart should work"
    );
}

#[tokio::test]
async fn server_only_messages_rejected_from_clients() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _leader_id, _client_id, _room_code) =
        setup_two_player_room(&server).await;

    // GameState from client should be silently rejected
    let gs = ServerMessage::GameState(GameStateMsg {
        tick: 999,
        state_data: vec![0xFF],
    });
    ws_send_server_msg(&mut leader, &gs).await;
    let maybe = ws_try_read_raw(&mut client, 500).await;
    assert!(
        maybe.is_none(),
        "Server-only GameState from client should be rejected"
    );

    // RoundEnd from client should be rejected
    let re = ServerMessage::RoundEnd(RoundEndMsg {
        round: 1,
        scores: vec![],
    });
    ws_send_server_msg(&mut client, &re).await;
    let maybe = ws_try_read_raw(&mut leader, 500).await;
    assert!(
        maybe.is_none(),
        "Server-only RoundEnd from client should be rejected"
    );

    // GameEnd from client should be rejected
    let ge = ServerMessage::GameEnd(GameEndMsg {
        final_scores: vec![],
    });
    ws_send_server_msg(&mut client, &ge).await;
    let maybe = ws_try_read_raw(&mut leader, 500).await;
    assert!(
        maybe.is_none(),
        "Server-only GameEnd from client should be rejected"
    );

    // Verify normal operations still work
    ws_request_game_start(&mut leader, "mini-golf").await;
    let msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Normal operations should work after rejected messages"
    );
}

#[tokio::test]
async fn player_input_with_real_golf_data() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _leader_id, client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Leader starts a real golf game on the server
    ws_request_game_start(&mut leader, "mini-golf").await;

    // Consume GameStart from both
    let _ = ws_read_server_msg(&mut leader).await;
    let _ = ws_read_server_msg(&mut client).await;

    // Consume initial GameState
    let initial_msg = ws_read_server_msg(&mut client).await;
    let initial_state = match initial_msg {
        ServerMessage::GameState(gs) => gs.state_data,
        other => panic!("Expected initial GameState, got: {other:?}"),
    };

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
        input_data,
    });
    ws_send_client_msg(&mut client, &input_msg).await;

    // Verify the server processed the golf input
    for _ in 0..50 {
        let msg = ws_read_server_msg(&mut client).await;
        if let ServerMessage::GameState(gs) = msg
            && gs.state_data != initial_state
        {
            let state: breakpoint_golf::GolfState = rmp_serde::from_slice(&gs.state_data).unwrap();
            assert_eq!(
                state.strokes[&client_id], 1,
                "Server should have processed the golf stroke"
            );
            return;
        }
    }
    panic!("Server never processed the golf input");
}

// ============================================================================
// Error path tests
// ============================================================================

#[tokio::test]
async fn join_empty_player_name_rejected() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    let resp = ws_join_room_with_name(&mut stream, "").await;
    assert!(!resp.success);
    let err = resp.error.as_deref().expect("Should have error message");
    assert!(
        err.contains("Invalid player name"),
        "Expected 'Invalid player name' error, got: {err}"
    );
}

#[tokio::test]
async fn join_control_chars_in_name_rejected() {
    let server = TestServer::new().await;

    // Test newline in name
    let mut stream1 = ws_connect(&server.ws_url()).await;
    let resp = ws_join_room_with_name(&mut stream1, "Alice\nBob").await;
    assert!(!resp.success);
    let err = resp.error.as_deref().expect("Should have error message");
    assert!(
        err.contains("Invalid player name"),
        "Newline in name should be rejected, got: {err}"
    );

    // Test null byte in name
    let mut stream2 = ws_connect(&server.ws_url()).await;
    let resp = ws_join_room_with_name(&mut stream2, "Alice\0Bob").await;
    assert!(!resp.success);
    let err = resp.error.as_deref().expect("Should have error message");
    assert!(
        err.contains("Invalid player name"),
        "Null byte in name should be rejected, got: {err}"
    );

    // Test tab character in name
    let mut stream3 = ws_connect(&server.ws_url()).await;
    let resp = ws_join_room_with_name(&mut stream3, "Alice\tBob").await;
    assert!(!resp.success);
    let err = resp.error.as_deref().expect("Should have error message");
    assert!(
        err.contains("Invalid player name"),
        "Tab in name should be rejected, got: {err}"
    );
}

#[tokio::test]
async fn join_too_long_name_rejected() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    // Exactly 33 characters (one over the 32-char limit)
    let long_name = "A".repeat(33);
    let resp = ws_join_room_with_name(&mut stream, &long_name).await;
    assert!(!resp.success);
    let err = resp.error.as_deref().expect("Should have error message");
    assert!(
        err.contains("Invalid player name"),
        "Name > 32 chars should be rejected, got: {err}"
    );

    // Exactly 32 characters should be accepted
    let mut stream2 = ws_connect(&server.ws_url()).await;
    let ok_name = "B".repeat(32);
    let resp2 = ws_join_room_with_name(&mut stream2, &ok_name).await;
    assert!(resp2.success, "Name of exactly 32 chars should be accepted");
}

#[tokio::test]
async fn oversized_message_dropped() {
    let server = TestServer::new().await;

    // Set up a two-player room with an active game
    let (mut leader, mut client, _leader_id, client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Start a game so PlayerInput routing is active
    ws_request_game_start(&mut leader, "mini-golf").await;
    let _ = ws_read_server_msg(&mut leader).await; // GameStart
    let _ = ws_read_server_msg(&mut client).await; // GameStart

    // Consume initial GameState
    let initial_msg = ws_read_server_msg(&mut client).await;
    let initial_state = match initial_msg {
        ServerMessage::GameState(gs) => gs.state_data,
        other => panic!("Expected initial GameState, got: {other:?}"),
    };

    // Construct oversized raw binary data (> 64 KiB)
    let mut oversized = Vec::with_capacity(65 * 1024 + 1);
    oversized.push(0x01); // PlayerInput message type byte
    oversized.resize(65 * 1024 + 1, 0xAA); // fill to > 64 KiB
    assert!(
        oversized.len() > breakpoint_core::net::protocol::MAX_MESSAGE_SIZE,
        "Test message should exceed MAX_MESSAGE_SIZE"
    );
    client
        .send(Message::Binary(oversized.into()))
        .await
        .unwrap();

    // Verify the connection is still alive by sending a valid golf input
    let golf_input = breakpoint_golf::GolfInput {
        aim_angle: 0.5,
        power: 0.6,
        stroke: true,
    };
    let input_data = rmp_serde::to_vec(&golf_input).unwrap();
    let normal_input = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: client_id,
        tick: 2,
        input_data,
    });
    ws_send_client_msg(&mut client, &normal_input).await;

    // The normal input should be processed by the server game loop
    for _ in 0..50 {
        let msg = ws_read_server_msg(&mut client).await;
        if let ServerMessage::GameState(gs) = msg
            && gs.state_data != initial_state
        {
            return; // Normal input was processed after oversized was dropped
        }
    }
    panic!("Connection should still work after oversized message drop");
}

#[tokio::test]
async fn protocol_version_mismatch_rejected() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    // Send JoinRoom with a mismatched protocol version (99)
    let msg = ClientMessage::JoinRoom(JoinRoomMsg {
        room_code: String::new(),
        player_name: "Alice".to_string(),
        player_color: PlayerColor::default(),
        protocol_version: 99,
        session_token: None,
    });
    let encoded = encode_client_message(&msg).unwrap();
    stream.send(Message::Binary(encoded.into())).await.unwrap();

    let resp = ws_read_server_msg(&mut stream).await;
    match resp {
        ServerMessage::JoinRoomResponse(join) => {
            assert!(!join.success, "Mismatched protocol version should fail");
            let err = join.error.as_deref().expect("Should have error message");
            assert!(
                err.contains("version mismatch") || err.contains("Protocol version mismatch"),
                "Error should mention version mismatch, got: {err}"
            );
        },
        other => panic!("Expected JoinRoomResponse error, got: {other:?}"),
    }
}

// ============================================================================
// Additional error path integration tests (Phase 3)
// ============================================================================

#[tokio::test]
async fn malformed_binary_message_ignored() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _leader_id, _client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Send malformed binary (valid type byte but garbage payload)
    let malformed = vec![0x01, 0xFF, 0xFF, 0xFF];
    client
        .send(Message::Binary(malformed.into()))
        .await
        .unwrap();

    // Connection should remain alive — leader can still start a game
    ws_request_game_start(&mut leader, "mini-golf").await;
    let msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Connection should survive malformed message"
    );
}

#[tokio::test]
async fn empty_binary_message_ignored() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _leader_id, _client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Send empty binary message
    client
        .send(Message::Binary(Vec::new().into()))
        .await
        .unwrap();

    // Connection should remain alive
    ws_request_game_start(&mut leader, "mini-golf").await;
    let msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Connection should survive empty message"
    );
}

#[tokio::test]
async fn invalid_type_byte_ignored() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _leader_id, _client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Send message with unknown type byte 0xFF
    let invalid = vec![0xFF, 0x01, 0x02, 0x03];
    client.send(Message::Binary(invalid.into())).await.unwrap();

    // Connection should remain alive
    ws_request_game_start(&mut leader, "mini-golf").await;
    let msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Connection should survive invalid type byte"
    );
}

#[tokio::test]
async fn text_frame_ignored() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _leader_id, _client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Send a text frame instead of binary
    client.send(Message::Text("hello".into())).await.unwrap();

    // Connection should remain alive
    ws_request_game_start(&mut leader, "mini-golf").await;
    let msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Connection should survive text frame"
    );
}

#[tokio::test]
async fn invalid_room_code_format_rejected() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    // Try joining with invalid room code format
    let resp = ws_join_room_expect_error(&mut stream, "not-a-valid-code!!!", "Alice").await;
    assert!(!resp.success);
    let err = resp.error.as_deref().unwrap();
    assert!(
        err.contains("Invalid room code") || err.contains("Room not found"),
        "Invalid room code format should be rejected, got: {err}"
    );
}

#[tokio::test]
async fn spoofed_player_input_has_no_effect() {
    let server = TestServer::new().await;
    let (mut leader, mut client, leader_id, client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Start a game
    ws_request_game_start(&mut leader, "mini-golf").await;
    let _ = ws_read_server_msg(&mut leader).await; // GameStart
    let _ = ws_read_server_msg(&mut client).await; // GameStart

    // Consume initial GameState for both
    let _ = ws_read_server_msg(&mut leader).await;
    let initial_msg = ws_read_server_msg(&mut client).await;
    let initial_state = match initial_msg {
        ServerMessage::GameState(gs) => gs.state_data,
        other => panic!("Expected GameState, got: {other:?}"),
    };

    // Client sends PlayerInput with the LEADER's player_id (spoofed)
    let golf_input = breakpoint_golf::GolfInput {
        aim_angle: 0.0,
        power: 1.0,
        stroke: true,
    };
    let input_data = rmp_serde::to_vec(&golf_input).unwrap();
    let spoofed = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: leader_id, // Spoofed! Client is client_id, not leader_id
        tick: 1,
        input_data: input_data.clone(),
    });
    ws_send_client_msg(&mut client, &spoofed).await;

    // Now send a legitimate input from the client
    let legit = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: client_id,
        tick: 2,
        input_data,
    });
    ws_send_client_msg(&mut client, &legit).await;

    // Verify that the client's stroke was counted (not the leader's)
    for _ in 0..50 {
        let msg = ws_read_server_msg(&mut client).await;
        if let ServerMessage::GameState(gs) = msg
            && gs.state_data != initial_state
        {
            let state: breakpoint_golf::GolfState = rmp_serde::from_slice(&gs.state_data).unwrap();
            // Client's stroke should be counted
            assert!(
                state.strokes.get(&client_id).copied().unwrap_or(0) >= 1,
                "Client's legitimate input should be processed"
            );
            return;
        }
    }
    panic!("GameState never reflected player input");
}

#[tokio::test]
async fn non_join_first_message_disconnects() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    // Send a PlayerInput as the first message (should be JoinRoom)
    let input = ClientMessage::PlayerInput(PlayerInputMsg {
        player_id: 1,
        tick: 0,
        input_data: vec![],
    });
    ws_send_client_msg(&mut stream, &input).await;

    // The server should close the connection — next read should be Close, error, or stream end
    use futures::StreamExt as _;
    let deadline = std::time::Duration::from_secs(2);
    let result = tokio::time::timeout(deadline, async {
        loop {
            match stream.next().await {
                Some(Ok(Message::Binary(_))) => continue, // skip any in-flight binary
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return true,
                _ => continue,
            }
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "Non-join first message should cause connection closure"
    );
}

#[tokio::test]
async fn whitespace_only_name_rejected() {
    let server = TestServer::new().await;
    let mut stream = ws_connect(&server.ws_url()).await;

    let resp = ws_join_room_with_name(&mut stream, "   ").await;
    assert!(!resp.success);
    let err = resp.error.as_deref().unwrap();
    assert!(
        err.contains("Invalid player name"),
        "Whitespace-only name should be rejected, got: {err}"
    );
}
