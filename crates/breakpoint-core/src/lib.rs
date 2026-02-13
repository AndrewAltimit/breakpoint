pub mod events;
pub mod game_registry;
pub mod game_trait;
pub mod net;
pub mod overlay;
pub mod player;
pub mod powerup;
pub mod room;
pub mod time;

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    use std::collections::HashMap;
    use std::time::Duration;

    use crate::events::{Event, EventType, Priority};
    use crate::game_trait::{GameConfig, PlayerId};
    use crate::player::{Player, PlayerColor};

    /// Create `n` test players with sequential IDs starting at 1.
    pub fn make_players(n: usize) -> Vec<Player> {
        (0..n)
            .map(|i| Player {
                id: i as PlayerId + 1,
                display_name: format!("Player{}", i + 1),
                color: PlayerColor::default(),
                is_host: i == 0,
                is_spectator: false,
            })
            .collect()
    }

    /// Create a default GameConfig with the given round duration in seconds.
    pub fn default_config(round_duration_secs: u64) -> GameConfig {
        GameConfig {
            round_count: 1,
            round_duration: Duration::from_secs(round_duration_secs),
            custom: HashMap::new(),
        }
    }

    /// Create a generic test event with the given id.
    pub fn make_test_event(id: &str) -> Event {
        Event {
            id: id.to_string(),
            event_type: EventType::PrOpened,
            source: "test".to_string(),
            priority: Priority::Notice,
            title: format!("Test event {id}"),
            body: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            url: None,
            actor: None,
            tags: vec![],
            action_required: false,
            group_key: None,
            expires_at: None,
            metadata: HashMap::new(),
        }
    }
}
