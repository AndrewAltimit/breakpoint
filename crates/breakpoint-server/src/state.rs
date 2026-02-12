use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::AuthConfig;
use crate::event_store::EventStore;
use crate::room_manager::RoomManager;

pub type SharedRoomManager = Arc<RwLock<RoomManager>>;
pub type SharedEventStore = Arc<RwLock<EventStore>>;

#[derive(Clone)]
pub struct AppState {
    pub rooms: SharedRoomManager,
    pub event_store: SharedEventStore,
    pub auth: AuthConfig,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(RoomManager::new())),
            event_store: Arc::new(RwLock::new(EventStore::new())),
            auth: AuthConfig::from_env(),
        }
    }
}
