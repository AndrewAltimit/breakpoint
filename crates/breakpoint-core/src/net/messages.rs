use serde::{Deserialize, Serialize};

/// Network message type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    // Client -> Host
    PlayerInput = 0x01,
    JoinRoom = 0x02,
    LeaveRoom = 0x03,
    ClaimAlert = 0x04,
    ChatMessage = 0x05,

    // Host -> Client
    GameState = 0x10,
    PlayerList = 0x11,
    RoomConfig = 0x12,
    GameStart = 0x13,
    RoundEnd = 0x14,
    GameEnd = 0x15,

    // Host -> Client (Alert channel)
    AlertEvent = 0x20,
    AlertClaimed = 0x21,
    AlertDismissed = 0x22,
}
