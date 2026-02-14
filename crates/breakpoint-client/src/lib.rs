mod app;
mod audio;
mod between_rounds;
mod camera;
mod effects;
pub mod game;
mod game_over;
pub mod lobby;
pub mod net_client;
pub mod overlay;
mod settings;
mod shaders;
mod storage;

use bevy::prelude::*;
use wasm_bindgen::prelude::*;

use app::AppState;
use audio::AudioPlugin;
use between_rounds::BetweenRoundsPlugin;
use camera::GameCameraPlugin;
use effects::EffectsPlugin;
use game::GamePlugin;
use game_over::GameOverPlugin;
use lobby::LobbyPlugin;
use net_client::WsClient;
use overlay::OverlayPlugin;
use settings::SettingsPlugin;
use shaders::ShadersPlugin;

/// WASM entry point.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(target_family = "wasm")]
    console_error_panic_hook::set_once();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                canvas: Some("#game-canvas".to_string()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .init_state::<AppState>()
        .insert_non_send_resource(WsClient::new())
        .add_plugins((
            LobbyPlugin,
            GamePlugin,
            GameCameraPlugin,
            EffectsPlugin,
            ShadersPlugin,
            BetweenRoundsPlugin,
            GameOverPlugin,
            OverlayPlugin,
            AudioPlugin,
            SettingsPlugin,
        ))
        .run();
}
