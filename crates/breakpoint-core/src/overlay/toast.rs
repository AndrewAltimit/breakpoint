use crate::events::Event;

/// Maximum number of simultaneously visible toast notifications.
pub const MAX_VISIBLE_TOASTS: usize = 3;

/// Default auto-dismiss duration for notice-level toasts in seconds.
pub const DEFAULT_TOAST_DURATION_SECS: u64 = 8;

/// A toast notification queued for display.
#[derive(Debug, Clone)]
pub struct Toast {
    pub event: Event,
    pub dismissed: bool,
    pub claimed_by: Option<String>,
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
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self::new()
    }
}
