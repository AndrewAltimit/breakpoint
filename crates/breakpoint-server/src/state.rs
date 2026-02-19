use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::auth::AuthConfig;
use crate::config::ServerConfig;
use crate::event_store::EventStore;
use crate::game_loop::ServerGameRegistry;
use crate::rate_limit::IpRateLimiter;
use crate::room_manager::RoomManager;

pub type SharedRoomManager = Arc<RwLock<RoomManager>>;
pub type SharedEventStore = Arc<RwLock<EventStore>>;

#[derive(Clone)]
pub struct AppState {
    pub rooms: SharedRoomManager,
    pub event_store: SharedEventStore,
    pub auth: AuthConfig,
    pub game_registry: Arc<ServerGameRegistry>,
    pub config: Arc<ServerConfig>,
    pub ws_connection_count: Arc<AtomicUsize>,
    pub sse_subscriber_count: Arc<AtomicUsize>,
    pub api_rate_limiter: Arc<IpRateLimiter>,
    pub ws_per_ip: Arc<Mutex<HashMap<IpAddr, usize>>>,
    pub shutdown: CancellationToken,
}

impl AppState {
    pub fn new(config: ServerConfig) -> Self {
        let auth = AuthConfig {
            bearer_token: config.auth.bearer_token.clone(),
            github_webhook_secret: config.auth.github_webhook_secret.clone(),
            require_webhook_signature: config.auth.require_webhook_signature,
        };
        let event_store = EventStore::with_capacity(
            config.limits.max_stored_events,
            config.limits.broadcast_capacity,
        );
        let api_rate_limiter = Arc::new(IpRateLimiter::new(
            config.limits.api_rate_limit_burst as f64,
            config.limits.api_rate_limit_per_sec,
        ));
        Self {
            rooms: Arc::new(RwLock::new(RoomManager::new())),
            event_store: Arc::new(RwLock::new(event_store)),
            auth,
            game_registry: Arc::new(ServerGameRegistry::new()),
            config: Arc::new(config),
            ws_connection_count: Arc::new(AtomicUsize::new(0)),
            sse_subscriber_count: Arc::new(AtomicUsize::new(0)),
            api_rate_limiter,
            ws_per_ip: Arc::new(Mutex::new(HashMap::new())),
            shutdown: CancellationToken::new(),
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

/// RAII guard that tracks per-IP WebSocket connection count.
pub struct IpConnectionGuard {
    ip: IpAddr,
    ws_per_ip: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl IpConnectionGuard {
    /// Attempt to acquire a per-IP connection slot. Returns `None` if the
    /// limit is exceeded.
    pub async fn try_acquire(
        ip: IpAddr,
        ws_per_ip: Arc<Mutex<HashMap<IpAddr, usize>>>,
        max_per_ip: usize,
    ) -> Option<Self> {
        let mut map = ws_per_ip.lock().await;
        let count = map.entry(ip).or_insert(0);
        if *count >= max_per_ip {
            return None;
        }
        *count += 1;
        drop(map);
        Some(Self { ip, ws_per_ip })
    }
}

impl Drop for IpConnectionGuard {
    fn drop(&mut self) {
        // Best-effort: spawn a task to decrement since we can't block in Drop
        let ip = self.ip;
        let ws_per_ip = Arc::clone(&self.ws_per_ip);
        tokio::spawn(async move {
            let mut map = ws_per_ip.lock().await;
            if let Some(count) = map.get_mut(&ip) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    map.remove(&ip);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn connection_guard_increments_and_decrements() {
        let counter = Arc::new(AtomicUsize::new(0));
        assert_eq!(counter.load(Ordering::Relaxed), 0);

        let guard = ConnectionGuard::new(Arc::clone(&counter));
        assert_eq!(counter.load(Ordering::Relaxed), 1);

        drop(guard);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn ip_guard_acquires_and_rejects_at_limit() {
        let ws_per_ip: Arc<Mutex<HashMap<IpAddr, usize>>> = Arc::new(Mutex::new(HashMap::new()));
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let max_per_ip = 2;

        let guard1 = IpConnectionGuard::try_acquire(ip, Arc::clone(&ws_per_ip), max_per_ip).await;
        assert!(guard1.is_some(), "First acquire should succeed");

        let guard2 = IpConnectionGuard::try_acquire(ip, Arc::clone(&ws_per_ip), max_per_ip).await;
        assert!(guard2.is_some(), "Second acquire should succeed");

        let guard3 = IpConnectionGuard::try_acquire(ip, Arc::clone(&ws_per_ip), max_per_ip).await;
        assert!(
            guard3.is_none(),
            "Third acquire should be rejected at limit"
        );

        // Keep guards alive so they aren't dropped early
        drop(guard1);
        drop(guard2);
    }

    #[tokio::test]
    async fn ip_guard_drop_decrements_counter() {
        let ws_per_ip: Arc<Mutex<HashMap<IpAddr, usize>>> = Arc::new(Mutex::new(HashMap::new()));
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        let guard = IpConnectionGuard::try_acquire(ip, Arc::clone(&ws_per_ip), 5)
            .await
            .unwrap();

        // Verify counter is 1
        {
            let map = ws_per_ip.lock().await;
            assert_eq!(*map.get(&ip).unwrap(), 1);
        }

        // Drop the guard — its Drop impl spawns a task
        drop(guard);

        // Yield to let the spawned decrement task run
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        // Counter should be 0 and the entry removed
        {
            let map = ws_per_ip.lock().await;
            assert!(
                map.get(&ip).is_none(),
                "Entry should be removed after last guard is dropped"
            );
        }
    }
}
