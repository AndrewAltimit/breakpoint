use serde::{Deserialize, Serialize};

use crate::overlay::config::OverlayConfigMsg;

use super::messages::{
    AlertClaimedMsg, AlertDismissedMsg, AlertEventMsg, ChatMessageMsg, ClaimAlertMsg,
    ClientMessage, GameEndMsg, GameStartMsg, GameStateMsg, JoinRoomMsg, JoinRoomResponseMsg,
    LeaveRoomMsg, MessageType, PlayerInputMsg, PlayerListMsg, RequestGameStartMsg,
    RoomConfigPayload, RoundEndMsg, ServerMessage,
};

/// Current protocol version.
pub const PROTOCOL_VERSION: u8 = 2;

/// Default game tick rate in Hz.
pub const DEFAULT_TICK_RATE_HZ: u32 = 10;

/// Maximum message payload size in bytes.
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024; // 64 KiB

#[derive(Debug)]
pub enum ProtocolError {
    EmptyMessage,
    UnknownMessageType(u8),
    PayloadTooLarge(usize),
    SerializeError(String),
    DeserializeError(String),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMessage => write!(f, "empty message"),
            Self::UnknownMessageType(b) => write!(f, "unknown message type: 0x{b:02x}"),
            Self::PayloadTooLarge(size) => {
                write!(
                    f,
                    "payload too large: {size} bytes (max {MAX_MESSAGE_SIZE})"
                )
            },
            Self::SerializeError(e) => write!(f, "serialize error: {e}"),
            Self::DeserializeError(e) => write!(f, "deserialize error: {e}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Encode a serializable payload with a 1-byte type prefix.
pub fn encode_message<T: Serialize>(
    msg_type: MessageType,
    payload: &T,
) -> Result<Vec<u8>, ProtocolError> {
    let payload_bytes =
        rmp_serde::to_vec(payload).map_err(|e| ProtocolError::SerializeError(e.to_string()))?;
    let total = 1 + payload_bytes.len();
    if total > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::PayloadTooLarge(total));
    }
    let mut buf = Vec::with_capacity(total);
    buf.push(msg_type as u8);
    buf.extend_from_slice(&payload_bytes);
    Ok(buf)
}

/// Encode a `ClientMessage` to wire format.
pub fn encode_client_message(msg: &ClientMessage) -> Result<Vec<u8>, ProtocolError> {
    match msg {
        ClientMessage::JoinRoom(m) => encode_message(MessageType::JoinRoom, m),
        ClientMessage::LeaveRoom(m) => encode_message(MessageType::LeaveRoom, m),
        ClientMessage::PlayerInput(m) => encode_message(MessageType::PlayerInput, m),
        ClientMessage::ChatMessage(m) => encode_message(MessageType::ChatMessage, m),
        ClientMessage::ClaimAlert(m) => encode_message(MessageType::ClaimAlert, m),
        ClientMessage::OverlayConfig(m) => encode_message(MessageType::OverlayConfig, m),
        ClientMessage::RequestGameStart(m) => encode_message(MessageType::RequestGameStart, m),
    }
}

/// Encode a `ServerMessage` to wire format.
pub fn encode_server_message(msg: &ServerMessage) -> Result<Vec<u8>, ProtocolError> {
    match msg {
        ServerMessage::JoinRoomResponse(m) => encode_message(MessageType::JoinRoomResponse, m),
        ServerMessage::PlayerList(m) => encode_message(MessageType::PlayerList, m),
        ServerMessage::RoomConfig(m) => encode_message(MessageType::RoomConfigMsg, m),
        ServerMessage::GameState(m) => encode_message(MessageType::GameState, m),
        ServerMessage::GameStart(m) => encode_message(MessageType::GameStart, m),
        ServerMessage::RoundEnd(m) => encode_message(MessageType::RoundEnd, m),
        ServerMessage::GameEnd(m) => encode_message(MessageType::GameEnd, m),
        ServerMessage::AlertEvent(m) => encode_message(MessageType::AlertEvent, m),
        ServerMessage::AlertClaimed(m) => encode_message(MessageType::AlertClaimed, m),
        ServerMessage::AlertDismissed(m) => encode_message(MessageType::AlertDismissed, m),
        ServerMessage::OverlayConfig(m) => encode_message(MessageType::OverlayConfig, m),
    }
}

/// Extract the message type byte from raw wire data.
pub fn decode_message_type(data: &[u8]) -> Result<MessageType, ProtocolError> {
    if data.is_empty() {
        return Err(ProtocolError::EmptyMessage);
    }
    MessageType::from_byte(data[0]).ok_or(ProtocolError::UnknownMessageType(data[0]))
}

/// Decode a MessagePack payload (bytes after the type prefix).
pub fn decode_payload<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, ProtocolError> {
    if data.is_empty() {
        return Err(ProtocolError::EmptyMessage);
    }
    rmp_serde::from_slice(&data[1..]).map_err(|e| ProtocolError::DeserializeError(e.to_string()))
}

/// Decode raw wire data into a `ClientMessage`.
pub fn decode_client_message(data: &[u8]) -> Result<ClientMessage, ProtocolError> {
    let msg_type = decode_message_type(data)?;
    match msg_type {
        MessageType::JoinRoom => Ok(ClientMessage::JoinRoom(decode_payload::<JoinRoomMsg>(
            data,
        )?)),
        MessageType::LeaveRoom => Ok(ClientMessage::LeaveRoom(decode_payload::<LeaveRoomMsg>(
            data,
        )?)),
        MessageType::PlayerInput => Ok(ClientMessage::PlayerInput(
            decode_payload::<PlayerInputMsg>(data)?,
        )),
        MessageType::ChatMessage => Ok(ClientMessage::ChatMessage(
            decode_payload::<ChatMessageMsg>(data)?,
        )),
        MessageType::ClaimAlert => Ok(ClientMessage::ClaimAlert(decode_payload::<ClaimAlertMsg>(
            data,
        )?)),
        MessageType::OverlayConfig => Ok(ClientMessage::OverlayConfig(decode_payload::<
            OverlayConfigMsg,
        >(data)?)),
        MessageType::RequestGameStart => Ok(ClientMessage::RequestGameStart(decode_payload::<
            RequestGameStartMsg,
        >(data)?)),
        _ => Err(ProtocolError::UnknownMessageType(data[0])),
    }
}

/// Decode raw wire data into a `ServerMessage`.
pub fn decode_server_message(data: &[u8]) -> Result<ServerMessage, ProtocolError> {
    let msg_type = decode_message_type(data)?;
    match msg_type {
        MessageType::JoinRoomResponse => Ok(ServerMessage::JoinRoomResponse(decode_payload::<
            JoinRoomResponseMsg,
        >(data)?)),
        MessageType::PlayerList => Ok(ServerMessage::PlayerList(decode_payload::<PlayerListMsg>(
            data,
        )?)),
        MessageType::RoomConfigMsg => Ok(ServerMessage::RoomConfig(decode_payload::<
            RoomConfigPayload,
        >(data)?)),
        MessageType::GameState => Ok(ServerMessage::GameState(decode_payload::<GameStateMsg>(
            data,
        )?)),
        MessageType::GameStart => Ok(ServerMessage::GameStart(decode_payload::<GameStartMsg>(
            data,
        )?)),
        MessageType::RoundEnd => Ok(ServerMessage::RoundEnd(decode_payload::<RoundEndMsg>(
            data,
        )?)),
        MessageType::GameEnd => Ok(ServerMessage::GameEnd(decode_payload::<GameEndMsg>(data)?)),
        MessageType::AlertEvent => Ok(ServerMessage::AlertEvent(Box::new(decode_payload::<
            AlertEventMsg,
        >(data)?))),
        MessageType::AlertClaimed => Ok(ServerMessage::AlertClaimed(decode_payload::<
            AlertClaimedMsg,
        >(data)?)),
        MessageType::AlertDismissed => Ok(ServerMessage::AlertDismissed(decode_payload::<
            AlertDismissedMsg,
        >(data)?)),
        MessageType::OverlayConfig => Ok(ServerMessage::OverlayConfig(decode_payload::<
            OverlayConfigMsg,
        >(data)?)),
        _ => Err(ProtocolError::UnknownMessageType(data[0])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Event, EventType, Priority};
    use crate::player::{Player, PlayerColor};
    use crate::room::RoomConfig;
    use std::collections::HashMap;

    fn test_player() -> Player {
        Player {
            id: 42,
            display_name: "Alice".to_string(),
            color: PlayerColor::default(),
            is_leader: true,
            is_spectator: false,
        }
    }

    fn test_event() -> Event {
        Event {
            id: "evt-1".to_string(),
            event_type: EventType::PrOpened,
            source: "github".to_string(),
            priority: Priority::Notice,
            title: "PR #1 opened".to_string(),
            body: None,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            url: None,
            actor: Some("bot".to_string()),
            tags: vec![],
            action_required: false,
            group_key: None,
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn roundtrip_join_room() {
        let msg = ClientMessage::JoinRoom(JoinRoomMsg {
            room_code: "ABCD-1234".to_string(),
            player_name: "Alice".to_string(),
            player_color: PlayerColor::default(),
            protocol_version: PROTOCOL_VERSION,
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_leave_room() {
        let msg = ClientMessage::LeaveRoom(LeaveRoomMsg { player_id: 7 });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_player_input() {
        let msg = ClientMessage::PlayerInput(PlayerInputMsg {
            player_id: 1,
            tick: 100,
            input_data: vec![0xDE, 0xAD],
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_chat_message() {
        let msg = ClientMessage::ChatMessage(ChatMessageMsg {
            player_id: 3,
            content: "Hello world!".to_string(),
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_claim_alert() {
        let msg = ClientMessage::ClaimAlert(ClaimAlertMsg {
            player_id: 5,
            event_id: "evt-123".to_string(),
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    /// Test decoding a PlayerInput message encoded by JS msgpackr
    /// (with Vec<u8> as array-of-integers, not binary).
    #[test]
    fn decode_player_input_from_js_encoding() {
        // Exact wire bytes captured from browser test:
        // 01 93 55 01 9c cc93 ccca 00 00 00 00 ccca 3f 19 cc99 cc9a ccc3
        let wire: Vec<u8> = vec![
            0x01, // type byte: PlayerInput
            0x93, // fixarray(3): [player_id, tick, input_data]
            0x55, // fixint(85): player_id
            0x01, // fixint(1): tick
            0x9C, // fixarray(12): input_data
            0xCC, 0x93, // uint8(147)
            0xCC, 0xCA, // uint8(202)
            0x00, // fixint(0)
            0x00, // fixint(0)
            0x00, // fixint(0)
            0x00, // fixint(0)
            0xCC, 0xCA, // uint8(202)
            0x3F, // fixint(63)
            0x19, // fixint(25)
            0xCC, 0x99, // uint8(153)
            0xCC, 0x9A, // uint8(154)
            0xCC, 0xC3, // uint8(195)
        ];

        let decoded = decode_client_message(&wire).expect("should decode PlayerInput from JS");
        match decoded {
            ClientMessage::PlayerInput(pi) => {
                assert_eq!(pi.player_id, 85);
                assert_eq!(pi.tick, 1);
                // input_data should be the 12 raw bytes of the GolfInput
                assert_eq!(
                    pi.input_data,
                    vec![
                        0x93, 0xCA, 0x00, 0x00, 0x00, 0x00, 0xCA, 0x3F, 0x19, 0x99, 0x9A, 0xC3
                    ]
                );
            },
            other => panic!("Expected PlayerInput, got {:?}", other),
        }
    }

    #[test]
    fn roundtrip_join_room_response() {
        let msg = ServerMessage::JoinRoomResponse(JoinRoomResponseMsg {
            success: true,
            player_id: Some(42),
            room_code: Some("ABCD-1234".to_string()),
            room_state: Some(crate::room::RoomState::Lobby),
            error: None,
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_player_list() {
        let msg = ServerMessage::PlayerList(PlayerListMsg {
            players: vec![test_player()],
            leader_id: 42,
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_room_config() {
        let msg = ServerMessage::RoomConfig(RoomConfigPayload {
            config: RoomConfig::default(),
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_game_state() {
        let msg = ServerMessage::GameState(GameStateMsg {
            tick: 500,
            state_data: vec![1, 2, 3, 4, 5],
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_game_start() {
        let msg = ServerMessage::GameStart(GameStartMsg {
            game_name: "mini-golf".to_string(),
            players: vec![test_player()],
            leader_id: 42,
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_round_end() {
        use crate::net::messages::PlayerScoreEntry;
        let msg = ServerMessage::RoundEnd(RoundEndMsg {
            round: 3,
            scores: vec![PlayerScoreEntry {
                player_id: 42,
                score: 5,
            }],
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_game_end() {
        use crate::net::messages::PlayerScoreEntry;
        let msg = ServerMessage::GameEnd(GameEndMsg {
            final_scores: vec![PlayerScoreEntry {
                player_id: 1,
                score: 10,
            }],
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_alert_event() {
        let msg = ServerMessage::AlertEvent(Box::new(AlertEventMsg {
            event: test_event(),
        }));
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_alert_claimed() {
        let msg = ServerMessage::AlertClaimed(AlertClaimedMsg {
            event_id: "evt-1".to_string(),
            claimed_by: 42,
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_alert_dismissed() {
        let msg = ServerMessage::AlertDismissed(AlertDismissedMsg {
            event_id: "evt-1".to_string(),
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn decode_empty_message_fails() {
        let result = decode_message_type(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_unknown_type_fails() {
        let result = decode_message_type(&[0xFF]);
        assert!(result.is_err());
    }

    #[test]
    fn message_type_byte_prefix() {
        let msg = ClientMessage::JoinRoom(JoinRoomMsg {
            room_code: "ABCD-1234".to_string(),
            player_name: "Test".to_string(),
            player_color: PlayerColor::default(),
            protocol_version: PROTOCOL_VERSION,
        });
        let encoded = encode_client_message(&msg).unwrap();
        assert_eq!(encoded[0], MessageType::JoinRoom as u8);
    }

    // ================================================================
    // Additional edge-case protocol tests (Phase 3)
    // ================================================================

    #[test]
    fn roundtrip_overlay_config() {
        use crate::overlay::config::OverlayConfigMsg;
        let msg = ClientMessage::OverlayConfig(OverlayConfigMsg {
            room_config: crate::overlay::config::OverlayRoomConfig::default(),
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_request_game_start() {
        let msg = ClientMessage::RequestGameStart(RequestGameStartMsg {
            game_name: "mini-golf".to_string(),
        });
        let encoded = encode_client_message(&msg).unwrap();
        let decoded = decode_client_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_server_overlay_config() {
        use crate::overlay::config::OverlayConfigMsg;
        let msg = ServerMessage::OverlayConfig(OverlayConfigMsg {
            room_config: crate::overlay::config::OverlayRoomConfig::default(),
        });
        let encoded = encode_server_message(&msg).unwrap();
        let decoded = decode_server_message(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn decode_client_msg_with_server_type_fails() {
        // Encode a server message, then try to decode as client → should fail
        let msg = ServerMessage::GameState(GameStateMsg {
            tick: 1,
            state_data: vec![],
        });
        let encoded = encode_server_message(&msg).unwrap();
        let result = decode_client_message(&encoded);
        assert!(
            result.is_err(),
            "Server message type should fail as client message"
        );
    }

    #[test]
    fn decode_server_msg_with_client_type_fails() {
        // Encode a client message, then try to decode as server → should fail
        let msg = ClientMessage::PlayerInput(PlayerInputMsg {
            player_id: 1,
            tick: 0,
            input_data: vec![],
        });
        let encoded = encode_client_message(&msg).unwrap();
        let result = decode_server_message(&encoded);
        assert!(
            result.is_err(),
            "Client message type should fail as server message"
        );
    }

    #[test]
    fn message_type_from_byte_exhaustive() {
        // Test all known byte values
        let known: Vec<(u8, MessageType)> = vec![
            (0x01, MessageType::PlayerInput),
            (0x02, MessageType::JoinRoom),
            (0x03, MessageType::LeaveRoom),
            (0x04, MessageType::ClaimAlert),
            (0x05, MessageType::ChatMessage),
            (0x06, MessageType::JoinRoomResponse),
            (0x10, MessageType::GameState),
            (0x11, MessageType::PlayerList),
            (0x12, MessageType::RoomConfigMsg),
            (0x13, MessageType::GameStart),
            (0x14, MessageType::RoundEnd),
            (0x15, MessageType::GameEnd),
            (0x20, MessageType::AlertEvent),
            (0x21, MessageType::AlertClaimed),
            (0x22, MessageType::AlertDismissed),
            (0x23, MessageType::OverlayConfig),
            (0x30, MessageType::RequestGameStart),
        ];
        for (byte, expected) in &known {
            assert_eq!(
                MessageType::from_byte(*byte),
                Some(*expected),
                "Byte 0x{byte:02x} should map to {expected:?}"
            );
        }

        // All other bytes should return None
        for byte in 0u8..=255 {
            if known.iter().any(|(b, _)| *b == byte) {
                continue;
            }
            assert!(
                MessageType::from_byte(byte).is_none(),
                "Byte 0x{byte:02x} should not map to any MessageType"
            );
        }
    }

    #[test]
    fn encode_message_preserves_type_byte() {
        // Verify all client message variants have the correct type prefix
        let cases: Vec<(ClientMessage, u8)> = vec![
            (
                ClientMessage::JoinRoom(JoinRoomMsg {
                    room_code: String::new(),
                    player_name: "A".to_string(),
                    player_color: PlayerColor::default(),
                    protocol_version: 0,
                }),
                0x02,
            ),
            (
                ClientMessage::LeaveRoom(LeaveRoomMsg { player_id: 1 }),
                0x03,
            ),
            (
                ClientMessage::PlayerInput(PlayerInputMsg {
                    player_id: 1,
                    tick: 0,
                    input_data: vec![],
                }),
                0x01,
            ),
            (
                ClientMessage::ChatMessage(ChatMessageMsg {
                    player_id: 1,
                    content: "hi".to_string(),
                }),
                0x05,
            ),
            (
                ClientMessage::ClaimAlert(ClaimAlertMsg {
                    player_id: 1,
                    event_id: "e1".to_string(),
                }),
                0x04,
            ),
            (
                ClientMessage::RequestGameStart(RequestGameStartMsg {
                    game_name: "g".to_string(),
                }),
                0x30,
            ),
        ];
        for (msg, expected_byte) in cases {
            let encoded = encode_client_message(&msg).unwrap();
            assert_eq!(
                encoded[0],
                expected_byte,
                "Type byte mismatch for {:?}",
                msg.message_type()
            );
        }
    }

    #[test]
    fn protocol_error_display() {
        assert_eq!(format!("{}", ProtocolError::EmptyMessage), "empty message");
        assert_eq!(
            format!("{}", ProtocolError::UnknownMessageType(0xFF)),
            "unknown message type: 0xff"
        );
        assert!(format!("{}", ProtocolError::PayloadTooLarge(99999)).contains("99999"));
        assert!(format!("{}", ProtocolError::SerializeError("boom".into())).contains("boom"));
        assert!(format!("{}", ProtocolError::DeserializeError("oops".into())).contains("oops"));
    }

    #[test]
    fn payload_too_large_rejected() {
        // Create a message with a payload exceeding MAX_MESSAGE_SIZE
        let huge_data = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let msg = ClientMessage::PlayerInput(PlayerInputMsg {
            player_id: 1,
            tick: 0,
            input_data: huge_data,
        });
        let result = encode_client_message(&msg);
        assert!(result.is_err(), "Oversized payload should be rejected");
        if let Err(ProtocolError::PayloadTooLarge(_)) = result {
            // expected
        } else {
            panic!("Expected PayloadTooLarge error");
        }
    }
}
