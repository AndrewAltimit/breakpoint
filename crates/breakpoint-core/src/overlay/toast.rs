use crate::events::Event;

/// Maximum number of simultaneously visible toast notifications.
pub const MAX_VISIBLE_TOASTS: usize = 3;

/// Default auto-dismiss duration for notice-level toasts in seconds.
pub const DEFAULT_TOAST_DURATION_SECS: f32 = 8.0;

/// A toast notification queued for display.
#[derive(Debug, Clone)]
pub struct Toast {
    pub event: Event,
    pub dismissed: bool,
    pub claimed_by: Option<String>,
    /// Time remaining before auto-dismiss (seconds).
    pub time_remaining: f32,
}

/// Queue managing toast notification display.
pub struct ToastQueue {
    visible: Vec<Toast>,
    pending: Vec<Toast>,
}

impl ToastQueue {
    pub fn new() -> Self {
        Self {
            visible: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Add a new toast to the queue.
    pub fn push(&mut self, event: Event) {
        let toast = Toast {
            event,
            dismissed: false,
            claimed_by: None,
            time_remaining: DEFAULT_TOAST_DURATION_SECS,
        };
        if self.visible.len() < MAX_VISIBLE_TOASTS {
            self.visible.push(toast);
        } else {
            self.pending.push(toast);
        }
    }

    /// Get currently visible toasts.
    pub fn visible(&self) -> &[Toast] {
        &self.visible
    }

    /// Dismiss a toast by event id. Returns true if found.
    pub fn dismiss(&mut self, event_id: &str) -> bool {
        if let Some(toast) = self.visible.iter_mut().find(|t| t.event.id == event_id) {
            toast.dismissed = true;
            true
        } else {
            false
        }
    }

    /// Mark a toast as claimed by a player name.
    pub fn mark_claimed(&mut self, event_id: &str, claimed_by: String) {
        for toast in self.visible.iter_mut().chain(self.pending.iter_mut()) {
            if toast.event.id == event_id {
                toast.claimed_by = Some(claimed_by);
                return;
            }
        }
    }

    /// Remove expired/dismissed toasts from visible, promote from pending.
    pub fn prune_expired(&mut self) {
        self.visible
            .retain(|t| !t.dismissed && t.time_remaining > 0.0);

        while self.visible.len() < MAX_VISIBLE_TOASTS {
            if let Some(toast) = self.pending.pop() {
                self.visible.push(toast);
            } else {
                break;
            }
        }
    }

    /// Decrement time_remaining on all visible toasts.
    pub fn tick(&mut self, delta_secs: f32) {
        for toast in &mut self.visible {
            toast.time_remaining -= delta_secs;
        }
    }

    /// Number of pending (not yet visible) toasts.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::make_test_event;

    #[test]
    fn dismiss_toast() {
        let mut q = ToastQueue::new();
        q.push(make_test_event("evt-1"));
        q.push(make_test_event("evt-2"));

        assert!(q.dismiss("evt-1"));
        assert!(!q.dismiss("nonexistent"));
        assert!(q.visible()[0].dismissed);
    }

    #[test]
    fn mark_claimed_toast() {
        let mut q = ToastQueue::new();
        q.push(make_test_event("evt-1"));
        q.mark_claimed("evt-1", "alice".to_string());
        assert_eq!(q.visible()[0].claimed_by.as_deref(), Some("alice"));
    }

    #[test]
    fn prune_promotes_pending() {
        let mut q = ToastQueue::new();
        for i in 0..5 {
            q.push(make_test_event(&format!("evt-{i}")));
        }
        assert_eq!(q.visible().len(), MAX_VISIBLE_TOASTS);
        assert_eq!(q.pending_count(), 2);

        // Dismiss one visible toast
        q.dismiss("evt-0");
        q.prune_expired();

        assert_eq!(q.visible().len(), MAX_VISIBLE_TOASTS);
        assert_eq!(q.pending_count(), 1);
    }

    #[test]
    fn prune_removes_expired() {
        let mut q = ToastQueue::new();
        q.push(make_test_event("evt-1"));
        q.visible.get_mut(0).unwrap().time_remaining = 0.0;
        q.prune_expired();
        assert!(q.visible().is_empty());
    }
}
