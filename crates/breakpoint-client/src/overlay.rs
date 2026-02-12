use bevy::ecs::system::NonSend;
use bevy::input::ButtonInput;
use bevy::prelude::*;

use breakpoint_core::events::{Event, Priority};
use breakpoint_core::game_trait::PlayerId;
use breakpoint_core::net::messages::{ClaimAlertMsg, ClientMessage};
use breakpoint_core::net::protocol::encode_client_message;
use breakpoint_core::overlay::ticker::TickerAggregator;
use breakpoint_core::overlay::toast::ToastQueue;

use crate::net_client::WsClient;

pub struct OverlayPlugin;

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(OverlayState::new())
            .insert_resource(OverlayEventQueue::default())
            .add_systems(Startup, setup_overlay_ui)
            .add_systems(
                Update,
                (
                    overlay_event_intake,
                    ticker_render,
                    toast_tick,
                    toast_render,
                    dashboard_toggle,
                    dashboard_render,
                    alert_badge_render,
                    claim_button_system,
                ),
            );
    }
}

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

/// Simple message queue resource for overlay events.
#[derive(Resource, Default)]
pub struct OverlayEventQueue {
    pub events: Vec<OverlayNetEvent>,
}

impl OverlayEventQueue {
    pub fn push(&mut self, event: OverlayNetEvent) {
        self.events.push(event);
    }
}

/// Resource holding all overlay state.
#[derive(Resource)]
pub struct OverlayState {
    pub ticker: TickerAggregator,
    pub toasts: ToastQueue,
    pub recent_events: Vec<Event>,
    pub dashboard_visible: bool,
    pub unread_count: u32,
    pub local_player_id: Option<PlayerId>,
}

impl OverlayState {
    fn new() -> Self {
        Self {
            ticker: TickerAggregator::new(120.0),
            toasts: ToastQueue::new(),
            recent_events: Vec::new(),
            dashboard_visible: false,
            unread_count: 0,
            local_player_id: None,
        }
    }
}

/// Maximum recent events stored for the dashboard.
const MAX_RECENT_EVENTS: usize = 10;

// --- Marker components ---

#[derive(Component)]
struct OverlayUi;

#[derive(Component)]
struct TickerBar;

#[derive(Component)]
struct TickerText;

#[derive(Component)]
struct ToastContainer;

#[derive(Component)]
struct ToastNode {
    event_id: String,
}

#[derive(Component)]
struct ToastClaimButton {
    event_id: String,
}

#[derive(Component)]
struct AlertBadge;

#[derive(Component)]
struct AlertBadgeText;

#[derive(Component)]
struct DashboardPanel;

#[derive(Component)]
struct DashboardContent;

// --- Setup ---

fn setup_overlay_ui(mut commands: Commands) {
    // Bottom ticker bar
    commands
        .spawn((
            OverlayUi,
            TickerBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Px(28.0),
                padding: UiRect::horizontal(Val::Px(8.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|parent| {
            parent.spawn((
                TickerText,
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgba(0.8, 0.8, 0.8, 0.9)),
            ));
        });

    // Toast container (bottom-right, above ticker)
    commands.spawn((
        OverlayUi,
        ToastContainer,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(36.0),
            right: Val::Px(12.0),
            flex_direction: FlexDirection::ColumnReverse,
            row_gap: Val::Px(8.0),
            width: Val::Px(320.0),
            ..default()
        },
    ));

    // Alert badge (top-right)
    commands
        .spawn((
            OverlayUi,
            AlertBadge,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                right: Val::Px(60.0),
                width: Val::Px(28.0),
                height: Val::Px(28.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.8, 0.2, 0.2)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                AlertBadgeText,
                Text::new("0"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

// --- Systems ---

/// Intake overlay network events, route to ticker (Ambient) or toasts (Notice+).
fn overlay_event_intake(
    mut queue: ResMut<OverlayEventQueue>,
    mut overlay: ResMut<OverlayState>,
    mut audio_queue: ResMut<crate::audio::AudioEventQueue>,
) {
    let events: Vec<OverlayNetEvent> = queue.events.drain(..).collect();
    for net_event in events {
        match net_event {
            OverlayNetEvent::AlertReceived(event) => {
                let event = *event;
                overlay.unread_count += 1;

                // Store in recent events
                overlay.recent_events.push(event.clone());
                if overlay.recent_events.len() > MAX_RECENT_EVENTS {
                    overlay.recent_events.remove(0);
                }

                match event.priority {
                    Priority::Ambient => {
                        overlay.ticker.push(&event);
                    },
                    Priority::Notice => {
                        audio_queue.push(crate::audio::AudioEvent::NoticeChime);
                        overlay.toasts.push(event);
                    },
                    Priority::Urgent => {
                        audio_queue.push(crate::audio::AudioEvent::UrgentAttention);
                        overlay.toasts.push(event);
                    },
                    Priority::Critical => {
                        audio_queue.push(crate::audio::AudioEvent::CriticalAlert);
                        overlay.toasts.push(event);
                    },
                }
            },
            OverlayNetEvent::AlertClaimed {
                event_id,
                claimed_by,
            } => {
                overlay.toasts.mark_claimed(&event_id, claimed_by);
            },
            OverlayNetEvent::AlertDismissed { event_id } => {
                overlay.toasts.dismiss(&event_id);
            },
        }
    }
}

/// Update ticker bar text.
fn ticker_render(
    time: Res<Time>,
    mut overlay: ResMut<OverlayState>,
    mut text_query: Query<&mut Text, With<TickerText>>,
) {
    overlay.ticker.prune(time.delta_secs());

    if let Ok(mut text) = text_query.single_mut() {
        let display = overlay.ticker.display_text();
        if display.is_empty() {
            **text = String::new();
        } else {
            **text = display;
        }
    }
}

/// Tick toast timers.
fn toast_tick(time: Res<Time>, mut overlay: ResMut<OverlayState>) {
    let dt = time.delta_secs();
    overlay.toasts.tick(dt);
    overlay.toasts.prune_expired();
}

/// Render toast UI nodes - sync spawned toast nodes with visible toasts.
fn toast_render(
    mut commands: Commands,
    overlay: Res<OverlayState>,
    container_query: Query<Entity, With<ToastContainer>>,
    existing_toasts: Query<(Entity, &ToastNode)>,
) {
    let Ok(container) = container_query.single() else {
        return;
    };

    let visible = overlay.toasts.visible();
    let visible_ids: Vec<&str> = visible.iter().map(|t| t.event.id.as_str()).collect();

    // Remove toast nodes that are no longer visible
    for (entity, toast_node) in &existing_toasts {
        if !visible_ids.contains(&toast_node.event_id.as_str()) {
            commands.entity(entity).despawn();
        }
    }

    // Check which visible toasts already have nodes
    let existing_ids: Vec<String> = existing_toasts
        .iter()
        .map(|(_, tn)| tn.event_id.clone())
        .collect();

    // Spawn new toast nodes for newly visible toasts
    for toast in visible {
        if existing_ids.contains(&toast.event.id) {
            continue;
        }

        let bg_color = match toast.event.priority {
            Priority::Critical => Color::srgba(0.7, 0.1, 0.1, 0.9),
            Priority::Urgent => Color::srgba(0.7, 0.4, 0.1, 0.9),
            Priority::Notice => Color::srgba(0.15, 0.3, 0.6, 0.9),
            Priority::Ambient => Color::srgba(0.2, 0.2, 0.2, 0.9),
        };

        let event_id = toast.event.id.clone();
        let title = toast.event.title.clone();
        let source = toast.event.source.clone();
        let actor = toast.event.actor.clone().unwrap_or_default();
        let claimed = toast.claimed_by.clone();

        let toast_entity = commands
            .spawn((
                ToastNode {
                    event_id: event_id.clone(),
                },
                Node {
                    padding: UiRect::all(Val::Px(10.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    width: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(bg_color),
            ))
            .with_children(|parent| {
                // Title
                parent.spawn((
                    Text::new(title),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));

                // Source and actor
                let meta = if actor.is_empty() {
                    source
                } else {
                    format!("{source} - {actor}")
                };
                parent.spawn((
                    Text::new(meta),
                    TextFont {
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.7, 0.7, 0.7, 0.9)),
                ));

                // Claimed indicator or Handle button
                if let Some(ref claimer) = claimed {
                    parent.spawn((
                        Text::new(format!("Claimed by {claimer}")),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.3, 0.8, 0.3)),
                    ));
                } else {
                    parent
                        .spawn((
                            ToastClaimButton {
                                event_id: event_id.clone(),
                            },
                            Button,
                            Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.2, 0.5, 0.2)),
                        ))
                        .with_child((
                            Text::new("Handle"),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                }
            })
            .id();

        commands.entity(container).add_child(toast_entity);
    }
}

/// Toggle dashboard visibility with Tab key.
fn dashboard_toggle(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<OverlayState>,
    mut commands: Commands,
    dashboard_query: Query<Entity, With<DashboardPanel>>,
) {
    if keyboard.just_pressed(KeyCode::Tab) {
        overlay.dashboard_visible = !overlay.dashboard_visible;
        overlay.unread_count = 0;

        if !overlay.dashboard_visible {
            // Despawn dashboard
            for entity in &dashboard_query {
                commands.entity(entity).despawn();
            }
        } else {
            // Spawn dashboard
            commands
                .spawn((
                    OverlayUi,
                    DashboardPanel,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(50.0),
                        right: Val::Px(12.0),
                        width: Val::Px(360.0),
                        max_height: Val::Percent(70.0),
                        padding: UiRect::all(Val::Px(12.0)),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.15, 0.95)),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Event Dashboard"),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.3, 0.7, 1.0)),
                    ));

                    parent.spawn((
                        DashboardContent,
                        Text::new(""),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                    ));
                });
        }
    }
}

/// Update dashboard content when visible.
fn dashboard_render(
    overlay: Res<OverlayState>,
    mut content_query: Query<&mut Text, With<DashboardContent>>,
) {
    if !overlay.dashboard_visible {
        return;
    }

    if let Ok(mut text) = content_query.single_mut() {
        let mut lines = Vec::new();

        lines.push(format!(
            "Pending actions: {}",
            overlay.toasts.pending_count()
        ));
        lines.push(format!("Recent events: {}", overlay.recent_events.len()));
        lines.push(String::new());

        for event in overlay.recent_events.iter().rev() {
            let priority_tag = match event.priority {
                Priority::Critical => "[CRIT]",
                Priority::Urgent => "[URG]",
                Priority::Notice => "[NOTE]",
                Priority::Ambient => "[AMB]",
            };
            let actor = event.actor.as_deref().unwrap_or("");
            lines.push(format!("{priority_tag} {} - {actor}", event.title));
        }

        **text = lines.join("\n");
    }
}

/// Update alert badge unread count.
fn alert_badge_render(
    overlay: Res<OverlayState>,
    mut badge_vis: Query<&mut Visibility, With<AlertBadge>>,
    mut badge_text: Query<&mut Text, With<AlertBadgeText>>,
) {
    if let Ok(mut vis) = badge_vis.single_mut() {
        if overlay.unread_count > 0 && !overlay.dashboard_visible {
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
    if let Ok(mut text) = badge_text.single_mut() {
        **text = overlay.unread_count.to_string();
    }
}

/// Handle claim button clicks — send ClaimAlert via WSS.
fn claim_button_system(
    interaction_query: Query<(&Interaction, &ToastClaimButton), Changed<Interaction>>,
    overlay: Res<OverlayState>,
    ws_client: NonSend<WsClient>,
) {
    for (interaction, claim_btn) in &interaction_query {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let Some(player_id) = overlay.local_player_id else {
            continue;
        };

        let msg = ClientMessage::ClaimAlert(ClaimAlertMsg {
            player_id,
            event_id: claim_btn.event_id.clone(),
        });
        if let Ok(data) = encode_client_message(&msg) {
            let _ = ws_client.send(&data);
        }
    }
}
