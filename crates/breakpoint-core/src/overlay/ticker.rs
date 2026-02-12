use crate::events::Event;

/// Aggregates ambient events for the scrolling ticker display.
pub struct TickerAggregator {
    events: Vec<TickerEntry>,
    max_age_secs: f32,
}

/// A single entry in the ambient ticker.
pub struct TickerEntry {
    pub text: String,
    pub group_key: Option<String>,
    pub count: u32,
    pub age_secs: f32,
}

impl TickerAggregator {
    pub fn new(max_age_secs: f32) -> Self {
        Self {
            events: Vec::new(),
            max_age_secs,
        }
    }

    /// Push a new event into the ticker, aggregating by group_key.
    pub fn push(&mut self, event: &Event) {
        if let Some(ref key) = event.group_key
            && let Some(entry) = self
                .events
                .iter_mut()
                .find(|e| e.group_key.as_deref() == Some(key))
        {
            entry.count += 1;
            entry.age_secs = 0.0;
            return;
        }
        self.events.push(TickerEntry {
            text: event.title.clone(),
            group_key: event.group_key.clone(),
            count: 1,
            age_secs: 0.0,
        });
    }

    /// Get all current ticker entries.
    pub fn entries(&self) -> &[TickerEntry] {
        &self.events
    }

    /// Maximum age in seconds before entries are removed.
    pub fn max_age_secs(&self) -> f32 {
        self.max_age_secs
    }

    /// Remove entries older than max_age_secs. Call each frame with delta_secs.
    pub fn prune(&mut self, delta_secs: f32) {
        for entry in &mut self.events {
            entry.age_secs += delta_secs;
        }
        self.events.retain(|e| e.age_secs < self.max_age_secs);
    }

    /// Build a display string from all current entries.
    pub fn display_text(&self) -> String {
        self.events
            .iter()
            .map(|e| {
                if e.count > 1 {
                    format!("{} (x{})", e.text, e.count)
                } else {
                    e.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("  |  ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventType, Priority};
    use std::collections::HashMap;

    fn make_event(title: &str, group_key: Option<&str>) -> Event {
        Event {
            id: "evt-1".to_string(),
            event_type: EventType::BranchPushed,
            source: "test".to_string(),
            priority: Priority::Ambient,
            title: title.to_string(),
            body: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            url: None,
            actor: None,
            tags: vec![],
            action_required: false,
            group_key: group_key.map(String::from),
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn prune_removes_old_entries() {
        let mut ticker = TickerAggregator::new(10.0);
        ticker.push(&make_event("old event", None));
        ticker.prune(11.0);
        assert!(ticker.entries().is_empty());
    }

    #[test]
    fn prune_keeps_recent_entries() {
        let mut ticker = TickerAggregator::new(10.0);
        ticker.push(&make_event("recent event", None));
        ticker.prune(5.0);
        assert_eq!(ticker.entries().len(), 1);
    }

    #[test]
    fn display_text_single() {
        let mut ticker = TickerAggregator::new(60.0);
        ticker.push(&make_event("CI passed", None));
        assert_eq!(ticker.display_text(), "CI passed");
    }

    #[test]
    fn display_text_aggregated() {
        let mut ticker = TickerAggregator::new(60.0);
        ticker.push(&make_event("push to main", Some("github:test/repo")));
        ticker.push(&make_event("push to main", Some("github:test/repo")));
        assert_eq!(ticker.display_text(), "push to main (x2)");
    }

    #[test]
    fn display_text_multiple_entries() {
        let mut ticker = TickerAggregator::new(60.0);
        ticker.push(&make_event("event A", None));
        ticker.push(&make_event("event B", None));
        assert_eq!(ticker.display_text(), "event A  |  event B");
    }
}
