use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::{GameId, PlayerId};

use crate::app::AppState;
use crate::game::{NetworkRole, RoundTracker};
use crate::lobby::LobbyState;
use crate::net_client::WsClient;
use crate::overlay::OverlayState;

pub struct BetweenRoundsPlugin;

impl Plugin for BetweenRoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::BetweenRounds), setup_between_rounds)
            .add_systems(
                Update,
                (between_rounds_countdown, between_rounds_network)
                    .run_if(in_state(AppState::BetweenRounds)),
            )
            .add_systems(OnExit(AppState::BetweenRounds), cleanup_between_rounds);
    }
}

/// Marker for between-rounds UI entities.
#[derive(Component)]
struct BetweenRoundsUi;

/// Countdown timer resource.
#[derive(Resource)]
struct BetweenRoundTimer {
    remaining: f32,
}

/// Marker for the countdown text.
#[derive(Component)]
struct CountdownText;

fn setup_between_rounds(
    mut commands: Commands,
    round_tracker: Res<RoundTracker>,
    lobby: Res<LobbyState>,
    mut overlay_state: ResMut<OverlayState>,
    course_info: Option<Res<crate::game::golf_plugin::GolfCourseInfo>>,
) {
    // Auto-expand dashboard overlay during between-rounds
    overlay_state.dashboard_visible = true;

    commands.insert_resource(BetweenRoundTimer { remaining: 30.0 });

    let bg_color = Color::srgba(0.05, 0.05, 0.12, 0.9);
    let text_color = Color::srgb(0.9, 0.9, 0.9);

    // Build scoreboard entries sorted by cumulative score (descending)
    let mut scores: Vec<(PlayerId, i32)> = round_tracker
        .cumulative_scores
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    scores.sort_by(|a, b| b.1.cmp(&a.1));

    let scoreboard_lines: Vec<String> = scores
        .iter()
        .enumerate()
        .map(|(i, (pid, score))| {
            let name = lobby
                .players
                .iter()
                .find(|p| p.id == *pid)
                .map(|p| p.display_name.as_str())
                .unwrap_or("???");
            format!("{}. {} - {} pts", i + 1, name, score)
        })
        .collect();

    commands
        .spawn((
            BetweenRoundsUi,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(bg_color),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(format!("Round {} Complete", round_tracker.current_round)),
                TextFont {
                    font_size: 36.0,
                    ..default()
                },
                TextColor(Color::srgb(0.3, 0.7, 1.0)),
            ));

            parent.spawn((
                Text::new("Scoreboard"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(text_color),
            ));

            // Scoreboard entries
            for line in &scoreboard_lines {
                parent.spawn((
                    Text::new(line.clone()),
                    TextFont {
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(text_color),
                ));
            }

            // Next hole info (golf-specific)
            if let Some(ref info) = course_info {
                let next_hole = round_tracker.current_round as usize; // 0-indexed next
                let courses = breakpoint_golf::course::all_courses();
                if next_hole < courses.len() {
                    let next_course = &courses[next_hole];
                    parent.spawn((
                        Text::new(format!(
                            "Hole {} of {} — Next: {} — Par {}",
                            next_hole + 1,
                            info.total_holes,
                            next_course.name,
                            next_course.par
                        )),
                        TextFont {
                            font_size: 20.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.3, 0.9, 0.3)),
                    ));
                }
            }

            // Countdown
            parent.spawn((
                CountdownText,
                Text::new("Next round in 30s"),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.3)),
            ));
        });
}

/// Tick down the between-round countdown and update the display.
fn between_rounds_countdown(
    time: Res<Time>,
    mut timer: ResMut<BetweenRoundTimer>,
    mut text_query: Query<&mut Text, With<CountdownText>>,
) {
    timer.remaining -= time.delta_secs();
    if let Ok(mut text) = text_query.single_mut() {
        let secs = timer.remaining.ceil().max(0.0) as u32;
        **text = format!("Next round in {secs}s");
    }
}

/// All clients listen for the server's GameStart to enter the next round.
fn between_rounds_network(
    ws_client: NonSend<WsClient>,
    mut next_state: ResMut<NextState<AppState>>,
    mut network_role: ResMut<NetworkRole>,
    mut overlay_queue: ResMut<crate::overlay::OverlayEventQueue>,
    mut lobby: ResMut<LobbyState>,
) {
    let messages = ws_client.drain_messages();
    for data in messages {
        let msg = match breakpoint_core::net::protocol::decode_server_message(&data) {
            Ok(m) => m,
            Err(_) => continue,
        };

        match msg {
            breakpoint_core::net::messages::ServerMessage::GameStart(gs) => {
                lobby.selected_game = GameId::from_str_opt(&gs.game_name).unwrap_or_default();
                // Promote spectators: check if our player is active in the new round
                if network_role.is_spectator {
                    let is_active = gs
                        .players
                        .iter()
                        .any(|p| p.id == network_role.local_player_id && !p.is_spectator);
                    if is_active {
                        network_role.is_spectator = false;
                        lobby.is_spectator = false;
                    }
                }
                next_state.set(AppState::InGame);
            },
            breakpoint_core::net::messages::ServerMessage::AlertEvent(ae) => {
                overlay_queue.push(crate::overlay::OverlayNetEvent::AlertReceived(Box::new(
                    ae.event,
                )));
            },
            breakpoint_core::net::messages::ServerMessage::AlertClaimed(ac) => {
                overlay_queue.push(crate::overlay::OverlayNetEvent::AlertClaimed {
                    event_id: ac.event_id,
                    claimed_by: ac.claimed_by.to_string(),
                });
            },
            breakpoint_core::net::messages::ServerMessage::AlertDismissed(ad) => {
                overlay_queue.push(crate::overlay::OverlayNetEvent::AlertDismissed {
                    event_id: ad.event_id,
                });
            },
            _ => {},
        }
    }
}

fn cleanup_between_rounds(mut commands: Commands, query: Query<Entity, With<BetweenRoundsUi>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<BetweenRoundTimer>();
}
