//! P2-2: Network fidelity tests — verify host and clients receive consistent state,
//! reconnection preserves rooms, and malformed messages are handled gracefully.

#[allow(dead_code)]
mod common;

use breakpoint_core::net::messages::{GameStateMsg, ServerMessage};
use common::{
    TestServer, ws_connect, ws_join_room, ws_read_server_msg, ws_send_server_msg, ws_try_read_raw,
};
use futures::SinkExt;
use tokio_tungstenite::tungstenite::Message;

/// Helper: create a 2-player room, returning (host, client, host_id, client_id, room_code).
async fn setup_two_player_room(
    server: &TestServer,
) -> (
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    u64,
    u64,
    String,
) {
    let mut host = ws_connect(&server.ws_url()).await;
    let (join_resp, room_code) = common::ws_create_room(&mut host, "Host").await;
    let host_id = join_resp.player_id.unwrap();
    let _ = ws_read_server_msg(&mut host).await; // PlayerList(1)

    let mut client = ws_connect(&server.ws_url()).await;
    let join_resp2 = ws_join_room(&mut client, &room_code, "Client").await;
    let client_id = join_resp2.player_id.unwrap();
    let _ = ws_read_server_msg(&mut client).await; // PlayerList(2)
    let _ = ws_read_server_msg(&mut host).await; // PlayerList(2)

    (host, client, host_id, client_id, room_code)
}

// REGRESSION: Both host and client should receive identical game state bytes
#[tokio::test]
async fn host_and_client_receive_same_game_state() {
    let server = TestServer::new().await;
    let (mut host, mut client, _, _, _) = setup_two_player_room(&server).await;

    // Host sends GameState
    let state_data = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let gs_msg = ServerMessage::GameState(GameStateMsg {
        tick: 100,
        state_data: state_data.clone(),
    });
    ws_send_server_msg(&mut host, &gs_msg).await;

    // Client receives the state
    let client_msg = ws_read_server_msg(&mut client).await;
    match client_msg {
        ServerMessage::GameState(gs) => {
            assert_eq!(gs.tick, 100, "Client should receive tick 100");
            assert_eq!(
                gs.state_data, state_data,
                "Client should receive identical state bytes"
            );
        },
        other => panic!("Expected GameState, got: {other:?}"),
    }

    // Host should NOT receive its own GameState back
    let maybe = ws_try_read_raw(&mut host, 200).await;
    assert!(maybe.is_none(), "Host should not receive its own GameState");
}

// REGRESSION: Rapid disconnect/reconnect should preserve room for remaining players
#[tokio::test]
async fn rapid_disconnect_reconnect_preserves_room() {
    let server = TestServer::new().await;
    let (mut host, client, _, _, room_code) = setup_two_player_room(&server).await;

    // Client disconnects abruptly
    drop(client);

    // Small delay to let server process disconnect
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Host should receive updated PlayerList (1 player)
    let msg = ws_read_server_msg(&mut host).await;
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
    let msg2 = ws_read_server_msg(&mut host).await;
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

// REGRESSION: Malformed WS messages should not crash server or affect other clients
#[tokio::test]
async fn malformed_ws_message_does_not_crash_server() {
    let server = TestServer::new().await;
    let (mut host, mut _client, _, _, _room_code) = setup_two_player_room(&server).await;

    // Send garbage binary data through a new connection
    let mut garbage_conn = ws_connect(&server.ws_url()).await;
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0x00, 0x01];
    garbage_conn
        .send(Message::Binary(garbage.into()))
        .await
        .unwrap();

    // Small delay
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Server should still be functional — host can send game state
    let gs_msg = ServerMessage::GameState(GameStateMsg {
        tick: 42,
        state_data: vec![1, 2, 3],
    });
    ws_send_server_msg(&mut host, &gs_msg).await;

    // Client in the room should still receive it
    let msg = ws_read_server_msg(&mut _client).await;
    match msg {
        ServerMessage::GameState(gs) => {
            assert_eq!(
                gs.tick, 42,
                "Client should still receive state after garbage"
            );
        },
        other => panic!("Expected GameState after garbage, got: {other:?}"),
    }
}
