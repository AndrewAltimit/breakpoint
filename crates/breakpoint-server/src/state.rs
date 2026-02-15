use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::RwLock;

use crate::auth::AuthConfig;
use crate::config::ServerConfig;
use crate::event_store::EventStore;
use crate::game_loop::ServerGameRegistry;
use crate::room_manager::RoomManager;

pub type SharedRoomManager = Arc<RwLock<RoomManager>>;
pub type SharedEventStore = Arc<RwLock<EventStore>>;

/// Maximum concurrent WebSocket connections.
pub const MAX_WS_CONNECTIONS: usize = 200;

/// Maximum concurrent SSE subscribers.
pub const MAX_SSE_SUBSCRIBERS: usize = 100;

#[derive(Clone)]
pub struct AppState {
    pub rooms: SharedRoomManager,
    pub event_store: SharedEventStore,
    pub auth: AuthConfig,
    pub game_registry: Arc<ServerGameRegistry>,
    #[allow(dead_code)]
    pub config: Arc<ServerConfig>,
    pub ws_connection_count: Arc<AtomicUsize>,
    pub sse_subscriber_count: Arc<AtomicUsize>,
}

impl AppState {
    pub fn new(config: ServerConfig) -> Self {
        let auth = AuthConfig {
            bearer_token: config.auth.bearer_token.clone(),
            github_webhook_secret: config.auth.github_webhook_secret.clone(),
            require_webhook_signature: config.auth.require_webhook_signature,
        };
        Self {
            rooms: Arc::new(RwLock::new(RoomManager::new())),
            event_store: Arc::new(RwLock::new(EventStore::new())),
            auth,
            game_registry: Arc::new(ServerGameRegistry::new()),
            config: Arc::new(config),
            ws_connection_count: Arc::new(AtomicUsize::new(0)),
            sse_subscriber_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

/// RAII guard that decrements a counter on drop.
pub struct ConnectionGuard {
    counter: Arc<AtomicUsize>,
}

impl ConnectionGuard {
    pub fn new(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self { counter }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}
