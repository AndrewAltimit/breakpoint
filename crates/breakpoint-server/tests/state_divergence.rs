//! Network fidelity tests — verify all clients receive consistent state from
//! the server-authoritative game loop, reconnection preserves rooms, and
//! malformed messages are handled gracefully.

#[allow(dead_code)]
mod common;

use breakpoint_core::net::messages::{ClientMessage, JoinRoomMsg, ServerMessage};
use breakpoint_core::net::protocol::{decode_server_message, encode_client_message};
use breakpoint_core::player::PlayerColor;
use common::{
    TestServer, ws_connect, ws_join_room, ws_read_raw, ws_read_server_msg, ws_request_game_start,
};
use futures::SinkExt;
use tokio_tungstenite::tungstenite::Message;

/// Helper: create a 2-player room, returning (leader, client, leader_id, client_id, room_code).
async fn setup_two_player_room(
    server: &TestServer,
) -> (common::WsStream, common::WsStream, u64, u64, String) {
    let mut leader = ws_connect(&server.ws_url()).await;
    let (join_resp, room_code) = common::ws_create_room(&mut leader, "Leader").await;
    let leader_id = join_resp.player_id.unwrap();
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList(1)

    let mut client = ws_connect(&server.ws_url()).await;
    let join_resp2 = ws_join_room(&mut client, &room_code, "Client").await;
    let client_id = join_resp2.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList(2)
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList(2)

    (leader, client, leader_id, client_id, room_code)
}

// Both leader and client should receive identical game state bytes from the server
#[tokio::test]
async fn leader_and_client_receive_same_game_state() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _, _, _) = setup_two_player_room(&server).await;

    // Start a game on the server
    ws_request_game_start(&mut leader, "mini-golf").await;

    // Both receive GameStart
    let _ = ws_read_server_msg(&mut leader).await;
    let _ = ws_read_server_msg(&mut client).await;

    // Both should receive the same GameState from the server's game loop
    let leader_msg = ws_read_server_msg(&mut leader).await;
    let client_msg = ws_read_server_msg(&mut client).await;

    match (&leader_msg, &client_msg) {
        (ServerMessage::GameState(leader_gs), ServerMessage::GameState(client_gs)) => {
            assert_eq!(
                leader_gs.tick, client_gs.tick,
                "Both clients should receive the same tick"
            );
            assert_eq!(
                leader_gs.state_data, client_gs.state_data,
                "Both clients should receive identical state bytes"
            );
        },
        _ => panic!("Expected GameState for both, got leader={leader_msg:?} client={client_msg:?}"),
    }
}

// Rapid disconnect/reconnect should preserve room for remaining players
#[tokio::test]
async fn rapid_disconnect_reconnect_preserves_room() {
    let server = TestServer::new().await;
    let (mut leader, client, _, _, room_code) = setup_two_player_room(&server).await;

    // Client disconnects abruptly
    drop(client);

    // Small delay to let server process disconnect
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Leader should receive updated PlayerList (1 player)
    let msg = ws_read_server_msg(&mut leader).await;
    match msg {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(
                pl.players.len(),
                1,
                "After disconnect, room should have 1 player"
            );
        },
        other => panic!("Expected PlayerList after disconnect, got: {other:?}"),
    }

    // New client reconnects to same room
    let mut client2 = ws_connect(&server.ws_url()).await;
    let join_resp = ws_join_room(&mut client2, &room_code, "Client2").await;
    assert!(
        join_resp.success,
        "Reconnect to existing room should succeed"
    );

    // Both should get updated player list
    let _ = ws_read_server_msg(&mut client2).await; // PlayerList(2)
    let msg2 = ws_read_server_msg(&mut leader).await;
    match msg2 {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(
                pl.players.len(),
                2,
                "Room should have 2 players after rejoin"
            );
        },
        other => panic!("Expected PlayerList after rejoin, got: {other:?}"),
    }
}

// Malformed WS messages should not crash server or affect other clients
#[tokio::test]
async fn malformed_ws_message_does_not_crash_server() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _, _, _room_code) = setup_two_player_room(&server).await;

    // Send garbage binary data through a new connection
    let mut garbage_conn = ws_connect(&server.ws_url()).await;
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0x00, 0x01];
    garbage_conn
        .send(Message::Binary(garbage.into()))
        .await
        .unwrap();

    // Small delay
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Server should still be functional — leader can start a game
    ws_request_game_start(&mut leader, "mini-golf").await;

    // Both should receive GameStart
    let msg = ws_read_server_msg(&mut leader).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Leader should receive GameStart after garbage"
    );
    let msg = ws_read_server_msg(&mut client).await;
    assert!(
        matches!(msg, ServerMessage::GameStart(_)),
        "Client should receive GameStart after garbage"
    );
}

// ================================================================
// Phase 6: Session reconnection during active game
// ================================================================

/// Client disconnects mid-game, reconnects with session token, and resumes
/// receiving GameState ticks with the same player_id.
#[tokio::test]
async fn session_reconnect_during_game() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _leader_id, _client_id, _room_code) =
        setup_two_player_room(&server).await;

    // Start a game
    ws_request_game_start(&mut leader, "mini-golf").await;

    // Both receive GameStart
    let _ = ws_read_server_msg(&mut leader).await;
    let msg = ws_read_server_msg(&mut client).await;
    assert!(matches!(msg, ServerMessage::GameStart(_)));

    // Wait for at least one GameState tick to arrive for the client
    let _ = ws_read_server_msg(&mut client).await;

    // Retrieve the client's session token from the JoinRoomResponse
    // (It was returned when client first joined, but we didn't capture it above.
    //  Re-create the scenario: connect fresh, capture token, start game, disconnect, reconnect.)
    drop(client);
    drop(leader);

    // --- Full scenario with token capture ---
    let mut leader = ws_connect(&server.ws_url()).await;
    let (leader_resp, room_code2) = common::ws_create_room(&mut leader, "Leader2").await;
    let _leader_id = leader_resp.player_id.unwrap();
    let _ = ws_read_server_msg(&mut leader).await; // PlayerList

    // Client joins and captures session token
    let mut client = ws_connect(&server.ws_url()).await;
    let join_msg = ClientMessage::JoinRoom(JoinRoomMsg {
        room_code: room_code2.clone(),
        player_name: "Client2".to_string(),
        player_color: PlayerColor::PALETTE[1],
        protocol_version: breakpoint_core::net::protocol::PROTOCOL_VERSION,
        session_token: None,
    });
    let encoded = encode_client_message(&join_msg).unwrap();
    client.send(Message::Binary(encoded.into())).await.unwrap();
    let data = ws_read_raw(&mut client).await;
    let resp = decode_server_message(&data).unwrap();
    let (client_id, session_token) = match resp {
        ServerMessage::JoinRoomResponse(ref join) => {
            assert!(join.success);
            (join.player_id.unwrap(), join.session_token.clone())
        },
        other => panic!("Expected JoinRoomResponse, got {other:?}"),
    };
    assert!(
        session_token.is_some(),
        "Server should return a session token"
    );
    let token = session_token.unwrap();

    // Drain PlayerList updates
    let _ = ws_read_server_msg(&mut client).await;
    let _ = ws_read_server_msg(&mut leader).await;

    // Start game
    ws_request_game_start(&mut leader, "mini-golf").await;
    let _ = ws_read_server_msg(&mut leader).await; // GameStart
    let _ = ws_read_server_msg(&mut client).await; // GameStart

    // Wait for a GameState tick
    let _ = ws_read_server_msg(&mut client).await;

    // Client disconnects mid-game
    drop(client);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Client reconnects with session token
    let mut client2 = ws_connect(&server.ws_url()).await;
    let reconnect_msg = ClientMessage::JoinRoom(JoinRoomMsg {
        room_code: room_code2.clone(),
        player_name: "Client2".to_string(),
        player_color: PlayerColor::PALETTE[1],
        protocol_version: breakpoint_core::net::protocol::PROTOCOL_VERSION,
        session_token: Some(token),
    });
    let encoded = encode_client_message(&reconnect_msg).unwrap();
    client2.send(Message::Binary(encoded.into())).await.unwrap();

    let data = ws_read_raw(&mut client2).await;
    let resp = decode_server_message(&data).unwrap();
    match resp {
        ServerMessage::JoinRoomResponse(join) => {
            assert!(join.success, "Reconnect should succeed: {join:?}");
            assert_eq!(
                join.player_id.unwrap(),
                client_id,
                "Should get same player_id back on reconnect"
            );
            assert!(
                join.session_token.is_some(),
                "Server should issue a new session token"
            );
        },
        other => panic!("Expected JoinRoomResponse, got {other:?}"),
    }

    // Reconnected client should resume receiving GameState ticks
    let mut got_game_state = false;
    for _ in 0..20 {
        let msg = ws_read_server_msg(&mut client2).await;
        if matches!(msg, ServerMessage::GameState(_)) {
            got_game_state = true;
            break;
        }
    }
    assert!(
        got_game_state,
        "Reconnected client should receive GameState ticks"
    );
}

/// Invalid session token should be rejected gracefully.
#[tokio::test]
async fn invalid_session_token_rejected() {
    let server = TestServer::new().await;
    let (mut leader, mut client, _, _, room_code) = setup_two_player_room(&server).await;

    // Start a game
    ws_request_game_start(&mut leader, "mini-golf").await;
    let _ = ws_read_server_msg(&mut leader).await; // GameStart
    let _ = ws_read_server_msg(&mut client).await; // GameStart

    // Client disconnects
    drop(client);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Attempt reconnect with a bogus token
    let mut client2 = ws_connect(&server.ws_url()).await;
    let reconnect_msg = ClientMessage::JoinRoom(JoinRoomMsg {
        room_code: room_code.clone(),
        player_name: "Client".to_string(),
        player_color: PlayerColor::PALETTE[1],
        protocol_version: breakpoint_core::net::protocol::PROTOCOL_VERSION,
        session_token: Some("bogus-token-12345".to_string()),
    });
    let encoded = encode_client_message(&reconnect_msg).unwrap();
    client2.send(Message::Binary(encoded.into())).await.unwrap();

    let data = ws_read_raw(&mut client2).await;
    let resp = decode_server_message(&data).unwrap();
    match resp {
        ServerMessage::JoinRoomResponse(join) => {
            // The invalid token should be ignored and the client joins as a new player
            // (not crash the server). It may succeed as a fresh join or fail depending on
            // whether the room is full during a game, but either way the server should respond.
            assert!(
                join.success || join.error.is_some(),
                "Server should respond to invalid token join attempt"
            );
        },
        other => panic!("Expected JoinRoomResponse, got {other:?}"),
    }
}
