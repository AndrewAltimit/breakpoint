use serde::{Deserialize, Serialize};

use crate::events::Event;
use crate::game_trait::PlayerId;
use crate::overlay::config::OverlayConfigMsg;
use crate::player::{Player, PlayerColor};
use crate::room::{RoomConfig, RoomState};

/// Network message type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    // Client -> Server
    PlayerInput = 0x01,
    JoinRoom = 0x02,
    LeaveRoom = 0x03,
    ClaimAlert = 0x04,
    ChatMessage = 0x05,
    RequestGameStart = 0x30,
    AddBot = 0x31,
    RemoveBot = 0x32,

    // Server -> Client
    JoinRoomResponse = 0x06,

    // Server -> Client (game lifecycle)
    GameState = 0x10,
    PlayerList = 0x11,
    RoomConfigMsg = 0x12,
    GameStart = 0x13,
    RoundEnd = 0x14,
    GameEnd = 0x15,

    // Server -> Client (Alert channel)
    AlertEvent = 0x20,
    AlertClaimed = 0x21,
    AlertDismissed = 0x22,

    // Overlay config
    OverlayConfig = 0x23,
}

impl MessageType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::PlayerInput),
            0x02 => Some(Self::JoinRoom),
            0x03 => Some(Self::LeaveRoom),
            0x04 => Some(Self::ClaimAlert),
            0x05 => Some(Self::ChatMessage),
            0x06 => Some(Self::JoinRoomResponse),
            0x10 => Some(Self::GameState),
            0x11 => Some(Self::PlayerList),
            0x12 => Some(Self::RoomConfigMsg),
            0x13 => Some(Self::GameStart),
            0x14 => Some(Self::RoundEnd),
            0x15 => Some(Self::GameEnd),
            0x20 => Some(Self::AlertEvent),
            0x21 => Some(Self::AlertClaimed),
            0x22 => Some(Self::AlertDismissed),
            0x23 => Some(Self::OverlayConfig),
            0x30 => Some(Self::RequestGameStart),
            0x31 => Some(Self::AddBot),
            0x32 => Some(Self::RemoveBot),
            _ => None,
        }
    }
}

// --- Payload structs ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinRoomMsg {
    pub room_code: String,
    pub player_name: String,
    pub player_color: PlayerColor,
    /// Protocol version for compatibility checks. Defaults to 0 for
    /// backwards compatibility with clients that don't send this field.
    #[serde(default)]
    pub protocol_version: u8,
    /// Session token from a previous connection, used for reconnection.
    #[serde(default)]
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinRoomResponseMsg {
    pub success: bool,
    pub player_id: Option<PlayerId>,
    pub room_code: Option<String>,
    pub room_state: Option<RoomState>,
    pub error: Option<String>,
    /// Session token for reconnection. Clients should store this and send
    /// it back in JoinRoomMsg to reclaim their player slot.
    #[serde(default)]
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LeaveRoomMsg {
    pub player_id: PlayerId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerInputMsg {
    pub player_id: PlayerId,
    pub tick: u32,
    pub input_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessageMsg {
    pub player_id: PlayerId,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestGameStartMsg {
    pub game_name: String,
    #[serde(default)]
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddBotMsg {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoveBotMsg {
    pub player_id: PlayerId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaimAlertMsg {
    pub player_id: PlayerId,
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerListMsg {
    pub players: Vec<Player>,
    pub leader_id: PlayerId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomConfigPayload {
    pub config: RoomConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameStateMsg {
    pub tick: u32,
    pub state_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameStartMsg {
    pub game_name: String,
    pub players: Vec<Player>,
    pub leader_id: PlayerId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoundEndMsg {
    pub round: u8,
    pub scores: Vec<PlayerScoreEntry>,
    /// Seconds until the next round starts.
    #[serde(default)]
    pub between_round_secs: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerScoreEntry {
    pub player_id: PlayerId,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameEndMsg {
    pub final_scores: Vec<PlayerScoreEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertEventMsg {
    pub event: Event,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertClaimedMsg {
    pub event_id: String,
    pub claimed_by: PlayerId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertDismissedMsg {
    pub event_id: String,
}

// --- Unified message enums ---

/// Messages sent from client to server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClientMessage {
    JoinRoom(JoinRoomMsg),
    LeaveRoom(LeaveRoomMsg),
    PlayerInput(PlayerInputMsg),
    ChatMessage(ChatMessageMsg),
    ClaimAlert(ClaimAlertMsg),
    OverlayConfig(OverlayConfigMsg),
    RequestGameStart(RequestGameStartMsg),
    AddBot(AddBotMsg),
    RemoveBot(RemoveBotMsg),
}

impl ClientMessage {
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::JoinRoom(_) => MessageType::JoinRoom,
            Self::LeaveRoom(_) => MessageType::LeaveRoom,
            Self::PlayerInput(_) => MessageType::PlayerInput,
            Self::ChatMessage(_) => MessageType::ChatMessage,
            Self::ClaimAlert(_) => MessageType::ClaimAlert,
            Self::OverlayConfig(_) => MessageType::OverlayConfig,
            Self::RequestGameStart(_) => MessageType::RequestGameStart,
            Self::AddBot(_) => MessageType::AddBot,
            Self::RemoveBot(_) => MessageType::RemoveBot,
        }
    }
}

/// Messages sent from server to client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServerMessage {
    JoinRoomResponse(JoinRoomResponseMsg),
    PlayerList(PlayerListMsg),
    RoomConfig(RoomConfigPayload),
    GameState(GameStateMsg),
    GameStart(GameStartMsg),
    RoundEnd(RoundEndMsg),
    GameEnd(GameEndMsg),
    AlertEvent(Box<AlertEventMsg>),
    AlertClaimed(AlertClaimedMsg),
    AlertDismissed(AlertDismissedMsg),
    OverlayConfig(OverlayConfigMsg),
}

impl ServerMessage {
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::JoinRoomResponse(_) => MessageType::JoinRoomResponse,
            Self::PlayerList(_) => MessageType::PlayerList,
            Self::RoomConfig(_) => MessageType::RoomConfigMsg,
            Self::GameState(_) => MessageType::GameState,
            Self::GameStart(_) => MessageType::GameStart,
            Self::RoundEnd(_) => MessageType::RoundEnd,
            Self::GameEnd(_) => MessageType::GameEnd,
            Self::AlertEvent(_) => MessageType::AlertEvent,
            Self::AlertClaimed(_) => MessageType::AlertClaimed,
            Self::AlertDismissed(_) => MessageType::AlertDismissed,
            Self::OverlayConfig(_) => MessageType::OverlayConfig,
        }
    }
}
