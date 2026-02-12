use serde::{Deserialize, Serialize};

use crate::events::Event;
use crate::game_trait::PlayerId;
use crate::player::{Player, PlayerColor};
use crate::room::{RoomConfig, RoomState};

/// Network message type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    // Client -> Server/Host
    PlayerInput = 0x01,
    JoinRoom = 0x02,
    LeaveRoom = 0x03,
    ClaimAlert = 0x04,
    ChatMessage = 0x05,

    // Server -> Client
    JoinRoomResponse = 0x06,

    // Host -> Client
    GameState = 0x10,
    PlayerList = 0x11,
    RoomConfigMsg = 0x12,
    GameStart = 0x13,
    RoundEnd = 0x14,
    GameEnd = 0x15,

    // Host -> Client (Alert channel)
    AlertEvent = 0x20,
    AlertClaimed = 0x21,
    AlertDismissed = 0x22,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinRoomResponseMsg {
    pub success: bool,
    pub player_id: Option<PlayerId>,
    pub room_code: Option<String>,
    pub room_state: Option<RoomState>,
    pub error: Option<String>,
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
pub struct ClaimAlertMsg {
    pub player_id: PlayerId,
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerListMsg {
    pub players: Vec<Player>,
    pub host_id: PlayerId,
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
    pub host_id: PlayerId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoundEndMsg {
    pub round: u8,
    pub scores: Vec<PlayerScoreEntry>,
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

/// Messages sent from client to server/host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClientMessage {
    JoinRoom(JoinRoomMsg),
    LeaveRoom(LeaveRoomMsg),
    PlayerInput(PlayerInputMsg),
    ChatMessage(ChatMessageMsg),
    ClaimAlert(ClaimAlertMsg),
}

impl ClientMessage {
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::JoinRoom(_) => MessageType::JoinRoom,
            Self::LeaveRoom(_) => MessageType::LeaveRoom,
            Self::PlayerInput(_) => MessageType::PlayerInput,
            Self::ChatMessage(_) => MessageType::ChatMessage,
            Self::ClaimAlert(_) => MessageType::ClaimAlert,
        }
    }
}

/// Messages sent from server/host to client.
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
        }
    }
}
