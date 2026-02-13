#[allow(dead_code)]
mod common;

use breakpoint_core::net::messages::{
    ChatMessageMsg, ClientMessage, GameStateMsg, PlayerInputMsg, ServerMessage,
};
use breakpoint_core::net::protocol::{decode_client_message, encode_client_message};
use common::{
    TestServer, ws_connect, ws_join_room, ws_join_room_expect_error, ws_read_raw,
    ws_read_server_msg, ws_try_read_raw,
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
