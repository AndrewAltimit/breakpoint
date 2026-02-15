pub mod app;
mod audio;
mod bridge;
mod camera_gl;
mod diag;
mod effects;
pub mod game;
mod input;
pub mod net_client;
pub mod overlay;
mod renderer;
mod scene;
mod storage;
pub mod theme;

use wasm_bindgen::prelude::*;

/// WASM entry point.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(target_family = "wasm")]
    console_error_panic_hook::set_once();

    #[cfg(target_family = "wasm")]
    app::run();
}
