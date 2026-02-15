use std::collections::VecDeque;

use breakpoint_core::events::Event;
use tokio::sync::broadcast;

/// Default maximum number of events stored before oldest are evicted.
const DEFAULT_MAX_STORED_EVENTS: usize = 500;

/// Default broadcast channel capacity for event fan-out.
const DEFAULT_BROADCAST_CAPACITY: usize = 1024;

/// An event stored in the EventStore with optional claim metadata.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub event: Event,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<String>,
}

/// Aggregate statistics about the event store.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EventStoreStats {
    pub total_stored: usize,
    pub total_claimed: usize,
    pub total_pending_actions: usize,
}

/// In-memory, bounded event store with broadcast fan-out.
pub struct EventStore {
    events: VecDeque<StoredEvent>,
    broadcast_tx: broadcast::Sender<Event>,
    max_stored_events: usize,
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EventStore {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_STORED_EVENTS, DEFAULT_BROADCAST_CAPACITY)
    }

    /// Create an EventStore with configurable capacity limits.
    pub fn with_capacity(max_stored_events: usize, broadcast_capacity: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(broadcast_capacity);
        Self {
            events: VecDeque::new(),
            broadcast_tx,
            max_stored_events,
        }
    }

    /// Insert a new event. Evicts the oldest event if at capacity.
    /// Also broadcasts the event to all subscribers.
    pub fn insert(&mut self, event: Event) {
        let _ = self.broadcast_tx.send(event.clone());
        self.events.push_back(StoredEvent {
            event,
            claimed_by: None,
            claimed_at: None,
        });
        while self.events.len() > self.max_stored_events {
            self.events.pop_front();
        }
    }

    /// Get a stored event by id.
    #[cfg(test)]
    pub fn get(&self, event_id: &str) -> Option<&StoredEvent> {
        self.events.iter().find(|e| e.event.id == event_id)
    }

    /// Claim an event. Returns true if the event was found and claimed.
    pub fn claim(&mut self, event_id: &str, claimed_by: String, claimed_at: String) -> bool {
        if let Some(stored) = self.events.iter_mut().find(|e| e.event.id == event_id) {
            stored.claimed_by = Some(claimed_by);
            stored.claimed_at = Some(claimed_at);
            true
        } else {
            false
        }
    }

    /// Get the most recent N events.
    pub fn recent(&self, count: usize) -> Vec<&StoredEvent> {
        self.events.iter().rev().take(count).collect()
    }

    /// Get all events with `action_required` that have not been claimed.
    pub fn pending_actions(&self) -> Vec<&StoredEvent> {
        self.events
            .iter()
            .filter(|e| e.event.action_required && e.claimed_by.is_none())
            .collect()
    }

    /// Subscribe to the broadcast channel for new events.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.broadcast_tx.subscribe()
    }

    /// Get aggregate statistics.
    pub fn stats(&self) -> EventStoreStats {
        let total_stored = self.events.len();
        let total_claimed = self
            .events
            .iter()
            .filter(|e| e.claimed_by.is_some())
            .count();
        let total_pending_actions = self
            .events
            .iter()
            .filter(|e| e.event.action_required && e.claimed_by.is_none())
            .count();
        EventStoreStats {
            total_stored,
            total_claimed,
            total_pending_actions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use breakpoint_core::events::{EventType, Priority};
    use std::collections::HashMap;

    fn make_event(id: &str) -> Event {
        Event {
            id: id.to_string(),
            event_type: EventType::PrOpened,
            source: "test".to_string(),
            priority: Priority::Notice,
            title: format!("Test event {id}"),
            body: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            url: None,
            actor: Some("bot".to_string()),
            tags: vec![],
            action_required: false,
            group_key: None,
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    fn make_action_event(id: &str) -> Event {
        let mut e = make_event(id);
        e.action_required = true;
        e
    }

    #[test]
    fn insert_and_retrieve() {
        let mut store = EventStore::new();
        store.insert(make_event("evt-1"));
        assert_eq!(store.get("evt-1").unwrap().event.id, "evt-1");
        assert!(store.get("nonexistent").is_none());
    }

    #[test]
    fn bounded_eviction() {
        let mut store = EventStore::new();
        for i in 0..600 {
            store.insert(make_event(&format!("evt-{i}")));
        }
        assert_eq!(store.events.len(), DEFAULT_MAX_STORED_EVENTS);
        // Oldest events (0..99) should be evicted
        assert!(store.get("evt-0").is_none());
        assert!(store.get("evt-99").is_none());
        assert!(store.get("evt-100").is_some());
        assert!(store.get("evt-599").is_some());
    }

    #[test]
    fn custom_capacity() {
        let mut store = EventStore::with_capacity(10, 16);
        for i in 0..20 {
            store.insert(make_event(&format!("evt-{i}")));
        }
        assert_eq!(store.events.len(), 10);
        assert!(store.get("evt-0").is_none());
        assert!(store.get("evt-10").is_some());
    }

    #[test]
    fn claim_and_unclaimed() {
        let mut store = EventStore::new();
        store.insert(make_action_event("evt-1"));
        store.insert(make_action_event("evt-2"));

        assert_eq!(store.pending_actions().len(), 2);

        let claimed = store.claim(
            "evt-1",
            "alice".to_string(),
            "2026-01-01T00:01:00Z".to_string(),
        );
        assert!(claimed);
        assert_eq!(store.pending_actions().len(), 1);

        let stored = store.get("evt-1").unwrap();
        assert_eq!(stored.claimed_by.as_deref(), Some("alice"));

        // Claiming nonexistent event returns false
        assert!(!store.claim(
            "nope",
            "bob".to_string(),
            "2026-01-01T00:02:00Z".to_string()
        ));
    }

    #[test]
    fn recent_returns_newest_first() {
        let mut store = EventStore::new();
        store.insert(make_event("evt-1"));
        store.insert(make_event("evt-2"));
        store.insert(make_event("evt-3"));

        let recent = store.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].event.id, "evt-3");
        assert_eq!(recent[1].event.id, "evt-2");
    }

    #[test]
    fn stats_are_correct() {
        let mut store = EventStore::new();
        store.insert(make_action_event("evt-1"));
        store.insert(make_event("evt-2"));
        store.insert(make_action_event("evt-3"));
        store.claim(
            "evt-1",
            "alice".to_string(),
            "2026-01-01T00:01:00Z".to_string(),
        );

        let stats = store.stats();
        assert_eq!(stats.total_stored, 3);
        assert_eq!(stats.total_claimed, 1);
        assert_eq!(stats.total_pending_actions, 1);
    }

    #[tokio::test]
    async fn broadcast_subscriber_receives_events() {
        let mut store = EventStore::new();
        let mut rx = store.subscribe();

        store.insert(make_event("evt-1"));

        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, "evt-1");
    }
}
