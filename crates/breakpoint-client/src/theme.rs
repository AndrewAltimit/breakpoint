use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Client visual theme, loaded from JSON at compile time.
/// All colors are stored as `[f32; 4]` (RGBA) or `[f32; 3]` (RGB).
#[derive(Debug, Clone, Default, Serialize, Deserialize, Resource)]
#[serde(default)]
pub struct Theme {
    pub ui: UiTheme,
    pub overlay: OverlayTheme,
    pub golf: GolfTheme,
    pub platformer: PlatformerTheme,
    pub lasertag: LaserTagTheme,
    pub camera: CameraTheme,
    pub animation: AnimationTheme,
    pub audio: AudioTheme,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiTheme {
    pub lobby_bg: [f32; 4],
    pub button_primary: [f32; 3],
    pub button_start: [f32; 3],
    pub text_title: [f32; 3],
    pub text_primary: [f32; 3],
    pub text_secondary: [f32; 3],
    pub text_accent: [f32; 3],
    pub panel_bg: [f32; 4],
    pub settings_button: [f32; 3],
    pub settings_close: [f32; 3],
    pub save_flash: [f32; 3],
    pub score_header: [f32; 3],
    pub score_positive: [f32; 3],
    pub score_gold: [f32; 3],
    pub error_bg: [f32; 4],
    pub return_button: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayTheme {
    pub toast_critical: [f32; 4],
    pub toast_urgent: [f32; 4],
    pub toast_notice: [f32; 4],
    pub toast_ambient: [f32; 4],
    pub ticker_bg: [f32; 4],
    pub ticker_text: [f32; 4],
    pub alert_badge: [f32; 3],
    pub dashboard_bg: [f32; 4],
    pub dashboard_title: [f32; 3],
    pub dashboard_text: [f32; 3],
    pub dashboard_tab: [f32; 3],
    pub claim_button: [f32; 3],
    pub toast_source: [f32; 4],
    pub toast_claim: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GolfTheme {
    pub ground_color: [f32; 3],
    pub wall_color: [f32; 3],
    pub bumper_color: [f32; 3],
    pub hole_color: [f32; 3],
    pub ball_color: [f32; 3],
    pub flag_color: [f32; 3],
    pub aim_line_color: [f32; 4],
    pub power_indicator_color: [f32; 3],
    pub hud_text: [f32; 3],
    pub dirt_color: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlatformerTheme {
    pub solid_tile: [f32; 3],
    pub grass_tile: [f32; 3],
    pub hazard_tile: [f32; 3],
    pub platform_tile: [f32; 3],
    pub finish_tile: [f32; 3],
    pub hud_text: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LaserTagTheme {
    pub arena_floor: [f32; 3],
    pub wall_solid: [f32; 3],
    pub wall_reflective: [f32; 3],
    pub smoke_zone: [f32; 4],
    pub hud_text: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraTheme {
    pub clear_color: [f32; 3],
    pub ambient_brightness: f32,
    pub directional_illuminance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnimationTheme {
    pub interpolation_speed: f32,
    pub toast_duration: f32,
    pub controls_hint_duration: f32,
    pub laser_trail_max_age: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioTheme {
    pub master_volume: f32,
    pub game_volume: f32,
    pub overlay_volume: f32,
}

// --- Default implementations matching current hardcoded values ---

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            lobby_bg: [0.1, 0.1, 0.18, 0.95],
            button_primary: [0.2, 0.4, 0.8],
            button_start: [0.1, 0.6, 0.2],
            text_title: [0.3, 0.7, 1.0],
            text_primary: [0.9, 0.9, 0.9],
            text_secondary: [0.8, 0.8, 0.8],
            text_accent: [0.6, 0.8, 0.6],
            panel_bg: [0.05, 0.05, 0.12, 0.95],
            settings_button: [0.25, 0.25, 0.4],
            settings_close: [0.5, 0.2, 0.2],
            save_flash: [0.3, 1.0, 0.3],
            score_header: [0.3, 0.7, 1.0],
            score_positive: [0.3, 0.9, 0.3],
            score_gold: [1.0, 0.85, 0.2],
            error_bg: [0.8, 0.1, 0.1, 0.9],
            return_button: [0.2, 0.4, 0.8],
        }
    }
}

impl Default for OverlayTheme {
    fn default() -> Self {
        Self {
            toast_critical: [0.7, 0.1, 0.1, 0.9],
            toast_urgent: [0.7, 0.4, 0.1, 0.9],
            toast_notice: [0.15, 0.3, 0.6, 0.9],
            toast_ambient: [0.2, 0.2, 0.2, 0.9],
            ticker_bg: [0.0, 0.0, 0.0, 0.6],
            ticker_text: [0.8, 0.8, 0.8, 0.9],
            alert_badge: [0.8, 0.2, 0.2],
            dashboard_bg: [0.08, 0.08, 0.15, 0.95],
            dashboard_title: [0.3, 0.7, 1.0],
            dashboard_text: [0.8, 0.8, 0.8],
            dashboard_tab: [0.25, 0.25, 0.35],
            claim_button: [0.2, 0.5, 0.2],
            toast_source: [0.7, 0.7, 0.7, 0.9],
            toast_claim: [0.3, 0.8, 0.3],
        }
    }
}

impl Default for GolfTheme {
    fn default() -> Self {
        Self {
            ground_color: [0.08, 0.35, 0.08],
            wall_color: [0.35, 0.2, 0.1],
            bumper_color: [0.55, 0.55, 0.65],
            hole_color: [0.03, 0.03, 0.03],
            ball_color: [0.7, 0.7, 0.7],
            flag_color: [1.0, 0.2, 0.2],
            aim_line_color: [1.0, 1.0, 1.0, 0.5],
            power_indicator_color: [1.0, 1.0, 0.3],
            hud_text: [0.8, 0.8, 0.8],
            dirt_color: [0.3, 0.2, 0.1],
        }
    }
}

impl Default for PlatformerTheme {
    fn default() -> Self {
        Self {
            solid_tile: [0.4, 0.4, 0.5],
            grass_tile: [0.3, 0.6, 0.3],
            hazard_tile: [0.9, 0.2, 0.1],
            platform_tile: [0.2, 0.5, 0.9],
            finish_tile: [1.0, 0.85, 0.1],
            hud_text: [0.9, 0.9, 0.9, 0.85],
        }
    }
}

impl Default for LaserTagTheme {
    fn default() -> Self {
        Self {
            arena_floor: [0.08, 0.08, 0.12],
            wall_solid: [0.3, 0.3, 0.4],
            wall_reflective: [0.5, 0.7, 0.9],
            smoke_zone: [0.4, 0.4, 0.4, 0.3],
            hud_text: [0.9, 0.9, 0.9, 0.85],
        }
    }
}

impl Default for CameraTheme {
    fn default() -> Self {
        Self {
            clear_color: [0.53, 0.81, 0.98],
            ambient_brightness: 300.0,
            directional_illuminance: 8000.0,
        }
    }
}

impl Default for AnimationTheme {
    fn default() -> Self {
        Self {
            interpolation_speed: 15.0,
            toast_duration: 8.0,
            controls_hint_duration: 8.0,
            laser_trail_max_age: 0.3,
        }
    }
}

impl Default for AudioTheme {
    fn default() -> Self {
        Self {
            master_volume: 0.5,
            game_volume: 0.7,
            overlay_volume: 0.8,
        }
    }
}

// --- Helper methods to convert theme colors to Bevy Color ---

impl Theme {
    /// Load theme from embedded JSON, falling back to defaults.
    pub fn load() -> Self {
        let json = include_str!("../../../web/theme.json");
        serde_json::from_str(json).unwrap_or_default()
    }
}

/// Convert an RGB [f32; 3] array to a Bevy Color.
pub fn rgb(c: &[f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

/// Convert an RGBA [f32; 4] array to a Bevy Color.
pub fn rgba(c: &[f32; 4]) -> Color {
    Color::srgba(c[0], c[1], c[2], c[3])
}

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Theme::load());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_roundtrips_through_json() {
        let theme = Theme::default();
        let json = serde_json::to_string_pretty(&theme).unwrap();
        let loaded: Theme = serde_json::from_str(&json).unwrap();
        assert_eq!(
            theme.camera.ambient_brightness,
            loaded.camera.ambient_brightness
        );
        assert_eq!(theme.ui.lobby_bg, loaded.ui.lobby_bg);
    }

    #[test]
    fn partial_json_fills_defaults() {
        let json = r#"{"camera": {"ambient_brightness": 500.0}}"#;
        let theme: Theme = serde_json::from_str(json).unwrap();
        assert_eq!(theme.camera.ambient_brightness, 500.0);
        // Other fields should be defaults
        assert_eq!(theme.camera.directional_illuminance, 8000.0);
        assert_eq!(theme.ui.lobby_bg, [0.1, 0.1, 0.18, 0.95]);
    }
}
