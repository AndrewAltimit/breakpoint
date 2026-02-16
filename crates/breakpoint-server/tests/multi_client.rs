#[allow(dead_code)]
mod common;

use breakpoint_core::net::messages::ServerMessage;
use common::{TestServer, make_event, ws_connect, ws_join_room_expect_error, ws_read_server_msg};

#[tokio::test]
async fn three_players_in_room() {
    let server = TestServer::new().await;

    // Host creates room
    let mut host = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut host, "Alice").await;
    let _ = ws_read_server_msg(&mut host).await; // PlayerList (1)

    // Client 2 joins
    let mut client2 = ws_connect(&server.ws_url()).await;
    let _ = common::ws_join_room(&mut client2, &room_code, "Bob").await;
    let _ = ws_read_server_msg(&mut client2).await; // PlayerList (2)
    let _ = ws_read_server_msg(&mut host).await; // PlayerList (2)

    // Client 3 joins
    let mut client3 = ws_connect(&server.ws_url()).await;
    let _ = common::ws_join_room(&mut client3, &room_code, "Carol").await;
    let pl3 = ws_read_server_msg(&mut client3).await; // PlayerList (3)
    let _ = ws_read_server_msg(&mut host).await; // PlayerList (3)
    let _ = ws_read_server_msg(&mut client2).await; // PlayerList (3)

    match pl3 {
        ServerMessage::PlayerList(pl) => {
            assert_eq!(pl.players.len(), 3);
            let names: Vec<&str> = pl.players.iter().map(|p| p.display_name.as_str()).collect();
            assert!(names.contains(&"Alice"));
            assert!(names.contains(&"Bob"));
            assert!(names.contains(&"Carol"));
        },
        other => panic!("Expected PlayerList, got: {other:?}"),
    }
}

#[tokio::test]
async fn event_broadcast_reaches_ws_clients() {
    let server = TestServer::new().await;

    // Host creates room
    let mut host = ws_connect(&server.ws_url()).await;
    let (_, _room_code) = common::ws_create_room(&mut host, "Alice").await;
    let _ = ws_read_server_msg(&mut host).await; // PlayerList

    // Give the event broadcaster time to subscribe
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Post event via REST API
    let client = reqwest::Client::new();
    let event = make_event("broadcast-evt-1");
    let resp = client
        .post(format!("{}/api/v1/events", server.base_url()))
        .json(&event)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Host should receive the AlertEvent via WebSocket
    let msg = ws_read_server_msg(&mut host).await;
    match msg {
        ServerMessage::AlertEvent(alert) => {
            assert_eq!(alert.event.id, "broadcast-evt-1");
        },
        other => panic!("Expected AlertEvent, got: {other:?}"),
    }
}

#[tokio::test]
async fn room_destroyed_after_all_leave() {
    let server = TestServer::new().await;

    // Host creates room
    let mut host = ws_connect(&server.ws_url()).await;
    let (_, room_code) = common::ws_create_room(&mut host, "Alice").await;
    let _ = ws_read_server_msg(&mut host).await; // PlayerList

    // Client joins
    let mut client = ws_connect(&server.ws_url()).await;
    let _ = common::ws_join_room(&mut client, &room_code, "Bob").await;
    let _ = ws_read_server_msg(&mut client).await; // PlayerList
    let _ = ws_read_server_msg(&mut host).await; // PlayerList

    // Both disconnect
    drop(client);
    // Wait for server to process disconnects
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    drop(host);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // New client tries to join the old room — should fail
    let mut new_client = ws_connect(&server.ws_url()).await;
    let resp = ws_join_room_expect_error(&mut new_client, &room_code, "Carol").await;
    assert!(!resp.success);
    assert!(resp.error.is_some());
}
