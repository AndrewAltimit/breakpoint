use wasm_bindgen::prelude::*;

/// WASM entry point.
#[wasm_bindgen(start)]
pub fn start() {
    web_sys::console::log_1(&"Breakpoint client initialized".into());
    // TODO(phase1): App state machine, lobby UI, WSS connection, overlay rendering
}
