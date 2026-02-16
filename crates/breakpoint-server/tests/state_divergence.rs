//! Network fidelity tests — verify all clients receive consistent state from
//! the server-authoritative game loop, reconnection preserves rooms, and
//! malformed messages are handled gracefully.

#[allow(dead_code)]
mod common;

use breakpoint_core::net::messages::ServerMessage;
use common::{TestServer, ws_connect, ws_join_room, ws_read_server_msg, ws_request_game_start};
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
