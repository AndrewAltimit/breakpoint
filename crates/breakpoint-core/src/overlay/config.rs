use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::events::Priority;

/// Position of the scrolling ticker bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TickerPosition {
    Top,
    #[default]
    Bottom,
}

/// Position of toast notifications.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToastPosition {
    #[default]
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

/// Controls how many notifications are shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationDensity {
    #[default]
    All,
    Compact,
    CriticalOnly,
}

/// Room-level overlay configuration set by the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayRoomConfig {
    /// Which event sources are enabled (empty = all).
    pub enabled_sources: Vec<String>,
    /// Override priority for specific event types (event_type string -> priority).
    pub priority_overrides: HashMap<String, Priority>,
    /// Where to display the ticker bar.
    pub ticker_position: TickerPosition,
    /// Auto-expand dashboard between rounds.
    pub dashboard_auto_expand_between_rounds: bool,
    /// Whether critical alerts pause all players.
    pub critical_alert_pauses_all: bool,
}

impl Default for OverlayRoomConfig {
    fn default() -> Self {
        Self {
            enabled_sources: Vec::new(),
            priority_overrides: HashMap::new(),
            ticker_position: TickerPosition::default(),
            dashboard_auto_expand_between_rounds: true,
            critical_alert_pauses_all: false,
        }
    }
}

/// Per-player overlay preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayPlayerPrefs {
    pub volume_ambient: f32,
    pub volume_notice: f32,
    pub volume_urgent: f32,
    pub volume_critical: f32,
    pub toast_position: ToastPosition,
    pub dashboard_hotkey: String,
    pub notification_density: NotificationDensity,
}

impl Default for OverlayPlayerPrefs {
    fn default() -> Self {
        Self {
            volume_ambient: 0.3,
            volume_notice: 0.6,
            volume_urgent: 0.8,
            volume_critical: 1.0,
            toast_position: ToastPosition::default(),
            dashboard_hotkey: "Tab".to_string(),
            notification_density: NotificationDensity::default(),
        }
    }
}

/// Combined overlay config message payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayConfigMsg {
    pub room_config: OverlayRoomConfig,
}
