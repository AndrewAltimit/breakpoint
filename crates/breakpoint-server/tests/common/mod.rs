use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use breakpoint_core::events::{Event, EventType, Priority};
use breakpoint_core::net::messages::{
    ClientMessage, JoinRoomMsg, JoinRoomResponseMsg, RequestGameStartMsg, ServerMessage,
};
use breakpoint_core::net::protocol::{
    decode_server_message, encode_client_message, encode_server_message,
};
use breakpoint_core::player::PlayerColor;

use breakpoint_server::config::{AuthFileConfig, ServerConfig};
use breakpoint_server::{build_app, spawn_event_broadcaster};

pub struct TestServer {
    pub addr: SocketAddr,
    _shutdown: tokio::task::JoinHandle<()>,
}

impl TestServer {
    /// Start a test server with no auth.
    pub async fn new() -> Self {
        Self::from_config(ServerConfig::default()).await
    }

    /// Start a test server with no auth and no webhook signature requirement.
    pub async fn with_no_webhook_requirement() -> Self {
        let config = ServerConfig {
            auth: AuthFileConfig {
                require_webhook_signature: false,
                ..AuthFileConfig::default()
            },
            ..ServerConfig::default()
        };
        Self::from_config(config).await
    }

    /// Start a test server with bearer token and webhook secret.
    pub async fn with_auth(token: &str, webhook_secret: &str) -> Self {
        let config = ServerConfig {
            auth: AuthFileConfig {
                bearer_token: Some(token.to_string()),
                github_webhook_secret: Some(webhook_secret.to_string()),
                require_webhook_signature: false,
            },
            ..ServerConfig::default()
        };
        Self::from_config(config).await
    }

    async fn from_config(config: ServerConfig) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (app, state) = build_app(config);
        spawn_event_broadcaster(state);

        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Give the server a moment to start accepting
        tokio::time::sleep(Duration::from_millis(20)).await;

        Self {
            addr,
            _shutdown: handle,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn ws_url(&self) -> String {
        format!("ws://{}/ws", self.addr)
    }
}

/// Connect a WebSocket client to the given URL.
pub async fn ws_connect(url: &str) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let (stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    stream
}

/// Send a JoinRoom message with empty room_code (create new room).
/// Returns (JoinRoomResponse, room_code).
pub async fn ws_create_room(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    name: &str,
) -> (JoinRoomResponseMsg, String) {
    let msg = ClientMessage::JoinRoom(JoinRoomMsg {
        room_code: String::new(),
        player_name: name.to_string(),
        player_color: PlayerColor::default(),
        protocol_version: breakpoint_core::net::protocol::PROTOCOL_VERSION,
        session_token: None,
    });
    let encoded = encode_client_message(&msg).unwrap();
    stream.send(Message::Binary(encoded.into())).await.unwrap();

    // First message back is JoinRoomResponse
    let data = ws_read_raw(stream).await;
    let resp = decode_server_message(&data).unwrap();
    match resp {
        ServerMessage::JoinRoomResponse(ref join) => {
            assert!(join.success, "Expected successful join: {join:?}");
            let code = join.room_code.clone().unwrap();
            (join.clone(), code)
        },
        other => panic!("Expected JoinRoomResponse, got: {other:?}"),
    }
}

/// Send a JoinRoom message with an existing room code.
/// Returns the JoinRoomResponse.
pub async fn ws_join_room(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    room_code: &str,
    name: &str,
) -> JoinRoomResponseMsg {
    let msg = ClientMessage::JoinRoom(JoinRoomMsg {
        room_code: room_code.to_string(),
        player_name: name.to_string(),
        player_color: PlayerColor::PALETTE[1],
        protocol_version: breakpoint_core::net::protocol::PROTOCOL_VERSION,
        session_token: None,
    });
    let encoded = encode_client_message(&msg).unwrap();
    stream.send(Message::Binary(encoded.into())).await.unwrap();

    let resp = ws_read_server_msg(stream).await;
    match resp {
        ServerMessage::JoinRoomResponse(join) => join,
        other => panic!("Expected JoinRoomResponse, got: {other:?}"),
    }
}

/// Send a JoinRoom for a nonexistent room and return the error response.
pub async fn ws_join_room_expect_error(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    room_code: &str,
    name: &str,
) -> JoinRoomResponseMsg {
    let msg = ClientMessage::JoinRoom(JoinRoomMsg {
        room_code: room_code.to_string(),
        player_name: name.to_string(),
        player_color: PlayerColor::default(),
        protocol_version: breakpoint_core::net::protocol::PROTOCOL_VERSION,
        session_token: None,
    });
    let encoded = encode_client_message(&msg).unwrap();
    stream.send(Message::Binary(encoded.into())).await.unwrap();

    let resp = ws_read_server_msg(stream).await;
    match resp {
        ServerMessage::JoinRoomResponse(join) => join,
        other => panic!("Expected JoinRoomResponse, got: {other:?}"),
    }
}

/// Send a JoinRoom (create new room) with an arbitrary name.
/// Returns the JoinRoomResponse (may be success or error).
pub async fn ws_join_room_with_name(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    name: &str,
) -> JoinRoomResponseMsg {
    let msg = ClientMessage::JoinRoom(JoinRoomMsg {
        room_code: String::new(),
        player_name: name.to_string(),
        player_color: PlayerColor::default(),
        protocol_version: breakpoint_core::net::protocol::PROTOCOL_VERSION,
        session_token: None,
    });
    let encoded = encode_client_message(&msg).unwrap();
    stream.send(Message::Binary(encoded.into())).await.unwrap();

    let resp = ws_read_server_msg(stream).await;
    match resp {
        ServerMessage::JoinRoomResponse(join) => join,
        other => panic!("Expected JoinRoomResponse, got: {other:?}"),
    }
}

/// Read raw binary data from a WebSocket stream (5s timeout).
pub async fn ws_read_raw(stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> Vec<u8> {
    let deadline = Duration::from_secs(5);
    tokio::time::timeout(deadline, async {
        loop {
            match stream.next().await {
                Some(Ok(Message::Binary(data))) => return data.to_vec(),
                Some(Ok(Message::Close(_))) => panic!("WebSocket closed unexpectedly"),
                Some(Err(e)) => panic!("WebSocket error: {e}"),
                None => panic!("WebSocket stream ended"),
                _ => continue,
            }
        }
    })
    .await
    .expect("Timed out waiting for WebSocket message")
}

/// Try to read raw binary data, returning None on timeout.
pub async fn ws_try_read_raw(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    timeout_ms: u64,
) -> Option<Vec<u8>> {
    let deadline = Duration::from_millis(timeout_ms);
    tokio::time::timeout(deadline, async {
        loop {
            match stream.next().await {
                Some(Ok(Message::Binary(data))) => return data.to_vec(),
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                    panic!("WebSocket error or closed")
                },
                _ => continue,
            }
        }
    })
    .await
    .ok()
}

/// Read the next ServerMessage from a WebSocket stream (5s timeout).
pub async fn ws_read_server_msg(
    stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> ServerMessage {
    let data = ws_read_raw(stream).await;
    decode_server_message(&data).unwrap()
}

/// Compute HMAC-SHA256 signature in `sha256=<hex>` format.
/// Uses the server crate's auth module to verify consistency.
pub fn sign_webhook(secret: &str, body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = <Hmac<Sha256>>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let result = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(result))
}

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Send a ServerMessage from a WS stream (used by host to send GameStart etc.)
pub async fn ws_send_server_msg(stream: &mut WsStream, msg: &ServerMessage) {
    let encoded = encode_server_message(msg).unwrap();
    stream.send(Message::Binary(encoded.into())).await.unwrap();
}

/// Send a ClientMessage from a WS stream.
pub async fn ws_send_client_msg(stream: &mut WsStream, msg: &ClientMessage) {
    let encoded = encode_client_message(msg).unwrap();
    stream.send(Message::Binary(encoded.into())).await.unwrap();
}

/// Send a RequestGameStart from a client (leader) to start a server-authoritative game.
pub async fn ws_request_game_start(stream: &mut WsStream, game_name: &str) {
    let msg = ClientMessage::RequestGameStart(RequestGameStartMsg {
        game_name: game_name.to_string(),
    });
    ws_send_client_msg(stream, &msg).await;
}

/// Construct a test event with the given id.
pub fn make_event(id: &str) -> Event {
    Event {
        id: id.to_string(),
        event_type: EventType::PipelineFailed,
        source: "github".to_string(),
        priority: Priority::Notice,
        title: format!("CI failed for {id}"),
        body: None,
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        url: None,
        actor: Some("test-bot".to_string()),
        tags: vec![],
        action_required: false,
        group_key: None,
        expires_at: None,
        metadata: HashMap::new(),
    }
}
