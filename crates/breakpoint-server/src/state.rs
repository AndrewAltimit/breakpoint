use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::AuthConfig;
use crate::config::ServerConfig;
use crate::event_store::EventStore;
use crate::room_manager::RoomManager;

pub type SharedRoomManager = Arc<RwLock<RoomManager>>;
pub type SharedEventStore = Arc<RwLock<EventStore>>;

#[derive(Clone)]
pub struct AppState {
    pub rooms: SharedRoomManager,
    pub event_store: SharedEventStore,
    pub auth: AuthConfig,
    #[allow(dead_code)]
    pub config: Arc<ServerConfig>,
}

impl AppState {
    pub fn new(config: ServerConfig) -> Self {
        let auth = AuthConfig {
            bearer_token: config.auth.bearer_token.clone(),
            github_webhook_secret: config.auth.github_webhook_secret.clone(),
        };
        Self {
            rooms: Arc::new(RwLock::new(RoomManager::new())),
            event_store: Arc::new(RwLock::new(EventStore::new())),
            auth,
            config: Arc::new(config),
        }
    }
}
