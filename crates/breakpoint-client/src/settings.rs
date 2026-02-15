use bevy::prelude::*;

use breakpoint_core::overlay::config::{NotificationDensity, OverlayPlayerPrefs, ToastPosition};

use crate::app::AppState;
use crate::audio::AudioSettings;
use crate::theme::{Theme, rgb, rgba};

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SettingsState>()
            .insert_resource(PlayerPrefs(OverlayPlayerPrefs::default()))
            .add_systems(OnEnter(AppState::Lobby), load_player_prefs)
            .add_systems(Update, settings_toggle.run_if(in_state(AppState::Lobby)))
            .add_systems(Update, (settings_ui_system, settings_flash_system));
    }
}

/// Bevy-compatible wrapper for `OverlayPlayerPrefs`.
#[derive(Resource)]
pub struct PlayerPrefs(pub OverlayPlayerPrefs);

/// Tracks whether the settings panel is visible and save feedback.
#[derive(Resource, Default)]
struct SettingsState {
    visible: bool,
    /// Countdown timer for "Saved!" flash message.
    save_flash_timer: f32,
}

#[derive(Component)]
struct SettingsPanel;

#[derive(Component)]
struct SettingsValueText;

#[derive(Component)]
struct SaveFlashText;

#[derive(Component)]
enum SettingsButton {
    ToggleMute,
    VolumeUp,
    VolumeDown,
    ToastPositionCycle,
    DensityCycle,
    Close,
}

/// Toggle settings panel with Escape key when in lobby.
fn settings_toggle(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<SettingsState>,
    mut commands: Commands,
    panel_query: Query<Entity, With<SettingsPanel>>,
    theme: Res<Theme>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        settings.visible = !settings.visible;

        if !settings.visible {
            for entity in &panel_query {
                commands.entity(entity).despawn();
            }
        } else {
            spawn_settings_panel(&mut commands, &theme);
        }
    }
}

fn spawn_settings_panel(commands: &mut Commands, theme: &Theme) {
    let bg_color = rgba(&theme.ui.panel_bg);
    let btn_color = rgb(&theme.ui.settings_button);

    commands
        .spawn((
            SettingsPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(60.0),
                left: Val::Percent(25.0),
                width: Val::Percent(50.0),
                max_height: Val::Percent(80.0),
                padding: UiRect::all(Val::Px(16.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(bg_color),
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new("Settings"),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(rgb(&theme.ui.text_title)),
            ));

            // Audio section
            parent.spawn((
                Text::new("Audio"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(rgb(&theme.ui.text_secondary)),
            ));

            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_settings_btn(row, "Mute/Unmute", SettingsButton::ToggleMute, btn_color);
                    spawn_settings_btn(row, "Vol -", SettingsButton::VolumeDown, btn_color);
                    spawn_settings_btn(row, "Vol +", SettingsButton::VolumeUp, btn_color);
                });

            // Overlay section
            parent.spawn((
                Text::new("Overlay"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(rgb(&theme.ui.text_secondary)),
            ));

            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_settings_btn(
                        row,
                        "Toast Position",
                        SettingsButton::ToastPositionCycle,
                        btn_color,
                    );
                    spawn_settings_btn(row, "Density", SettingsButton::DensityCycle, btn_color);
                });

            // Current values display
            parent.spawn((
                SettingsValueText,
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(rgb(&theme.ui.text_accent)),
            ));

            // Save flash text (hidden initially)
            parent.spawn((
                SaveFlashText,
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(rgb(&theme.ui.save_flash)),
            ));

            // Close button
            spawn_settings_btn(
                parent,
                "Close (Esc)",
                SettingsButton::Close,
                rgb(&theme.ui.settings_close),
            );
        });
}

fn spawn_settings_btn(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: SettingsButton,
    color: Color,
) {
    parent
        .spawn((
            action,
            Button,
            Node {
                padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(color),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::WHITE),
        ));
}

#[allow(clippy::too_many_arguments)]
fn settings_ui_system(
    interaction_query: Query<(&Interaction, &SettingsButton), Changed<Interaction>>,
    mut audio_settings: ResMut<AudioSettings>,
    mut player_prefs: ResMut<PlayerPrefs>,
    mut settings_state: ResMut<SettingsState>,
    mut commands: Commands,
    panel_query: Query<Entity, With<SettingsPanel>>,
    mut value_text: Query<&mut Text, (With<SettingsValueText>, Without<SaveFlashText>)>,
    mut flash_text: Query<&mut Text, (With<SaveFlashText>, Without<SettingsValueText>)>,
) {
    // Update current values display
    if let Ok(mut text) = value_text.single_mut() {
        let mute_str = if audio_settings.muted {
            "Muted"
        } else {
            "Unmuted"
        };
        **text = format!(
            "Volume: {:.0}% ({}) | Toasts: {:?} | Density: {:?}",
            audio_settings.master_volume * 100.0,
            mute_str,
            player_prefs.0.toast_position,
            player_prefs.0.notification_density,
        );
    }

    for (interaction, button) in &interaction_query {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let mut saved = false;

        match button {
            SettingsButton::ToggleMute => {
                audio_settings.muted = !audio_settings.muted;
                save_audio_prefs(&audio_settings);
                saved = true;
            },
            SettingsButton::VolumeUp => {
                audio_settings.master_volume = (audio_settings.master_volume + 0.1).min(1.0);
                save_audio_prefs(&audio_settings);
                saved = true;
            },
            SettingsButton::VolumeDown => {
                audio_settings.master_volume = (audio_settings.master_volume - 0.1).max(0.0);
                save_audio_prefs(&audio_settings);
                saved = true;
            },
            SettingsButton::ToastPositionCycle => {
                player_prefs.0.toast_position = match player_prefs.0.toast_position {
                    ToastPosition::TopRight => ToastPosition::TopLeft,
                    ToastPosition::TopLeft => ToastPosition::BottomRight,
                    ToastPosition::BottomRight => ToastPosition::BottomLeft,
                    ToastPosition::BottomLeft => ToastPosition::TopRight,
                };
                save_overlay_prefs(&player_prefs.0);
                saved = true;
            },
            SettingsButton::DensityCycle => {
                player_prefs.0.notification_density = match player_prefs.0.notification_density {
                    NotificationDensity::All => NotificationDensity::Compact,
                    NotificationDensity::Compact => NotificationDensity::CriticalOnly,
                    NotificationDensity::CriticalOnly => NotificationDensity::All,
                };
                save_overlay_prefs(&player_prefs.0);
                saved = true;
            },
            SettingsButton::Close => {
                settings_state.visible = false;
                for entity in &panel_query {
                    commands.entity(entity).despawn();
                }
            },
        }

        if saved {
            settings_state.save_flash_timer = 1.5;
            if let Ok(mut text) = flash_text.single_mut() {
                **text = "Saved!".to_string();
            }
        }
    }
}

/// Fade out the "Saved!" flash message.
fn settings_flash_system(
    time: Res<Time>,
    mut settings: ResMut<SettingsState>,
    mut flash_text: Query<&mut Text, With<SaveFlashText>>,
) {
    if settings.save_flash_timer > 0.0 {
        settings.save_flash_timer -= time.delta_secs();
        if settings.save_flash_timer <= 0.0
            && let Ok(mut text) = flash_text.single_mut()
        {
            **text = String::new();
        }
    }
}

/// Persist audio settings to localStorage.
#[allow(unused_variables)]
fn save_audio_prefs(settings: &AudioSettings) {
    crate::storage::with_local_storage(|storage| {
        let _ = storage.set_item("audio_muted", &settings.muted.to_string());
        let _ = storage.set_item("audio_master_volume", &settings.master_volume.to_string());
    });
}

/// Persist overlay prefs to localStorage.
#[allow(unused_variables)]
fn save_overlay_prefs(prefs: &OverlayPlayerPrefs) {
    crate::storage::with_local_storage(|storage| {
        let _ = storage.set_item(
            "overlay_toast_position",
            &format!("{:?}", prefs.toast_position),
        );
        let _ = storage.set_item(
            "overlay_notification_density",
            &format!("{:?}", prefs.notification_density),
        );
    });
}

/// Load player prefs from localStorage on entering lobby.
fn load_player_prefs(mut prefs: ResMut<PlayerPrefs>) {
    crate::storage::with_local_storage(|storage| {
        if let Ok(Some(val)) = storage.get_item("overlay_toast_position") {
            prefs.0.toast_position = match val.as_str() {
                "TopRight" => ToastPosition::TopRight,
                "TopLeft" => ToastPosition::TopLeft,
                "BottomRight" => ToastPosition::BottomRight,
                "BottomLeft" => ToastPosition::BottomLeft,
                _ => ToastPosition::default(),
            };
        }
        if let Ok(Some(val)) = storage.get_item("overlay_notification_density") {
            prefs.0.notification_density = match val.as_str() {
                "All" => NotificationDensity::All,
                "Compact" => NotificationDensity::Compact,
                "CriticalOnly" => NotificationDensity::CriticalOnly,
                _ => NotificationDensity::default(),
            };
        }
    });
}
