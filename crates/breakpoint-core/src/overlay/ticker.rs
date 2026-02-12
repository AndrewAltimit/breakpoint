use crate::events::Event;

/// Aggregates ambient events for the scrolling ticker display.
pub struct TickerAggregator {
    events: Vec<TickerEntry>,
    max_age_secs: u64,
}

/// A single entry in the ambient ticker.
pub struct TickerEntry {
    pub text: String,
    pub group_key: Option<String>,
    pub count: u32,
    pub timestamp: String,
}

impl TickerAggregator {
    pub fn new(max_age_secs: u64) -> Self {
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
            entry.timestamp = event.timestamp.clone();
            return;
        }
        self.events.push(TickerEntry {
            text: event.title.clone(),
            group_key: event.group_key.clone(),
            count: 1,
            timestamp: event.timestamp.clone(),
        });
    }

    /// Get all current ticker entries.
    pub fn entries(&self) -> &[TickerEntry] {
        &self.events
    }

    /// Maximum age in seconds before entries are removed.
    pub fn max_age_secs(&self) -> u64 {
        self.max_age_secs
    }
}
