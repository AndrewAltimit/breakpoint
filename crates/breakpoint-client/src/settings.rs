use bevy::prelude::*;

use breakpoint_core::overlay::config::{NotificationDensity, OverlayPlayerPrefs, ToastPosition};

use crate::app::AppState;
use crate::audio::AudioSettings;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SettingsState>()
            .insert_resource(PlayerPrefs(OverlayPlayerPrefs::default()))
            .add_systems(OnEnter(AppState::Lobby), load_player_prefs)
            .add_systems(Update, settings_toggle.run_if(in_state(AppState::Lobby)))
            .add_systems(Update, settings_ui_system);
    }
}

/// Bevy-compatible wrapper for `OverlayPlayerPrefs`.
#[derive(Resource)]
pub struct PlayerPrefs(pub OverlayPlayerPrefs);

/// Tracks whether the settings panel is visible.
#[derive(Resource, Default)]
struct SettingsState {
    visible: bool,
}

#[derive(Component)]
struct SettingsPanel;

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
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        settings.visible = !settings.visible;

        if !settings.visible {
            for entity in &panel_query {
                commands.entity(entity).despawn();
            }
        } else {
            spawn_settings_panel(&mut commands);
        }
    }
}

fn spawn_settings_panel(commands: &mut Commands) {
    let bg_color = Color::srgba(0.05, 0.05, 0.12, 0.95);
    let btn_color = Color::srgb(0.25, 0.25, 0.4);

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
                TextColor(Color::srgb(0.3, 0.7, 1.0)),
            ));

            // Audio section
            parent.spawn((
                Text::new("Audio"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
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
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
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

            // Close button
            spawn_settings_btn(
                parent,
                "Close (Esc)",
                SettingsButton::Close,
                Color::srgb(0.5, 0.2, 0.2),
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

fn settings_ui_system(
    interaction_query: Query<(&Interaction, &SettingsButton), Changed<Interaction>>,
    mut audio_settings: ResMut<AudioSettings>,
    mut player_prefs: ResMut<PlayerPrefs>,
    mut settings_state: ResMut<SettingsState>,
    mut commands: Commands,
    panel_query: Query<Entity, With<SettingsPanel>>,
) {
    for (interaction, button) in &interaction_query {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match button {
            SettingsButton::ToggleMute => {
                audio_settings.muted = !audio_settings.muted;
                save_audio_prefs(&audio_settings);
            },
            SettingsButton::VolumeUp => {
                audio_settings.master_volume = (audio_settings.master_volume + 0.1).min(1.0);
                save_audio_prefs(&audio_settings);
            },
            SettingsButton::VolumeDown => {
                audio_settings.master_volume = (audio_settings.master_volume - 0.1).max(0.0);
                save_audio_prefs(&audio_settings);
            },
            SettingsButton::ToastPositionCycle => {
                player_prefs.0.toast_position = match player_prefs.0.toast_position {
                    ToastPosition::TopRight => ToastPosition::TopLeft,
                    ToastPosition::TopLeft => ToastPosition::BottomRight,
                    ToastPosition::BottomRight => ToastPosition::BottomLeft,
                    ToastPosition::BottomLeft => ToastPosition::TopRight,
                };
                save_overlay_prefs(&player_prefs.0);
            },
            SettingsButton::DensityCycle => {
                player_prefs.0.notification_density = match player_prefs.0.notification_density {
                    NotificationDensity::All => NotificationDensity::Compact,
                    NotificationDensity::Compact => NotificationDensity::CriticalOnly,
                    NotificationDensity::CriticalOnly => NotificationDensity::All,
                };
                save_overlay_prefs(&player_prefs.0);
            },
            SettingsButton::Close => {
                settings_state.visible = false;
                for entity in &panel_query {
                    commands.entity(entity).despawn();
                }
            },
        }
    }
}

/// Persist audio settings to localStorage.
#[allow(unused_variables)]
fn save_audio_prefs(settings: &AudioSettings) {
    #[cfg(target_family = "wasm")]
    {
        if let Some(window) = web_sys::window()
            && let Ok(Some(storage)) = window.local_storage()
        {
            let _ = storage.set_item("audio_muted", &settings.muted.to_string());
            let _ = storage.set_item("audio_master_volume", &settings.master_volume.to_string());
        }
    }
}

/// Persist overlay prefs to localStorage.
#[allow(unused_variables)]
fn save_overlay_prefs(prefs: &OverlayPlayerPrefs) {
    #[cfg(target_family = "wasm")]
    {
        if let Some(window) = web_sys::window()
            && let Ok(Some(storage)) = window.local_storage()
        {
            let _ = storage.set_item(
                "overlay_toast_position",
                &format!("{:?}", prefs.toast_position),
            );
            let _ = storage.set_item(
                "overlay_notification_density",
                &format!("{:?}", prefs.notification_density),
            );
        }
    }
}

/// Load player prefs from localStorage on entering lobby.
fn load_player_prefs(
    #[cfg_attr(not(target_family = "wasm"), allow(unused_mut))] mut prefs: ResMut<PlayerPrefs>,
) {
    #[cfg(target_family = "wasm")]
    {
        if let Some(window) = web_sys::window()
            && let Ok(Some(storage)) = window.local_storage()
        {
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
        }
    }
    let _ = &prefs;
}
