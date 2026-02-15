use bevy::prelude::*;

use breakpoint_core::game_trait::PlayerId;

use crate::app::AppState;
use crate::game::RoundTracker;
use crate::lobby::LobbyState;
use crate::theme::{Theme, rgb, rgba};

pub struct GameOverPlugin;

impl Plugin for GameOverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::GameOver), setup_game_over)
            .add_systems(Update, game_over_input.run_if(in_state(AppState::GameOver)))
            .add_systems(OnExit(AppState::GameOver), cleanup_game_over);
    }
}

/// Marker for game-over UI entities.
#[derive(Component)]
struct GameOverUi;

/// Button to return to lobby.
#[derive(Component)]
struct ReturnToLobbyButton;

fn setup_game_over(
    mut commands: Commands,
    round_tracker: Res<RoundTracker>,
    lobby: Res<LobbyState>,
    theme: Res<Theme>,
) {
    let bg_color = rgba(&theme.ui.panel_bg);
    let text_color = rgb(&theme.ui.text_primary);

    // Build final standings sorted by cumulative score (descending)
    let mut scores: Vec<(PlayerId, i32)> = round_tracker
        .cumulative_scores
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    scores.sort_by(|a, b| b.1.cmp(&a.1));

    let winner_name = scores.first().and_then(|(pid, _)| {
        lobby
            .players
            .iter()
            .find(|p| p.id == *pid)
            .map(|p| p.display_name.clone())
    });

    commands
        .spawn((
            GameOverUi,
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
                Text::new("Game Over!"),
                TextFont {
                    font_size: 48.0,
                    ..default()
                },
                TextColor(rgb(&theme.ui.score_gold)),
            ));

            if let Some(name) = &winner_name {
                parent.spawn((
                    Text::new(format!("Winner: {name}")),
                    TextFont {
                        font_size: 28.0,
                        ..default()
                    },
                    TextColor(rgb(&theme.ui.score_positive)),
                ));
            }

            parent.spawn((
                Text::new("Final Standings"),
                TextFont {
                    font_size: 22.0,
                    ..default()
                },
                TextColor(text_color),
            ));

            for (i, (pid, score)) in scores.iter().enumerate() {
                let name = lobby
                    .players
                    .iter()
                    .find(|p| p.id == *pid)
                    .map(|p| p.display_name.as_str())
                    .unwrap_or("???");

                let medal = match i {
                    0 => " [1st]",
                    1 => " [2nd]",
                    2 => " [3rd]",
                    _ => "",
                };

                parent.spawn((
                    Text::new(format!("{}. {} - {} pts{medal}", i + 1, name, score)),
                    TextFont {
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(if i == 0 {
                        rgb(&theme.ui.score_gold)
                    } else {
                        text_color
                    }),
                ));
            }

            // Return to lobby button
            parent
                .spawn((
                    ReturnToLobbyButton,
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(24.0), Val::Px(12.0)),
                        margin: UiRect::top(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(rgb(&theme.ui.return_button)),
                ))
                .with_child((
                    Text::new("Return to Lobby"),
                    TextFont {
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
        });
}

fn game_over_input(
    interaction_query: Query<&Interaction, (With<ReturnToLobbyButton>, Changed<Interaction>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::Lobby);
        }
    }
}

fn cleanup_game_over(mut commands: Commands, query: Query<Entity, With<GameOverUi>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}
