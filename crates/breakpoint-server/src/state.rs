use std::sync::Arc;
use tokio::sync::RwLock;

use crate::room_manager::RoomManager;

pub type SharedRoomManager = Arc<RwLock<RoomManager>>;

#[derive(Clone)]
pub struct AppState {
    pub rooms: SharedRoomManager,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(RoomManager::new())),
        }
    }
}
