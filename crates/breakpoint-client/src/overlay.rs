use breakpoint_core::events::{Event, Priority};
use breakpoint_core::game_trait::PlayerId;
use breakpoint_core::overlay::dashboard::DashboardFilter;
use breakpoint_core::overlay::ticker::TickerAggregator;
use breakpoint_core::overlay::toast::ToastQueue;

use crate::audio::{AudioEvent, AudioEventQueue};

/// Overlay network event, pushed by lobby/game systems, drained by overlay.
#[derive(Debug, Clone)]
pub enum OverlayNetEvent {
    AlertReceived(Box<Event>),
    AlertClaimed {
        event_id: String,
        claimed_by: String,
    },
    AlertDismissed {
        event_id: String,
    },
}

/// Simple message queue for overlay events.
#[derive(Default)]
pub struct OverlayEventQueue {
    pub events: Vec<OverlayNetEvent>,
}

impl OverlayEventQueue {
    pub fn push(&mut self, event: OverlayNetEvent) {
        self.events.push(event);
    }
}

/// Maximum recent events stored for the dashboard.
const MAX_RECENT_EVENTS: usize = 10;

/// Holds all overlay state (ticker, toasts, dashboard, badge).
pub struct OverlayState {
    pub ticker: TickerAggregator,
    pub toasts: ToastQueue,
    pub recent_events: Vec<Event>,
    pub dashboard_visible: bool,
    pub unread_count: u32,
    pub local_player_id: Option<PlayerId>,
    pub dashboard_filter: DashboardFilter,
}

impl OverlayState {
    pub fn new() -> Self {
        Self {
            ticker: TickerAggregator::new(120.0),
            toasts: ToastQueue::new(),
            recent_events: Vec::new(),
            dashboard_visible: false,
            unread_count: 0,
            local_player_id: None,
            dashboard_filter: DashboardFilter::default(),
        }
    }

    /// Process queued overlay events, routing to ticker or toasts.
    pub fn process_events(
        &mut self,
        queue: &mut OverlayEventQueue,
        audio_queue: &mut AudioEventQueue,
    ) {
        let events: Vec<OverlayNetEvent> = queue.events.drain(..).collect();
        for net_event in events {
            match net_event {
                OverlayNetEvent::AlertReceived(event) => {
                    let event = *event;
                    self.unread_count += 1;
                    self.recent_events.push(event.clone());
                    if self.recent_events.len() > MAX_RECENT_EVENTS {
                        self.recent_events.remove(0);
                    }

                    match event.priority {
                        Priority::Ambient => {
                            self.ticker.push(&event);
                        },
                        Priority::Notice => {
                            audio_queue.push(AudioEvent::NoticeChime);
                            self.toasts.push(event);
                        },
                        Priority::Urgent => {
                            audio_queue.push(AudioEvent::UrgentAttention);
                            self.toasts.push(event);
                        },
                        Priority::Critical => {
                            audio_queue.push(AudioEvent::CriticalAlert);
                            self.toasts.push(event);
                        },
                    }
                },
                OverlayNetEvent::AlertClaimed {
                    event_id,
                    claimed_by,
                } => {
                    self.toasts.mark_claimed(&event_id, claimed_by);
                },
                OverlayNetEvent::AlertDismissed { event_id } => {
                    self.toasts.dismiss(&event_id);
                },
            }
        }
    }

    /// Claim an alert via WebSocket.
    pub fn claim_alert(&self, event_id: &str, ws: &crate::net_client::WsClient) {
        let Some(player_id) = self.local_player_id else {
            return;
        };
        use breakpoint_core::net::messages::{ClaimAlertMsg, ClientMessage};
        use breakpoint_core::net::protocol::encode_client_message;

        let msg = ClientMessage::ClaimAlert(ClaimAlertMsg {
            player_id,
            event_id: event_id.to_string(),
        });
        if let Ok(data) = encode_client_message(&msg) {
            let _ = ws.send(&data);
        }
    }
}

impl Default for OverlayState {
    fn default() -> Self {
        Self::new()
    }
}
