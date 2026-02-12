use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};

use breakpoint_core::net::messages::MessageType;

/// A connected client in a relay room.
struct RelayClient {
    tx: mpsc::UnboundedSender<Vec<u8>>,
}

/// A relay room: first joiner is host, subsequent are clients.
struct RelayRoom {
    host_tx: mpsc::UnboundedSender<Vec<u8>>,
    clients: HashMap<u64, RelayClient>,
    next_id: u64,
}

impl RelayRoom {
    fn new(host_tx: mpsc::UnboundedSender<Vec<u8>>) -> Self {
        Self {
            host_tx,
            clients: HashMap::new(),
            next_id: 1,
        }
    }

    fn add_client(&mut self, tx: mpsc::UnboundedSender<Vec<u8>>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.clients.insert(id, RelayClient { tx });
        id
    }

    fn remove_client(&mut self, id: u64) {
        self.clients.remove(&id);
    }

    /// Forward message from a client to the host.
    fn forward_to_host(&self, data: &[u8]) {
        let _ = self.host_tx.send(data.to_vec());
    }

    /// Forward message from the host to all clients.
    fn forward_to_all_clients(&self, data: &[u8]) {
        for client in self.clients.values() {
            let _ = client.tx.send(data.to_vec());
        }
    }

    fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

/// Manages all relay rooms.
#[allow(dead_code)]
pub struct RelayState {
    rooms: HashMap<String, RelayRoom>,
    max_rooms: usize,
}

impl RelayState {
    pub fn new(max_rooms: usize) -> Self {
        Self {
            rooms: HashMap::new(),
            max_rooms,
        }
    }

    /// Create a new room, returning the room code. The creator is the host.
    pub fn create_room(
        &mut self,
        code: String,
        host_tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), String> {
        if self.rooms.len() >= self.max_rooms {
            return Err("Maximum room limit reached".to_string());
        }
        if self.rooms.contains_key(&code) {
            return Err("Room already exists".to_string());
        }
        self.rooms.insert(code, RelayRoom::new(host_tx));
        Ok(())
    }

    /// Join an existing room as a client. Returns a client ID.
    pub fn join_room(
        &mut self,
        code: &str,
        tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<u64, String> {
        let room = self
            .rooms
            .get_mut(code)
            .ok_or_else(|| "Room not found".to_string())?;
        Ok(room.add_client(tx))
    }

    /// Remove a client from a room. Returns true if the room was destroyed.
    pub fn leave_room(&mut self, code: &str, client_id: u64) -> bool {
        if let Some(room) = self.rooms.get_mut(code) {
            room.remove_client(client_id);
            if room.is_empty() {
                self.rooms.remove(code);
                return true;
            }
        }
        false
    }

    /// Remove a room entirely (when host disconnects).
    pub fn destroy_room(&mut self, code: &str) {
        self.rooms.remove(code);
    }

    /// Forward a message from a client to the host.
    pub fn relay_to_host(&self, code: &str, data: &[u8]) {
        if let Some(room) = self.rooms.get(code) {
            room.forward_to_host(data);
        }
    }

    /// Forward a message from the host to all clients.
    pub fn relay_to_clients(&self, code: &str, data: &[u8]) {
        if let Some(room) = self.rooms.get(code) {
            room.forward_to_all_clients(data);
        }
    }

    pub fn room_exists(&self, code: &str) -> bool {
        self.rooms.contains_key(code)
    }

    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }
}

/// Shared relay state behind an async RwLock.
pub type SharedRelayState = Arc<RwLock<RelayState>>;

/// Peek at the first byte of a message to determine routing.
/// Returns the MessageType if recognizable.
pub fn peek_message_type(data: &[u8]) -> Option<MessageType> {
    if data.is_empty() {
        return None;
    }
    MessageType::from_byte(data[0])
}

/// Determine if a message type should be forwarded from host to clients.
pub fn is_host_to_client(msg_type: MessageType) -> bool {
    matches!(
        msg_type,
        MessageType::JoinRoomResponse
            | MessageType::GameState
            | MessageType::PlayerList
            | MessageType::RoomConfigMsg
            | MessageType::GameStart
            | MessageType::RoundEnd
            | MessageType::GameEnd
            | MessageType::AlertEvent
            | MessageType::AlertClaimed
            | MessageType::AlertDismissed
            | MessageType::OverlayConfig
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_join_room() {
        let mut state = RelayState::new(10);
        let (host_tx, _host_rx) = mpsc::unbounded_channel();
        state.create_room("ABCD-1234".to_string(), host_tx).unwrap();

        let (client_tx, _client_rx) = mpsc::unbounded_channel();
        let client_id = state.join_room("ABCD-1234", client_tx).unwrap();
        assert_eq!(client_id, 1);
        assert!(state.room_exists("ABCD-1234"));
    }

    #[test]
    fn join_nonexistent_room_fails() {
        let mut state = RelayState::new(10);
        let (tx, _rx) = mpsc::unbounded_channel();
        assert!(state.join_room("NOPE-0000", tx).is_err());
    }

    #[test]
    fn max_rooms_enforced() {
        let mut state = RelayState::new(1);
        let (tx1, _rx1) = mpsc::unbounded_channel();
        state.create_room("AAAA-0001".to_string(), tx1).unwrap();
        let (tx2, _rx2) = mpsc::unbounded_channel();
        assert!(state.create_room("BBBB-0002".to_string(), tx2).is_err());
    }

    #[test]
    fn leave_room_cleanup() {
        let mut state = RelayState::new(10);
        let (host_tx, _host_rx) = mpsc::unbounded_channel();
        state.create_room("ABCD-1234".to_string(), host_tx).unwrap();

        let (client_tx, _client_rx) = mpsc::unbounded_channel();
        let cid = state.join_room("ABCD-1234", client_tx).unwrap();

        // Remove the only client — room still exists (host is still there)
        let destroyed = state.leave_room("ABCD-1234", cid);
        assert!(destroyed); // empty of clients, removed
    }

    #[test]
    fn forward_to_host() {
        let mut state = RelayState::new(10);
        let (host_tx, mut host_rx) = mpsc::unbounded_channel();
        state.create_room("ABCD-1234".to_string(), host_tx).unwrap();

        let (client_tx, _client_rx) = mpsc::unbounded_channel();
        let _cid = state.join_room("ABCD-1234", client_tx).unwrap();

        state.relay_to_host("ABCD-1234", &[0x01, 0x02, 0x03]);
        let received = host_rx.try_recv().unwrap();
        assert_eq!(received, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn forward_to_clients() {
        let mut state = RelayState::new(10);
        let (host_tx, _host_rx) = mpsc::unbounded_channel();
        state.create_room("ABCD-1234".to_string(), host_tx).unwrap();

        let (client_tx1, mut client_rx1) = mpsc::unbounded_channel();
        let _cid1 = state.join_room("ABCD-1234", client_tx1).unwrap();
        let (client_tx2, mut client_rx2) = mpsc::unbounded_channel();
        let _cid2 = state.join_room("ABCD-1234", client_tx2).unwrap();

        state.relay_to_clients("ABCD-1234", &[0x10, 0x20]);
        assert_eq!(client_rx1.try_recv().unwrap(), vec![0x10, 0x20]);
        assert_eq!(client_rx2.try_recv().unwrap(), vec![0x10, 0x20]);
    }

    #[test]
    fn host_disconnect_destroys_room() {
        let mut state = RelayState::new(10);
        let (host_tx, _host_rx) = mpsc::unbounded_channel();
        state.create_room("ABCD-1234".to_string(), host_tx).unwrap();
        assert!(state.room_exists("ABCD-1234"));

        state.destroy_room("ABCD-1234");
        assert!(!state.room_exists("ABCD-1234"));
    }

    #[test]
    fn peek_message_type_works() {
        assert_eq!(peek_message_type(&[0x01]), Some(MessageType::PlayerInput));
        assert_eq!(peek_message_type(&[0x10]), Some(MessageType::GameState));
        assert_eq!(peek_message_type(&[0xFF]), None);
        assert_eq!(peek_message_type(&[]), None);
    }

    #[test]
    fn host_to_client_routing() {
        assert!(is_host_to_client(MessageType::GameState));
        assert!(is_host_to_client(MessageType::PlayerList));
        assert!(!is_host_to_client(MessageType::PlayerInput));
        assert!(!is_host_to_client(MessageType::JoinRoom));
    }
}
